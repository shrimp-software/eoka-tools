use super::*;
use futures_util::{SinkExt, StreamExt};
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

struct PausePeer {
    transport: Arc<eoka::cdp::Transport>,
    events: tokio::sync::mpsc::Sender<String>,
    seen: tokio::sync::mpsc::Receiver<Value>,
    resolutions: Arc<std::sync::Mutex<HashMap<String, usize>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for PausePeer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl PausePeer {
    async fn start(mode: &'static str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (events, mut events_rx) = tokio::sync::mpsc::channel::<String>(4);
        let (seen_tx, seen) = tokio::sync::mpsc::channel(16);
        let resolutions = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let consumed = resolutions.clone();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            let mut delayed = None;
            loop {
                tokio::select! {
                    Some(request) = events_rx.recv() => {
                        ws.send(Message::Text(json!({"method":"Fetch.requestPaused","sessionId":"fixture-session","params":{"requestId":request,"request":{"url":"http://fixture.test/accepted","method":"GET","headers":{"X-Client-Data":"strip","Other":"keep"}}}}).to_string().into())).await.unwrap();
                    }
                    message = ws.next() => {
                        let Some(Ok(Message::Text(text))) = message else { break; };
                        let command: Value = serde_json::from_str(&text).unwrap();
                        let duplicate = if let Some(id) = command["params"]["requestId"].as_str() {
                            let mut consumed = consumed.lock().unwrap();
                            let count = consumed.entry(id.to_string()).or_insert(0);
                            *count += 1;
                            *count > 1
                        } else { false };
                        seen_tx.send(command.clone()).await.unwrap();
                        if duplicate {
                            ws.send(Message::Text(json!({"id":command["id"],"error":{"code":-32000,"message":"pause already consumed"}}).to_string().into())).await.unwrap();
                            continue;
                        }
                        if command["params"]["requestId"] == "first" {
                            if mode == "drop" { continue; }
                            if mode == "late" { delayed = Some(command["id"].clone()); continue; }
                            if mode == "error" {
                                ws.send(Message::Text(json!({"id":command["id"], "error":{"code":-32000,"message":"resolution status unavailable"}}).to_string().into())).await.unwrap();
                                continue;
                            }
                        }
                        if command["method"] == "Fixture.stillConnected" {
                            if let Some(id) = delayed.take() {
                                ws.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())).await.unwrap();
                            }
                        }
                        ws.send(Message::Text(json!({"id":command["id"],"result":{}}).to_string().into())).await.unwrap();
                    }
                }
            }
        });
        let transport = Arc::new(
            eoka::cdp::Transport::connect_with_options(&url, 30, true, true)
                .await
                .unwrap(),
        );
        Self {
            transport,
            events,
            seen,
            resolutions,
            task,
        }
    }

    async fn queue(&self, failures: &Arc<std::sync::atomic::AtomicU64>) -> FetchQueue {
        let (sender, receiver) = tokio::sync::mpsc::channel(8);
        self.transport
            .install_request_route("fixture-session", sender, failures.clone())
            .unwrap();
        self.events.send("first".into()).await.unwrap();
        self.events.send("second".into()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while receiver.len() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        FetchQueue {
            receiver,
            transport: self.transport.clone(),
            failures: failures.clone(),
        }
    }

    async fn next(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(2), self.seen.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn assert_progress_without_duplicate_resolution(&mut self) {
        self.transport.remove_request_route("fixture-session");
        self.events.send("future".into()).await.unwrap();
        self.transport
            .send::<_, Value>("Fixture.stillConnected", &json!({}))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !self.resolutions.lock().unwrap().contains_key("future") {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let counts = self.resolutions.lock().unwrap();
        for id in ["first", "second", "future"] {
            assert_eq!(counts.get(id), Some(&1), "{id}: {counts:?}");
        }
    }
}

#[tokio::test]
async fn ambiguous_resolution_is_not_replayed_and_queued_pauses_progress() {
    for respond in [true, false] {
        for (mode, cancel) in [
            ("drop", false),
            ("late", false),
            ("error", false),
            ("drop", true),
        ] {
            let files = tempfile::tempdir().unwrap();
            let path = files.path().join("response");
            std::fs::write(&path, b"fixture").unwrap();
            let mut peer = PausePeer::start(mode).await;
            let failures = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let queue = peer.queue(&failures).await;
            let config = FetchDrainConfig {
                transport: peer.transport.clone(),
                failures: failures.clone(),
                rules: vec![InterceptRule {
                    id: 1,
                    url_pattern: "*/accepted".into(),
                    capture_path: None,
                    respond_path: respond.then_some(path),
                    respond_status: 200,
                }],
            };
            let (stop, stop_rx) = tokio::sync::oneshot::channel();
            let worker = tokio::spawn(fetch_drain_until_stopped(queue, config, stop_rx));
            let first = peer.next().await;
            assert_eq!(first["params"]["requestId"], "first");
            assert_eq!(
                first["method"],
                if respond {
                    "Fetch.fulfillRequest"
                } else {
                    "Fetch.continueRequest"
                }
            );
            if cancel {
                worker.abort();
            } else {
                stop.send(()).unwrap();
            }
            let result = tokio::time::timeout(Duration::from_secs(4), worker)
                .await
                .unwrap();
            if cancel {
                assert!(result.err().unwrap().is_cancelled());
            } else {
                assert!(result.ok().unwrap().0.receiver.is_empty());
            }
            assert_ne!(failures.load(Ordering::Relaxed), 0);
            peer.assert_progress_without_duplicate_resolution().await;
        }
    }
}

#[tokio::test]
async fn cancellation_before_resolution_dispatch_hands_off_owned_and_queued_pauses() {
    let mut peer = PausePeer::start("normal").await;
    let failures = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let queue = peer.queue(&failures).await;
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("response");
    std::fs::write(&path, b"fixture").unwrap();
    let _permit = intercept_files::FILE_SLOT
        .clone()
        .acquire_owned()
        .await
        .unwrap();
    while tokio::time::timeout(
        Duration::from_millis(1),
        intercept_files::FILE_READ_STARTED.notified(),
    )
    .await
    .is_ok()
    {}
    let config = FetchDrainConfig {
        transport: peer.transport.clone(),
        failures: failures.clone(),
        rules: vec![InterceptRule {
            id: 1,
            url_pattern: "*/accepted".into(),
            capture_path: None,
            respond_path: Some(path),
            respond_status: 200,
        }],
    };
    let (_stop, stop_rx) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(fetch_drain_until_stopped(queue, config, stop_rx));
    tokio::time::timeout(
        Duration::from_secs(2),
        intercept_files::FILE_READ_STARTED.notified(),
    )
    .await
    .unwrap();
    assert!(peer.resolutions.lock().unwrap().is_empty());
    worker.abort();
    assert!(worker.await.err().unwrap().is_cancelled());
    for _ in 0..2 {
        let command = peer.next().await;
        assert_eq!(command["method"], "Fetch.continueRequest");
        assert_eq!(
            command["params"]["headers"],
            json!([{"name":"Other","value":"keep"}])
        );
    }
    assert_ne!(failures.load(Ordering::Relaxed), 0);
    peer.assert_progress_without_duplicate_resolution().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome daemon idle-file deadline regression"]
async fn delayed_idle_file_operation_keeps_commands_and_shutdown_serviceable() {
    tokio::task::LocalSet::new()
        .run_until(async {
            async fn command(name: &str, cmd: &str, args: Value) -> eoka_sdk::Response {
                let mut socket =
                    tokio::net::UnixStream::connect(eoka_sdk::session::socket_path(name))
                        .await
                        .unwrap();
                let request = eoka_sdk::request_from_cmd(cmd, args).unwrap();
                eoka_sdk::write_msg(&mut socket, &request).await.unwrap();
                tokio::time::timeout(Duration::from_secs(3), eoka_sdk::read_msg(&mut socket))
                    .await
                    .unwrap()
                    .unwrap()
            }
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
            };
            let daemon_name = name.clone();
            let daemon =
                tokio::task::spawn_local(
                    async move { crate::daemon::run(&daemon_name, spec).await },
                );
            for _ in 0..100 {
                if eoka_sdk::session::socket_path(&name).exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let fixture = eoka_datadome::fixture::Fixture::scenario("absent").await;
            let local_url = fixture.url.replace("app.test", "127.0.0.1");
            assert!(command(&name, "open", json!({"url":local_url})).await.ok);
            let files = tempfile::tempdir().unwrap();
            let path = files.path().join("response");
            std::fs::write(&path, b"fixture").unwrap();
            assert!(
                command(
                    &name,
                    "intercept_add",
                    json!({"url_pattern":"*/accepted", "respond":path})
                )
                .await
                .ok
            );
            let _delay = intercept_files::FILE_SLOT
                .clone()
                .acquire_owned()
                .await
                .unwrap();
            for next in ["info", "shutdown"] {
                assert!(
                    command(
                        &name,
                        "exec",
                        json!({"code":"setTimeout(()=>fetch('/accepted'),100); void 0"})
                    )
                    .await
                    .ok
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
                let started = tokio::time::Instant::now();
                assert!(command(&name, next, json!({})).await.ok);
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

#[tokio::test]
#[ignore = "local Chrome idle-entry integrity regression"]
async fn idle_file_failure_during_datadome_entry_cannot_become_a_clean_baseline() {
    let port_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = port_listener.local_addr().unwrap().port();
    drop(port_listener);
    let browser = eoka::Browser::launch_with(|config| {
        config.extra_args.extend([
            format!("--remote-debugging-port={port}"),
            "--host-resolver-rules=MAP * 127.0.0.1".into(),
            "--no-proxy-server".into(),
        ])
    })
    .await
    .unwrap();
    let fixture = eoka_datadome::fixture::Fixture::scenario("simple").await;
    let page = browser.new_page(&fixture.url).await.unwrap();
    let ws_url = eoka::cdp::discover_browser_ws("127.0.0.1", port).unwrap();
    let mut handler = Handler::new("entry-integrity", LaunchSpec::Connect { ws_url });
    assert!(
        handler
            .handle("tab_attach", &json!({"tab_id":page.target_id()}))
            .await
            .ok
    );
    let files = tempfile::tempdir().unwrap();
    assert!(
        handler
            .handle(
                "intercept_add",
                &json!({"url_pattern":"*/idle", "respond":files.path().join("missing")})
            )
            .await
            .ok
    );
    let permit = intercept_files::FILE_SLOT
        .clone()
        .acquire_owned()
        .await
        .unwrap();
    while tokio::time::timeout(
        Duration::from_millis(1),
        intercept_files::FILE_READ_STARTED.notified(),
    )
    .await
    .is_ok()
    {}
    handler.service_idle_fetch();
    page.execute_sync("fetch('/idle').then(()=>document.body.dataset.idle='done'); void 0")
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        intercept_files::FILE_READ_STARTED.notified(),
    )
    .await
    .unwrap();
    assert_eq!(handler.fetch_dropped.load(Ordering::Relaxed), 0);
    let args = json!({"timeout_ms":20000});
    let command = handler.handle("captcha_datadome", &args);
    let release = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(permit);
    };
    let (result, ()) = tokio::join!(command, release);
    assert!(page
        .evaluate_sync::<bool>(
            "document.body.dataset.restored==='true' && document.body.dataset.idle==='done'"
        )
        .await
        .unwrap());
    assert_eq!(
        result.error_detail.as_ref().map(|e| e.code.as_str()),
        Some("datadome_cleanup_failed"),
        "{result:?}"
    );
    assert!(handler.handle("close", &json!({})).await.ok);
    browser.close().await.unwrap();
}
