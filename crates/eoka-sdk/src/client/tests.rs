use super::*;
use tokio::net::UnixListener;

async fn handshake(listener: &UnixListener) {
    handshake_with(listener, serde_json::json!({})).await;
}

async fn handshake_with(listener: &UnixListener, extra: serde_json::Value) {
    let (stream, _) = listener.accept().await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    let request: Request = read_msg(&mut reader).await.unwrap();
    assert!(matches!(request, Request::SessionHello));
    let mut hello = serde_json::json!({"protocol_major": eoka_protocol::sidecar::PROTOCOL_VERSION.major,"mode":"headless","configuration_fingerprint":test_client("fixture").spec.configuration_fingerprint()});
    for (key, value) in extra.as_object().into_iter().flatten() {
        hello[key] = value.clone();
    }
    write_msg(&mut writer, &Response::ok(hello)).await.unwrap();
}

fn fake_daemon(prefix: &str) -> (String, std::path::PathBuf, UnixListener) {
    let session = format!("{prefix}-{}", uuid::Uuid::new_v4());
    session::ensure_runtime_dir().unwrap();
    let socket = session::socket_path(&session);
    let listener = UnixListener::bind(&socket).unwrap();
    (session, socket, listener)
}

#[tokio::test]
async fn a_daemon_that_accepts_but_stays_silent_is_reported_as_unresponsive() {
    let (session, socket, listener) = fake_daemon("silent-daemon");
    let server = tokio::spawn(async move {
        let (held, _) = listener.accept().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1500), listener.accept())
                .await
                .is_err(),
            "nothing may be sent after an unanswered handshake"
        );
        drop(held);
    });
    let started = std::time::Instant::now();
    let response = test_client(&session).info().await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    let detail = response.error_detail.unwrap();
    assert_eq!(detail.code, "daemon_unresponsive");
    assert!(detail.message.contains("No command was sent"));
    let hint = detail.hint.unwrap();
    assert!(hint.contains(&format!("eoka --session {session} status")));
    assert!(hint.contains(&format!("eoka --session {session} kill")));
    assert_eq!(
        detail.context.unwrap().action_status,
        Some(eoka_protocol::ActionStatus::NotAttempted)
    );
    server.await.unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[tokio::test]
async fn an_unanswered_read_times_out_with_the_daemon_activity_and_is_not_replayed() {
    let (session, socket, listener) = fake_daemon("reply-timeout");
    let server = tokio::spawn(async move {
        handshake_with(&listener, serde_json::json!({"command_limit_ms":100})).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, _held) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        assert_eq!(request.cmd(), "info");
        handshake_with(
            &listener,
            serde_json::json!({"activity":{"busy":{"cmd":"info","for_ms":400},"queued":0,"blocker":null}}),
        )
        .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(300), listener.accept())
                .await
                .is_err(),
            "an unanswered command must not be sent again"
        );
    });
    let started = std::time::Instant::now();
    let response = test_client(&session).info().await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!response.ok);
    let detail = response.error_detail.unwrap();
    assert_eq!(detail.code, "reply_timeout");
    assert!(
        detail.message.contains("No reply to `info`"),
        "{}",
        detail.message
    );
    assert!(
        detail.message.contains("still running `info`"),
        "{}",
        detail.message
    );
    assert!(detail.message.contains("Nothing was sent again"));
    assert_eq!(detail.retryable, Some(true));
    assert!(detail.context.is_none());
    assert!(detail.hint.unwrap().contains("status"));
    server.await.unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[tokio::test]
async fn an_unanswered_script_reports_that_it_may_have_run() {
    let (session, socket, listener) = fake_daemon("script-timeout");
    let server = tokio::spawn(async move {
        handshake_with(&listener, serde_json::json!({"command_limit_ms":100})).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, held) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        assert_eq!(request.cmd(), "exec");
        let (probe, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_millis(1700)).await;
        drop((probe, held));
    });
    let response = test_client(&session)
        .exec(serde_json::from_value(serde_json::json!({"code":"() => 1"})).unwrap())
        .await
        .unwrap();
    let detail = response.error_detail.unwrap();
    assert_eq!(detail.code, "reply_timeout");
    assert!(detail.message.contains("does not answer status probes"));
    assert!(detail
        .message
        .contains("may have run; it was not sent again"));
    assert_eq!(detail.retryable, Some(false));
    assert_eq!(
        detail.context.unwrap().action_status,
        Some(eoka_protocol::ActionStatus::MayHaveExecuted)
    );
    assert!(detail.hint.unwrap().contains("kill"));
    server.await.unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[tokio::test]
async fn a_busy_refusal_is_returned_as_is_and_never_replayed() {
    let (session, socket, listener) = fake_daemon("busy-daemon");
    let server = tokio::spawn(async move {
        handshake(&listener).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        let _: Request = read_msg(&mut reader).await.unwrap();
        let busy = Response::err_detail(
            eoka_protocol::ErrorDetail::new(
                "daemon_busy",
                "Session `work` is busy: `wait` has been running for 9.0 s. `text` was not started.",
            )
            .retryable(true)
            .hint("Run it again once the running command returns."),
        );
        write_msg(&mut writer, &busy).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err(),
            "a refused command must not be retried by the client"
        );
    });
    let response = test_client(&session).text().await.unwrap();
    let detail = response.error_detail.unwrap();
    assert_eq!(detail.code, "daemon_busy");
    assert!(detail.message.contains("`wait` has been running"));
    server.await.unwrap();
    std::fs::remove_file(socket).unwrap();
}

#[tokio::test]
async fn a_missing_session_names_the_command_that_starts_it() {
    let name = format!("absent-{}", uuid::Uuid::new_v4());
    let client = test_client(&name);
    let read = client.info().await.unwrap();
    let detail = read.error_detail.unwrap();
    assert_eq!(detail.code, "session_unavailable");
    assert!(detail.message.contains(&name));
    assert!(detail
        .hint
        .unwrap()
        .contains(&format!("eoka --session {name} open URL")));
    let observed = client
        .agent_observe(Default::default())
        .await
        .unwrap()
        .error_detail
        .unwrap();
    assert_eq!(observed.code, "session_unavailable");
    let step = client
        .agent_step(
            serde_json::from_value(serde_json::json!({"action":{"kind":"press_key","key":"Tab"}}))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(step.data.unwrap()["status"], "not_dispatched");
    assert!(!session::socket_path(&name).exists());
}

#[test]
fn every_request_has_a_reply_deadline_derived_from_its_own_timeout() {
    let request =
        |cmd: &str, args: serde_json::Value| eoka_protocol::request_from_cmd(cmd, args).unwrap();
    let budget = |cmd: &str, args: serde_json::Value, limit: Option<u64>| {
        reply_budget(&request(cmd, args), limit).as_millis() as u64
    };
    let none = serde_json::json!({});
    assert_eq!(
        budget("info", none.clone(), Some(30_000)),
        30_000 + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget("info", none.clone(), Some(90_000)),
        90_000 + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget("info", none.clone(), None),
        UNKNOWN_COMMAND_LIMIT_MS + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget(
            "wait",
            serde_json::json!({"text":"Done","timeout":45_000}),
            Some(30_000)
        ),
        45_000 + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget("wait", serde_json::json!({"ms":2_000}), Some(30_000)),
        2_000 + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget(
            "open",
            serde_json::json!({"url":"https://example.test/","timeout_ms":5_000}),
            None
        ),
        5_000 + LAUNCH_MARGIN_MS + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget(
            "agent_observe",
            serde_json::json!({"timeout_ms":3_000}),
            None
        ),
        4_000
    );
    assert_eq!(
        budget(
            "agent_step",
            serde_json::json!({"action":{"kind":"back"},"timeout_ms":120_000}),
            None
        ),
        61_000
    );
    assert_eq!(
        budget(
            "operation_wait",
            serde_json::json!({"id":"x","timeout_ms":70_000}),
            None
        ),
        70_000 + REPLY_MARGIN_MS
    );
    assert_eq!(
        budget("session_hello", none, None),
        HELLO_LIMIT.as_millis() as u64
    );
}

#[tokio::test]
async fn agent_step_allocates_id_before_transport_and_retains_it_on_disconnect() {
    let session = format!("agent-step-{}", uuid::Uuid::new_v4());
    session::ensure_runtime_dir().unwrap();
    let socket = session::socket_path(&session);
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        handshake(&listener).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, _) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        let Request::AgentStep(args) = request else {
            panic!("wrong request")
        };
        args.operation_id.unwrap()
    });
    let request =
        serde_json::from_value(serde_json::json!({"action":{"kind":"press_key","key":"Tab"}}))
            .unwrap();
    let response = test_client(&session).agent_step(request).await.unwrap();
    let id = server.await.unwrap();
    assert!(!response.ok);
    assert!(response.error.unwrap().contains(&id));
    assert_eq!(response.error_detail.unwrap().retryable, Some(false));
    std::fs::remove_file(socket).unwrap();
}

#[test]
fn the_daemon_is_the_eoka_cli_not_whatever_program_uses_the_sdk() {
    let cli = std::path::PathBuf::from("/opt/tools/eoka-renamed");
    assert_eq!(
        daemon_program(Some(&cli), Some("/elsewhere/eoka".into())),
        cli
    );
    assert_eq!(
        daemon_program(None, Some("/elsewhere/eoka".into())),
        std::path::PathBuf::from("/elsewhere/eoka")
    );
    assert_eq!(
        daemon_program(None, Some("".into())),
        std::path::PathBuf::from("eoka")
    );
    assert_eq!(daemon_program(None, None), std::path::PathBuf::from("eoka"));
}

#[test]
fn disabling_the_sandbox_is_a_creation_setting() {
    let spec = |sandbox| LaunchSpec::Launch {
        headless: true,
        from_profile: None,
        clone_state_from: None,
        no_stealth: false,
        proxy: None,
        no_js: false,
        js_allow: Vec::new(),
        js_block: Vec::new(),
        persist: false,
        geo_align: true,
        auto_solve_cloudflare: false,
        sandbox,
    };
    assert!(!spec(true).has_creation_settings());
    assert!(spec(false).has_creation_settings());
    assert_ne!(
        spec(true).configuration_fingerprint(),
        spec(false).configuration_fingerprint()
    );
}

fn test_client(session: &str) -> EokaClient {
    EokaClient::new(
        session,
        LaunchSpec::Launch {
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
        },
    )
}

#[tokio::test]
async fn typed_mouse_down_helper_sends_protocol_request() {
    let session = format!("eoka-sdk-held-input-test-{}", std::process::id());
    session::ensure_runtime_dir().unwrap();
    let sock = session::socket_path(&session);
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(async move {
        handshake(&listener).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        assert_eq!(request.cmd(), "mouse_down");
        assert_eq!(request.args_json()["x"], 10.0);
        assert_eq!(request.args_json()["y"], 20.0);
        assert_eq!(request.args_json()["button"], "left");
        write_msg(&mut writer, &Response::ok_text("ok"))
            .await
            .unwrap();
    });

    let response = test_client(&session)
        .mouse_down(MouseButtonArgs {
            x: 10.0,
            y: 20.0,
            button: eoka_protocol::MouseButton::Left,
        })
        .await
        .unwrap();

    assert!(response.ok);
    server.await.unwrap();
    let _ = std::fs::remove_file(&sock);
}

#[tokio::test]
async fn existing_session_requests_never_launch_a_daemon() {
    let name = format!("datadome-missing-{}", std::process::id());
    let response = test_client(&name)
        .captcha_datadome(CaptchaDatadomeArgs::default())
        .await
        .unwrap();
    assert!(!response.ok);
    assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
    assert!(!session::socket_path(&name).exists());
    assert!(!session::pid_path(&name).exists());
    for request in [
        Request::Frames,
        Request::FrameEval(FrameEvalArgs {
            frame: "id:fixture-frame".into(),
            code: Some("1+1".into()),
            file: None,
        }),
    ] {
        let response = test_client(&name).call(request).await.unwrap();
        assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
        assert!(!session::socket_path(&name).exists());
        assert!(!session::pid_path(&name).exists());
    }
    let invalid = test_client(&name)
        .captcha_datadome(CaptchaDatadomeArgs {
            max_attempts: 0,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(invalid.error_detail.unwrap().code, "invalid_input");
}

#[tokio::test]
async fn existing_session_transport_failure_and_old_daemon_are_not_replayed() {
    for request in [
        Request::CaptchaDatadome(CaptchaDatadomeArgs::default()),
        Request::Frames,
        Request::FrameEval(FrameEvalArgs {
            frame: "id:fixture-frame".into(),
            code: Some("1+1".into()),
            file: None,
        }),
    ] {
        let cmd = request.cmd();
        for (index, error) in [
            None,
            Some("channel is empty and sending half is closed".to_string()),
            Some(format!("Invalid command request: unknown variant {cmd}")),
        ]
        .into_iter()
        .enumerate()
        {
            let name = format!("{cmd}-transport-{}-{index}", std::process::id());
            session::ensure_runtime_dir().unwrap();
            let sock = session::socket_path(&name);
            let listener = UnixListener::bind(&sock).unwrap();
            let server = tokio::spawn(async move {
                handshake(&listener).await;
                let (stream, _) = listener.accept().await.unwrap();
                let (mut reader, mut writer) = stream.into_split();
                let request: Request = read_msg(&mut reader).await.unwrap();
                assert_eq!(request.cmd(), cmd);
                if let Some(message) = error {
                    write_msg(&mut writer, &Response::err(message))
                        .await
                        .unwrap();
                }
                drop(reader);
                drop(writer);
                assert!(
                    tokio::time::timeout(Duration::from_millis(150), listener.accept())
                        .await
                        .is_err()
                );
            });
            let client = test_client(&name);
            let response = client.call(request.clone()).await.unwrap();
            assert!(!response.ok);
            if index == 0 {
                assert_eq!(
                    response.error_detail.as_ref().unwrap().code,
                    if cmd == "captcha_datadome" {
                        "datadome_transport_error"
                    } else {
                        "frame_transport_error"
                    }
                );
            }
            if index == 2 {
                assert_eq!(
                    response.error_detail.as_ref().unwrap().code,
                    "unsupported_operation"
                );
            }
            server.await.unwrap();
            std::fs::remove_file(sock).unwrap();
        }
    }
}

#[tokio::test]
async fn downloads_require_existing_sessions_and_never_replay() {
    let name = format!("download-missing-{}", std::process::id());
    let response = test_client(&name)
        .download_start(eoka_protocol::DownloadStartArgs {
            operation_id: None,
            click: Some("css:#download".into()),
            url: None,
            output: "/tmp/not-created".into(),
            expect_bytes: None,
            start_timeout_ms: 1000,
        })
        .await
        .unwrap();
    assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
    assert!(!session::pid_path(&name).exists());
    for (index, error) in [
        None,
        Some("Invalid command request: unknown variant download_start"),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("download-transport-{}-{index}", std::process::id());
        session::ensure_runtime_dir().unwrap();
        let sock = session::socket_path(&name);
        let listener = UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            handshake(&listener).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert!(
                matches!(request,Request::ActionCommand(ref a) if a.command=="download_start" && !a.operation_id.is_empty())
            );
            if let Some(message) = error {
                write_msg(&mut writer, &Response::err(message))
                    .await
                    .unwrap();
            }
            drop(reader);
            drop(writer);
            assert!(
                tokio::time::timeout(Duration::from_millis(150), listener.accept())
                    .await
                    .is_err()
            );
        });
        let response = test_client(&name)
            .download_start(eoka_protocol::DownloadStartArgs {
                operation_id: None,
                click: Some("css:#download".into()),
                url: None,
                output: "/tmp/not-created".into(),
                expect_bytes: None,
                start_timeout_ms: 1000,
            })
            .await
            .unwrap();
        assert_eq!(
            response.error_detail.unwrap().code,
            if error.is_some() {
                "unsupported_operation"
            } else {
                "download_transport_error"
            }
        );
        server.await.unwrap();
        std::fs::remove_file(sock).unwrap();
    }
}

#[tokio::test]
async fn typed_open_helper_sends_protocol_request() {
    let session = format!("eoka-sdk-helper-test-{}", std::process::id());
    session::ensure_runtime_dir().unwrap();
    let sock = session::socket_path(&session);
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(async move {
        handshake(&listener).await;
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        assert_eq!(request.cmd(), "open");
        assert_eq!(request.args_json()["url"], "https://example.com");
        write_msg(&mut writer, &Response::ok_text("ok"))
            .await
            .unwrap();
    });

    let response = test_client(&session)
        .open(OpenArgs {
            url: "https://example.com".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(response.ok);
    server.await.unwrap();
    let _ = std::fs::remove_file(&sock);
}
