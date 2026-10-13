use eoka_runner::{Config, Runner};
use serde_json::json;

#[tokio::test]
#[ignore = "requires Chrome; runner action routing and Unicode logging"]
async fn durable_workflow_handles_unicode_debug_logs_and_large_scrolls() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .unwrap();
    let unicode = "😀".repeat(20);
    let html = "<input id='name'><select id='choice'><option value='a'>First</option><option value='b'>Second</option></select><button id='save' onclick=\"window.hits=(window.hits||0)+1;document.getElementById('status').textContent='Saved'\">Save</button><p id='status'>Pending</p>";
    let config = Config::parse(&json!({
        "name": "Durable runner regression",
        "browser": {"headless": true},
        "target": {"url": format!("data:text/html,{html}")},
        "actions": [
            {"fill": {"selector": "#name", "value": "Ada"}},
            {"type": {"selector": "#name", "value": " Lovelace", "expect": {
                "kind": "value", "target": {"query": {"kind": "css", "selector": "#name"}}, "value": "Ada Lovelace"
            }}},
            {"select": {"selector": "#choice", "value": "Second"}},
            {"hover": {"selector": "#save"}},
            {"click": {"selector": "#save", "expect": {
                "kind": "text", "target": {"query": {"kind": "css", "selector": "#status"}}, "value": "Saved"
            }}},
            {"execute": {"js": format!("window.note='{unicode}'")}},
            {"scroll": {"direction": "down", "amount": i32::MAX}},
            {"scroll_to": {"selector": "#save"}}
        ],
        "success": {"all": [{"text_contains": "Saved"}]}
    }).to_string()).unwrap();
    let mut runner = Runner::new(&config.browser).await.unwrap();
    let result = runner.run(&config).await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.actions_executed, 8);
    assert_eq!(result.retries, 0);
    let state: serde_json::Value = runner.page().evaluate(
        "({name:document.getElementById('name').value,choice:document.getElementById('choice').value,hits:window.hits,note:window.note})"
    ).await.unwrap();
    assert_eq!(state["name"], "Ada Lovelace");
    assert_eq!(state["choice"], "b");
    assert_eq!(state["hits"], 1);
    assert_eq!(state["note"], unicode);
    runner.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome; disabled retry and zero-attempt behavior"]
async fn retry_settings_cannot_replay_a_workflow() {
    let mut config = Config::parse(
        &json!({
            "name": "Retry policy regression",
            "browser": {"headless": true},
            "target": {"url": "data:text/html,unexpected navigation"},
            "on_failure": {"retry": {"attempts": 1, "delay_ms": 1}}
        })
        .to_string(),
    )
    .unwrap();
    config
        .on_failure
        .as_mut()
        .unwrap()
        .retry
        .as_mut()
        .unwrap()
        .attempts = 0;
    let mut runner = Runner::new(&config.browser).await.unwrap();
    let result = runner.run(&config).await.unwrap();
    assert!(!result.success);
    assert!(result.error.is_none());
    assert_eq!(result.actions_executed, 0);
    assert_eq!(runner.page().url().await.unwrap(), "about:blank");
    config
        .on_failure
        .as_mut()
        .unwrap()
        .retry
        .as_mut()
        .unwrap()
        .attempts = 2;
    assert!(runner
        .run(&config)
        .await
        .unwrap_err()
        .to_string()
        .contains("Whole-workflow retries are disabled"));
    assert_eq!(runner.page().url().await.unwrap(), "about:blank");
    runner.close().await.unwrap();
}
