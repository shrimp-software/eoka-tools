use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

use eoka_protocol::sidecar::{method_names, MAX_IN_FLIGHT, MAX_MESSAGE_BYTES, PROTOCOL_VERSION};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Sidecar {
    child: Child,
    input: Option<ChildStdin>,
    replies: Receiver<Value>,
}

impl Sidecar {
    fn start(state: &Path, scratch: &Path) -> Self {
        Self::start_with_temp(state, scratch, scratch)
    }

    fn start_with_temp(state: &Path, scratch: &Path, temp: &Path) -> Self {
        std::fs::create_dir_all(scratch).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_eoka-server"))
            .env("EOKA_STATE_DIR", state)
            .env("TMPDIR", temp)
            .env("TMP", temp)
            .env("TEMP", temp)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (send, replies) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let line = line.unwrap();
                assert!(line.len() <= MAX_MESSAGE_BYTES);
                let value =
                    serde_json::from_str(&line).expect("stdout must contain only JSON responses");
                if send.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input: Some(input),
            replies,
        }
    }

    fn send(&mut self, value: Value) {
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(&mut *input, &value).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"id":id,"method":method,"params":params}));
        let response = self.reply();
        assert_eq!(response["id"], id);
        response
    }

    fn reply(&self) -> Value {
        let response = self
            .replies
            .recv_timeout(Duration::from_secs(45))
            .expect("sidecar response deadline");
        assert_ne!(
            response.get("result").is_some(),
            response.get("error").is_some()
        );
        response
    }

    fn exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "sidecar exited with {status}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "sidecar did not exit by shutdown deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn shutdown(&mut self, id: u64) {
        assert_eq!(
            self.request(id, "server.shutdown", json!({}))["result"],
            json!({})
        );
        self.exit();
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        if let Some(mut input) = self.input.take() {
            let _ = input.write_all(
                b"{\"id\":9007199254740991,\"method\":\"server.shutdown\",\"params\":{}}\n",
            );
        }
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn directories() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::Builder::new().prefix("ep1-").tempdir().unwrap();
    let state = root.path().join("state");
    let scratch = root.path().join("tmp");
    (root, state, scratch)
}

#[test]
fn recording_admission_recovery_is_ordered_before_independent_controls() {
    let (_root, state, scratch) = directories();
    let mut sidecar = Sidecar::start(&state, &scratch);
    let recording_id = "22222222-2222-4222-8222-222222222222";
    let params =
        json!({"recordingId":recording_id,"pageId":"missing","path":scratch.join("video.mp4")});
    sidecar.send(json!({"id":1,"method":"recording.start","params":params}));
    sidecar.send(json!({"id":2,"method":"recording.get","params":{"recordingId":recording_id}}));
    let mut replies = [sidecar.reply(), sidecar.reply()];
    replies.sort_by_key(|reply| reply["id"].as_u64().unwrap());
    assert_eq!(replies[0]["error"]["code"], "InvalidPage");
    assert_eq!(replies[0]["error"]["data"]["recordingId"], recording_id);
    assert_eq!(replies[0]["error"]["data"]["recording"]["state"], "failed");
    assert_eq!(replies[1]["result"]["state"], "failed");
    assert_eq!(replies[1]["result"]["failure"]["stage"], "start");
    assert!(replies[1]["result"].get("artifact").is_none());
    assert_eq!(
        sidecar.request(3, "recording.start", params.clone())["result"]["state"],
        "failed"
    );
    let mut conflicting = params;
    conflicting["fps"] = json!(30);
    assert_eq!(
        sidecar.request(4, "recording.start", conflicting)["error"]["code"],
        "RecordingConflict"
    );
    assert_eq!(
        sidecar.request(5, "recording.stop", json!({"recordingId":recording_id}))["result"]
            ["state"],
        "failed"
    );
    assert_eq!(
        sidecar.request(6, "recording.cancel", json!({"recordingId":recording_id}))["result"]
            ["state"],
        "failed"
    );
    sidecar.shutdown(7);
}

#[test]
fn hello_negotiates_real_versions_without_launch_and_rejects_mismatch() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let response = server.request(1, "server.hello", json!({"protocol":PROTOCOL_VERSION}));
    assert_eq!(response["result"]["protocol"], json!(PROTOCOL_VERSION));
    assert_eq!(
        response["result"]["serverVersion"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(response["result"]["coreVersion"], env!("EOKA_CORE_VERSION"));
    assert_eq!(response["result"]["methods"], json!(method_names()));
    assert_eq!(
        response["result"]["limits"],
        json!({"maxMessageBytes":MAX_MESSAGE_BYTES,"maxInFlight":MAX_IN_FLIGHT})
    );
    assert!(!state.exists());
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    for (id, protocol) in [
        (2, json!({"major":PROTOCOL_VERSION.major + 1,"minor":0})),
        (
            3,
            json!({"major":PROTOCOL_VERSION.major,"minor":PROTOCOL_VERSION.minor + 1}),
        ),
    ] {
        assert_eq!(
            server.request(id, "server.hello", json!({"protocol":protocol}))["error"]["code"],
            "ProtocolMismatch"
        );
    }
    assert!(!state.exists());
    assert!(server
        .request(4, "server.hello", json!({"protocol":{"major":1,"minor":0}}))
        .get("result")
        .is_some());
    server.shutdown(5);
}

#[test]
fn framing_handles_fragmented_utf8_multiple_lines_and_numeric_ids() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let input = "{\"id\":1,\"method\":\"unknown.é\",\"params\":{}}\n{\"id\":2,\"method\":\"server.hello\",\"params\":{\"protocol\":{\"major\":1,\"minor\":0}}}\n";
    for byte in input.as_bytes() {
        server.input.as_mut().unwrap().write_all(&[*byte]).unwrap();
        server.input.as_mut().unwrap().flush().unwrap();
    }
    let mut replies = [server.reply(), server.reply()];
    replies.sort_by_key(|value| value["id"].as_u64());
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["error"]["code"], "UnknownMethod");
    assert_eq!(replies[1]["id"], 2);
    assert!(replies[1].get("result").is_some());
    for id in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("4"),
        json!(9_007_199_254_740_992_u64),
    ] {
        server.send(
            json!({"id":id,"method":"server.hello","params":{"protocol":{"major":1,"minor":0}}}),
        );
        let response = server.reply();
        assert_eq!(response["id"], Value::Null);
        assert_eq!(response["error"]["code"], "InvalidParams");
    }
    server.send(json!({"id":7,"params":{}}));
    assert_eq!(server.reply()["id"], 7);
    server
        .input
        .as_mut()
        .unwrap()
        .write_all(b"not json\n")
        .unwrap();
    assert_eq!(server.reply()["id"], Value::Null);
    server.shutdown(8);
}

#[test]
fn oversized_input_is_bounded_and_framing_recovers() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let chunk = [b'x'; 64 * 1024];
    for _ in 0..MAX_MESSAGE_BYTES / chunk.len() {
        server.input.as_mut().unwrap().write_all(&chunk).unwrap();
    }
    server.input.as_mut().unwrap().write_all(b"x\n").unwrap();
    let response = server.reply();
    assert_eq!(response["error"]["code"], "MessageTooLarge");
    assert_eq!(response["id"], Value::Null);
    assert!(server
        .request(2, "server.hello", json!({"protocol":{"major":1,"minor":0}}))
        .get("result")
        .is_some());
    server.shutdown(3);
}

#[test]
fn failed_launch_and_eof_cleanly_terminate_without_browser() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("not-a-directory");
    std::fs::write(&profile, b"profile").unwrap();
    let mut server = Sidecar::start(&state, &scratch);
    assert!(server
        .request(1, "browser.launch", json!({"userDataDir":profile}))
        .get("error")
        .is_some());
    server.shutdown(2);
    let mut server = Sidecar::start(&state, &scratch);
    server.input.take();
    server.exit();
}

#[test]
fn managed_validation_and_invalid_executables_never_launch_a_fallback() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let runtime = root.path().join("runtime");
    let mut server = Sidecar::start(&state, &scratch);
    let mut id = 1;
    for (method, params) in [
        (
            "browser.launch",
            json!({"executablePath":"relative/chrome"}),
        ),
        ("browser.launch", json!({"runtimeDir":"relative/runtime"})),
        ("browser.launch", json!({"viewportWidth":640})),
        (
            "browser.launch",
            json!({"viewportWidth":2147483648_u64,"viewportHeight":360}),
        ),
        ("browser.launch", json!({"deviceScaleFactor":0})),
        ("browser.launch", json!({"sandbox":"false"})),
        (
            "browser.connect",
            json!({"wsEndpoint":"http://127.0.0.1/json/version"}),
        ),
        (
            "browser.connect",
            json!({"wsEndpoint":"wss://127.0.0.1/devtools/browser/id"}),
        ),
        (
            "browser.connect",
            json!({"wsEndpoint":"ws://127.0.0.1/devtools/page/id"}),
        ),
        (
            "browser.connect",
            json!({"wsEndpoint":"ws://127.0.0.1/devtools/browser/id","sandbox":true}),
        ),
        (
            "page.set_viewport",
            json!({"pageId":"unknown","width":0,"height":360}),
        ),
        (
            "page.set_viewport",
            json!({"pageId":"unknown","width":640,"height":360,"deviceScaleFactor":-1}),
        ),
    ] {
        let response = server.request(id, method, params);
        assert_eq!(response["error"]["code"], "InvalidParams", "{response}");
        id += 1;
    }
    assert!(!state.exists());
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    let non_executable = root.path().join("not-executable");
    std::fs::write(&non_executable, b"not a browser").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&non_executable, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for executable in [
        root.path().join("missing-chrome"),
        root.path().to_owned(),
        non_executable,
    ] {
        let response = server.request(
            id,
            "browser.launch",
            json!({
                "executablePath":executable,"userDataDir":profile,"runtimeDir":runtime,
            }),
        );
        assert!(response.get("error").is_some(), "{response}");
        assert!(!profile.exists(), "invalid executable launched a fallback");
        assert!(!runtime.exists() || std::fs::read_dir(&runtime).unwrap().count() == 0);
        id += 1;
    }
    server.shutdown(id);
}

#[test]
fn closed_stdout_triggers_cleanup_even_with_stdin_still_open() {
    let (_root, state, scratch) = directories();
    std::fs::create_dir_all(&scratch).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_eoka-server"))
        .env("EOKA_STATE_DIR", state)
        .env("TMPDIR", scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let input = child.stdin.take();
    drop(child.stdout.take());
    let (send, replies) = mpsc::channel();
    drop(send);
    let mut server = Sidecar {
        child,
        input,
        replies,
    };
    server
        .send(json!({"id":1,"method":"server.hello","params":{"protocol":{"major":1,"minor":0}}}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.child.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "writer failure left blocked stdin intake alive"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    server.exit();
}

#[test]
fn operation_errors_retain_identity_and_canonical_detail() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let id = uuid::Uuid::new_v4().to_string();
    let response = server.request(1, "operations.get", json!({"id":id}));
    assert_eq!(response["error"]["code"], "Operation");
    assert_eq!(response["error"]["data"]["operation_id"], id);
    assert_eq!(
        response["error"]["data"]["detail"]["code"],
        "operation_error"
    );
    let response = server.request(
        2,
        "operations.wait",
        json!({"id":id,"timeout_ms":86_400_001}),
    );
    assert_eq!(response["error"]["code"], "InvalidParams");
    assert_eq!(response["error"]["data"]["operation_id"], id);
    server.shutdown(3);
}

#[test]
fn store_lock_does_not_block_independent_hello() {
    use fs2::FileExt;

    let (_root, state, scratch) = directories();
    std::fs::create_dir_all(&state).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(state.join("store.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let mut server = Sidecar::start(&state, &scratch);
    let operation = uuid::Uuid::new_v4().to_string();
    server.send(json!({"id":1,"method":"operations.get","params":{"id":operation}}));
    server.send(json!({"id":2,"method":"server.hello","params":{"protocol":PROTOCOL_VERSION}}));
    let hello = server.replies.recv_timeout(Duration::from_secs(2));
    drop(lock);
    let hello = hello.expect("hello must respond while operation storage is locked");
    assert_eq!(hello["id"], 2);
    assert_eq!(hello["result"]["protocol"], json!(PROTOCOL_VERSION));
    let response = server.reply();
    assert_eq!(response["id"], 1);
    assert_eq!(response["error"]["code"], "Operation");
    assert_eq!(response["error"]["data"]["operation_id"], operation);
    assert_eq!(
        response["error"]["data"]["detail"]["code"],
        "operation_error"
    );
    server.shutdown(3);
}

fn launch(server: &mut Sidecar, profile: &Path) -> String {
    let mut params = json!({"headless":true,"userDataDir":profile});
    if let Ok(executable) = std::env::var("EOKA_TEST_CHROME_PATH") {
        params["executablePath"] = json!(executable);
    }
    let response = server.request(1, "browser.launch", params);
    assert_eq!(response["result"]["ownership"], "owned", "{response}");
    assert!(!response["result"]["browserVersion"]
        .as_str()
        .unwrap()
        .is_empty());
    let response = server.request(2, "browser.new_page", json!({}));
    response["result"]["pageId"].as_str().unwrap().to_owned()
}

#[cfg(target_os = "linux")]
fn browser_pid(profile: &Path) -> i32 {
    let link = std::fs::read_link(profile.join("SingletonLock")).unwrap();
    link.to_string_lossy()
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[cfg(target_os = "linux")]
fn assert_process_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "owned Chrome {pid} survived sidecar cleanup"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires Chrome"]
fn waits_are_independent_pending_ceiling_is_enforced_and_shutdown_cancels_all() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &profile);
    #[cfg(target_os = "linux")]
    let pid = browser_pid(&profile);
    let operation = uuid::Uuid::new_v4().to_string();
    let action = eoka_server::eoka::Action::Navigate {
        url: "about:blank".into(),
        referrer: None,
    };
    let record = server.request(
        3,
        "actions.perform",
        json!({"pageId":page,"operation_id":operation,"action":action,"options":{}}),
    );
    assert_eq!(record["result"]["id"], operation);
    let record = server.request(
        4,
        "operations.wait",
        json!({"id":operation,"timeout_ms":30_000}),
    );
    assert_ne!(record["result"]["state"], "running");
    server.send(json!({"id":5,"method":"page.wait_for_text","params":{"pageId":page,"text":"never present","timeoutMs":600_000}}));
    std::thread::sleep(Duration::from_millis(50));
    server.send(
        json!({"id":6,"method":"operations.wait","params":{"id":operation,"timeout_ms":30_000}}),
    );
    let independent = server
        .replies
        .recv_timeout(Duration::from_secs(2))
        .expect("wait must bypass blocked ordinary dispatch");
    assert_eq!(independent["id"], 6);
    for (id, method, params) in [
        (136, "operations.get", json!({"id":operation})),
        (137, "operations.list", json!({})),
        (138, "operations.cancel", json!({"id":operation})),
    ] {
        server.send(json!({"id":id,"method":method,"params":params}));
        let independent = server
            .replies
            .recv_timeout(Duration::from_secs(2))
            .expect("operation controls must bypass blocked ordinary dispatch");
        assert_eq!(independent["id"], id);
        if method == "operations.list" {
            assert!(independent["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|record| record["id"] == operation));
        } else {
            assert_eq!(independent["result"]["id"], operation);
            assert_ne!(independent["result"]["state"], "running");
        }
    }
    for id in 7..=133 {
        server.send(json!({"id":id,"method":"page.title","params":{"pageId":page}}));
    }
    server.send(json!({"id":134,"method":"page.title","params":{"pageId":page}}));
    let busy = server.reply();
    assert_eq!(busy["id"], 134);
    assert_eq!(busy["error"]["code"], "ServerBusy");
    server.send(json!({"id":135,"method":"server.shutdown","params":{}}));
    let mut terminated = std::collections::BTreeSet::new();
    for _ in 0..129 {
        let response = server.reply();
        let id = response["id"].as_u64().unwrap();
        assert!(terminated.insert(id));
        if id == 135 {
            assert_eq!(response["result"], json!({}));
        } else {
            assert!(
                matches!(
                    response["error"]["code"].as_str(),
                    Some("Cancelled" | "ShuttingDown")
                ),
                "{response}"
            );
        }
    }
    assert!(terminated.contains(&5));
    assert!(terminated.contains(&135));
    server.exit();
    #[cfg(target_os = "linux")]
    assert_process_gone(pid);
}

#[test]
#[ignore = "requires Chrome"]
fn fresh_operation_controls_preserve_queued_admission_order() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &root.path().join("profile"));
    let operation = uuid::Uuid::new_v4().to_string();
    let action = eoka_server::eoka::Action::Navigate {
        url: "about:blank".into(),
        referrer: None,
    };
    server.send(json!({
        "id":3,"method":"page.wait_for_text",
        "params":{"pageId":page,"text":"never present","timeoutMs":2_000}
    }));
    server.send(json!({
        "id":4,"method":"actions.perform",
        "params":{"pageId":page,"operation_id":operation,"action":action,"options":{}}
    }));
    server.send(json!({"id":5,"method":"operations.get","params":{"id":operation}}));
    server.send(json!({"id":6,"method":"operations.cancel","params":{"id":operation}}));
    let mut replies = [
        server.reply(),
        server.reply(),
        server.reply(),
        server.reply(),
    ];
    replies.sort_by_key(|reply| reply["id"].as_u64().unwrap());
    assert_eq!(replies[0]["id"], 3);
    assert!(replies[0].get("error").is_some());
    for (reply, id) in replies[1..].iter().zip(4..=6) {
        assert_eq!(reply["id"], id);
        assert_eq!(reply["result"]["id"], operation, "{reply}");
    }
    server.shutdown(7);
}

#[test]
#[ignore = "requires Chrome"]
fn eof_during_dispatched_action_preserves_durable_unknown_outcome_and_closes_chrome() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &profile);
    #[cfg(target_os = "linux")]
    let pid = browser_pid(&profile);
    let response = server.request(
        3,
        "page.execute",
        json!({"pageId":page,"js":"document.body.innerHTML='<button id=button>click</button>'"}),
    );
    assert!(response.get("result").is_some(), "{response}");
    let operation = uuid::Uuid::new_v4().to_string();
    let target = eoka_server::eoka::LocatorSpec::css("#button");
    let action = eoka_server::eoka::Action::Click {
        target: target.clone(),
        human: false,
        double: false,
    };
    let expect = eoka_server::eoka::Expectation::Text {
        target,
        value: "never appears".into(),
        contains: false,
    };
    let response = server.request(4, "actions.perform", json!({"pageId":page,"operation_id":operation,"action":action,"options":{"expect":expect,"timeout_ms":600_000}}));
    assert_eq!(response["result"]["id"], operation);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut id = 5;
    loop {
        let record = server.request(id, "operations.get", json!({"id":operation}));
        id += 1;
        if record["result"]["dispatch_intent"] == true {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "action never reached durable dispatch intent"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    for request_id in id..id + 3 {
        server.send(json!({"id":request_id,"method":"operations.wait","params":{"id":operation,"timeout_ms":600_000}}));
    }
    server.input.take();
    let mut completed = std::collections::BTreeSet::new();
    for _ in 0..3 {
        let response = server.reply();
        assert!(completed.insert(response["id"].as_u64().unwrap()));
        assert_eq!(response["error"]["code"], "ShuttingDown");
        assert_eq!(response["error"]["data"]["operation_id"], operation);
    }
    assert_eq!(completed, (id..id + 3).collect());
    server.exit();
    #[cfg(target_os = "linux")]
    assert_process_gone(pid);
    let mut reopened = Sidecar::start(&state, &scratch);
    let record = reopened.request(1, "operations.get", json!({"id":operation}));
    assert_eq!(record["result"]["outcome"], "outcome_unknown");
    assert_ne!(record["result"]["state"], "running");
    reopened.shutdown(2);
}

#[test]
#[ignore = "requires Chrome"]
fn oversized_evaluation_returns_structured_size_error_and_remains_usable() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &root.path().join("profile"));
    for (id, expression) in [
        (3, format!("'x'.repeat({})", MAX_MESSAGE_BYTES + 1)),
        (4, "'\"'.repeat(9 * 1024 * 1024)".to_owned()),
        (5, "'🌍'.repeat(5 * 1024 * 1024)".to_owned()),
    ] {
        let response = server.request(id, "page.evaluate", json!({"pageId":page,"js":expression}));
        assert_eq!(response["error"]["code"], "MessageTooLarge", "{response}");
    }
    let response = server.request(
        6,
        "page.evaluate",
        json!({"pageId":page,"js":"'x'.repeat(15 * 1024 * 1024)"}),
    );
    let value = response["result"]["result"].as_str().unwrap();
    assert_eq!(value.len(), 15 * 1024 * 1024);
    assert!(value.bytes().all(|byte| byte == b'x'));
    assert!(server
        .request(7, "page.title", json!({"pageId":page}))
        .get("result")
        .is_some());
    server.shutdown(8);
}

#[test]
#[ignore = "requires Chrome"]
fn shutdown_during_launch_does_not_abandon_the_owned_browser() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut server = Sidecar::start(&state, &scratch);
    server.send(
        json!({"id":1,"method":"browser.launch","params":{"headless":true,"userDataDir":profile}}),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !profile.exists() {
        assert!(
            Instant::now() < deadline,
            "launch preparation never started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    server.send(json!({"id":2,"method":"server.shutdown","params":{}}));
    let mut replies = [server.reply(), server.reply()];
    replies.sort_by_key(|value| value["id"].as_u64());
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[1]["id"], 2);
    assert_eq!(replies[1]["result"], json!({}));
    server.exit();
    #[cfg(target_os = "linux")]
    if let Ok(link) = std::fs::read_link(profile.join("SingletonLock")) {
        let pid = link
            .to_string_lossy()
            .rsplit('-')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_process_gone(pid);
    }
}

#[test]
#[ignore = "requires Chrome"]
fn borrowed_browser_survives_sidecar_shutdown() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut owner = Sidecar::start(&state, &scratch);
    launch(&mut owner, &profile);
    #[cfg(target_os = "linux")]
    let pid = browser_pid(&profile);
    let mut borrower = Sidecar::start(
        &root.path().join("borrower-state"),
        &root.path().join("borrower-tmp"),
    );
    let response = borrower.request(
        1,
        "browser.launch",
        json!({"headless":true,"userDataDir":profile}),
    );
    assert_eq!(response["result"]["ownership"], "borrowed", "{response}");
    assert!(!response["result"]["browserVersion"]
        .as_str()
        .unwrap()
        .is_empty());
    borrower.shutdown(2);
    assert!(owner
        .request(3, "browser.tabs", json!({}))
        .get("result")
        .is_some());
    #[cfg(target_os = "linux")]
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    owner.shutdown(4);
    #[cfg(target_os = "linux")]
    assert_process_gone(pid);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "subprocess fixture for abrupt_parent_death_closes_owned_chrome"]
fn abrupt_parent_fixture() {
    let root = std::path::PathBuf::from(
        std::env::var_os("EOKA_ABRUPT_PARENT_FIXTURE").expect("fixture directory"),
    );
    let profile = root.join("profile");
    let mut server = Sidecar::start(&root.join("state"), &root.join("tmp"));
    launch(&mut server, &profile);
    std::fs::write(
        root.join("ready.json"),
        serde_json::to_vec(&json!({
            "server":server.child.id(),
            "chrome":browser_pid(&profile),
        }))
        .unwrap(),
    )
    .unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

#[cfg(target_os = "linux")]
fn process_running(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat
            .rsplit_once(") ")
            .is_some_and(|(_, state)| state.starts_with('Z')),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("cannot inspect process {pid}: {error}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires Chrome"]
fn abrupt_parent_death_closes_owned_chrome() {
    let (root, _, _) = directories();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "abrupt_parent_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("EOKA_ABRUPT_PARENT_FIXTURE", root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let ready = root.path().join("ready.json");
    while !ready.exists() {
        assert!(
            parent.try_wait().unwrap().is_none(),
            "fixture exited before launch"
        );
        assert!(Instant::now() < deadline, "fixture did not launch Chrome");
        std::thread::sleep(Duration::from_millis(20));
    }
    let info: Value = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
    parent.kill().unwrap();
    parent.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    let server = info["server"].as_u64().unwrap() as u32;
    let chrome = info["chrome"].as_u64().unwrap() as u32;
    while process_running(server) || process_running(chrome) {
        assert!(
            Instant::now() < deadline,
            "sidecar or owned Chrome survived abrupt parent EOF"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn page_metrics(server: &mut Sidecar, id: u64, page: &str) -> Value {
    let response = server.request(
        id,
        "page.evaluate",
        json!({
            "pageId":page,"js":"({width:innerWidth,height:innerHeight,scale:devicePixelRatio})"
        }),
    );
    assert!(response.get("result").is_some(), "{response}");
    response["result"]["result"].clone()
}

fn assert_png_dimensions(server: &mut Sidecar, id: u64, page: &str, expected: (u32, u32)) {
    use base64::Engine;
    let response = server.request(id, "page.screenshot", json!({"pageId":page}));
    let png = base64::engine::general_purpose::STANDARD
        .decode(response["result"]["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(
        (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        ),
        expected,
    );
}

#[test]
#[ignore = "requires Chrome and EOKA_TEST_CHROME_PATH absolute executable"]
fn explicit_executable_runtime_and_viewport_have_real_metadata_and_capture_pixels() {
    let executable = std::env::var("EOKA_TEST_CHROME_PATH").expect("absolute Chrome executable");
    assert!(Path::new(&executable).is_absolute());
    let (root, state, scratch) = directories();
    let runtime = root.path().join("runtime");
    std::fs::create_dir(&runtime).unwrap();
    let unusable_temp = root.path().join("not-a-temp-directory");
    std::fs::write(&unusable_temp, b"not a directory").unwrap();
    let profile = root.path().join("caller-profile");
    std::fs::create_dir(&profile).unwrap();
    std::fs::write(profile.join("caller-sentinel"), b"keep").unwrap();
    let mut server = Sidecar::start_with_temp(&state, &scratch, &unusable_temp);
    let launched = server.request(
        1,
        "browser.launch",
        json!({
            "headless":true,"executablePath":executable,"runtimeDir":runtime,
            "userDataDir":profile,"sandbox":false,"userAgent":"sidecar-test-spoof",
            "viewportWidth":640,"viewportHeight":360,"deviceScaleFactor":2.0,
        }),
    );
    assert_eq!(launched["result"]["ownership"], "owned", "{launched}");
    let version = launched["result"]["browserVersion"].as_str().unwrap();
    assert!(version.contains("Chrome/"), "{launched}");
    assert_ne!(version, "sidecar-test-spoof");
    assert!(std::fs::read_dir(&runtime).unwrap().next().is_some());
    let response = server.request(2, "browser.new_page", json!({}));
    let page = response["result"]["pageId"].as_str().unwrap().to_owned();
    let metrics = page_metrics(&mut server, 3, &page);
    assert_eq!(metrics["width"].as_u64(), Some(640));
    assert_eq!(metrics["height"].as_u64(), Some(360));
    assert_eq!(metrics["scale"].as_f64(), Some(2.0));
    assert_png_dimensions(&mut server, 4, &page, (1280, 720));
    assert_eq!(
        server.request(
            5,
            "page.set_viewport",
            json!({
                "pageId":page,"width":800,"height":450,"deviceScaleFactor":1.5,
            })
        )["result"],
        json!({})
    );
    assert_eq!(
        page_metrics(&mut server, 6, &page),
        json!({"width":800,"height":450,"scale":1.5})
    );
    assert_png_dimensions(&mut server, 7, &page, (1200, 675));
    let before = page_metrics(&mut server, 8, &page);
    assert_eq!(
        server.request(
            9,
            "page.set_viewport",
            json!({
                "pageId":page,"width":800,"height":450,"deviceScaleFactor":0,
            })
        )["error"]["code"],
        "InvalidParams"
    );
    assert_eq!(page_metrics(&mut server, 10, &page), before);
    assert_eq!(
        server.request(11, "page.reset_viewport", json!({"pageId":page}))["result"],
        json!({})
    );
    let native = page_metrics(&mut server, 12, &page);
    let native_width =
        (native["width"].as_f64().unwrap() * native["scale"].as_f64().unwrap()).round() as u32;
    let native_height =
        (native["height"].as_f64().unwrap() * native["scale"].as_f64().unwrap()).round() as u32;
    assert!(native_width > 0 && native_height > 0);
    assert_png_dimensions(&mut server, 13, &page, (native_width, native_height));
    assert_eq!(
        server.request(
            14,
            "page.set_viewport",
            json!({
                "pageId":page,"width":320,"height":200,
            })
        )["result"],
        json!({})
    );
    let resized = page_metrics(&mut server, 15, &page);
    assert_eq!(resized["width"].as_u64(), Some(320));
    assert_eq!(resized["height"].as_u64(), Some(200));
    assert_eq!(resized["scale"].as_f64(), Some(1.0));
    server.request(16, "page.reset_viewport", json!({"pageId":page}));
    assert_eq!(page_metrics(&mut server, 17, &page), native);
    server.shutdown(18);
    assert_eq!(std::fs::read_dir(&runtime).unwrap().count(), 0);
    assert_eq!(
        std::fs::read(profile.join("caller-sentinel")).unwrap(),
        b"keep"
    );
}

#[test]
#[ignore = "requires Chrome; external owner survives borrowed close, EOF and startup shutdown"]
fn connected_external_browser_keeps_tabs_profile_and_releases_held_input() {
    let (root, _, _) = directories();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let profile = root.path().join("external-profile");
    std::fs::create_dir(&profile).unwrap();
    std::fs::write(profile.join("caller-sentinel"), b"keep").unwrap();
    let owner = runtime
        .block_on(eoka_server::eoka::Browser::launch_with(|config| {
            config.patch_binary = false;
            config.live_session = true;
            config.user_data_dir = Some(profile.to_string_lossy().into_owned());
            config
                .extra_args
                .push(format!("--remote-debugging-port={port}"));
        }))
        .unwrap();
    let version = runtime.block_on(owner.version()).unwrap();
    let original = runtime
        .block_on(owner.new_page("data:text/html,<title>external tab</title>"))
        .unwrap();
    let original_id = original.target_id().to_owned();
    runtime
        .block_on(original.set_viewport(640, 360, 2.0))
        .unwrap();
    runtime.block_on(original.execute(
        "globalThis.sidecarHeld=false;document.addEventListener('keydown',()=>globalThis.sidecarHeld=true);document.addEventListener('keyup',()=>globalThis.sidecarHeld=false)"
    )).unwrap();
    let endpoint: Value = runtime.block_on(async {
        reqwest::get(format!("http://127.0.0.1:{port}/json/version"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    });
    let ws = endpoint["webSocketDebuggerUrl"].as_str().unwrap();
    for (index, mode) in ["close", "eof", "startup-shutdown"].into_iter().enumerate() {
        let mut borrower = Sidecar::start(
            &root.path().join(format!("state-{index}")),
            &root.path().join(format!("scratch-{index}")),
        );
        if mode == "startup-shutdown" {
            borrower.send(json!({"id":1,"method":"browser.connect","params":{"wsEndpoint":ws}}));
            borrower.send(json!({"id":2,"method":"server.shutdown","params":{}}));
            let mut replies = [borrower.reply(), borrower.reply()];
            replies.sort_by_key(|reply| reply["id"].as_u64());
            assert_eq!(replies[1]["result"], json!({}), "{replies:?}");
            borrower.exit();
        } else {
            let attached = borrower.request(1, "browser.connect", json!({"wsEndpoint":ws}));
            assert_eq!(
                attached["result"],
                json!({"ownership":"borrowed","browserVersion":version})
            );
            assert_eq!(
                borrower.request(2, "page.title", json!({"pageId":original_id}))["result"]["title"],
                "external tab"
            );
            let before: Value = runtime
                .block_on(
                    original
                        .evaluate("({width:innerWidth,height:innerHeight,scale:devicePixelRatio})"),
                )
                .unwrap();
            assert_eq!(page_metrics(&mut borrower, 3, &original_id), before);
            let later = runtime
                .block_on(owner.new_page("data:text/html,<title>later tab</title>"))
                .unwrap();
            let later_id = later.target_id();
            let tabs = borrower.request(4, "browser.tabs", json!({}));
            assert!(tabs["result"]["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tab| tab["id"] == later_id));
            assert_eq!(
                borrower.request(5, "page.title", json!({"pageId":later_id}))["result"]["title"],
                "later tab"
            );
            runtime.block_on(owner.close_tab(later_id)).unwrap();
            let tabs = borrower.request(6, "browser.tabs", json!({}));
            assert!(!tabs["result"]["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tab| tab["id"] == later_id));
            assert_eq!(
                borrower.request(7, "page.title", json!({"pageId":later_id}))["error"]["code"],
                "InvalidPage"
            );
            assert_eq!(
                borrower.request(
                    8,
                    "page.set_viewport",
                    json!({
                        "pageId":original_id,"width":800,"height":450,"deviceScaleFactor":1.5,
                    })
                )["result"],
                json!({})
            );
            assert_png_dimensions(&mut borrower, 9, &original_id, (1200, 675));
            assert_eq!(
                borrower.request(
                    10,
                    "page.key_down",
                    json!({"pageId":original_id,"key":"Shift"})
                )["result"],
                json!({})
            );
            assert!(runtime
                .block_on(original.evaluate::<bool>("globalThis.sidecarHeld"))
                .unwrap());
            if mode == "close" {
                assert_eq!(
                    borrower.request(11, "browser.close", json!({}))["result"],
                    json!({})
                );
            } else {
                borrower.input.take();
            }
            borrower.exit();
            assert!(!runtime
                .block_on(original.evaluate::<bool>("globalThis.sidecarHeld"))
                .unwrap());
        }
        assert_eq!(runtime.block_on(owner.version()).unwrap(), version);
        assert_eq!(runtime.block_on(original.title()).unwrap(), "external tab");
        assert_eq!(
            std::fs::read(profile.join("caller-sentinel")).unwrap(),
            b"keep"
        );
    }
    runtime.block_on(owner.close()).unwrap();
    assert_eq!(
        std::fs::read(profile.join("caller-sentinel")).unwrap(),
        b"keep"
    );
}
