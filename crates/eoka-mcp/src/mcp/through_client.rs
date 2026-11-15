use super::surface::{call, connect, text, Client};
use super::EokaServer;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

async fn answer(client: &Client, tool: &str, arguments: Value) -> (bool, String) {
    match call(client, tool, arguments).await {
        Ok(result) => (result.is_error != Some(true), text(&result)),
        Err(error) => (false, error),
    }
}

async fn done(client: &Client, tool: &str, arguments: Value) -> String {
    let (ok, reply) = answer(client, tool, arguments).await;
    assert!(ok, "{tool} failed: {reply}");
    reply
}

async fn failed(client: &Client, tool: &str, arguments: Value) -> String {
    let (ok, reply) = answer(client, tool, arguments).await;
    assert!(!ok, "{tool} should have failed: {reply}");
    reply
}

fn data(reply: &str) -> Value {
    let parsed: Value =
        serde_json::from_str(reply).unwrap_or_else(|_| Value::String(reply.to_owned()));
    match parsed.get("data") {
        Some(data) if parsed.get("ok").is_some() => data.clone(),
        _ => parsed,
    }
}

async fn page_value(client: &Client, code: &str) -> Value {
    data(&done(client, "extract", json!({"code": code})).await)
}

fn tab_entries(listing: &str) -> Vec<String> {
    let lines: Vec<&str> = listing.lines().collect();
    let mut entries: Vec<String> = lines.chunks(2).map(|entry| entry.join("\n")).collect();
    entries.sort();
    entries
}

async fn finish(client: Client) {
    done(&client, "close", json!({})).await;
    let _ = client.cancel().await;
}

#[tokio::test]
#[ignore = "requires Chrome; MCP agent loop"]
async fn actions_work_without_ids_and_find_fields_by_label() {
    let client = connect(EokaServer::listing(false)).await;
    done(
        &client,
        "navigate",
        json!({"url": "data:text/html,<label>Name<input id=name></label><button id=go onclick=\"document.body.insertAdjacentHTML('beforeend','<p id=done>Done</p>')\">Join</button>"}),
    )
    .await;

    let fill = data(&done(&client, "fill", json!({"target": "Name", "text": "Ada"})).await);
    assert_eq!(fill["outcome"], "verified_complete", "{fill}");
    assert!(uuid::Uuid::parse_str(fill["id"].as_str().unwrap()).is_ok());
    assert_eq!(
        page_value(&client, "() => document.querySelector('#name').value").await,
        "Ada"
    );

    let unverified = data(&done(&client, "click", json!({"target": "Join"})).await);
    assert_eq!(unverified["status"], "outcome_unknown", "{unverified}");
    assert!(unverified["note"]
        .as_str()
        .unwrap()
        .contains("not verified"));
    assert_eq!(
        page_value(&client, "() => !!document.querySelector('#done')").await,
        true
    );
    done(
        &client,
        "exec",
        json!({"code": "() => { document.querySelector('#done').remove() }"}),
    )
    .await;

    let click = data(
        &done(
            &client,
            "click",
            json!({
                "target": "Join",
                "expect": {"kind": "visible", "target": {"query": {"kind": "css", "selector": "#done"}}}
            }),
        )
        .await,
    );
    assert_eq!(click["outcome"], "verified_complete", "{click}");
    let step = data(
        &done(
            &client,
            "agent_step",
            json!({"action": {"kind": "fill", "target": "Name", "text": "Grace"}}),
        )
        .await,
    );
    assert_eq!(step["operation"]["outcome"], "verified_complete", "{step}");
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; MCP agent loop"]
async fn mcp_agent_step_returns_fresh_state_and_preserves_unchanged_targets() {
    let client = connect(EokaServer::listing(false)).await;
    done(
        &client,
        "navigate",
        json!({"url": "data:text/html,<label>Name<input id=name></label><button id=go>Save</button><p id=status></p>"}),
    )
    .await;
    done(
        &client,
        "exec",
        json!({"code": "() => { window.hits = 0; document.querySelector('#go').onclick = () => { window.hits++; document.querySelector('#status').textContent = 'Saved' } }"}),
    )
    .await;
    let view = data(&done(&client, "agent_observe", json!({})).await);
    let first_tab = view["tab_id"].as_str().unwrap().to_owned();
    let controls = view["controls"].as_array().unwrap();
    let field = controls
        .iter()
        .find(|control| control["name"] == "Name")
        .unwrap()["target"]
        .clone();
    let button = controls
        .iter()
        .find(|control| control["name"] == "Save")
        .unwrap()["target"]
        .clone();
    let fill = data(
        &done(
            &client,
            "agent_step",
            json!({"action": {"kind": "fill", "target": field, "text": "Ada"}}),
        )
        .await,
    );
    assert_eq!(fill["operation"]["outcome"], "verified_complete", "{fill}");
    assert!(fill["observation"]["controls"]
        .as_array()
        .unwrap()
        .iter()
        .any(|control| control["target"] == button));
    let click = data(
        &done(
            &client,
            "agent_step",
            json!({
                "action": {"kind": "click", "target": button},
                "expect": {"kind": "text", "target": {"query": {"kind": "css", "selector": "#status"}}, "value": "Saved"}
            }),
        )
        .await,
    );
    assert_eq!(
        click["operation"]["outcome"], "verified_complete",
        "{click}"
    );
    assert!(click["operation_id"].as_str().is_some());
    assert!(click["observation"]["page"]["text_excerpt"]
        .as_str()
        .unwrap()
        .contains("Saved"));
    assert_eq!(page_value(&client, "() => window.hits").await, 1);

    done(
        &client,
        "exec",
        json!({"code": "() => { document.querySelector('#go').onclick = () => { window.hits++; const child = window.open('about:blank'); child.document.body.innerHTML = '<button id=ready>Ready</button>' } }"}),
    )
    .await;
    let request = json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "action": {"kind": "click", "target": button},
        "expect": {"kind": "visible", "target": {"query": {"kind": "css", "selector": "#ready"}}}
    });
    let first = data(&done(&client, "agent_step", request.clone()).await);
    let second = data(&done(&client, "agent_step", request).await);
    assert_eq!(first["operation"], second["operation"]);
    assert_eq!(
        first["observation"]["tab_id"],
        second["observation"]["tab_id"]
    );
    assert_ne!(first["observation"]["tab_id"], first_tab.as_str());
    done(&client, "switch_tab", json!({"tab_id": first_tab})).await;
    assert_eq!(page_value(&client, "() => window.hits").await, 2);
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; catalog tools run on the MCP session's current tab"]
async fn catalog_tools_share_the_mcp_browser_and_current_tab() {
    let client = connect(EokaServer::listing(false)).await;
    failed(&client, "content", json!({})).await;
    done(
        &client,
        "navigate",
        json!({"url": "data:text/html,<title>one</title><h1 id='h'>Heading</h1><a href='/next'>next</a>"}),
    )
    .await;
    let content = done(&client, "content", json!({})).await;
    assert!(content.contains("Heading"), "{content}");
    let attribute = done(
        &client,
        "get_attribute",
        json!({"selector": "a", "name": "href"}),
    )
    .await;
    assert!(attribute.contains("/next"), "{attribute}");
    failed(&client, "get_text", json!({})).await;
    let absent = failed(&client, "get_text", json!({"selector": "#absent"})).await;
    assert!(absent.contains("ElementNotFound"), "{absent}");
    let reload = done(&client, "reload", json!({})).await;
    assert!(reload.contains("verified_complete"), "{reload}");
    let info = done(&client, "page_info", json!({})).await;
    assert!(info.contains("one"), "{info}");
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; durable navigation diagnostic"]
async fn intercepted_click_keeps_durable_identity_and_does_not_replay() {
    let client = connect(EokaServer::listing(false)).await;
    let navigation = uuid::Uuid::new_v4().to_string();
    done(
        &client,
        "navigate",
        json!({
            "operation_id": navigation,
            "url": "data:text/html,<button id='save' onclick=\"window.clicks=(window.clicks||0)+1;history.pushState({},'',String.fromCharCode(35)+'report');document.getElementById('status').textContent='Saved'\">Save</button><p id='status'></p>"
        }),
    )
    .await;
    let navigated = done(
        &client,
        "operation_wait",
        json!({"id": navigation, "timeout_ms": 30000}),
    )
    .await;
    assert!(navigated.contains("verified_complete"), "{navigated}");
    let id = uuid::Uuid::new_v4().to_string();
    for _ in 0..2 {
        done(
            &client,
            "click_intercept_nav",
            json!({
                "target": "css:#save",
                "allow_nav": false,
                "operation_id": id,
                "expect": {"kind": "text", "target": {"query": {"kind": "css", "selector": "#status"}}, "value": "Saved"},
                "timeout_ms": 3000
            }),
        )
        .await;
        let waited = done(
            &client,
            "operation_wait",
            json!({"id": id, "timeout_ms": 3000}),
        )
        .await;
        assert!(waited.contains("verified_complete"), "{waited}");
    }
    let result = page_value(
        &client,
        "() => ({clicks: window.clicks, nav: window.__eoka_nav, hash: location.hash})",
    )
    .await;
    assert_eq!(result["clicks"], 1, "{result}");
    assert_eq!(result["nav"]["url"], "#report", "{result}");
    assert_eq!(result["hash"], "", "{result}");
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; MCP observation and download evidence"]
async fn mcp_observation_and_download_failures_share_page_context() {
    let client = connect(EokaServer::listing(false)).await;
    let navigation = uuid::Uuid::new_v4().to_string();
    done(
        &client,
        "navigate",
        json!({
            "operation_id": navigation,
            "url": "data:text/html,<title>MCP fixture</title><h1>Unavailable</h1><p role='alert'>Transfer unavailable</p><button>Features</button>"
        }),
    )
    .await;
    let navigated = done(
        &client,
        "operation_wait",
        json!({"id": navigation, "timeout_ms": 30000}),
    )
    .await;
    assert!(navigated.contains("verified_complete"), "{navigated}");
    let structured = data(&done(&client, "observe", json!({"structured": true, "max": 1})).await);
    let plain = done(&client, "observe", json!({})).await;
    let refused = failed(
        &client,
        "download_start",
        json!({
            "click": "text:Download",
            "output": std::env::temp_dir().join("eoka-mcp-not-created.bin"),
            "start_timeout_ms": 1000
        }),
    )
    .await;
    finish(client).await;
    assert_eq!(structured["page"]["title"], "MCP fixture", "{structured}");
    assert_eq!(structured["page"]["messages"][0], "Transfer unavailable");
    assert_eq!(structured["elements"].as_array().unwrap().len(), 1);
    assert!(
        plain.contains("Page message: Transfer unavailable"),
        "{plain}"
    );
    let refused: Value = serde_json::from_str(&refused).expect(&refused);
    let context = &refused["error_detail"]["context"];
    assert_eq!(context["action_status"], "not_attempted", "{refused}");
    assert_eq!(
        context["page"]["messages"],
        json!(["Transfer unavailable"]),
        "{refused}"
    );
}

#[tokio::test]
#[ignore = "requires Chrome; MCP function execution contract"]
async fn mcp_functions_preserve_arguments_exceptions_and_limits() {
    let client = connect(EokaServer::listing(false)).await;
    done(&client, "navigate", json!({"url": "about:blank"})).await;
    for _ in 0..2 {
        let value = data(
            &done(
                &client,
                "extract",
                json!({
                    "code": "value=>{const local=value;return local}",
                    "arguments": [{"quoted": "é😀\""}]
                }),
            )
            .await,
        );
        assert_eq!(value["quoted"], "é😀\"", "{value}");
    }
    let thrown = failed(
        &client,
        "exec",
        json!({"code": "async()=>{throw new Error('mcp-fixture-exception')}"}),
    )
    .await;
    assert!(thrown.contains("mcp-fixture-exception"), "{thrown}");
    for code in [
        "()=>({missing:undefined})",
        "()=>({n:NaN})",
        "()=>9007199254740992",
        "()=>new Date(0)",
        "()=> 'é'.repeat(524288)",
    ] {
        failed(&client, "extract", json!({"code": code})).await;
    }
    let oversized = failed(
        &client,
        "extract",
        json!({"code": "()=> 'oversized'", "max_size": 2}),
    )
    .await;
    assert!(oversized.contains("byte limit"), "{oversized}");
    let unawaited = failed(&client, "exec", json!({"code": "()=>0", "no_await": true})).await;
    assert!(unawaited.contains("requires raw"), "{unawaited}");
    let (_, unknown) = answer(
        &client,
        "exec",
        json!({"code": "Promise.resolve(0)", "raw": true, "no_await": true}),
    )
    .await;
    assert!(unknown.contains("outcome_unknown"), "{unknown}");
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; a dialog or a silent server is diagnosed instead of timing out"]
async fn blocked_tabs_are_diagnosed_and_never_relaunch_the_browser() {
    let client = connect(EokaServer::listing(false)).await;
    let first = "data:text/html,<title>one</title><h1>Heading</h1>";
    done(&client, "navigate", json!({"url": first})).await;
    let tabs = done(&client, "list_tabs", json!({})).await;
    answer(
        &client,
        "exec",
        json!({"code": "setTimeout(() => alert('saved'), 100); 0", "raw": true}),
    )
    .await;
    let opened = Instant::now();
    let refused = loop {
        let (ok, reply) = answer(&client, "page_info", json!({})).await;
        if !ok {
            break reply;
        }
        assert!(
            opened.elapsed() < Duration::from_secs(5),
            "the dialog was never reported"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(refused.contains("dialog_open"), "{refused}");
    assert!(refused.contains("accept_dialog"), "{refused}");
    assert!(refused.contains("saved"), "{refused}");
    done(&client, "dismiss_dialog", json!({})).await;
    done(&client, "page_info", json!({})).await;

    let started = Instant::now();
    let interrupted = failed(
        &client,
        "extract",
        json!({"code": "() => new Promise(() => setTimeout(() => alert('later'), 300))"}),
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "a read in flight must be interrupted by the dialog"
    );
    assert!(interrupted.contains("dialog_open"), "{interrupted}");
    done(&client, "dismiss_dialog", json!({})).await;
    done(&client, "page_info", json!({})).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let silent = format!("http://{}/", listener.local_addr().unwrap());
    let held = tokio::spawn(async move {
        let mut connections = Vec::new();
        while let Ok((connection, _)) = listener.accept().await {
            connections.push(connection);
        }
    });
    let started = Instant::now();
    let (_, stalled) = answer(
        &client,
        "navigate",
        json!({"url": silent, "timeout_ms": 2000}),
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(15));
    assert!(stalled.contains("navigation_timeout"), "{stalled}");
    done(&client, "navigate", json!({"url": first})).await;
    let info = done(&client, "page_info", json!({})).await;
    assert!(info.contains("one"), "{info}");
    assert_eq!(
        tab_entries(&done(&client, "list_tabs", json!({})).await),
        tab_entries(&tabs),
        "the same browser and tab are still in use"
    );
    held.abort();
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; a dialog that opens before any page tool ran is still named"]
async fn a_dialog_that_opens_before_the_first_page_tool_is_reported() {
    let client = connect(EokaServer::listing(false)).await;
    done(
        &client,
        "new_tab",
        json!({"url": "data:text/html,<title>early</title><h1>Early</h1><script>setTimeout(() => alert('early'), 400)</script>"}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let started = Instant::now();
    let refused = failed(&client, "page_text", json!({})).await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the open dialog was not known in advance: {refused}"
    );
    assert!(refused.contains("dialog_open"), "{refused}");
    assert!(refused.contains("early"), "{refused}");
    done(&client, "accept_dialog", json!({})).await;
    let read = done(&client, "page_text", json!({})).await;
    assert!(read.contains("Early"), "{read}");
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; operations can be listed and cancelled while a page tool is running"]
async fn operation_tools_answer_while_another_tool_holds_the_browser() {
    let client = connect(EokaServer::listing(false)).await;
    done(
        &client,
        "navigate",
        json!({"url": "data:text/html,<title>one</title><p>text</p>"}),
    )
    .await;
    let waiting = answer(
        &client,
        "wait_for_text",
        json!({"text": "never shown", "timeout_ms": 8000}),
    );
    let asked = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let started = Instant::now();
        let listed = answer(&client, "operation_list", json!({"limit": 5})).await;
        let downloads = answer(&client, "download_list", json!({})).await;
        (listed, downloads, started.elapsed())
    };
    let (waited, (listed, downloads, elapsed)) = tokio::join!(waiting, asked);
    assert!(!waited.0, "{}", waited.1);
    assert!(listed.0, "{}", listed.1);
    assert!(downloads.0, "{}", downloads.1);
    assert!(
        elapsed < Duration::from_secs(3),
        "operation and download queries waited {elapsed:?} for the page tool"
    );
    finish(client).await;
}

#[tokio::test]
#[ignore = "requires Chrome; a browser launched after close starts without the old network recording"]
async fn a_relaunched_browser_does_not_inherit_the_old_network_recording() {
    let client = connect(EokaServer::listing(true)).await;
    let page = "data:text/html,<title>one</title><p>text</p>";
    done(&client, "navigate", json!({"url": page})).await;
    done(&client, "network_record_start", json!({})).await;
    done(&client, "reload", json!({})).await;
    let recording = data(&done(&client, "network_record_status", json!({})).await);
    assert_eq!(recording["status"]["active"], true, "{recording}");
    done(&client, "close", json!({})).await;

    done(&client, "navigate", json!({"url": page})).await;
    let relaunched = data(&done(&client, "network_record_status", json!({})).await);
    assert_eq!(relaunched["status"]["active"], false, "{relaunched}");
    assert_eq!(relaunched["status"]["entry_count"], 0, "{relaunched}");
    done(&client, "network_record_start", json!({})).await;
    let again = data(&done(&client, "network_record_status", json!({})).await);
    assert_eq!(again["status"]["active"], true, "{again}");
    finish(client).await;
}
