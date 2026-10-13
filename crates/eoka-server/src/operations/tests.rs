use super::*;
use eoka_protocol::OperationRecord;

fn record(store: &Store, id: &str) -> OperationRecord {
    OperationRecord {
        id: id.into(),
        runtime_id: store.runtime.clone(),
        target_id: "target".into(),
        action: "click".into(),
        state: OperationState::Running,
        outcome: ActionOutcome::NotDispatched,
        dispatch_intent: false,
        preparation_intent: false,
        started_at_ms: timestamp(),
        finished_at_ms: None,
        receipt: None,
        error_code: None,
    }
}

#[tokio::test]
async fn admission_is_unique_conflicts_are_rejected_and_values_are_not_saved() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path().into()).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let canonical = b"super-secret-password".to_vec();
    assert!(
        store
            .admit(record(&store, &id), canonical.clone())
            .await
            .unwrap()
            .1
    );
    assert!(!store.admit(record(&store, &id), canonical).await.unwrap().1);
    assert!(store
        .admit(record(&store, &id), b"different".to_vec())
        .await
        .is_err());
    for entry in std::fs::read_dir(directory.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes
            .windows(b"super-secret-password".len())
            .any(|s| s == b"super-secret-password"));
    }
}

#[tokio::test]
async fn live_runtime_is_not_recovered_by_a_second_owner() {
    let directory = tempfile::tempdir().unwrap();
    let first = Store::open(directory.path().into()).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    first.admit(record(&first, &id), vec![]).await.unwrap();
    let second = Store::open(directory.path().into()).await.unwrap();
    assert!(matches!(
        second.get(id.clone()).await.unwrap().state,
        OperationState::Running
    ));
    drop(first);
    let third = Store::open(directory.path().into()).await.unwrap();
    assert!(matches!(
        third.get(id).await.unwrap().state,
        OperationState::Interrupted
    ));
}

#[test]
fn crash_child() {
    let Ok(path) = std::env::var("EOKA_TEST_CRASH_STATE") else {
        return;
    };
    let stage = std::env::var("EOKA_TEST_CRASH_STAGE").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let store = Store::open(path.into()).await.unwrap();
        let id = "6e8b0670-a457-4ce1-8d21-f1e4cf8c8e9b";
        store
            .admit(record(&store, id), b"action".to_vec())
            .await
            .unwrap();
        if stage == "intent" || stage == "complete" {
            store
                .update(id.into(), |record| {
                    record.dispatch_intent = true;
                    record.outcome = ActionOutcome::OutcomeUnknown
                })
                .await
                .unwrap();
        }
        if stage == "complete" {
            store
                .update(id.into(), |record| {
                    record.state = OperationState::Finished;
                    record.outcome = ActionOutcome::VerifiedComplete;
                    record.finished_at_ms = Some(timestamp());
                })
                .await
                .unwrap();
        }
        std::process::exit(86);
    });
}

#[tokio::test]
async fn abrupt_process_exit_preserves_admission_intent_and_completion() {
    for stage in ["admission", "intent", "complete"] {
        let directory = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "operations::tests::crash_child", "--nocapture"])
            .env("EOKA_TEST_CRASH_STATE", directory.path())
            .env("EOKA_TEST_CRASH_STAGE", stage)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        let store = Store::open(directory.path().into()).await.unwrap();
        let record = store
            .get("6e8b0670-a457-4ce1-8d21-f1e4cf8c8e9b".into())
            .await
            .unwrap();
        assert_eq!(
            record.outcome,
            match stage {
                "admission" => ActionOutcome::NotDispatched,
                "intent" => ActionOutcome::OutcomeUnknown,
                _ => ActionOutcome::VerifiedComplete,
            }
        );
        assert!(!matches!(record.state, OperationState::Running));
    }
}
