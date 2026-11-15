mod frontdoor;
#[cfg(test)]
mod idle_file_tests;

use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::launch_spec::LaunchSpec;
use crate::protocol::{write_msg, Request, Response};
use crate::session;
use eoka_server::engine::Engine;

fn engine_arguments(raw: &serde_json::Value) -> serde_json::Value {
    let mut args = raw
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if args.is_null() {
        args = serde_json::json!({});
    }
    if let Some(id) = raw.get("operation_id") {
        args["operation_id"] = id.clone();
    }
    args
}

fn engine_request(raw: &serde_json::Value, parsed: Request) -> Request {
    eoka_protocol::request_from_cmd(parsed.cmd(), engine_arguments(raw)).unwrap_or(parsed)
}

fn idle_timeout() -> Duration {
    let ms = std::env::var("EOKA_IDLE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30 * 60 * 1000);
    Duration::from_millis(ms)
}

pub async fn run(session_name: &str, spec: LaunchSpec) -> anyhow::Result<()> {
    let _session_lock = session::acquire_daemon_lock(session_name)?;

    let sock_path = session::trusted_socket_path(session_name)?;
    let pid_path = session::pid_path(session_name);

    if sock_path.exists() {
        let _ = std::fs::remove_file(&sock_path);
    }

    let listener = UnixListener::bind(&sock_path)?;
    session::write_pid_file(session_name)?;

    eprintln!(
        "[eoka] daemon started (session={}, pid={}, mode={})",
        session_name,
        std::process::id(),
        if spec.is_live() { "connect" } else { "launch" }
    );
    eprintln!("[eoka] socket: {}", sock_path.display());

    eoka_server::engine::set_host_version(env!("CARGO_PKG_VERSION"));
    eoka_server::engine::blockers::set_surface(eoka_server::engine::blockers::Surface::Cli);
    let command_limit_ms: u64 = match &spec {
        LaunchSpec::Launch { proxy: Some(_), .. } => 90_000,
        _ => 30_000,
    };
    let mut handler = Engine::new(session_name, spec);
    let mut last_activity = Instant::now();
    let mut waiters = tokio::task::JoinSet::new();
    let timeout = idle_timeout();

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let mut sigint =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
        tokio::select! {
            _ = sigterm.recv() => eprintln!("[eoka] received SIGTERM"),
            _ = sigint.recv() => eprintln!("[eoka] received SIGINT"),
        }
        let _ = shutdown_tx.send(()).await;
    });

    let mut hello = handler
        .handle(Request::SessionHello)
        .await
        .data
        .unwrap_or_default();
    if hello.is_object() {
        hello["command_limit_ms"] = serde_json::json!(command_limit_ms);
    }
    let (accepted, mut incoming) = mpsc::channel(1);
    let board = frontdoor::Board::new(session_name, hello);
    let frontdoor = tokio::spawn(frontdoor::serve(
        listener,
        board.clone(),
        handler.operation_manager(),
        accepted,
    ));

    let mut maintenance = tokio::time::interval(Duration::from_millis(25));
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            request = incoming.recv() => {
                let Some(frontdoor::Admitted { mut reader, mut writer, request: req, raw, turn }) = request else { break };
                last_activity = Instant::now();
                if matches!(req, Request::Shutdown) {
                    let _ = handler.handle(Request::Close).await;
                    let resp = Response::ok_text("Daemon shutting down");
                    let _ = write_msg(&mut writer, &resp).await;
                    break;
                }

                if let Request::DownloadWait(args) = req {
                    let manager = handler.download_manager();
                    waiters.spawn(async move {
                        let mut disconnected = [0u8; 1];
                        tokio::select! {
                            response = manager.wait(&args.id, args.timeout_ms) => {
                                let _ = write_msg(&mut writer, &response).await;
                            },
                            _ = reader.read(&mut disconnected) => {},
                        }
                    });
                    continue;
                }
                let args = engine_arguments(&raw);
                let req = engine_request(&raw, req);
                let (caller_left, left) = tokio::sync::watch::channel(false);
                let watchdog = tokio::spawn(async move {
                    let mut disconnected = [0u8; 1];
                    let _ = reader.read(&mut disconnected).await;
                    let _ = caller_left.send(true);
                });
                board.started(req.cmd(), &args);
                let resp = eoka_server::engine::blockers::abandonable(
                    left,
                    handler.handle(req.clone()),
                )
                .await;
                watchdog.abort();
                board.finished(handler.current_page());
                last_activity = Instant::now();
                if let Err(e) = write_msg(&mut writer, &resp).await {
                    eprintln!("[eoka] caller left before `{}` answered: {}", req.cmd(), e);
                }
                drop(turn);
            }
            _ = maintenance.tick() => {
                handler.service_idle_fetch();
                last_activity = last_activity.max(board.last_request());
                if board.has_waiters() || !waiters.is_empty() || handler.download_manager().is_active() || handler.operation_manager().is_active() || handler.is_recording() { last_activity = Instant::now(); }
                if last_activity.elapsed() > timeout && !handler.is_network_recording().await {
                    eprintln!("[eoka] idle timeout ({}s), shutting down", timeout.as_secs());
                    let _ = handler.handle(Request::Close).await;
                    break;
                }
            }
            _ = waiters.join_next(), if !waiters.is_empty() => { last_activity=Instant::now(); },
            _ = shutdown_rx.recv() => {
                let _ = handler.handle(Request::Close).await;
                break;
            }
        }
    }

    board.finished(None);
    frontdoor.abort();
    let _ = frontdoor.await;
    waiters.shutdown().await;
    drop(board);
    drop(handler);
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&pid_path);
    eprintln!("[eoka] daemon stopped");
    Ok(())
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    fn sent_by_the_cli(cmd: &str, args: serde_json::Value) -> serde_json::Value {
        json!({"cmd": cmd, "args": args, "operation_id": ID, "protocol_major": 2})
    }

    fn action(command: &str, arguments: serde_json::Value) -> serde_json::Value {
        sent_by_the_cli(
            "action_command",
            json!({"operation_id": ID, "command": command, "arguments": arguments}),
        )
    }

    #[test]
    fn requests_from_an_older_cli_reach_the_engine_unchanged() {
        let expect = json!({"kind": "url", "value": "/next", "contains": true});
        let envelopes = [
            (
                "click",
                json!({"operation_id": ID, "expect": expect, "target": "css:#save"}),
            ),
            (
                "dblclick",
                json!({"operation_id": ID, "target": "text:Open"}),
            ),
            (
                "fill",
                json!({"operation_id": ID, "target": "placeholder:Name", "text": "Ada", "input_mode": "sequential"}),
            ),
            (
                "select",
                json!({"operation_id": ID, "target": "css:select", "value": "b"}),
            ),
            (
                "hover",
                json!({"operation_id": ID, "target": "role:button"}),
            ),
            (
                "key",
                json!({"operation_id": ID, "timeout_ms": 5000, "key": "Enter"}),
            ),
            ("back", json!({"expect": expect, "timeout_ms": 5000})),
            ("forward", json!({"expect": null, "timeout_ms": null})),
            ("reload", json!(null)),
        ];
        for (command, arguments) in envelopes {
            let raw = action(command, arguments.clone());
            let request: Request = serde_json::from_value(raw.clone())
                .unwrap_or_else(|error| panic!("{command} envelope is rejected: {error}"));
            let request = engine_request(&raw, request);
            assert_eq!(request.cmd(), "action_command", "{command}");
            assert_eq!(
                request.args_json(),
                json!({"operation_id": ID, "command": command, "arguments": arguments}),
                "{command}"
            );
        }
        let direct = [
            (
                "open",
                json!({"url": "https://example.com/"}),
                json!({"url": "https://example.com/", "operation_id": ID}),
            ),
            (
                "agent_step",
                json!({"action": {"kind": "back"}}),
                json!({"action": {"kind": "back"}, "operation_id": ID}),
            ),
            (
                "action_perform",
                json!({"operation_id": ID, "action": {"kind": "back"}, "options": {"timeout_ms": 900}}),
                json!({"operation_id": ID, "action": {"kind": "back"}}),
            ),
            (
                "snapshot",
                json!({"interactive": true}),
                json!({"interactive": true}),
            ),
            ("info", json!(null), json!({})),
        ];
        for (cmd, args, reaches_engine) in direct {
            let raw = sent_by_the_cli(cmd, args);
            let request: Request = serde_json::from_value(raw.clone())
                .unwrap_or_else(|error| panic!("{cmd} is rejected: {error}"));
            let request = engine_request(&raw, request);
            assert_eq!(request.cmd(), cmd);
            let reached = request.args_json();
            for (field, value) in reaches_engine.as_object().unwrap() {
                assert_eq!(&reached[field], value, "{cmd} {field}");
            }
        }
        let timed = sent_by_the_cli(
            "action_perform",
            json!({"operation_id": ID, "action": {"kind": "back"}, "options": {"timeout_ms": 900}}),
        );
        let request: Request = serde_json::from_value(timed.clone()).unwrap();
        assert_eq!(
            engine_request(&timed, request).args_json()["options"]["timeout_ms"],
            900
        );
        let bare = json!({"cmd": "text", "protocol_major": 2});
        let request: Request = serde_json::from_value(bare.clone()).unwrap();
        assert_eq!(engine_request(&bare, request).args_json(), json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn idle_timeout_shuts_daemon_down_with_no_clients() {
        unsafe {
            std::env::set_var("EOKA_IDLE_TIMEOUT", "50");
        }
        let session_name = format!("eoka-idle-test-{}", std::process::id());
        let spec = LaunchSpec::Launch {
            headless: true,
            from_profile: None,
            clone_state_from: None,
            no_stealth: false,
            proxy: None,
            no_js: false,
            js_allow: Vec::new(),
            js_block: Vec::new(),
            persist: false,
            geo_align: false,
            auto_solve_cloudflare: false,
            sandbox: true,
        };

        let result = tokio::time::timeout(Duration::from_secs(15), run(&session_name, spec)).await;

        unsafe {
            std::env::remove_var("EOKA_IDLE_TIMEOUT");
        }

        assert!(
            result.is_ok(),
            "daemon did not shut itself down within 15s of being idle"
        );
        assert!(result.unwrap().is_ok(), "daemon run() returned an error");
        assert!(!session::socket_path(&session_name).exists());
        assert!(!session::pid_path(&session_name).exists());
    }
}
