use crate::support::*;

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
        "captcha.turnstile",
        json!({"pageId":page,"timeout_ms":5_000}),
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
        "wait",
        json!({"pageId":challenge,"text":"checking","timeout":5_000}),
    );
    assert!(loaded.get("error").is_none(), "{loaded}");
    let stuck = server.request(
        5,
        "captcha.turnstile",
        json!({"pageId":challenge,"timeout_ms":1_500}),
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
    server.send(json!({"id":5,"method":"wait","params":{"pageId":page,"text":"never present","timeout":600_000}}));
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
        server.send(json!({"id":id,"method":"info","params":{"pageId":page}}));
    }
    server.send(json!({"id":134,"method":"info","params":{"pageId":page}}));
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
        "id":3,"method":"wait",
        "params":{"pageId":page,"text":"never present","timeout":2_000}
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
        .request(7, "info", json!({"pageId":page}))
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
#[test]
#[ignore = "requires Chrome"]
fn abrupt_parent_death_closes_owned_chrome() {
    let (root, _, _) = directories();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "lifecycle::abrupt_parent_fixture",
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
                borrower.request(2, "info", json!({"pageId":original_id}))["result"]["title"],
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
                borrower.request(5, "info", json!({"pageId":later_id}))["result"]["title"],
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
                borrower.request(7, "info", json!({"pageId":later_id}))["error"]["code"],
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
                borrower.request(10, "key.down", json!({"pageId":original_id,"key":"Shift"}))
                    ["result"],
                json!("Held Shift")
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
