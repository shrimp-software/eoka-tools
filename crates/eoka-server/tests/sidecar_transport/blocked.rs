use crate::support::*;

#[test]
#[ignore = "requires Chrome"]
fn an_open_dialog_is_reported_at_once_and_clearing_it_frees_the_tab() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.open_alert(&page);
    for (method, params) in [
        ("info", json!({"pageId":page})),
        (
            "eval",
            json!({"pageId":page,"function":true,"code":"()=>1"}),
        ),
        ("get_text", json!({"pageId":page,"selector":"h1"})),
        ("exists", json!({"pageId":page,"selector":"h1"})),
        ("screenshot", json!({"pageId":page,"inline":true})),
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
        "eval",
        json!({"pageId":page,"function":true,"code":"()=>confirm('are you sure?')"}),
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
            .contains("`eval` returned without a result"),
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
    let text = blocked.ok("get_text", json!({"pageId":page,"selector":"#s"}));
    assert_eq!(text, "Deleted", "{text}");
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
        "eval",
        json!({"pageId":page,"function":true,"code":"()=>window.clicks"}),
    );
    assert_eq!(clicks, 1, "{clicks}");
    let stored = blocked.ok("operations.get", json!({"id":operation}));
    assert_eq!(stored["error_code"], "dialog_open", "{stored}");
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn a_navigation_the_page_started_is_named_when_it_stalls_and_a_new_one_recovers_the_tab() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.stall_navigation(&page);
    std::thread::sleep(Duration::from_millis(500));
    let (early, elapsed) = blocked.call("info", json!({"pageId":page}));
    assert_navigation_pending(&early);
    assert!(elapsed < Duration::from_millis(8000), "{elapsed:?}");
    assert!(elapsed > Duration::from_millis(3000), "{elapsed:?}");
    for (method, params) in [
        ("info", json!({"pageId":page})),
        (
            "eval",
            json!({"pageId":page,"function":true,"code":"()=>1"}),
        ),
        ("exists", json!({"pageId":page,"selector":"h1"})),
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
        "captcha.turnstile",
        json!({"pageId":page,"timeout_ms":20_000}),
    );
    let queued = blocked.send("emulate", json!({"pageId":page,"reset":true}));
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
        "captcha.turnstile",
        json!({"pageId":page,"timeout_ms":20_000}),
    );
    std::thread::sleep(Duration::from_millis(300));
    let (closed, elapsed) = blocked.call("browser.close_tab", json!({"pageId":page}));
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
    let (gone, _) = blocked.call("info", json!({"pageId":page}));
    assert_eq!(gone["error"]["code"], "InvalidPage", "{gone}");
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome; waits for the 15 second read limit"]
fn a_page_whose_main_thread_is_busy_is_reported_as_unresponsive() {
    let mut blocked = Blocked::current();
    let other = blocked.page();
    let page = blocked.page();
    blocked.ok(
        "exec",
        json!({"pageId":page,"function":true,"code":"()=>{setTimeout(()=>{while(true){}},0)}"}),
    );
    std::thread::sleep(Duration::from_millis(300));
    let (reply, elapsed) = blocked.call("info", json!({"pageId":page}));
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
    let (closed, _) = blocked.call("browser.close_tab", json!({"pageId":page}));
    assert!(closed.get("result").is_some(), "{closed}");
    blocked.finish();
}

#[test]
#[ignore = "requires Chrome; staying on a page frees the tab for the next click"]
fn dismissing_a_leave_page_dialog_frees_the_tab_for_the_next_click() {
    let mut blocked = Blocked::serving(leave_fixture());
    let page = blocked.page_at("/leave");
    let (filled, _) = blocked.act(
        &page,
        json!({"kind":"fill","target":css("#note"),"value":"draft"}),
        None,
        5000,
    );
    assert_eq!(filled["outcome"], "verified_complete", "{filled}");
    let leave = json!({"kind":"click","target":css("#summary")});
    let expect = json!({"kind":"url","value":"/summary","contains":true});
    let (first, _) = blocked.act(&page, leave.clone(), Some(expect.clone()), 8000);
    assert_eq!(first["error_code"], "dialog_open", "{first}");
    let (dismissed, elapsed) = blocked.call("dialog.dismiss", json!({"pageId":page}));
    assert!(dismissed.get("result").is_some(), "{dismissed}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    let (again, elapsed) = blocked.act(&page, leave, Some(expect), 8000);
    assert_eq!(again["error_code"], "dialog_open", "{again}");
    assert!(elapsed < Duration::from_secs(4), "{elapsed:?}: {again}");
    let (accepted, _) = blocked.call("dialog.accept", json!({"pageId":page}));
    assert_eq!(accepted["result"], "Dialog accepted", "{accepted}");
    blocked.ok(
        "wait",
        json!({"pageId":page,"text":"Summary","timeout":5000}),
    );
    let title = blocked.ok("info", json!({"pageId":page}));
    assert_eq!(title["title"], "summary", "{title}");
    blocked.finish();
}
