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
        error: None,
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
        let path = entry.unwrap().path();
        // The store holds its lock files open; Windows locks are mandatory,
        // so they cannot be read. They are always empty.
        if path.extension().is_some_and(|e| e == "lock") {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
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

fn sensitive(value: &str) -> Vec<String> {
    vec![value.to_owned()]
}

fn fuses_with_a_word(message: &str) -> bool {
    message.match_indices(REDACTED).any(|(start, marker)| {
        message[..start]
            .chars()
            .next_back()
            .is_some_and(word_character)
            || message[start + marker.len()..]
                .chars()
                .next()
                .is_some_and(word_character)
    })
}

#[test]
fn redaction_never_rewrites_the_inside_of_a_word() {
    let prose = "Timeout: Action deadline exceeded during preparation: target not found";
    for value in ["x", "e", "ex", "ded", "prep", "o", " ", ": ", "t n"] {
        let message = redact(prose, &sensitive(value));
        assert!(message.contains("exceeded"), "{value:?}: {message}");
        assert!(!fuses_with_a_word(&message), "{value:?}: {message}");
    }
    assert_eq!(redact(prose, &sensitive("x")), prose);
    assert_eq!(redact(prose, &sensitive("")), prose);
    assert_eq!(redact(prose, &[]), prose);
    assert_eq!(
        redact(
            "CDP command 'Runtime.evaluate' timed out after 30s (id=48)",
            &sensitive("30")
        ),
        "CDP command 'Runtime.evaluate' timed out after 30s (id=48)"
    );
}

#[test]
fn redaction_still_removes_an_echoed_value() {
    for (value, message, expected) in [
        (
            "hunter2-Secret!",
            "Evaluation failed: rejected value \"hunter2-Secret!\" for #password",
            "Evaluation failed: rejected value \"[redacted]\" for #password",
        ),
        (
            "correct horse battery",
            "TypeError: cannot store correct horse battery, correct horse battery.",
            "TypeError: cannot store [redacted], [redacted].",
        ),
        ("x", "unexpected value: x", "unexpected value: [redacted]"),
        (
            "123",
            "invalid cvv=123; retry",
            "invalid cvv=[redacted]; retry",
        ),
        (
            "4111 1111",
            "card '4111 1111' declined",
            "card '[redacted]' declined",
        ),
        ("pässwörd", "wrong: pässwörd", "wrong: [redacted]"),
        (
            "a\"b\\c",
            r#"SyntaxError: bad literal "a\"b\\c" near a"b\c"#,
            r#"SyntaxError: bad literal "[redacted]" near [redacted]"#,
        ),
        ("secret", "secret", "[redacted]"),
    ] {
        let redacted = redact(message, &sensitive(value));
        assert_eq!(redacted, expected, "{value:?}");
        assert!(!fuses_with_a_word(&redacted), "{value:?}: {redacted}");
    }
}

#[test]
fn redaction_leaves_a_value_embedded_in_a_longer_word_alone() {
    assert_eq!(
        redact("unexpected token in maxed_out", &sensitive("x")),
        "unexpected token in maxed_out"
    );
    assert_eq!(
        redact("id=sessionid42 rejected 42", &sensitive("42")),
        "id=sessionid42 rejected [redacted]"
    );
}

fn receipt(dispatch: DispatchState) -> ActionReceipt {
    ActionReceipt {
        destination_target_id: None,
        started_at_ms: 0,
        finished_at_ms: 0,
        download_guid: None,
        action: "navigate".into(),
        target_id: "target".into(),
        outcome: ActionOutcome::OutcomeUnknown,
        dispatch,
        preparation_may_have_executed: false,
        verification: None,
        elapsed_ms: 0,
        already_satisfied: false,
        cleanup_pending: false,
    }
}

fn failed(dispatch: DispatchState) -> Option<eoka::Result<ActionReceipt>> {
    Some(Err(Error::Action {
        code: "action_timeout".into(),
        receipt: Box::new(receipt(dispatch)),
        source: Box::new(Error::Timeout(
            "Action deadline exceeded during preparation".into(),
        )),
    }))
}

#[test]
fn only_a_navigation_whose_command_never_returned_is_abandoned() {
    assert!(navigation_unanswered(
        &failed(DispatchState::MayHaveBeenSent),
        true
    ));
    assert!(!navigation_unanswered(
        &failed(DispatchState::Acknowledged),
        true
    ));
    assert!(!navigation_unanswered(
        &failed(DispatchState::NotSent),
        false
    ));
    assert!(!navigation_unanswered(
        &Some(Ok(receipt(DispatchState::Acknowledged))),
        true
    ));
    assert!(!navigation_unanswered(
        &Some(Err(Error::Timeout("unrelated".into()))),
        true
    ));
    assert!(navigation_unanswered(&None, true));
    assert!(!navigation_unanswered(&None, false));
}

#[test]
fn navigation_actions_are_the_only_ones_that_can_be_abandoned() {
    let target = eoka::LocatorSpec::css("#field");
    for action in [
        Action::Navigate {
            url: "https://example.test/".into(),
            referrer: None,
        },
        Action::NavigateWithHeaders {
            url: "https://example.test/".into(),
            headers: Default::default(),
        },
        Action::Reload,
        Action::Back,
        Action::Forward,
        Action::History { delta: -2 },
    ] {
        assert!(navigates(&action), "{}", action.kind());
    }
    for action in [
        Action::Click {
            target: target.clone(),
            human: false,
            double: false,
        },
        Action::Fill {
            target,
            value: "value".into(),
            input_mode: Default::default(),
        },
        Action::PressKey {
            key: "Enter".into(),
        },
    ] {
        assert!(!navigates(&action), "{}", action.kind());
    }
}

#[tokio::test]
async fn stopping_one_target_leaves_other_targets_running() {
    let manager = OperationManager::default();
    let mut receivers = Vec::new();
    for (id, target) in [("first", "a"), ("second", "b"), ("third", "a")] {
        let (cancel, canceled) = watch::channel(false);
        manager.inner.jobs.lock().unwrap().insert(
            id.into(),
            ActiveOperation {
                target_id: target.into(),
                cancel,
            },
        );
        receivers.push((id, canceled));
    }
    let signalled = |receivers: &[(&str, watch::Receiver<bool>)]| -> Vec<String> {
        receivers
            .iter()
            .filter(|(_, canceled)| *canceled.borrow())
            .map(|(id, _)| (*id).to_owned())
            .collect()
    };
    let selected = manager.signal(|target| target == "a");
    assert_eq!(
        selected
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        ["first", "third"].into_iter().collect()
    );
    assert_eq!(signalled(&receivers), ["first", "third"]);
    manager.signal(|_| true);
    assert_eq!(signalled(&receivers), ["first", "second", "third"]);
}
