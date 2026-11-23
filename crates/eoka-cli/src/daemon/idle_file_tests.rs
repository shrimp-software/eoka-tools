use eoka_protocol::launch_spec::LaunchSpec;
use serde_json::{json, Value};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome daemon idle-file deadline regression"]
async fn delayed_idle_file_operation_keeps_commands_and_shutdown_serviceable() {
    tokio::task::LocalSet::new()
        .run_until(async {
            async fn command(
                name: &str,
                cmd: &str,
                args: Value,
                deadline: Duration,
            ) -> eoka_sdk::Response {
                let socket = tokio::net::UnixStream::connect(eoka_sdk::session::socket_path(name))
                    .await
                    .unwrap();
                let (reader, mut writer) = socket.into_split();
                let mut reader = tokio::io::BufReader::new(reader);
                let frame = eoka_protocol::wire::request_frame(cmd, args);
                tokio::time::timeout(
                    deadline,
                    eoka_protocol::wire::call(&mut reader, &mut writer, Some(&frame)),
                )
                .await
                .unwrap_or_else(|_| panic!("`{cmd}` did not answer within {deadline:?}"))
                .unwrap()
            }
            const SETUP: Duration = Duration::from_secs(30);
            const SERVICE: Duration = Duration::from_secs(3);
            let name = format!("idle-file-fixture-{}", std::process::id());
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
            let daemon_name = name.clone();
            let daemon = tokio::task::spawn_local(async move {
                super::run(&daemon_name, spec, Duration::from_secs(1800)).await
            });
            for _ in 0..100 {
                if eoka_sdk::session::socket_path(&name).exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let fixture = eoka_solvers::datadome::fixture::Fixture::scenario("absent").await;
            let local_url = fixture.url.replace("app.test", "127.0.0.1");
            assert!(
                command(&name, "open", json!({"url":local_url}), SETUP)
                    .await
                    .ok
            );
            let files = tempfile::tempdir().unwrap();
            let path = files.path().join("response");
            std::fs::write(&path, b"fixture").unwrap();
            assert!(
                command(
                    &name,
                    "intercept_add",
                    json!({"url_pattern":"*/accepted", "respond":path}),
                    SETUP
                )
                .await
                .ok
            );
            let _delay = eoka_server::engine::test_support::FILE_SLOT
                .clone()
                .acquire_owned()
                .await
                .unwrap();
            for next in ["info", "shutdown"] {
                assert!(
                    command(
                        &name,
                        "exec",
                        json!({"raw":true,"code":"setTimeout(()=>fetch('/accepted'),100); void 0"}),
                        SERVICE
                    )
                    .await
                    .ok
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
                let started = tokio::time::Instant::now();
                assert!(command(&name, next, json!({}), SERVICE).await.ok);
                assert!(started.elapsed() < Duration::from_secs(2));
            }
            tokio::time::timeout(Duration::from_secs(3), daemon)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        })
        .await;
}
