mod frontdoor;
#[cfg(test)]
mod idle_file_tests;

use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::launch_spec::LaunchSpec;
use crate::protocol::{Request, Response};
use crate::session;
use eoka_server::engine::Engine;

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

    let mut hello = handler.hello();
    hello.command_limit_ms = Some(command_limit_ms);
    let (accepted, mut incoming) = mpsc::channel(1);
    let board = frontdoor::Board::new(session_name, hello);
    let frontdoor = tokio::spawn(frontdoor::serve(
        listener,
        board.clone(),
        handler.operation_manager(),
        handler.download_manager(),
        accepted,
    ));

    let mut maintenance = tokio::time::interval(Duration::from_millis(25));
    maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            request = incoming.recv() => {
                let Some(frontdoor::Admitted { mut reader, mut writer, request: req, arguments: args, turn }) = request else { break };
                last_activity = Instant::now();
                if matches!(req, Request::Shutdown) {
                    let _ = handler.handle(Request::Close).await;
                    let resp = Response::ok_text("Daemon shutting down");
                    let _ = writer.send(&resp).await;
                    break;
                }

                if let Request::DownloadWait(args) = req {
                    let manager = handler.download_manager();
                    waiters.spawn(async move {
                        let mut disconnected = [0u8; 1];
                        tokio::select! {
                            response = manager.wait(&args.id, args.timeout_ms) => {
                                let _ = writer.send(&response).await;
                            },
                            _ = reader.read(&mut disconnected) => {},
                        }
                    });
                    continue;
                }
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
                if let Err(e) = writer.send(&resp).await {
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
    use eoka_protocol::wire;
    use serde_json::json;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    fn parsed(cmd: &str, params: serde_json::Value) -> eoka_protocol::Request {
        let frame = wire::request_frame(cmd, params);
        wire::incoming(frame)
            .ok()
            .unwrap()
            .request()
            .unwrap_or_else(|error| panic!("{cmd} is rejected: {}", error.message))
    }

    #[test]
    fn session_frames_reach_the_engine_unchanged() {
        let expect = json!({"kind": "url", "value": "/next", "contains": true});
        let actions = [
            (
                "click",
                json!({"operation_id": ID, "expect": expect, "target": "css:#save"}),
            ),
            (
                "fill",
                json!({"operation_id": ID, "target": "placeholder:Name", "text": "Ada", "input_mode": "sequential"}),
            ),
            (
                "key",
                json!({"operation_id": ID, "timeout_ms": 5000, "key": "Enter"}),
            ),
            ("back", json!({"expect": expect, "timeout_ms": 5000})),
            ("reload", json!(null)),
        ];
        for (command, arguments) in actions {
            let args = json!({"operation_id": ID, "command": command, "arguments": arguments});
            let request = parsed("action_command", args.clone());
            assert_eq!(request.cmd(), "action_command", "{command}");
            assert_eq!(request.args_json(), args, "{command}");
        }
        let direct = [
            (
                "open",
                json!({"url": "https://example.com/", "operation_id": ID}),
            ),
            (
                "agent_step",
                json!({"action": {"kind": "back"}, "operation_id": ID}),
            ),
            (
                "action_perform",
                json!({"operation_id": ID, "action": {"kind": "back"}, "options": {"timeout_ms": 900}}),
            ),
            ("snapshot", json!({"interactive": true})),
        ];
        for (cmd, params) in direct {
            let reached = parsed(cmd, params.clone()).args_json();
            for (field, value) in params.as_object().unwrap() {
                if !value.is_object() {
                    assert_eq!(&reached[field], value, "{cmd} {field}");
                }
            }
        }
        assert_eq!(
            parsed("action_perform", json!({"operation_id": ID, "action": {"kind": "back"}, "options": {"timeout_ms": 900}}))
                .args_json()["options"]["timeout_ms"],
            900
        );
        assert_eq!(parsed("text", json!(null)).cmd(), "text");
        assert_eq!(parsed("tab_new", json!({})).cmd(), "tab_new");
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
