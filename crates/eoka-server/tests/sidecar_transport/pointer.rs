use crate::support::*;

#[test]
#[ignore = "requires Chrome"]
fn pointer_actions_reach_a_tab_behind_another_promptly_and_it_stays_behind() {
    let prompt = Duration::from_millis(3000);
    let mut tabs = Blocked::serving(pointer_fixture());
    let (behind, front) = behind_and_front(&mut tabs, "/");
    let click = json!({"kind":"click","target":css("#go")});
    let clicked = shows("#outcome", "clicked");

    let (record, elapsed) = tabs.act(&behind, click.clone(), Some(clicked.clone()), 8000);
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    assert_eq!(record["receipt"]["dispatch"], "acknowledged", "{record}");
    assert!(elapsed < prompt, "click behind took {elapsed:?}");
    let (record, elapsed) = tabs.act(
        &behind,
        json!({"kind":"hover","target":css("#handle")}),
        None,
        8000,
    );
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    assert!(elapsed < prompt, "hover behind took {elapsed:?}");
    let (record, elapsed) = tabs.act(&front, click.clone(), Some(clicked.clone()), 8000);
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    assert!(elapsed < prompt, "click in front took {elapsed:?}");

    tabs.clear(&[&behind, &front]);
    let started = Instant::now();
    let together =
        [&behind, &front].map(|page| tabs.admit(page, click.clone(), Some(clicked.clone()), 8000));
    for operation in together {
        let record = tabs.ok(
            "operations.wait",
            json!({"id":operation,"timeout_ms":10_000}),
        );
        assert_eq!(record["outcome"], "verified_complete", "{record}");
    }
    assert!(
        started.elapsed() < Duration::from_millis(5000),
        "{:?}",
        started.elapsed()
    );

    tabs.clear(&[&behind]);
    for (name, action, expect) in [
        (
            "human click",
            json!({"kind":"click","target":css("#go"),"human":true}),
            Some(clicked.clone()),
        ),
        (
            "double click",
            json!({"kind":"click","target":css("#go"),"double":true}),
            Some(shows("#twice", "twice")),
        ),
        (
            "human hover",
            json!({"kind":"hover","target":css("#go"),"human":true}),
            None,
        ),
        (
            "drag",
            json!({"kind":"drag","target":css("#handle"),"dx":60,"dy":0}),
            Some(shows("#dragged", "dragged")),
        ),
        (
            "scroll into view",
            json!({"kind":"scroll_into_view","target":css("#far")}),
            None,
        ),
        (
            "click after scrolling",
            json!({"kind":"click","target":css("#far")}),
            Some(shows("#outcome", "far")),
        ),
    ] {
        let (record, elapsed) = tabs.act(&behind, action, expect, 8000);
        assert_eq!(record["outcome"], "verified_complete", "{name}: {record}");
        assert!(elapsed < prompt, "{name} behind took {elapsed:?}");
    }

    tabs.clear(&[&behind]);
    let typed = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    tabs.ok(
        "page.human_click",
        json!({"pageId":behind,"operation_id":typed,"selector":"#go",
            "options":{"expect":clicked,"timeout_ms":8000}}),
    );
    let record = tabs.ok("operations.wait", json!({"id":typed,"timeout_ms":9000}));
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    assert!(started.elapsed() < prompt, "{:?}", started.elapsed());

    tabs.clear(&[&behind]);
    for (method, params) in [
        (
            "agent.step",
            json!({"pageId":behind,"operation_id":uuid::Uuid::new_v4().to_string(),
                "action":{"kind":"click","target":"css:#go"},"expect":shows("#outcome", "clicked"),
                "action_timeout_ms":8000,"timeout_ms":8000}),
        ),
        (
            "agent.step",
            json!({"pageId":behind,"operation_id":uuid::Uuid::new_v4().to_string(),
                "action":{"kind":"hover","target":"css:#handle"},
                "action_timeout_ms":8000,"timeout_ms":8000}),
        ),
    ] {
        let (reply, elapsed) = tabs.call(method, params);
        assert_eq!(
            reply["result"]["status"], "verified_complete",
            "{method}: {reply}"
        );
        assert!(elapsed < prompt, "{method} behind took {elapsed:?}");
    }
    for (operation, params) in [
        (
            "click",
            json!({"pageId":behind,"target":"css:#far","expect":shows("#outcome", "far"),"timeout_ms":8000}),
        ),
        (
            "hover",
            json!({"pageId":behind,"target":"css:#far","timeout_ms":8000}),
        ),
        ("scroll", json!({"pageId":behind,"target":"css:#go"})),
    ] {
        let (reply, elapsed) = tabs.call(operation, params);
        assert_eq!(
            reply["result"]["outcome"], "verified_complete",
            "catalog {operation}: {reply}"
        );
        assert!(
            elapsed < prompt,
            "catalog {operation} behind took {elapsed:?}"
        );
    }
    for (method, params) in [
        ("mouse.move", json!({"pageId":behind,"x":40,"y":40})),
        ("mouse.move", json!({"pageId":behind,"x":60,"y":60})),
        (
            "click_shadow",
            json!({"pageId":behind,"selector":"#go","timeout_ms":8000}),
        ),
        ("click_shadow", json!({"pageId":behind,"selector":"#go"})),
        (
            "mouse.down",
            json!({"pageId":behind,"x":40,"y":40,"button":"left"}),
        ),
        (
            "mouse.up",
            json!({"pageId":behind,"x":40,"y":40,"button":"left"}),
        ),
        (
            "mouse.down",
            json!({"pageId":behind,"x":60,"y":60,"button":"left"}),
        ),
        (
            "mouse.up",
            json!({"pageId":behind,"x":60,"y":60,"button":"left"}),
        ),
        ("key.down", json!({"pageId":behind,"key":"Shift"})),
        ("key.up", json!({"pageId":behind,"key":"Shift"})),
        ("key.down", json!({"pageId":behind,"key":"Shift"})),
        ("key.up", json!({"pageId":behind,"key":"Shift"})),
    ] {
        let (reply, elapsed) = tabs.call(method, params);
        assert!(reply.get("result").is_some(), "{method}: {reply}");
        assert!(elapsed < prompt, "{method} behind took {elapsed:?}");
    }

    tabs.stops_painting(&behind);
    assert_stayed_behind(&mut tabs, &behind, &front);
    tabs.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn a_hidden_tab_is_given_frames_only_while_its_action_runs() {
    let mut tabs = Blocked::serving(pointer_fixture());
    let (behind, front) = behind_and_front(&mut tabs, "/");
    let missing = json!({"kind":"click","target":css("#missing")});
    let never = shows("#outcome", "never");
    assert!(!tabs.painting(&behind));

    let waiting = tabs.admit(&behind, missing.clone(), Some(never.clone()), 30_000);
    std::thread::sleep(Duration::from_millis(500));
    assert!(tabs.painting(&behind));
    let (shot, elapsed) = tabs.call("page.screenshot", json!({"pageId":behind}));
    png(&shot);
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    assert!(tabs.painting(&behind));
    tabs.ok("operations.cancel", json!({"id":waiting}));
    let record = tabs.ok("operations.wait", json!({"id":waiting,"timeout_ms":5000}));
    assert_eq!(record["error_code"], "operation_canceled", "{record}");
    tabs.stops_painting(&behind);

    tabs.evaluate(&behind, "()=>{window.later(12000);return 0}");
    let (record, elapsed) = tabs.act(
        &behind,
        json!({"kind":"click","target":css("#arrived")}),
        Some(shows("#outcome", "arrived")),
        25_000,
    );
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    assert!(elapsed > Duration::from_secs(10), "{elapsed:?}");
    tabs.stops_painting(&behind);

    let (record, elapsed) = tabs.act(
        &behind,
        json!({"kind":"click","target":css("#ask")}),
        Some(never.clone()),
        8000,
    );
    assert_eq!(record["error_code"], "dialog_open", "{record}");
    assert!(elapsed < Duration::from_millis(3000), "{elapsed:?}");
    tabs.ok("dialog.accept", json!({"pageId":behind}));
    tabs.stops_painting(&behind);
    assert_stayed_behind(&mut tabs, &behind, &front);

    tabs.evaluate(&front, "()=>{window.later(1500);return 0}");
    let covered = tabs.admit(
        &front,
        json!({"kind":"click","target":css("#arrived")}),
        Some(shows("#outcome", "arrived")),
        15_000,
    );
    let cover = tabs.page_at("/");
    assert_eq!(tabs.seen(&front)["now"], "hidden");
    let record = tabs.ok("operations.wait", json!({"id":covered,"timeout_ms":20_000}));
    assert_eq!(record["outcome"], "verified_complete", "{record}");

    let waiting = tabs.admit(&behind, missing, Some(never), 30_000);
    std::thread::sleep(Duration::from_millis(500));
    let (closed, elapsed) = tabs.call("page.close", json!({"pageId":behind}));
    assert!(closed.get("result").is_some(), "{closed}");
    assert!(elapsed < QUICK, "{elapsed:?}");
    let record = tabs.ok("operations.get", json!({"id":waiting}));
    assert_eq!(record["error_code"], "operation_canceled", "{record}");
    assert_eq!(tabs.seen(&cover)["now"], "visible");
    let (record, _) = tabs.act(
        &cover,
        json!({"kind":"click","target":css("#go")}),
        Some(shows("#outcome", "clicked")),
        8000,
    );
    assert_eq!(record["outcome"], "verified_complete", "{record}");
    tabs.finish();
}

#[test]
#[ignore = "requires Chrome"]
fn a_pointer_action_on_a_page_that_runs_no_frames_says_so() {
    let mut tabs = Blocked::serving(pointer_fixture());
    let (behind, front) = behind_and_front(&mut tabs, "/still");
    let click = json!({"kind":"click","target":css("#go")});
    let clicked = shows("#outcome", "clicked");

    let (record, _) = tabs.act(&behind, click.clone(), Some(clicked.clone()), 2000);
    assert_eq!(record["outcome"], "not_dispatched", "{record}");
    assert_eq!(record["error_code"], "action_timeout", "{record}");
    assert_eq!(record["receipt"]["dispatch"], "not_sent", "{record}");
    assert_eq!(
        record["error"]["context"]["phase"], "preparation",
        "{record}"
    );
    let message = record["error"]["message"].as_str().unwrap();
    assert!(
        message.starts_with("Timeout: Action deadline exceeded during preparation"),
        "{message}"
    );
    assert!(message.contains("the tab is hidden"), "{message}");
    assert!(message.contains("ran no animation frame"), "{message}");
    assert!(message.contains("a click waits two frames"), "{message}");
    assert!(
        record["error"]["hint"]
            .as_str()
            .unwrap()
            .contains("Bring this tab to the front"),
        "{record}"
    );

    let (record, _) = tabs.act(&front, click, Some(clicked), 2000);
    assert_eq!(record["outcome"], "not_dispatched", "{record}");
    let message = record["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("the page ran no animation frame"),
        "{message}"
    );
    assert!(!message.contains("hidden"), "{message}");

    let (record, _) = tabs.act(
        &behind,
        json!({"kind":"fill","target":css("#missing"),"value":"text"}),
        None,
        1500,
    );
    assert_eq!(record["outcome"], "not_dispatched", "{record}");
    let message = record["error"]["message"].as_str().unwrap();
    assert!(!message.contains("animation frame"), "{message}");
    assert!(record["error"].get("hint").is_none(), "{record}");
    assert_stayed_behind(&mut tabs, &behind, &front);
    tabs.finish();
}
