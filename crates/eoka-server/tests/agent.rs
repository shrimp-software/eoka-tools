use eoka_protocol::{ActionOutcome, AgentAction, AgentMode, AgentObserveArgs, AgentStepArgs};
use eoka_server::{
    agent::AgentSession,
    eoka::{Browser, StealthConfig},
    operations::OperationManager,
};

fn step(action: AgentAction) -> AgentStepArgs {
    serde_json::from_value(
        serde_json::json!({"action":action,"action_timeout_ms":1000,"timeout_ms":2000}),
    )
    .unwrap()
}

async fn browser() -> Browser {
    Browser::launch_with_config(StealthConfig {
        headless: true,
        geo_align: false,
        ..Default::default()
    })
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires Chrome; agent tool regression"]
async fn observe_step_replay_and_verification_preserve_actual_page_effects() {
    let browser = browser().await;
    let page = browser.new_blank_page().await.unwrap();
    page.execute("document.body.innerHTML='<h1>Reports</h1><button id=go>Generate report</button><label>Name<input id=name></label><p role=status id=status>Ready</p>';window.hits=0;document.querySelector('#go').onclick=()=>{window.hits++;document.querySelector('#status').textContent='Generated'}").await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let mut agent = AgentSession::default();
    let view = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    assert!(view
        .page
        .as_ref()
        .unwrap()
        .headings
        .iter()
        .any(|text| text == "Reports"));
    assert!(view
        .tabs
        .iter()
        .any(|tab| tab.selected && tab.id == page.target_id()));
    let target = view
        .controls
        .iter()
        .find(|c| c.name == "Generate report")
        .unwrap()
        .target
        .clone();
    let mut request = step(AgentAction::Click { target });
    request.operation_id = Some(uuid::Uuid::new_v4().to_string());
    let result = agent
        .step(&browser, &page, &operations, request.clone())
        .await
        .unwrap();
    assert!(
        result.operation_error.is_none(),
        "{:?}",
        result.operation_error
    );
    assert_eq!(
        result.operation.unwrap().outcome,
        ActionOutcome::OutcomeUnknown
    );
    assert!(result
        .observation
        .unwrap()
        .page
        .unwrap()
        .text_excerpt
        .contains("Generated"));
    page.execute("document.querySelector('#go').remove()")
        .await
        .unwrap();
    let result = agent
        .step(&browser, &page, &operations, request.clone())
        .await
        .unwrap();
    assert!(result.operation_error.is_none());
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
    request.action = AgentAction::PressKey {
        key: "Enter".into(),
    };
    let result = agent
        .step(&browser, &page, &operations, request)
        .await
        .unwrap();
    assert!(result
        .operation_error
        .unwrap()
        .contains("operation_id_conflict"));
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
    let view = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    let target = view
        .controls
        .iter()
        .find(|c| c.name == "Name")
        .unwrap()
        .target
        .clone();
    let result = agent
        .step(
            &browser,
            &page,
            &operations,
            step(AgentAction::Fill {
                target,
                text: "Ada".into(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        result.operation.unwrap().outcome,
        ActionOutcome::VerifiedComplete
    );
    assert_eq!(
        page.evaluate::<String>("document.querySelector('#name').value")
            .await
            .unwrap(),
        "Ada"
    );
    page.execute("document.body.innerHTML='<label>Checked<input type=checkbox checked></label><label>Unchecked<input type=checkbox></label><label>Mixed<input id=mixed type=checkbox></label>';document.querySelector('#mixed').indeterminate=true").await.unwrap();
    let view = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    for (name, expected) in [
        ("Checked", Some(true)),
        ("Unchecked", Some(false)),
        ("Mixed", None),
    ] {
        assert_eq!(
            view.controls
                .iter()
                .find(|control| control.name == name)
                .unwrap()
                .checked,
            expected,
            "checkbox state for {name}"
        );
    }
    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome; agent tool regression"]
async fn stale_replaced_and_wrong_tab_references_never_click_another_control() {
    let browser = browser().await;
    let page = browser.new_blank_page().await.unwrap();
    page.execute(
        "document.body.innerHTML='<button id=go onclick=window.hits++>Go</button>';window.hits=0",
    )
    .await
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let mut agent = AgentSession::default();
    let view = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    let old = view.controls[0].target.clone();
    let unchanged = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    assert_eq!(unchanged.controls[0].target, old);
    page.execute(
        "document.querySelector('#go').outerHTML='<button id=go onclick=window.hits++>Go</button>'",
    )
    .await
    .unwrap();
    agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    let result = agent
        .step(
            &browser,
            &page,
            &operations,
            step(AgentAction::Click { target: old }),
        )
        .await
        .unwrap();
    assert!(result.operation_error.unwrap().contains("stale_reference"));
    let current = result.observation.unwrap().controls[0].target.clone();
    let other = browser.new_blank_page().await.unwrap();
    let result = agent
        .step(
            &browser,
            &other,
            &operations,
            step(AgentAction::Click {
                target: current.clone(),
            }),
        )
        .await
        .unwrap();
    assert!(result.operation_error.unwrap().contains("stale_reference"));
    page.execute(
        "document.querySelector('#go').outerHTML='<button id=go onclick=window.hits++>Go</button>'",
    )
    .await
    .unwrap();
    let result = agent
        .step(
            &browser,
            &page,
            &operations,
            step(AgentAction::Click { target: current }),
        )
        .await
        .unwrap();
    assert_eq!(
        result.operation.unwrap().outcome,
        ActionOutcome::NotDispatched
    );
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 0);
    let target = result.observation.unwrap().controls[0].target.clone();
    let mut request = step(AgentAction::Click { target });
    request.mode = AgentMode::Verify;
    let result = agent
        .step(&browser, &page, &operations, request)
        .await
        .unwrap();
    assert_eq!(
        result.operation.unwrap().outcome,
        ActionOutcome::NotDispatched
    );
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 0);
    let old_document_target = result.observation.unwrap().controls[0].target.clone();
    page.reload().await.unwrap();
    page.execute(
        "document.body.innerHTML='<button onclick=window.hits++>Go</button>';window.hits=0",
    )
    .await
    .unwrap();
    let result = agent
        .step(
            &browser,
            &page,
            &operations,
            step(AgentAction::Click {
                target: old_document_target,
            }),
        )
        .await
        .unwrap();
    assert!(result
        .operation_error
        .unwrap()
        .contains("document navigated"));
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 0);
    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome; agent tool regression"]
async fn short_response_budget_keeps_operation_inspectable_without_replay() {
    let browser = browser().await;
    let page = browser.new_blank_page().await.unwrap();
    page.execute("document.body.innerHTML='<button id=go disabled>Go</button>';window.hits=0;document.querySelector('#go').onclick=()=>window.hits++").await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let mut agent = AgentSession::default();
    let view = agent
        .observe(&browser, &page, AgentObserveArgs::default())
        .await
        .unwrap();
    assert!(view.controls[0].disabled);
    let mut request = step(AgentAction::Click {
        target: view.controls[0].target.clone(),
    });
    request.timeout_ms = 500;
    request.action_timeout_ms = 3000;
    operations.list(Default::default()).await.unwrap();
    let started = std::time::Instant::now();
    let result = agent
        .step(&browser, &page, &operations, request)
        .await
        .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_millis(1000));
    page.execute("document.querySelector('#go').disabled=false")
        .await
        .unwrap();
    let operation = operations.wait(&result.operation_id, 2000).await.unwrap();
    assert_eq!(operation.outcome, ActionOutcome::OutcomeUnknown);
    assert!(operation.receipt.is_some());
    assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
    browser.close().await.unwrap();
}
