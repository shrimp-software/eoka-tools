use crate::support::*;

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
        let legacy = server.request(id + 10, "info", json!({"pageId":page}));
        assert_eq!(legacy["result"]["title"], title, "{legacy}");
    }

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
    assert_eq!(missing["error"]["code"], "Timeout", "{missing}");
    let absent = server.request(49, "get_text", json!({"pageId":first,"selector":"#absent"}));
    assert_eq!(absent["error"]["code"], "ElementNotFound", "{absent}");
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
        title = server.request(62 + attempt, "info", json!({"pageId":shadow}));
        if title["result"]["title"] == "shadow-clicked" {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(title["result"]["title"], "shadow-clicked", "{title}");

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
    let scrolled = server.request(105, "scroll", json!({"pageId":shadow,"target":reference}));
    assert!(scrolled.get("result").is_some(), "{scrolled}");
    let stale = server.request(106, "scroll", json!({"pageId":shadow,"target":"@e999"}));
    assert!(stale.get("error").is_some(), "{stale}");

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

#[test]
#[ignore = "requires Chrome; calls every projected catalog operation once"]
fn every_catalog_operation_answers_through_the_sidecar() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &root.path().join("profile"));
    let origin = format!("http://{}", fixture_server());
    let file = |name: &str| scratch.join(name).to_string_lossy().into_owned();
    let status = json!({"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"clicked"});

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
            Some(code @ ("UnknownMethod" | "InvalidParams" | "Internal" | "InvalidPage")) => {
                problems.push(format!("{method} failed with {code}: {summary}"))
            }
            Some(_) if must_succeed => problems.push(format!("{method} failed: {summary}")),
            Some(_) => {}
        }
        if must_succeed && response["result"]["outcome"].is_string() {
            let outcome = response["result"]["outcome"].as_str().unwrap();
            let unverified_by_design = matches!(method, "double_click" | "key");
            if outcome != "verified_complete" && !unverified_by_design {
                problems.push(format!("{method} ended {outcome}: {summary}"));
            }
        }
    }
    let title = server.request(5000, "info", json!({"pageId":page}));
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
    let url = borrower.request(5, "info", json!({"pageId":page}));
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
#[ignore = "requires Chrome; locks what every hand-written sidecar method answers"]
fn hand_written_sidecar_methods_match_the_lock() {
    let (root, state, scratch) = directories();
    let mut server = Sidecar::start(&state, &scratch);
    let page = launch(&mut server, &root.path().join("profile"));
    let origin = format!("http://{}", fixture_server());
    let goto = server.request(3, "open", json!({"pageId":page,"url":format!("{origin}/")}));
    assert!(goto.get("result").is_some(), "{goto}");
    let shot = scratch.join("shot.png").to_string_lossy().into_owned();
    let p = page.as_str();
    let calls: Vec<(&str, Value)> = vec![
        ("text", json!({"pageId":p})),
        ("text", json!({"pageId":"no-such-page"})),
        ("content", json!({"pageId":p})),
        ("info", json!({"pageId":p})),
        ("info", json!({"pageId":p})),
        ("get_text", json!({"pageId":p,"selector":"h1"})),
        ("get_text", json!({"pageId":p,"selector":"#missing"})),
        (
            "get_attribute",
            json!({"pageId":p,"selector":"#link","name":"href"}),
        ),
        (
            "get_attribute",
            json!({"pageId":p,"selector":"#link","name":"nope"}),
        ),
        ("exists", json!({"pageId":p,"selector":"h1"})),
        ("exists", json!({"pageId":p,"selector":"#missing"})),
        (
            "wait",
            json!({"pageId":p,"selector":"h1","state":"attached","timeout":2000}),
        ),
        (
            "wait",
            json!({"pageId":p,"selector":"#missing","state":"attached","timeout":300}),
        ),
        (
            "wait",
            json!({"pageId":p,"selector":"h1","state":"visible","timeout":2000}),
        ),
        ("wait", json!({"pageId":p,"text":"Heading","timeout":2000})),
        (
            "wait",
            json!({"pageId":p,"text":"absent words","timeout":300}),
        ),
        (
            "page.evaluate",
            json!({"pageId":p,"function":"(a, b) => a + b","arguments":[2,3]}),
        ),
        (
            "page.evaluate",
            json!({"pageId":p,"function":"() => { throw new Error('boom') }"}),
        ),
        (
            "page.execute",
            json!({"pageId":p,"function":"() => { document.title = 'fixture' }"}),
        ),
        (
            "page.evaluate_raw",
            json!({"pageId":p,"js":"({a: 1, b: [true, null]})"}),
        ),
        (
            "page.evaluate_raw",
            json!({"pageId":p,"js":"undefinedName.x"}),
        ),
        ("page.execute_raw", json!({"pageId":p,"js":"void 0"})),
        ("frames", json!({"pageId":p})),
        (
            "frame_eval",
            json!({"pageId":p,"frame":"id:no-such-frame","code":"1"}),
        ),
        (
            "click_shadow",
            json!({"pageId":p,"selector":"#missing","timeout_ms":300}),
        ),
        ("captcha.turnstile", json!({"pageId":p,"timeout_ms":300})),
        (
            "page.fetch",
            json!({"pageId":p,"url":format!("{origin}/data")}),
        ),
        (
            "page.fetch_bounded",
            json!({"pageId":p,"url":format!("{origin}/data"),"maxBytes":1}),
        ),
        ("page.capture_state", json!({"pageId":p})),
        ("page.screenshot", json!({"pageId":p})),
        (
            "page.screenshot",
            json!({"pageId":p,"format":"png","quality":5}),
        ),
        ("page.screenshot_to_file", json!({"pageId":p,"path":shot})),
        (
            "page.set_viewport",
            json!({"pageId":p,"width":640,"height":360,"deviceScaleFactor":2.0}),
        ),
        ("page.reset_viewport", json!({"pageId":p})),
        ("mouse.move", json!({"pageId":p,"x":10,"y":10})),
        (
            "mouse.down",
            json!({"pageId":p,"x":10,"y":10,"button":"left"}),
        ),
        (
            "mouse.up",
            json!({"pageId":p,"x":10,"y":10,"button":"left"}),
        ),
        ("key.down", json!({"pageId":p,"key":"Shift"})),
        ("key.up", json!({"pageId":p,"key":"Shift"})),
        ("release_all_inputs", json!({"pageId":p})),
        (
            "page.solve_captcha",
            json!({"pageId":p,"apiKey":"k","captchaType":"nope","captchaMode":"token","websiteKey":"w","websiteURL":"http://127.0.0.1/"}),
        ),
        ("dialog.accept", json!({"pageId":p})),
        ("dialog.dismiss", json!({"pageId":p})),
        ("browser.tabs", json!({})),
        ("browser.close_tab", json!({"pageId":"no-such-page"})),
        ("page.close", json!({"pageId":"no-such-page"})),
    ];
    let root_path = root.path().to_string_lossy().into_owned();
    let mut rendered = String::new();
    for (index, (method, params)) in calls.into_iter().enumerate() {
        let reply = server.request(100 + index as u64, method, params.clone());
        let answer = reply
            .get("result")
            .map(|result| json!({"result": result}))
            .unwrap_or_else(|| json!({"error": reply["error"]}));
        let row = json!({
            "method": method,
            "params": masked(&params, &page, &root_path, &origin),
            "reply": masked(&answer, &page, &root_path, &origin),
        });
        rendered.push_str(&serde_json::to_string(&row).unwrap());
        rendered.push('\n');
    }
    let lock = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/locks/sidecar-replies.jsonl");
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, &rendered).unwrap();
        return;
    }
    let locked = std::fs::read_to_string(&lock).unwrap_or_default();
    let changed: Vec<String> = locked
        .lines()
        .zip(rendered.lines())
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("- {a}\n+ {b}"))
        .collect();
    assert!(
        locked == rendered,
        "Sidecar replies changed. If that is intended, rerun with EOKA_UPDATE_LOCKS=1 and list it in the pull request.\n{}",
        changed.join("\n")
    );
}
