use eoka_protocol::{AgentObserveArgs, AgentStepArgs, OperationStatus};
use eoka_server::{
    agent::AgentSession,
    eoka::{Browser, StealthConfig},
    operations::OperationManager,
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires Chrome; pagination and target identity"]
async fn pagination_filters_state_and_readers_preserve_scoped_targets() {
    let browser = Browser::launch_with_config(StealthConfig {
        headless: true,
        geo_align: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let page = browser.new_blank_page().await.unwrap();
    page.execute("document.body.innerHTML='<label>Name<input id=name required readonly aria-invalid=true></label>'+Array.from({length:250},(_,i)=>`<button id=b${i}>Button ${i}</button>`).join('');document.querySelector('#name').focus()").await.unwrap();
    let mut agent = AgentSession::default();
    let first = agent
        .observe(
            &browser,
            &page,
            AgentObserveArgs {
                max_controls: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(first.total_controls, 251);
    let field = first.controls.iter().find(|c| c.name == "Name").unwrap();
    assert!(field.focused && field.readonly && field.required && field.invalid && !field.editable);
    let target = first
        .controls
        .iter()
        .find(|c| c.name == "Button 0")
        .unwrap()
        .target
        .clone();
    let mut controls = first.controls;
    let mut cursor = first.next_cursor;
    while cursor.is_some() {
        let view = agent
            .observe(
                &browser,
                &page,
                AgentObserveArgs {
                    cursor,
                    max_controls: 100,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        controls.extend(view.controls);
        cursor = view.next_cursor;
    }
    assert_eq!(controls.len(), 251);
    eoka_server::snapshot::snapshot(&page, false).await.unwrap();
    let filtered = agent
        .observe(
            &browser,
            &page,
            AgentObserveArgs {
                role: Some("button".into()),
                name: Some("Button 24".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(filtered.total_controls, 11);
    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let args: AgentStepArgs =
        serde_json::from_value(json!({"action":{"kind":"hover","target":target}})).unwrap();
    let result = agent
        .step(&browser, &page, &operations, args.clone())
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::VerifiedComplete);
    page.execute("document.querySelector('#b0').outerHTML='<button id=b0>Replacement</button>'")
        .await
        .unwrap();
    let mut args = args;
    args.action_timeout_ms = 100;
    let result = agent
        .step(&browser, &page, &operations, args)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::NotDispatched);
    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome; honest action outcomes"]
async fn verify_is_default_and_timeout_preserves_diagnostics_without_input_values() {
    let browser = Browser::launch_with_config(StealthConfig {
        headless: true,
        geo_align: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let page = browser.new_blank_page().await.unwrap();
    page.execute("document.body.innerHTML='<button id=go>Save</button><label>Name<input disabled></label>';window.hits=0;document.querySelector('#go').onclick=()=>window.hits++").await.unwrap();
    let mut agent = AgentSession::default();
    let view = agent
        .observe(&browser, &page, Default::default())
        .await
        .unwrap();
    let button = &view
        .controls
        .iter()
        .find(|c| c.name == "Save")
        .unwrap()
        .target;
    let field = &view
        .controls
        .iter()
        .find(|c| c.name == "Name")
        .unwrap()
        .target;
    let directory = tempfile::tempdir().unwrap();
    let operations = OperationManager::in_directory(directory.path().into());
    let click = serde_json::from_value(json!({"action":{"kind":"click","target":button}})).unwrap();
    let result = agent
        .step(&browser, &page, &operations, click)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::NotDispatched);
    assert_eq!(page.evaluate::<u32>("hits").await.unwrap(), 0);
    let fill = serde_json::from_value(json!({"action":{"kind":"fill","target":field,"text":"sensitive-fixture"},"action_timeout_ms":150})).unwrap();
    let result = agent
        .step(&browser, &page, &operations, fill)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::NotDispatched);
    let error = result.operation_error.as_ref().unwrap();
    assert!(error.message.contains("disabled"), "{}", error.message);
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sensitive-fixture"));
    let stored = operations.get(&result.operation_id).await.unwrap();
    assert_eq!(stored.error.unwrap().message, error.message);
    browser.close().await.unwrap();
}
