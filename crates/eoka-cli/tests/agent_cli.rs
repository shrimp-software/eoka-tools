mod support;

use serde_json::{json, Value};
use support::Session;

#[test]
#[ignore = "requires Chrome; cancellation while browser handler waits"]
fn cancellation_and_status_remain_responsive_during_fill_and_step() {
    let session = Session::agent("cancel-cli");
    session.run(&["open", "about:blank"]);
    for step in [false, true] {
        session.run(&[
            "exec",
            "()=>{document.body.innerHTML='<input aria-label=Name id=field>'}",
        ]);
        let id = uuid::Uuid::new_v4().to_string();
        let text = "x".repeat(400);
        let request = json!({"operation_id":id,"action":{"kind":"fill","target":session.run(&["view"])["controls"][0]["target"],"text":text},"timeout_ms":20000}).to_string();
        let mut command = if step {
            session.command(&["step", &request])
        } else {
            session.command(&["--operation-id", &id, "fill", "css:#field", &text])
        };
        let child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        loop {
            let (_, response, _) = session.output(&["operations", "get", &id]);
            if response["data"]["dispatch_intent"] == true {
                break;
            }
            assert!(
                start.elapsed().as_secs() < 5,
                "operation was not admitted: {response}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        session.run(&["operations", "cancel", &id]);
        assert!(
            start.elapsed().as_secs() < 5,
            "control requests waited for the fill"
        );
        let output = child.wait_with_output().unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["data"]["status"], "outcome_unknown", "{response}");
        let length = session
            .run(&["eval", "()=>field.value.length"])
            .as_u64()
            .unwrap();
        assert!(length < 400, "cancellation arrived after the complete fill");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(session.run(&["eval", "()=>field.value.length"]), length);
    }
}

#[test]
#[ignore = "requires Chrome; popup recovery after selected tab changes"]
fn popup_step_recovers_without_replaying_after_destination_selection() {
    let session = Session::agent("popup-recovery");
    session.run(&["open", "about:blank"]);
    session.run(&["exec", "()=>{window.hits=0;document.body.innerHTML='<button id=go>Go</button>';go.onclick=()=>{hits++;const child=window.open('about:blank');child.document.body.innerHTML='<button id=ready>Ready</button>'}}"]);
    let target = session.run(&["view"])["controls"][0]["target"].clone();
    let mut request = json!({"operation_id":uuid::Uuid::new_v4().to_string(),"action":{"kind":"click","target":target},"expect":{"kind":"visible","target":{"query":{"kind":"css","selector":"#ready"}}}});
    let first = session.run(&["step", &request.to_string()]);
    let second = session.run(&["step", &request.to_string()]);
    assert_eq!(first["operation"], second["operation"]);
    assert_eq!(
        first["observation"]["tab_id"],
        second["observation"]["tab_id"]
    );
    assert_eq!(
        first["observation"]["tabs"].as_array().unwrap().len(),
        second["observation"]["tabs"].as_array().unwrap().len()
    );
    request["expect"]["target"]["query"]["selector"] = json!("#different");
    let (_, response, _) = session.output(&["step", &request.to_string()]);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("operation_id_conflict"));
    session.run(&[
        "tab",
        "switch",
        first["operation"]["target_id"].as_str().unwrap(),
    ]);
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 1);
}

#[test]
#[ignore = "requires Chrome; shared JSON result validation"]
fn cli_rejects_lossy_function_results() {
    let session = Session::agent("json-cli");
    session.run(&["open", "about:blank"]);
    for code in [
        "()=>({missing:undefined})",
        "()=>({n:NaN})",
        "()=>9007199254740992",
        "()=>new Date(0)",
        "()=> 'é'.repeat(524288)",
    ] {
        let (output, response, _) = session.output(&["eval", code]);
        assert!(!output.status.success(), "{code}: {response}");
        assert_eq!(response["ok"], false);
    }
    session.run(&["exec", "()=>{const v={};v.self=v;return v}"]);
}

#[test]
#[ignore = "requires Chrome; CLI agent loop through SDK and daemon"]
fn view_step_and_recovery_work_through_the_actual_cli() {
    let session = Session::agent("agent-cli");
    session.run(&["open", "about:blank"]);
    session.run(&["exec", "--raw", "document.body.innerHTML='<button id=go>Save</button><p id=status>Ready</p>';window.hits=0;document.querySelector('#go').onclick=()=>{window.hits++;document.querySelector('#status').textContent='Saved'}"]);
    let view = session.run(&["view"]);
    let target = view["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|control| control["name"] == "Save")
        .unwrap()["target"]
        .clone();
    let request = json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "action": {"kind":"click", "target":target},
        "expect":{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}
    })
    .to_string();
    let result = session.run(&["step", &request]);
    assert_eq!(result["operation"]["outcome"], "verified_complete");
    assert!(result["observation"]["page"]["text_excerpt"]
        .as_str()
        .unwrap()
        .contains("Saved"));
    session.run(&["step", &request]);
    let view = session.run(&["view"]);
    assert!(view["controls"]
        .as_array()
        .unwrap()
        .iter()
        .any(|control| control["target"] == target));
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 1);
}

#[test]
#[ignore = "requires headed Chrome and DISPLAY; session and completion contracts"]
fn headed_session_is_reused_and_failed_actions_are_not_successful() {
    let session = Session::agent("identity");
    session.run(&["--headed", "open", "about:blank"]);
    session.run(&["exec", "()=>{window.marker=42;window.hits=0;document.body.innerHTML='<button onclick=hits++>Go</button>'}"]);
    assert_eq!(session.run(&["eval", "()=>window.marker"]), 42);
    let doctor = session.run(&["doctor"]);
    assert_eq!(doctor["daemon"]["mode"], "headed");
    assert_eq!(doctor["session"], session.name);
    let (_, result, _) = session.output(&["--headless", "info"]);
    assert_eq!(result["error_detail"]["code"], "session_mode_conflict");
    assert_eq!(session.run(&["eval", "()=>window.marker"]), 42);
    let view = session.run(&["view"]);
    let target = &view["controls"][0]["target"];
    session.run(&["snapshot"]);
    session.run(&["hover", target.as_str().unwrap()]);
    let request = json!({"action":{"kind":"click","target":target}}).to_string();
    let (output, result, _) = session.output(&["step", &request]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(result["data"]["status"], "not_dispatched");
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 0);
    let request = json!({"action":{"kind":"click","target":target},"mode":"explore"}).to_string();
    let (output, result, _) = session.output(&["step", &request]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(result["data"]["status"], "outcome_unknown");
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 1);
    let (output, result, _) =
        session.output(&["exec", "async()=>{throw new Error('exception-marker')}"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("exception-marker"));
}
