use crate::support::*;

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
        (6, json!({"major":3,"minor":0})),
    ] {
        assert_eq!(
            server.request(id, "server.hello", json!({"protocol":protocol}))["error"]["code"],
            "ProtocolMismatch"
        );
    }
    assert!(!state.exists());
    assert!(server
        .request(4, "server.hello", json!({"protocol":{"major":4,"minor":0}}))
        .get("result")
        .is_some());
    server.shutdown(5);
}

#[test]
fn framing_handles_fragmented_utf8_multiple_lines_and_numeric_ids() {
    let (_root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let input = "{\"id\":1,\"method\":\"unknown.é\",\"params\":{}}\n{\"id\":2,\"method\":\"server.hello\",\"params\":{\"protocol\":{\"major\":4,\"minor\":0}}}\n";
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
            json!({"id":id,"method":"server.hello","params":{"protocol":{"major":4,"minor":0}}}),
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
        .request(2, "server.hello", json!({"protocol":{"major":4,"minor":0}}))
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
            "emulate",
            json!({"pageId":"unknown","width":-1,"height":360,"desktop":true}),
        ),
        (
            "emulate",
            json!({"pageId":"unknown","width":640,"height":360,"deviceScaleFactor":1}),
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
        .send(json!({"id":1,"method":"server.hello","params":{"protocol":{"major":4,"minor":0}}}));
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
