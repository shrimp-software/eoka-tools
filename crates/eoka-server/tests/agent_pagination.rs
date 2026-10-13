use eoka_protocol::{AgentObserveArgs, AgentStepArgs};
use eoka_server::{agent::AgentSession, eoka::Browser, operations::OperationManager};
use serde_json::{json, Value};

#[tokio::test]
#[ignore = "requires Chrome; paginated agent observation"]
async fn controls_beyond_the_limit_remain_actionable_without_replaying_steps() {
    let browser = Browser::launch_with(|c| {
        c.headless = true;
        c.geo_align = false;
    })
    .await
    .unwrap();
    let page = browser.new_blank_page().await.unwrap();
    page.execute("document.body.innerHTML=Array.from({length:250},(_,i)=>`<button style='display:block' id='button-${i}' onclick='window.hits++'>Button ${i}</button>`).join('');window.hits=0").await.unwrap();
    let mut agent = AgentSession::default();
    let first = agent
        .observe(
            &browser,
            &page,
            serde_json::from_value::<AgentObserveArgs>(json!({"max_controls":200})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.controls.len(), 200);
    assert_eq!(serde_json::to_value(&first).unwrap()["next_offset"], 200);
    let omitted = first.controls[0].target.clone();
    page.execute("document.querySelector('#button-249').scrollIntoView()")
        .await
        .unwrap();
    let last = agent
        .observe(
            &browser,
            &page,
            serde_json::from_value::<AgentObserveArgs>(json!({"offset":200,"max_controls":50}))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(last.controls.len(), 50);
    assert_eq!(last.controls.first().unwrap().name, "Button 200");
    assert_eq!(last.controls.last().unwrap().name, "Button 249");
    assert!(!last.truncated);
    assert!(serde_json::to_value(&last).unwrap()["next_offset"].is_null());
    assert!(!agent.references(page.target_id()).contains_key(&omitted));

    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let request = json!({
        "operation_id":uuid::Uuid::new_v4().to_string(),
        "action":{"kind":"click","target":last.controls.last().unwrap().target},
        "offset":200,"max_controls":50,"timeout_ms":5000,"action_timeout_ms":2000
    });
    let step = agent
        .step(
            &browser,
            &page,
            &operations,
            serde_json::from_value::<AgentStepArgs>(request.clone()).unwrap(),
        )
        .await
        .unwrap();
    assert!(step.operation_error.is_none(), "{:?}", step.operation_error);
    assert!(
        step.observation_error.is_none(),
        "{:?}",
        step.observation_error
    );
    assert_eq!(
        step.observation.unwrap().controls.last().unwrap().name,
        "Button 249"
    );
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
    let mut replay = request;
    replay["offset"] = json!(0);
    let replay = agent
        .step(
            &browser,
            &page,
            &operations,
            serde_json::from_value::<AgentStepArgs>(replay).unwrap(),
        )
        .await
        .unwrap();
    assert!(
        replay.operation_error.is_none(),
        "{:?}",
        replay.operation_error
    );
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
    assert_eq!(replay.observation.unwrap().controls[0].name, "Button 0");

    let empty = agent
        .observe(
            &browser,
            &page,
            serde_json::from_value::<AgentObserveArgs>(json!({"offset":usize::MAX})).unwrap(),
        )
        .await
        .unwrap();
    assert!(empty.controls.is_empty());
    assert_eq!(
        serde_json::to_value(empty).unwrap()["next_offset"],
        Value::Null
    );
    browser.close().await.unwrap();
}
