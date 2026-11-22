use crate::support::*;

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
    server.send(
        json!({"id":10,"method":"eval","params":{"pageId":first,"function":true,"code":pause}}),
    );
    server.send(
        json!({"id":11,"method":"eval","params":{"pageId":second,"function":true,"code":pause}}),
    );
    for id in [10, 11] {
        let reply = inbox.take(&server, id, Duration::from_secs(20)).unwrap();
        assert_eq!(reply["result"], 1, "{reply}");
    }
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1000), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1600), "{elapsed:?}");
    server.send(
        json!({"id":12,"method":"eval","params":{"pageId":first,"function":true,
        "code":"()=>new Promise(resolve=>setTimeout(()=>{window.seen=['slow'];resolve(1)},300))"}}),
    );
    server.send(
        json!({"id":13,"method":"eval","params":{"pageId":first,"function":true,
        "code":"()=>{(window.seen??=[]).push('fast');return window.seen}"}}),
    );
    server.send(json!({"id":14,"method":"info","params":{"pageId":first}}));
    assert!(inbox.take(&server, 12, Duration::from_secs(20)).is_some());
    let ordered = inbox.take(&server, 13, Duration::from_secs(20)).unwrap();
    assert_eq!(ordered["result"], json!(["slow", "fast"]), "{ordered}");
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
    server.send(json!({"id":10,"method":"eval","params":{"pageId":stuck,"function":true,"code":"()=>new Promise(()=>{})"}}));
    std::thread::sleep(Duration::from_millis(200));
    let (title, elapsed) = inbox.timed(&mut server, 11, "info", json!({"pageId":other}));
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
    server.send(json!({"id":15,"method":"info","params":{"pageId":stuck}}));
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
    let (title, elapsed) = inbox.timed(&mut server, 11, "info", json!({"pageId":other}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    let (created, elapsed) = inbox.timed(&mut server, 12, "browser.new_page", json!({}));
    assert!(created["result"]["pageId"].is_string(), "{created}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    let asked = Instant::now();
    server.send(json!({"id":13,"method":"info","params":{"pageId":stuck}}));
    let (title, elapsed) = inbox.timed(&mut server, 14, "info", json!({"pageId":other}));
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
    let (url, elapsed) = inbox.timed(&mut server, 16, "info", json!({"pageId":stuck}));
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
    let (title, elapsed) = inbox.timed(&mut server, 102, "info", json!({"pageId":stuck}));
    assert_eq!(title["result"]["title"], "fixture", "{title}");
    assert!(elapsed < Duration::from_millis(1000), "{elapsed:?}");
    server.shutdown(103);
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
    let closed = server.request(13, "browser.close_tab", json!({"pageId":acting}));
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
            "actions.perform",
            json!({"pageId":page,"operation_id":operation,"action":{"kind":"fill","target":{"query":{"kind":"css","selector":"#missing"}},"value":value},"options":{"timeout_ms":500}}),
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
    let refused = server.request(12, "screenshot", json!({"pageId":page,"output":occupied}));
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
            "exec",
            json!({"pageId":page,"function":true,"code":"()=>{window.visibilityChanges=[];document.addEventListener('visibilitychange',()=>window.visibilityChanges.push(document.visibilityState))}"}),
        );
        assert!(watched.get("result").is_some(), "{watched}");
    }
    std::thread::sleep(Duration::from_secs(3));
    let mut inbox = Inbox::default();

    let (first, elapsed) = inbox.timed(
        &mut server,
        10,
        "screenshot",
        json!({"pageId":behind,"inline":true}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    let before = png(&first);
    let output = std::fs::canonicalize(root.path())
        .unwrap()
        .join("behind.png");
    let (saved, elapsed) = inbox.timed(
        &mut server,
        11,
        "screenshot",
        json!({"pageId":behind,"output":output}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    assert!(saved["result"]["bytes"].as_u64().unwrap() > 0, "{saved}");
    assert_eq!(&std::fs::read(&output).unwrap()[..8], PNG_SIGNATURE);
    let (ahead, elapsed) = inbox.timed(
        &mut server,
        12,
        "screenshot",
        json!({"pageId":front,"inline":true}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    png(&ahead);

    let started = Instant::now();
    server.send(json!({"id":13,"method":"screenshot","params":{"pageId":behind,"inline":true}}));
    server.send(json!({"id":14,"method":"screenshot","params":{"pageId":front,"inline":true}}));
    for id in [13, 14] {
        png(&inbox.take(&server, id, Duration::from_secs(40)).unwrap());
    }
    assert!(started.elapsed() < Duration::from_secs(10));

    let painted = server.request(
        15,
        "exec",
        json!({"pageId":behind,"function":true,"code":"()=>{document.body.style.background='rgb(200,0,0)'}"}),
    );
    assert!(painted.get("result").is_some(), "{painted}");
    std::thread::sleep(Duration::from_secs(3));
    let (repainted, elapsed) = inbox.timed(
        &mut server,
        16,
        "screenshot",
        json!({"pageId":behind,"inline":true}),
    );
    assert!(elapsed < prompt, "{elapsed:?}");
    assert_ne!(png(&repainted), before, "a stale frame was captured");
    let (whole, elapsed) = inbox.timed(
        &mut server,
        17,
        "screenshot",
        json!({"pageId":behind,"inline":true,"full_page":true,"format":"jpeg","quality":40}),
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
        "exec",
        json!({"pageId":behind,"function":true,"code":"()=>{let n=0;setInterval(()=>{document.body.style.background=++n%2?'rgb(0,0,200)':'rgb(0,200,0)'},100)}"}),
    );
    assert!(animated.get("result").is_some(), "{animated}");
    std::thread::sleep(Duration::from_millis(1500));
    let (during, elapsed) = inbox.timed(
        &mut server,
        32,
        "screenshot",
        json!({"pageId":behind,"inline":true}),
    );
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

#[test]
#[ignore = "requires Chrome"]
fn a_slow_operation_on_one_page_does_not_delay_reads_or_input_on_another() {
    let mut blocked = Blocked::current();
    let busy = blocked.page();
    let other = blocked.page();
    let slow = blocked.send(
        "eval",
        json!({"pageId":busy,"code":"async () => { await new Promise(done => setTimeout(done, 4000)); return 1; }"}),
    );
    std::thread::sleep(Duration::from_millis(300));
    for (method, params) in [
        ("info", json!({"pageId":other})),
        ("text", json!({"pageId":other})),
        ("content", json!({"pageId":other})),
        ("exists", json!({"pageId":other,"selector":"body"})),
        (
            "get_attribute",
            json!({"pageId":other,"selector":"html","name":"lang"}),
        ),
        ("frames", json!({"pageId":other})),
        (
            "wait",
            json!({"pageId":other,"selector":"body","state":"attached","timeout":1000}),
        ),
        ("mouse.move", json!({"pageId":other,"x":5,"y":5})),
        ("key.down", json!({"pageId":other,"key":"Shift"})),
        ("key.up", json!({"pageId":other,"key":"Shift"})),
        ("release_all_inputs", json!({"pageId":other})),
    ] {
        let (reply, elapsed) = blocked.call(method, params);
        assert!(reply.get("result").is_some(), "{method}: {reply}");
        assert!(
            elapsed < QUICK,
            "{method} waited {elapsed:?} behind another page"
        );
    }
    let finished = blocked
        .inbox
        .take(&blocked.server, slow, Duration::from_secs(10))
        .expect("the slow evaluation must finish");
    assert!(finished.get("result").is_some(), "{finished}");
    blocked.finish();
}
