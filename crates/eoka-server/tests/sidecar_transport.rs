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
        .request(4, "server.hello", json!({"protocol":{"major":2,"minor":0}}))
        .get("result")
        .is_some());
    server.shutdown(5);
}

#[test]
fn framing_handles_fragmented_utf8_multiple_lines_and_numeric_ids() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let input = "{\"id\":1,\"method\":\"unknown.é\",\"params\":{}}\n{\"id\":2,\"method\":\"server.hello\",\"params\":{\"protocol\":{\"major\":2,\"minor\":0}}}\n";
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
            json!({"id":id,"method":"server.hello","params":{"protocol":{"major":2,"minor":0}}}),
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
        .request(2, "server.hello", json!({"protocol":{"major":2,"minor":0}}))
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
    let mut invalid = vec![root.path().join("missing-chrome"), root.path().to_owned()];
    // Windows has no execute permission, so a plain file there is only
    // rejected when it fails to spawn, after the profile exists.
    if cfg!(unix) {
        invalid.push(non_executable);
    }
    for executable in invalid {
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
        .send(json!({"id":1,"method":"server.hello","params":{"protocol":{"major":2,"minor":0}}}));
    let deadline = Instant::now() + Duration::from_secs(30);
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

#[test]
fn a_client_that_refills_each_answered_slot_is_never_refused() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let total = 6_000_u64;
    let hello =
        |id: u64| json!({"id":id,"method":"server.hello","params":{"protocol":PROTOCOL_VERSION}});
    let mut next = 1;
    while next <= MAX_IN_FLIGHT as u64 {
        server.send(hello(next));
        next += 1;
    }
    for _ in 0..total {
        let reply = server.reply();
        assert!(reply.get("result").is_some(), "{reply}");
        if next <= total {
            server.send(hello(next));
            next += 1;
        }
    }
    server.shutdown(total + 1);
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
fn turnstile_solve_and_auto_solve_watcher_stop_with_the_browser() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut server = Sidecar::start(&state, &scratch);
    let mut params = json!({
        "headless":true,"userDataDir":profile,
        "autoSolveCloudflare":{"intervalMs":100,"timeoutMs":60_000}
    });
    if let Ok(executable) = std::env::var("EOKA_TEST_CHROME_PATH") {
        params["executablePath"] = json!(executable);
    }
    let response = server.request(1, "browser.launch", params);
    assert_eq!(response["result"]["ownership"], "owned", "{response}");
    let page = server.request(2, "browser.new_page", json!({}))["result"]["pageId"]
        .as_str()
        .unwrap()
        .to_owned();
    let normal = server.request(
        3,
        "page.solve_turnstile",
        json!({"pageId":page,"timeoutMs":5_000}),
    );
    assert_eq!(normal["result"]["clicks"], 0, "{normal}");
    let challenge = server.request(
        4,
        "browser.new_page",
        json!({"url":"data:text/html,<title>Just a moment...</title><p>checking</p>"}),
    )["result"]["pageId"]
        .as_str()
        .unwrap()
        .to_owned();
    let loaded = server.request(
        41,
        "page.wait_for_text",
        json!({"pageId":challenge,"text":"checking","timeoutMs":5_000}),
    );
    assert!(loaded.get("error").is_none(), "{loaded}");
    let stuck = server.request(
        5,
        "page.solve_turnstile",
        json!({"pageId":challenge,"timeoutMs":1_500}),
    );
    assert!(
        stuck["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("did not resolve")),
        "{stuck}"
    );
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    server.shutdown(6);
    assert!(started.elapsed() < Duration::from_secs(10));
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
        "page.execute_raw",
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
        let response = server.request(
            id,
            "page.evaluate_raw",
            json!({"pageId":page,"js":expression}),
        );
        assert_eq!(response["error"]["code"], "MessageTooLarge", "{response}");
    }
    let response = server.request(
        6,
        "page.evaluate_raw",
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
fn catalog_operations_run_on_the_addressed_page_and_report_structured_errors() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let hello = server.request(90, "server.hello", json!({"protocol":PROTOCOL_VERSION}));
    assert!(hello["result"]["capabilities"]
        .as_array()
        .unwrap()
        .contains(&json!("operationCatalog")));
    let first = launch(&mut server, &root.path().join("profile"));
    let second = server.request(3, "browser.new_page", json!({}))["result"]["pageId"]
        .as_str()
        .unwrap()
        .to_owned();

    // Each call targets the page named by pageId, in either order.
    for (id, page, title) in [(4, &first, "first"), (5, &second, "second")] {
        let response = server.request(
            id,
            "exec",
            json!({"pageId":page,"raw":true,"code":format!("document.title = '{title}'; void 0")}),
        );
        assert!(response.get("result").is_some(), "{response}");
    }
    for (id, page, title) in [(6, &second, "second"), (7, &first, "first")] {
        let response = server.request(id, "info", json!({"pageId":page}));
        assert!(response["result"].to_string().contains(title), "{response}");
        // The sidecar's own method sees the same page.
        let legacy = server.request(id + 10, "page.title", json!({"pageId":page}));
        assert_eq!(legacy["result"]["title"], title, "{legacy}");
    }

    // An operation with no sidecar equivalent: console capture.
    assert!(server
        .request(20, "console", json!({"pageId":first}))
        .get("result")
        .is_some());
    server.request(
        21,
        "exec",
        json!({"pageId":first,"raw":true,"code":"console.log('catalog-marker'); void 0"}),
    );
    let console = server.request(22, "console", json!({"pageId":first}));
    assert!(
        console["result"].to_string().contains("catalog-marker"),
        "{console}"
    );
    let other = server.request(23, "console", json!({"pageId":second}));
    assert!(
        !other["result"].to_string().contains("catalog-marker"),
        "{other}"
    );

    // Reads, waits and shadow clicks folded in from the sidecar-only methods.
    let markup = "document.body.innerHTML = '<h1>Heading</h1><a id=link href=\"/next\">Next</a>'; \
        setTimeout(() => { const late = document.createElement('p'); late.id = 'late'; late.textContent = 'Late'; document.body.append(late); }, 200); void 0";
    server.request(50, "exec", json!({"pageId":first,"raw":true,"code":markup}));
    let content = server.request(51, "content", json!({"pageId":first}));
    assert!(
        content["result"]
            .as_str()
            .unwrap()
            .contains("<h1>Heading</h1>"),
        "{content}"
    );
    let text = server.request(52, "get_text", json!({"pageId":first,"selector":"h1"}));
    assert_eq!(text["result"], "Heading", "{text}");
    for (id, name, value) in [(53, "href", json!("/next")), (54, "missing", Value::Null)] {
        let attribute = server.request(
            id,
            "get_attribute",
            json!({"pageId":first,"selector":"#link","name":name}),
        );
        assert_eq!(attribute["result"]["value"], value, "{attribute}");
    }
    for (id, selector, exists) in [(55, "#link", true), (56, "#absent", false)] {
        let response = server.request(id, "exists", json!({"pageId":first,"selector":selector}));
        assert_eq!(response["result"]["exists"], exists, "{response}");
    }
    let waited = server.request(
        57,
        "wait",
        json!({"pageId":first,"selector":"#late","timeout":5000}),
    );
    assert!(waited.get("result").is_some(), "{waited}");
    let hidden = server.request(
        58,
        "wait",
        json!({"pageId":first,"selector":"#absent","state":"hidden","timeout":2000}),
    );
    assert!(hidden.get("result").is_some(), "{hidden}");
    let missing = server.request(
        59,
        "wait",
        json!({"pageId":first,"selector":"#absent","state":"attached","timeout":300}),
    );
    // Core errors keep their kind through the catalog path.
    assert_eq!(missing["error"]["code"], "Timeout", "{missing}");
    let absent = server.request(49, "get_text", json!({"pageId":first,"selector":"#absent"}));
    assert_eq!(absent["error"]["code"], "ElementNotFound", "{absent}");
    // The shadow search needs a real document rather than about:blank.
    let shadow_url = "data:text/html,<!doctype html><title>start</title><div id=host></div><script>\
        const root = document.getElementById('host').attachShadow({mode:'closed'});\
        root.innerHTML = '<button id=inner style=\"width:120px;height:40px\">go</button>';\
        root.getElementById('inner').onclick = () => { document.title = 'shadow-clicked'; };</script>";
    let shadow = server.request(60, "browser.new_page", json!({"url":shadow_url}));
    let shadow = shadow["result"]["pageId"].as_str().unwrap().to_owned();
    let clicked = server.request(
        61,
        "click_shadow",
        json!({"pageId":shadow,"selector":"button#inner"}),
    );
    assert_eq!(clicked["result"]["selector"], "button#inner", "{clicked}");
    let mut title = Value::Null;
    for attempt in 0..20 {
        title = server.request(62 + attempt, "page.title", json!({"pageId":shadow}));
        if title["result"]["title"] == "shadow-clicked" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(title["result"]["title"], "shadow-clicked", "{title}");

    // Actions need no caller-supplied operation ID and return their record,
    // including when the outcome is not verified.
    let reloaded = server.request(100, "reload", json!({"pageId":first}));
    assert_eq!(
        reloaded["result"]["outcome"], "verified_complete",
        "{reloaded}"
    );
    let generated = reloaded["result"]["id"].as_str().unwrap().to_owned();
    let fetched = server.request(101, "operations.get", json!({"id":generated}));
    assert_eq!(
        fetched["result"]["id"], reloaded["result"]["id"],
        "{fetched}"
    );
    let unverified = server.request(
        102,
        "click",
        json!({"pageId":first,"target":"css:body","timeout_ms":500}),
    );
    assert!(
        unverified["result"]["id"].is_string(),
        "an unverified action must still return its record: {unverified}"
    );

    // A reference from the sidecar's own observation resolves in a catalog action.
    server.request(
        103,
        "exec",
        json!({"pageId":shadow,"raw":true,"code":"document.body.innerHTML = '<button id=go>Go</button>'; void 0"}),
    );
    let observed = server.request(104, "agent.observe", json!({"pageId":shadow}));
    let reference = observed["result"]["controls"][0]["target"]
        .as_str()
        .unwrap_or_else(|| panic!("no control in {observed}"))
        .to_owned();
    // The shadow page is the foreground tab; background tabs do not run the
    // animation frames that actionability checks wait for.
    let scrolled = server.request(105, "scroll", json!({"pageId":shadow,"target":reference}));
    assert!(scrolled.get("result").is_some(), "{scrolled}");
    let stale = server.request(106, "scroll", json!({"pageId":shadow,"target":"@e999"}));
    assert!(stale.get("error").is_some(), "{stale}");

    // An interception rule must not stall requests made outside the engine:
    // nothing polls an adopted engine, so it has to keep draining by itself.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(32) {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 2048];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\naccess-control-allow-origin: *\r\ncontent-type: text/html\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
            );
        }
    });
    // Same origin as the fixture, so the credentialed fetch below is allowed.
    let opened = server.request(
        108,
        "open",
        json!({"pageId":first,"url":format!("http://{address}/page")}),
    );
    assert!(opened.get("result").is_some(), "{opened}");
    let baseline = server.request(
        109,
        "page.fetch",
        json!({"pageId":first,"url":format!("http://{address}/data")}),
    );
    assert_eq!(baseline["result"]["status"], 200, "{baseline}");
    let rule = server.request(
        110,
        "intercept.add",
        json!({"pageId":first,"url_pattern":"*never-matches*","capture":scratch.join("capture")}),
    );
    assert!(rule.get("result").is_some(), "{rule}");
    let started = Instant::now();
    let direct = server.request(
        111,
        "page.fetch",
        json!({"pageId":first,"url":format!("http://{address}/data")}),
    );
    assert_eq!(direct["result"]["status"], 200, "{direct}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "request stalled behind interception for {:?}",
        started.elapsed()
    );
    let rules = server.request(112, "intercept.list", json!({"pageId":first}));
    assert!(
        rules["result"].to_string().contains("never-matches"),
        "{rules}"
    );

    // Unknown and camelCase fields are named instead of ignored.
    let misspelt = server.request(
        113,
        "click_shadow",
        json!({"pageId":first,"selector":"b","timeoutMs":5}),
    );
    assert_eq!(misspelt["error"]["code"], "InvalidParams", "{misspelt}");

    for (id, method, params, code) in [
        (30, "info", json!({}), "InvalidParams"),
        (31, "info", json!({"pageId":"missing"}), "InvalidPage"),
        (
            32,
            "console",
            json!({"pageId":first,"clear":"yes"}),
            "InvalidParams",
        ),
        (33, "find", json!({"pageId":first}), "InvalidParams"),
        (34, "tab.list", json!({"pageId":first}), "UnknownMethod"),
        (35, "close", json!({"pageId":first}), "UnknownMethod"),
    ] {
        let response = server.request(id, method, params);
        assert_eq!(response["error"]["code"], code, "{method}: {response}");
    }

    // The browser stays under sidecar ownership and closes with it.
    assert!(server
        .request(40, "browser.close_tab", json!({"pageId":second}))
        .get("result")
        .is_some());
    let closed = server.request(41, "info", json!({"pageId":second}));
    assert_eq!(closed["error"]["code"], "InvalidPage", "{closed}");
    assert!(server
        .request(42, "text", json!({"pageId":first}))
        .get("result")
        .is_some());
    server.shutdown(43);
}

/// A same-origin fixture: a small form page at any path, plain text at `/data`.
fn fixture_server() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><h1>Heading</h1>\
        <input id=name aria-label=Name><select id=pick><option value=a>A<option value=b>B</select>\
        <button id=go onclick=\"document.getElementById('status').textContent='clicked'\">Go</button>\
        <p id=status></p><a id=link href=/next>Next</a><iframe id=inner srcdoc='<p>inner</p>'></iframe>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..read]).to_string();
                let (kind, body) = if head.starts_with("GET /data") {
                    ("text/plain", "ok")
                } else {
                    ("text/html", PAGE)
                };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

#[test]
#[ignore = "requires Chrome; calls every projected catalog operation once"]
fn every_catalog_operation_answers_through_the_sidecar() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &root.path().join("profile"));
    let origin = format!("http://{}", fixture_server());
    let file = |name: &str| scratch.join(name).to_string_lossy().into_owned();
    let status = json!({"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"clicked"});

    // `true` means the call must succeed. `false` means the operation has
    // nothing to act on in this fixture (no CAPTCHA, no WASM, no dialog) and
    // only has to answer with a structured operation error.
    let calls: Vec<(&str, Value, bool)> = vec![
        ("open", json!({"url":format!("{origin}/page")}), true),
        ("info", json!({}), true),
        ("text", json!({}), true),
        ("content", json!({}), true),
        ("find", json!({"text":"Go"}), true),
        ("observe", json!({}), true),
        ("snapshot", json!({}), true),
        ("exists", json!({"selector":"#go"}), true),
        ("get_text", json!({"selector":"h1"}), true),
        (
            "get_attribute",
            json!({"selector":"#link","name":"href"}),
            true,
        ),
        ("frames", json!({}), true),
        ("frame_eval", json!({"frame":"#inner","code":"1+1"}), true),
        ("eval", json!({"code":"() => 1 + 1"}), true),
        (
            "exec",
            json!({"code":"() => { window.touched = true; }"}),
            true,
        ),
        ("console", json!({}), true),
        ("errors", json!({}), true),
        ("set_cookie", json!({"name":"sid","value":"1"}), true),
        ("cookies", json!({}), true),
        ("delete_cookie", json!({"name":"sid"}), true),
        ("clear_cookies", json!({}), true),
        ("set_storage", json!({"key":"k","value":"v"}), true),
        ("storage", json!({"key":"k"}), true),
        ("dump_storage", json!({}), true),
        ("save_state", json!({"path":file("state.json")}), true),
        (
            "load_state",
            json!({"path":file("state.json"),"no_navigate":true}),
            true,
        ),
        (
            "headers",
            json!({"headers_json":"{\"x-test\":\"1\"}"}),
            true,
        ),
        (
            "emulate",
            json!({"width":800,"height":600,"desktop":true}),
            true,
        ),
        ("emulate", json!({"reset":true}), true),
        ("screenshot", json!({"output":file("shot.png")}), true),
        ("wait", json!({"ms":10}), true),
        ("wait", json!({"selector":"h1","timeout":3000}), true),
        ("fetch", json!({"url":format!("{origin}/data")}), true),
        ("scroll", json!({"target":"css:#go"}), true),
        ("hover", json!({"target":"css:#go","timeout_ms":5000}), true),
        (
            "click",
            json!({"target":"css:#go","expect":status,"timeout_ms":5000}),
            true,
        ),
        (
            "double_click",
            json!({"target":"css:#go","timeout_ms":1000}),
            true,
        ),
        (
            "fill",
            json!({"target":"css:#name","text":"ada","timeout_ms":5000}),
            true,
        ),
        (
            "select",
            json!({"target":"css:#pick","value":"b","timeout_ms":5000}),
            true,
        ),
        ("key", json!({"key":"Tab","timeout_ms":1000}), true),
        ("key.down", json!({"key":"Shift"}), true),
        ("key.up", json!({"key":"Shift"}), true),
        ("mouse.move", json!({"x":10,"y":10}), true),
        ("mouse.down", json!({"x":10,"y":10,"button":"left"}), true),
        ("mouse.up", json!({"x":10,"y":10,"button":"left"}), true),
        ("release_all_inputs", json!({}), true),
        ("network.record.start", json!({}), true),
        ("network.record.status", json!({}), true),
        ("fetch", json!({"url":format!("{origin}/data")}), true),
        ("network.log", json!({}), true),
        ("network.show", json!({"id":1}), false),
        (
            "network.wait",
            json!({"timeout":200,"pattern":"*never*"}),
            false,
        ),
        ("network.export", json!({"path":file("capture.har")}), true),
        ("network.clear", json!({}), true),
        ("network.record.stop", json!({}), true),
        (
            "intercept.add",
            json!({"url_pattern":"*never-matches*","capture":file("capture")}),
            true,
        ),
        ("intercept.list", json!({}), true),
        ("intercept.log", json!({}), true),
        ("fetch", json!({"url":format!("{origin}/data")}), true),
        ("intercept.remove", json!({"id":"1"}), false),
        ("js.list", json!({}), true),
        ("js.allow", json!({"domain":"example.com"}), true),
        ("js.block", json!({"domain":"ads.example"}), true),
        ("js.remove", json!({"domain":"example.com"}), true),
        ("js.mode", json!({"mode":"allow-all"}), true),
        ("spa.info", json!({}), true),
        ("spa.navigate", json!({"path":"/next"}), false),
        ("reload", json!({}), true),
        ("back", json!({}), true),
        ("forward", json!({}), true),
        ("dialog.accept", json!({}), false),
        ("dialog.dismiss", json!({}), false),
        ("captcha.detect", json!({}), true),
        ("captcha.inject", json!({"token":"token"}), false),
        ("captcha.turnstile", json!({"timeout_ms":1000}), false),
        (
            "captcha.datadome",
            json!({"max_attempts":1,"timeout_ms":5000}),
            false,
        ),
        (
            "click_shadow",
            json!({"selector":"#absent","timeout_ms":300}),
            false,
        ),
        ("wasm.info", json!({}), false),
        ("wasm.read", json!({"addr":"0x0","len":4}), false),
        ("wasm.write", json!({"addr":"0x0","hex":"00"}), false),
        ("wasm.find", json!({"pattern":"00"}), false),
        ("download.list", json!({}), true),
        ("download.get", json!({"id":"missing"}), false),
        (
            "download.wait",
            json!({"id":"missing","timeout_ms":100}),
            false,
        ),
        ("download.cancel", json!({"id":"missing"}), false),
        (
            "download.start",
            json!({"output":file("download.txt"),"url":format!("{origin}/data"),"start_timeout_ms":3000}),
            false,
        ),
        (
            "clone_from",
            json!({"source":"ws://127.0.0.1:1/devtools/browser/none"}),
            false,
        ),
        ("fake_camera", json!({"file":file("missing.mp4")}), false),
    ];

    let mut called = std::collections::BTreeSet::new();
    let mut problems = Vec::new();
    for (index, (method, params, must_succeed)) in calls.into_iter().enumerate() {
        called.insert(method);
        let mut params = params;
        params["pageId"] = json!(page);
        let started = Instant::now();
        let response = server.request(1000 + index as u64, method, params);
        let elapsed = started.elapsed();
        let summary: String = response.to_string().chars().take(260).collect();
        if elapsed > Duration::from_secs(20) {
            problems.push(format!("#{index} {method} took {elapsed:?}: {summary}"));
        }
        match response["error"]["code"].as_str() {
            None => {}
            // A wrong name, a rejected input or a server fault is a defect in
            // the projection whatever the fixture contains.
            Some(code @ ("UnknownMethod" | "InvalidParams" | "Internal" | "InvalidPage")) => {
                problems.push(format!("{method} failed with {code}: {summary}"))
            }
            Some(_) if must_succeed => problems.push(format!("{method} failed: {summary}")),
            Some(_) => {}
        }
        // An action that was admitted answers with its record either way.
        if must_succeed && response["result"]["outcome"].is_string() {
            let outcome = response["result"]["outcome"].as_str().unwrap();
            let unverified_by_design = matches!(method, "double_click" | "key");
            if outcome != "verified_complete" && !unverified_by_design {
                problems.push(format!("{method} ended {outcome}: {summary}"));
            }
        }
    }
    // The server is still healthy and the page is still the one addressed.
    let title = server.request(5000, "page.title", json!({"pageId":page}));
    if title["result"]["title"] != "fixture" {
        problems.push(format!("page was lost: {title}"));
    }
    let missing: Vec<&str> = eoka_protocol::sidecar::catalog_operations()
        .map(|operation| operation.path)
        .filter(|path| !called.contains(path))
        .collect();
    assert!(missing.is_empty(), "sweep does not call {missing:?}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    server.shutdown(5001);
}

#[test]
#[ignore = "requires Chrome; catalog open on a browser the sidecar does not own"]
fn catalog_open_navigates_the_addressed_page_on_a_borrowed_browser() {
    let (root, state, scratch) = directories();
    let profile = root.path().join("profile");
    let mut owner = Sidecar::start(&state, &scratch);
    launch(&mut owner, &profile);
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
    let page = borrower.request(2, "browser.new_page", json!({}))["result"]["pageId"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = borrower.request(3, "browser.tabs", json!({}))["result"]["tabs"]
        .as_array()
        .unwrap()
        .len();
    let origin = format!("http://{}", fixture_server());
    let opened = borrower.request(
        4,
        "open",
        json!({"pageId":page,"url":format!("{origin}/page")}),
    );
    assert!(opened.get("result").is_some(), "{opened}");
    let url = borrower.request(5, "page.url", json!({"pageId":page}));
    assert_eq!(url["result"]["url"], format!("{origin}/page"), "{url}");
    let after = borrower.request(6, "browser.tabs", json!({}))["result"]["tabs"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(after, before, "open must not create a tab");
    borrower.shutdown(7);
    assert!(owner
        .request(8, "browser.tabs", json!({}))
        .get("result")
        .is_some());
    owner.shutdown(9);
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
    let Some(root) = std::env::var_os("EOKA_ABRUPT_PARENT_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
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
        "page.evaluate_raw",
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

fn stalling_server() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><input id=name>\
        <button id=go>Go</button><p id=status></p>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                if String::from_utf8_lossy(&request[..read]).starts_with("GET /hang") {
                    std::thread::sleep(Duration::from_secs(600));
                    return;
                }
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{PAGE}",
                        PAGE.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

#[derive(Default)]
struct Inbox {
    held: std::collections::HashMap<u64, Value>,
}

impl Inbox {
    fn take(&mut self, server: &Sidecar, id: u64, within: Duration) -> Option<Value> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(reply) = self.held.remove(&id) {
                return Some(reply);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let reply = server.replies.recv_timeout(remaining).ok()?;
            self.held.insert(reply["id"].as_u64().unwrap(), reply);
        }
    }

    fn timed(
        &mut self,
        server: &mut Sidecar,
        id: u64,
        method: &str,
        params: Value,
    ) -> (Value, Duration) {
        let started = Instant::now();
        server.send(json!({"id":id,"method":method,"params":params}));
        let reply = self
            .take(server, id, Duration::from_secs(45))
            .unwrap_or_else(|| panic!("{method} did not answer"));
        (reply, started.elapsed())
    }
}

fn two_fixture_pages(server: &mut Sidecar, profile: &Path, origin: &str) -> (String, String) {
    launch(server, profile);
    let mut open = |id: u64| {
        let response = server.request(id, "browser.new_page", json!({"url":format!("{origin}/")}));
        response["result"]["pageId"]
            .as_str()
            .unwrap_or_else(|| panic!("{response}"))
            .to_owned()
    };
    (open(3), open(4))
}

#[test]
#[ignore = "requires Chrome"]
fn pages_run_requests_concurrently_and_each_page_keeps_its_order() {
    let (root, state, scratch) = directories();
    let origin = format!("http://{}", stalling_server());
    let mut server = Sidecar::start(&state, &scratch);
    let (first, second) = two_fixture_pages(&mut server, &root.path().join("profile"), &origin);
    let mut inbox = Inbox::default();
    let pause = "()=>new Promise(resolve=>setTimeout(()=>resolve(1),1000))";
    let started = Instant::now();
    server
        .send(json!({"id":10,"method":"page.evaluate","params":{"pageId":first,"function":pause}}));
    server.send(
        json!({"id":11,"method":"page.evaluate","params":{"pageId":second,"function":pause}}),
    );
    for id in [10, 11] {
        let reply = inbox.take(&server, id, Duration::from_secs(20)).unwrap();
        assert_eq!(reply["result"]["result"], 1, "{reply}");
    }
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1000), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1600), "{elapsed:?}");
    server.send(json!({"id":12,"method":"page.evaluate","params":{"pageId":first,
        "function":"()=>new Promise(resolve=>setTimeout(()=>{window.seen=['slow'];resolve(1)},300))"}}));
    server.send(
        json!({"id":13,"method":"page.evaluate","params":{"pageId":first,
        "function":"()=>{(window.seen??=[]).push('fast');return window.seen}"}}),
    );
    server.send(json!({"id":14,"method":"page.title","params":{"pageId":first}}));
    assert!(inbox.take(&server, 12, Duration::from_secs(20)).is_some());
    let ordered = inbox.take(&server, 13, Duration::from_secs(20)).unwrap();
    assert_eq!(
        ordered["result"]["result"],
        json!(["slow", "fast"]),
        "{ordered}"
    );
    let title = inbox.take(&server, 14, Duration::from_secs(20)).unwrap();
    assert_eq!(title["result"]["title"], "fixture");
    server.shutdown(15);
}

#[test]
#[ignore = "requires Chrome"]
fn a_page_stuck_in_a_script_does_not_delay_other_pages_or_new_pages() {
    let (root, state, scratch) = directories();
    let origin = format!("http://{}", stalling_server());
    let mut server = Sidecar::start(&state, &scratch);
    let (stuck, other) = two_fixture_pages(&mut server, &root.path().join("profile"), &origin);
    let mut inbox = Inbox::default();
    server.send(json!({"id":10,"method":"page.evaluate","params":{"pageId":stuck,"function":"()=>new Promise(()=>{})"}}));
    std::thread::sleep(Duration::from_millis(200));
    let (title, elapsed) = inbox.timed(&mut server, 11, "page.title", json!({"pageId":other}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    let (created, elapsed) = inbox.timed(&mut server, 12, "browser.new_page", json!({}));
    assert!(created["result"]["pageId"].is_string(), "{created}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    let (tabs, elapsed) = inbox.timed(&mut server, 13, "browser.tabs", json!({}));
    for page in [
        json!(stuck),
        json!(other),
        created["result"]["pageId"].clone(),
    ] {
        assert!(
            tabs["result"]["tabs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tab| tab["id"] == page),
            "{tabs}"
        );
    }
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    let (closed, elapsed) = inbox.timed(
        &mut server,
        14,
        "browser.close_tab",
        json!({"pageId":created["result"]["pageId"]}),
    );
    assert!(closed.get("result").is_some(), "{closed}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    server.send(json!({"id":15,"method":"page.title","params":{"pageId":stuck}}));
    assert!(
        inbox
            .take(&server, 15, Duration::from_millis(500))
            .is_none(),
        "a request behind the stuck script on the same page must keep its place"
    );
    assert!(inbox.take(&server, 10, Duration::from_millis(1)).is_none());
    let started = Instant::now();
    server.send(json!({"id":16,"method":"server.shutdown","params":{}}));
    let interrupted = inbox.take(&server, 10, Duration::from_secs(20)).unwrap();
    assert_eq!(interrupted["error"]["code"], "Cancelled", "{interrupted}");
    let queued = inbox.take(&server, 15, Duration::from_secs(20)).unwrap();
    assert_eq!(queued["error"]["code"], "ShuttingDown", "{queued}");
    let shutdown = inbox.take(&server, 16, Duration::from_secs(20)).unwrap();
    assert_eq!(shutdown["result"], json!({}), "{shutdown}");
    assert!(started.elapsed() < Duration::from_secs(10));
    server.exit();
}

#[test]
#[ignore = "requires Chrome"]
fn an_unanswered_navigation_is_isolated_then_abandoned_so_the_page_answers_again() {
    let (root, state, scratch) = directories();
    let origin = format!("http://{}", stalling_server());
    let mut server = Sidecar::start(&state, &scratch);
    let (stuck, other) = two_fixture_pages(&mut server, &root.path().join("profile"), &origin);
    let mut inbox = Inbox::default();
    let navigate = eoka_server::eoka::Action::Navigate {
        url: format!("{origin}/hang"),
        referrer: None,
    };
    let timed_out = uuid::Uuid::new_v4().to_string();
    let record = server.request(
        10,
        "actions.perform",
        json!({"pageId":stuck,"operation_id":timed_out,"action":navigate,"options":{"timeout_ms":4000}}),
    );
    assert_eq!(record["result"]["id"], timed_out, "{record}");
    std::thread::sleep(Duration::from_millis(500));
    let (title, elapsed) = inbox.timed(&mut server, 11, "page.title", json!({"pageId":other}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    let (created, elapsed) = inbox.timed(&mut server, 12, "browser.new_page", json!({}));
    assert!(created["result"]["pageId"].is_string(), "{created}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    let asked = Instant::now();
    server.send(json!({"id":13,"method":"page.title","params":{"pageId":stuck}}));
    let (title, elapsed) = inbox.timed(&mut server, 14, "page.title", json!({"pageId":other}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    let finished = server_wait(&mut inbox, &mut server, 15, &timed_out);
    assert_eq!(finished["result"]["state"], "finished", "{finished}");
    assert_eq!(
        finished["result"]["outcome"], "outcome_unknown",
        "{finished}"
    );
    assert_eq!(
        finished["result"]["error_code"], "action_timeout",
        "{finished}"
    );
    let title = inbox
        .take(&server, 13, Duration::from_secs(5))
        .expect("the read must answer once the unanswered navigation is abandoned");
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(asked.elapsed() < Duration::from_secs(35));
    let (url, elapsed) = inbox.timed(&mut server, 16, "page.url", json!({"pageId":stuck}));
    assert_eq!(url["result"]["url"], format!("{origin}/"), "{url}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");

    let cancelled = uuid::Uuid::new_v4().to_string();
    let record = server.request(
        20,
        "actions.perform",
        json!({"pageId":stuck,"operation_id":cancelled,"action":navigate,"options":{"timeout_ms":120_000}}),
    );
    assert_eq!(record["result"]["id"], cancelled, "{record}");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut id = 21;
    loop {
        let record = server.request(id, "operations.get", json!({"id":cancelled}));
        id += 1;
        if record["result"]["dispatch_intent"] == true {
            break;
        }
        assert!(Instant::now() < deadline, "navigation never dispatched");
        std::thread::sleep(Duration::from_millis(25));
    }
    std::thread::sleep(Duration::from_millis(300));
    let record = server.request(100, "operations.cancel", json!({"id":cancelled}));
    assert_eq!(record["result"]["id"], cancelled, "{record}");
    let finished = server_wait(&mut inbox, &mut server, 101, &cancelled);
    assert_eq!(
        finished["result"]["error_code"], "operation_canceled",
        "{finished}"
    );
    let (title, elapsed) = inbox.timed(&mut server, 102, "page.title", json!({"pageId":stuck}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    server.shutdown(103);
}

fn server_wait(inbox: &mut Inbox, server: &mut Sidecar, id: u64, operation: &str) -> Value {
    inbox
        .timed(
            server,
            id,
            "operations.wait",
            json!({"id":operation,"timeout_ms":30_000}),
        )
        .0
}

#[test]
#[ignore = "requires Chrome"]
fn closing_one_tab_leaves_an_operation_on_another_tab_running() {
    let (root, state, scratch) = directories();
    let origin = format!("http://{}", stalling_server());
    let mut server = Sidecar::start(&state, &scratch);
    let (acting, closing) = two_fixture_pages(&mut server, &root.path().join("profile"), &origin);
    let operation = uuid::Uuid::new_v4().to_string();
    let target = eoka_server::eoka::LocatorSpec::css("#go");
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
    let record = server.request(
        10,
        "actions.perform",
        json!({"pageId":acting,"operation_id":operation,"action":action,"options":{"expect":expect,"timeout_ms":60_000}}),
    );
    assert_eq!(record["result"]["id"], operation, "{record}");
    let closed = server.request(11, "browser.close_tab", json!({"pageId":closing}));
    assert!(closed.get("result").is_some(), "{closed}");
    let record = server.request(12, "operations.get", json!({"id":operation}));
    assert_eq!(record["result"]["state"], "running", "{record}");
    let closed = server.request(13, "page.close", json!({"pageId":acting}));
    assert!(closed.get("result").is_some(), "{closed}");
    let record = server.request(14, "operations.get", json!({"id":operation}));
    assert_eq!(record["result"]["state"], "finished", "{record}");
    assert_eq!(
        record["result"]["error_code"], "operation_canceled",
        "{record}"
    );
    server.shutdown(15);
}

#[test]
#[ignore = "requires Chrome"]
fn diagnostics_keep_their_wording_and_file_output_names_the_occupied_path() {
    let (root, state, scratch) = directories();
    let origin = format!("http://{}", stalling_server());
    let mut server = Sidecar::start(&state, &scratch);
    let (page, _) = two_fixture_pages(&mut server, &root.path().join("profile"), &origin);
    for (id, value) in [(20, "x"), (30, "o"), (40, "e"), (50, "t")] {
        let operation = uuid::Uuid::new_v4().to_string();
        let record = server.request(
            id,
            "page.fill",
            json!({"pageId":page,"operation_id":operation,"selector":"#missing","value":value,"options":{"timeout_ms":500}}),
        );
        assert_eq!(record["result"]["id"], operation, "{record}");
        let finished = server.request(
            id + 1,
            "operations.wait",
            json!({"id":operation,"timeout_ms":30_000}),
        );
        let stored = server.request(id + 2, "operations.get", json!({"id":operation}));
        for record in [&finished, &stored] {
            let error = &record["result"]["error"];
            assert_eq!(
                error["message"],
                "Timeout: Action deadline exceeded during preparation: target not found",
                "{value:?}: {record}"
            );
            assert_eq!(error["code"], "action_timeout", "{record}");
            assert_eq!(error["context"]["phase"], "preparation", "{record}");
            assert!(!record.to_string().contains("[redacted]"), "{record}");
        }
    }

    let directory = std::fs::canonicalize(root.path()).unwrap();
    let occupied = directory.join("occupied.png");
    std::fs::write(&occupied, b"keep").unwrap();
    let refused = server.request(
        12,
        "page.screenshot_to_file",
        json!({"pageId":page,"path":occupied}),
    );
    assert_eq!(refused["error"]["code"], "FileOutput", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(message.contains(occupied.to_str().unwrap()), "{message}");
    assert!(message.contains("never overwrites"), "{message}");
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");

    let recording = uuid::Uuid::new_v4().to_string();
    let refused = server.request(
        13,
        "recording.start",
        json!({"recordingId":recording,"pageId":page,"path":occupied}),
    );
    let failure = if refused.get("error").is_some() {
        refused["error"]["message"].clone()
    } else {
        refused["result"]["failure"]["message"].clone()
    };
    let message = failure.as_str().unwrap_or_else(|| panic!("{refused}"));
    assert!(message.contains(occupied.to_str().unwrap()), "{message}");
    assert!(message.contains("never overwrites"), "{message}");
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
    server.shutdown(14);
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

fn png(reply: &Value) -> Vec<u8> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            reply["result"]["dataBase64"]
                .as_str()
                .unwrap_or_else(|| panic!("{reply}")),
        )
        .unwrap();
    assert_eq!(&bytes[..8], PNG_SIGNATURE);
    bytes
}

fn visibility(server: &mut Sidecar, id: u64, page: &str) -> Value {
    let seen = server.request(
        id,
        "page.evaluate",
        json!({"pageId":page,"function":"()=>({now:document.visibilityState,changes:window.visibilityChanges})"}),
    );
    seen["result"]["result"].clone()
}

#[test]
#[ignore = "requires Chrome"]
fn a_tab_behind_another_is_captured_promptly_and_stays_behind() {
    let prompt = Duration::from_secs(5);
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    launch(&mut server, &root.path().join("profile"));
    let mut open = |id: u64| {
        let response = server.request(
            id,
            "browser.new_page",
            json!({"url":"data:text/html,<title>t</title><h1>hello</h1>"}),
        );
        response["result"]["pageId"].as_str().unwrap().to_owned()
    };
    let (behind, front) = (open(3), open(4));
    for (id, page) in [(5, &behind), (6, &front)] {
        let watched = server.request(
            id,
            "page.execute",
            json!({"pageId":page,"function":"()=>{window.visibilityChanges=[];document.addEventListener('visibilitychange',()=>window.visibilityChanges.push(document.visibilityState))}"}),
        );
        assert!(watched.get("result").is_some(), "{watched}");
    }
    std::thread::sleep(Duration::from_secs(3));
    let mut inbox = Inbox::default();

    let (first, elapsed) =
        inbox.timed(&mut server, 10, "page.screenshot", json!({"pageId":behind}));
    assert!(elapsed < prompt, "{elapsed:?}");
    let before = png(&first);
    let output = std::fs::canonicalize(root.path())
        .unwrap()
        .join("behind.png");
    let (saved, elapsed) = inbox.timed(
        &mut server,
        11,
        "page.screenshot_to_file",
        json!({"pageId":behind,"path":output}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    assert!(saved["result"]["bytes"].as_u64().unwrap() > 0, "{saved}");
    assert_eq!(&std::fs::read(&output).unwrap()[..8], PNG_SIGNATURE);
    let (ahead, elapsed) = inbox.timed(&mut server, 12, "page.screenshot", json!({"pageId":front}));
    assert!(elapsed < prompt, "{elapsed:?}");
    png(&ahead);

    let started = Instant::now();
    server.send(json!({"id":13,"method":"page.screenshot","params":{"pageId":behind}}));
    server.send(json!({"id":14,"method":"page.screenshot","params":{"pageId":front}}));
    for id in [13, 14] {
        png(&inbox.take(&server, id, Duration::from_secs(40)).unwrap());
    }
    assert!(started.elapsed() < Duration::from_secs(10));

    let painted = server.request(
        15,
        "page.execute",
        json!({"pageId":behind,"function":"()=>{document.body.style.background='rgb(200,0,0)'}"}),
    );
    assert!(painted.get("result").is_some(), "{painted}");
    std::thread::sleep(Duration::from_secs(3));
    let (repainted, elapsed) =
        inbox.timed(&mut server, 16, "page.screenshot", json!({"pageId":behind}));
    assert!(elapsed < prompt, "{elapsed:?}");
    assert_ne!(png(&repainted), before, "a stale frame was captured");
    let (whole, elapsed) = inbox.timed(
        &mut server,
        17,
        "page.screenshot",
        json!({"pageId":behind,"fullPage":true,"format":"jpeg","quality":40}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    assert!(
        whole["result"]["dataBase64"]
            .as_str()
            .is_some_and(|data| !data.is_empty()),
        "{whole}"
    );
    let catalogued = std::fs::canonicalize(root.path())
        .unwrap()
        .join("catalog.png");
    let (catalog, elapsed) = inbox.timed(
        &mut server,
        18,
        "screenshot",
        json!({"pageId":behind,"output":catalogued}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    assert!(catalog["result"]["size"].as_u64().unwrap() > 0, "{catalog}");
    assert_eq!(&std::fs::read(&catalogued).unwrap()[..8], PNG_SIGNATURE);

    assert_eq!(
        visibility(&mut server, 19, &behind),
        json!({"now":"hidden","changes":[]})
    );
    assert_eq!(
        visibility(&mut server, 20, &front),
        json!({"now":"visible","changes":[]})
    );

    let recording = uuid::Uuid::new_v4().to_string();
    let video = std::fs::canonicalize(root.path())
        .unwrap()
        .join("behind.mp4");
    let record = server.request(
        30,
        "recording.start",
        json!({"recordingId":recording,"pageId":behind,"path":video}),
    );
    assert_eq!(record["result"]["state"], "running", "{record}");
    let animated = server.request(
        31,
        "page.execute",
        json!({"pageId":behind,"function":"()=>{let n=0;setInterval(()=>{document.body.style.background=++n%2?'rgb(0,0,200)':'rgb(0,200,0)'},100)}"}),
    );
    assert!(animated.get("result").is_some(), "{animated}");
    std::thread::sleep(Duration::from_millis(1500));
    let (during, elapsed) =
        inbox.timed(&mut server, 32, "page.screenshot", json!({"pageId":behind}));
    assert!(elapsed < prompt, "{elapsed:?}");
    png(&during);
    std::thread::sleep(Duration::from_millis(1000));
    let stopped = server.request(33, "recording.stop", json!({"recordingId":recording}));
    assert_eq!(stopped["result"]["state"], "completed", "{stopped}");
    assert!(
        stopped["result"]["artifact"]["bytes"].as_u64().unwrap() > 4096,
        "{stopped}"
    );
    assert_eq!(
        std::fs::metadata(&video).unwrap().len(),
        stopped["result"]["artifact"]["bytes"].as_u64().unwrap()
    );
    server.shutdown(34);
}

const QUICK: Duration = Duration::from_millis(2000);
const UNAFFECTED: Duration = Duration::from_millis(1000);

fn blocked_fixture() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><h1>Blocked tabs</h1>\
        <button id=danger onclick=\"window.clicks=(window.clicks||0)+1;alert('are you sure?');\
        document.getElementById('s').textContent='Deleted'\">Delete</button>\
        <button id=plain onclick=\"document.getElementById('s').textContent='Plain'\">Plain</button>\
        <p id=s></p>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                if String::from_utf8_lossy(&request[..read]).starts_with("GET /hang") {
                    std::thread::sleep(Duration::from_secs(600));
                    return;
                }
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{PAGE}",
                        PAGE.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

struct Blocked {
    server: Sidecar,
    inbox: Inbox,
    origin: String,
    next: u64,
    _root: TempDir,
}

impl Blocked {
    fn start(hello: Option<Value>) -> Self {
        let (root, state, scratch) = directories();
        let mut server = Sidecar::start(&state, &scratch);
        if let Some(hello) = hello {
            let greeting = server.request(900, "server.hello", hello);
            assert_eq!(
                greeting["result"]["protocol"],
                json!(PROTOCOL_VERSION),
                "{greeting}"
            );
            assert!(greeting["result"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("pageBlockers")));
        }
        launch(&mut server, &root.path().join("profile"));
        Self {
            server,
            inbox: Inbox::default(),
            origin: format!("http://{}", blocked_fixture()),
            next: 1000,
            _root: root,
        }
    }

    fn current() -> Self {
        Self::start(Some(json!({"protocol":PROTOCOL_VERSION})))
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn call(&mut self, method: &str, params: Value) -> (Value, Duration) {
        let id = self.id();
        self.inbox.timed(&mut self.server, id, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let (reply, _) = self.call(method, params);
        assert!(reply.get("result").is_some(), "{method}: {reply}");
        reply["result"].clone()
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
        let id = self.id();
        self.server
            .send(json!({"id":id,"method":method,"params":params}));
        id
    }

    fn page(&mut self) -> String {
        let url = format!("{}/", self.origin);
        self.ok("browser.new_page", json!({"url":url}))["pageId"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn open_alert(&mut self, page: &str) {
        self.ok(
            "page.execute",
            json!({"pageId":page,"function":"()=>{setTimeout(()=>alert('are you sure?'),0)}"}),
        );
        std::thread::sleep(Duration::from_millis(300));
    }

    fn stall_navigation(&mut self, page: &str) {
        self.ok(
            "page.execute",
            json!({"pageId":page,"function":"()=>{setTimeout(()=>{location.href='/hang'},0)}"}),
        );
    }

    fn title(&mut self, page: &str, within: Duration) {
        let (reply, elapsed) = self.call("page.title", json!({"pageId":page}));
        assert_eq!(reply["result"]["title"], "fixture", "{reply}");
        assert!(elapsed < within, "{elapsed:?}");
    }

    fn finish(mut self) {
        let id = self.id();
        self.server.shutdown(id);
    }
}

fn assert_dialog_open(reply: &Value, method: &str) {
    let error = &reply["error"];
    assert_eq!(error["code"], "DialogOpen", "{method}: {reply}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("dialog is open"), "{message}");
    assert!(message.contains("are you sure?"), "{message}");
    assert_eq!(error["data"]["detail"]["code"], "dialog_open", "{reply}");
    assert!(
        error["data"]["detail"]["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("dialog.accept") && hint.contains("dialog.dismiss")),
        "{reply}"
    );
    let blocker = &error["data"]["blocker"];
    assert_eq!(blocker["kind"], "dialog", "{reply}");
    assert_eq!(blocker["dialogType"], "alert", "{reply}");
    assert_eq!(blocker["message"], "are you sure?", "{reply}");
    assert!(blocker["elapsedMs"].is_u64(), "{reply}");
}

#[test]
#[ignore = "requires Chrome"]
fn an_open_dialog_is_reported_at_once_and_clearing_it_frees_the_tab() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.open_alert(&page);
    for (method, params) in [
        ("page.title", json!({"pageId":page})),
        ("page.evaluate", json!({"pageId":page,"function":"()=>1"})),
        ("page.get_text", json!({"pageId":page,"selector":"h1"})),
        ("page.exists", json!({"pageId":page,"selector":"h1"})),
        ("page.screenshot", json!({"pageId":page})),
        ("agent.observe", json!({"pageId":page})),
        ("text", json!({"pageId":page})),
        ("eval", json!({"pageId":page,"code":"1"})),
    ] {
        let (reply, elapsed) = blocked.call(method, params);
        assert_dialog_open(&reply, method);
        assert!(elapsed < QUICK, "{method}: {elapsed:?}");
    }
    let operation = uuid::Uuid::new_v4().to_string();
    let (refused, elapsed) = blocked.call(
        "actions.perform",
        json!({"pageId":page,"operation_id":operation,
            "action":{"kind":"hover","target":{"query":{"kind":"css","selector":"#plain"}}}}),
    );
    assert_dialog_open(&refused, "actions.perform");
    assert_eq!(
        refused["error"]["data"]["operation_id"], operation,
        "{refused}"
    );
    assert!(elapsed < QUICK, "{elapsed:?}");
    let (unknown, _) = blocked.call("operations.get", json!({"id":operation}));
    assert!(unknown.get("error").is_some(), "{unknown}");
    blocked.title(&other, UNAFFECTED);

    let (accepted, elapsed) = blocked.call("dialog.accept", json!({"pageId":page}));
    assert_eq!(accepted["result"], "Dialog accepted", "{accepted}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    blocked.title(&page, QUICK);

    blocked.open_alert(&page);
    let (dismissed, elapsed) = blocked.call("dialog.dismiss", json!({"pageId":page}));
    assert_eq!(dismissed["result"], "Dialog dismissed", "{dismissed}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    blocked.title(&page, QUICK);

    let (confirm, elapsed) = blocked.call(
        "page.evaluate",
        json!({"pageId":page,"function":"()=>confirm('are you sure?')"}),
    );
    assert_eq!(confirm["error"]["code"], "DialogOpen", "{confirm}");
    assert_eq!(
        confirm["error"]["data"]["blocker"]["dialogType"], "confirm",
        "{confirm}"
    );
    assert!(
        confirm["error"]["message"]
            .as_str()
            .unwrap()
            .contains("`page.evaluate` returned without a result"),
        "{confirm}"
    );
    assert!(elapsed < QUICK, "{elapsed:?}");
    blocked.ok("dialog.dismiss", json!({"pageId":page}));
    blocked.title(&page, QUICK);
    let (none, _) = blocked.call("dialog.accept", json!({"pageId":page}));
    assert_eq!(none["error"]["code"], "Cdp", "{none}");
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn a_click_that_opens_a_dialog_ends_its_operation_and_is_sent_once() {
    let mut blocked = Blocked::current();
    let page = blocked.page();
    let status = json!({"query":{"kind":"css","selector":"#s"}});
    let operation = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let record = blocked.ok(
        "actions.perform",
        json!({"pageId":page,"operation_id":operation,
            "action":{"kind":"click","target":{"query":{"kind":"css","selector":"#danger"}}},
            "options":{"expect":{"kind":"text","target":status,"value":"Deleted"},"timeout_ms":8000}}),
    );
    assert_eq!(record["id"], operation, "{record}");
    let record = blocked.ok("operations.wait", json!({"id":operation,"timeout_ms":8000}));
    assert!(
        started.elapsed() < Duration::from_millis(3000),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(record["state"], "finished", "{record}");
    assert_eq!(record["error_code"], "dialog_open", "{record}");
    assert_eq!(record["outcome"], "outcome_unknown", "{record}");
    assert_eq!(record["dispatch_intent"], true, "{record}");
    assert_eq!(
        record["receipt"]["dispatch"], "may_have_been_sent",
        "{record}"
    );
    assert_eq!(record["receipt"]["target_id"], page, "{record}");
    assert_eq!(record["receipt"]["outcome"], "outcome_unknown", "{record}");
    assert_eq!(record["error"]["code"], "dialog_open", "{record}");
    let message = record["error"]["message"].as_str().unwrap();
    assert!(message.contains("dialog is open"), "{message}");
    assert!(message.contains("are you sure?"), "{message}");
    assert!(message.contains("was dispatched"), "{message}");
    assert!(message.contains("was sent to the page once"), "{message}");
    assert!(
        record["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("dialog.accept"),
        "{record}"
    );
    assert_eq!(
        record["error"]["context"]["action_status"], "may_have_executed",
        "{record}"
    );
    std::thread::sleep(Duration::from_secs(6));
    let (accepted, elapsed) = blocked.call("dialog.accept", json!({"pageId":page}));
    assert_eq!(accepted["result"], "Dialog accepted", "{accepted}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    let text = blocked.ok("page.get_text", json!({"pageId":page,"selector":"#s"}));
    assert_eq!(text["text"], "Deleted", "{text}");
    let next = uuid::Uuid::new_v4().to_string();
    blocked.ok(
        "actions.perform",
        json!({"pageId":page,"operation_id":next,
            "action":{"kind":"click","target":{"query":{"kind":"css","selector":"#plain"}}},
            "options":{"expect":{"kind":"text","target":status,"value":"Plain"},"timeout_ms":8000}}),
    );
    let record = blocked.ok("operations.wait", json!({"id":next,"timeout_ms":9000}));
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    let clicks = blocked.ok(
        "page.evaluate",
        json!({"pageId":page,"function":"()=>window.clicks"}),
    );
    assert_eq!(clicks["result"], 1, "{clicks}");
    let stored = blocked.ok("operations.get", json!({"id":operation}));
    assert_eq!(stored["error_code"], "dialog_open", "{stored}");
    blocked.finish();
}

fn assert_navigation_pending(reply: &Value) {
    let error = &reply["error"];
    assert_eq!(error["code"], "NavigationPending", "{reply}");
    assert!(
        error["message"].as_str().unwrap().contains("/hang"),
        "{reply}"
    );
    assert_eq!(
        error["data"]["detail"]["code"], "navigation_pending",
        "{reply}"
    );
    assert_eq!(error["data"]["blocker"]["kind"], "navigation", "{reply}");
    assert!(
        error["data"]["blocker"]["url"]
            .as_str()
            .is_some_and(|url| url.ends_with("/hang")),
        "{reply}"
    );
    assert!(error["data"]["blocker"]["elapsedMs"].is_u64(), "{reply}");
}

#[test]
#[ignore = "requires Chrome"]
fn a_navigation_the_page_started_is_named_when_it_stalls_and_a_new_one_recovers_the_tab() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.stall_navigation(&page);
    std::thread::sleep(Duration::from_millis(500));
    let (early, elapsed) = blocked.call("page.title", json!({"pageId":page}));
    assert_navigation_pending(&early);
    assert!(elapsed < Duration::from_millis(8000), "{elapsed:?}");
    assert!(elapsed > Duration::from_millis(3000), "{elapsed:?}");
    for (method, params) in [
        ("page.url", json!({"pageId":page})),
        ("page.evaluate", json!({"pageId":page,"function":"()=>1"})),
        ("page.exists", json!({"pageId":page,"selector":"h1"})),
        ("text", json!({"pageId":page})),
    ] {
        let (reply, elapsed) = blocked.call(method, params);
        assert_navigation_pending(&reply);
        assert!(elapsed < QUICK, "{method}: {elapsed:?}");
    }
    blocked.title(&other, UNAFFECTED);
    let navigate = |blocked: &mut Blocked, page: &str| {
        let operation = uuid::Uuid::new_v4().to_string();
        let url = format!("{}/", blocked.origin);
        let started = Instant::now();
        blocked.ok(
            "actions.perform",
            json!({"pageId":page,"operation_id":operation,
                "action":{"kind":"navigate","url":url},"options":{"timeout_ms":8000}}),
        );
        let record = blocked.ok("operations.wait", json!({"id":operation,"timeout_ms":8000}));
        assert_eq!(record["outcome"], "verified_complete", "{record}");
        assert!(started.elapsed() < Duration::from_millis(8000));
    };
    navigate(&mut blocked, &page);
    blocked.title(&page, QUICK);

    let unread = blocked.page();
    blocked.stall_navigation(&unread);
    std::thread::sleep(Duration::from_millis(1000));
    navigate(&mut blocked, &unread);
    blocked.title(&unread, QUICK);
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn clearing_a_dialog_and_closing_the_tab_overtake_a_request_stuck_on_that_page() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.open_alert(&page);
    let stuck = blocked.send(
        "page.solve_turnstile",
        json!({"pageId":page,"timeoutMs":20_000}),
    );
    let queued = blocked.send("page.reset_viewport", json!({"pageId":page}));
    std::thread::sleep(Duration::from_millis(300));
    assert!(blocked
        .inbox
        .take(&blocked.server, stuck, Duration::from_millis(1))
        .is_none());
    let (dismissed, elapsed) = blocked.call("dialog.dismiss", json!({"pageId":page}));
    assert_eq!(dismissed["result"], "Dialog dismissed", "{dismissed}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    for id in [stuck, queued] {
        let reply = blocked
            .inbox
            .take(&blocked.server, id, Duration::from_secs(5))
            .expect("the request behind the dialog must finish once it is cleared");
        assert!(reply.get("result").is_some(), "{reply}");
    }
    blocked.title(&page, QUICK);

    blocked.open_alert(&page);
    let stuck = blocked.send(
        "page.solve_turnstile",
        json!({"pageId":page,"timeoutMs":20_000}),
    );
    std::thread::sleep(Duration::from_millis(300));
    let (closed, elapsed) = blocked.call("page.close", json!({"pageId":page}));
    assert!(closed.get("result").is_some(), "{closed}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    blocked.title(&other, UNAFFECTED);
    let tabs = blocked.ok("browser.tabs", json!({}));
    assert!(
        !tabs["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tab| tab["id"] == json!(page)),
        "{tabs}"
    );
    let ended = blocked
        .inbox
        .take(&blocked.server, stuck, Duration::from_secs(40))
        .expect("the request on the closed tab must end");
    assert!(ended.get("error").is_some(), "{ended}");
    let (gone, _) = blocked.call("page.title", json!({"pageId":page}));
    assert_eq!(gone["error"]["code"], "InvalidPage", "{gone}");
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn a_client_that_did_not_ask_for_the_blocker_codes_receives_operation_errors() {
    for hello in [
        None,
        Some(json!({"protocol":{"major":2,"minor":1}})),
        Some(json!({"protocol":{"major":2,"minor":0}})),
    ] {
        let mut blocked = Blocked::start(hello.clone());
        let page = blocked.page();
        blocked.open_alert(&page);
        for (method, params) in [
            ("page.title", json!({"pageId":page})),
            ("page.exists", json!({"pageId":page,"selector":"h1"})),
            ("text", json!({"pageId":page})),
        ] {
            let (reply, elapsed) = blocked.call(method, params);
            assert_eq!(
                reply["error"]["code"], "Operation",
                "{hello:?} {method}: {reply}"
            );
            assert_eq!(
                reply["error"]["data"]["detail"]["code"], "dialog_open",
                "{reply}"
            );
            assert!(reply["error"]["data"].get("blocker").is_none(), "{reply}");
            assert!(reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("are you sure?"));
            assert!(elapsed < QUICK, "{elapsed:?}");
        }
        blocked.ok("dialog.dismiss", json!({"pageId":page}));
        blocked.title(&page, QUICK);
        blocked.finish();
    }
    let mut understands = Blocked::start(Some(
        json!({"protocol":{"major":2,"minor":0},"understands":{"major":2,"minor":2}}),
    ));
    let page = understands.page();
    understands.open_alert(&page);
    let (reply, _) = understands.call("page.title", json!({"pageId":page}));
    assert_dialog_open(&reply, "page.title");
    understands.ok("dialog.dismiss", json!({"pageId":page}));
    understands.finish();
}

#[test]
#[ignore = "requires Chrome; waits for the 15 second read limit"]
fn a_page_whose_main_thread_is_busy_is_reported_as_unresponsive() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.ok(
        "page.execute",
        json!({"pageId":page,"function":"()=>{setTimeout(()=>{while(true){}},0)}"}),
    );
    std::thread::sleep(Duration::from_millis(300));
    let (reply, elapsed) = blocked.call("page.title", json!({"pageId":page}));
    assert_eq!(reply["error"]["code"], "PageUnresponsive", "{reply}");
    assert_eq!(
        reply["error"]["data"]["detail"]["code"], "page_unresponsive",
        "{reply}"
    );
    assert!(reply["error"]["data"].get("blocker").is_none(), "{reply}");
    assert!(
        reply["error"]["data"]["detail"]["hint"]
            .as_str()
            .unwrap()
            .contains("browser.new_page"),
        "{reply}"
    );
    assert!(elapsed < Duration::from_secs(18), "{elapsed:?}");
    assert!(elapsed >= Duration::from_secs(14), "{elapsed:?}");
    blocked.title(&other, UNAFFECTED);
    let (closed, _) = blocked.call("page.close", json!({"pageId":page}));
    assert!(closed.get("result").is_some(), "{closed}");
    blocked.finish();
}
