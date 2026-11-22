use std::future::Future;
use std::time::Duration;

use eoka::Page;
use eoka_protocol::sidecar::{PageBlocker, ServerError};
use eoka_protocol::ErrorDetail;
use serde_json::Value;

use crate::engine::blockers::{self, Blocker, Policy, Remedies, Stage};
use crate::operations::OperationManager;

const BLOCKER_MESSAGE_LIMIT: usize = 4096;

pub(crate) struct Call {
    method: String,
    policy: Policy,
    limit: Option<Duration>,
    ends_itself: bool,
}

fn navigates(action: &Value) -> bool {
    matches!(
        action["kind"].as_str(),
        Some("navigate" | "navigate_with_headers" | "reload" | "back" | "forward" | "history")
    )
}

pub(crate) fn policy(method: &str, params: &Value) -> Policy {
    match method {
        "page.capture_state" => Policy::Read,
        "agent.observe" => Policy::Observe,
        "actions.perform" | "agent.step" if navigates(&params["action"]) => Policy::Navigate,
        "actions.perform" | "agent.step" | "page.solve_captcha" => Policy::Gate,
        _ => Policy::Free,
    }
}

fn ends_itself(method: &str) -> bool {
    method.starts_with("agent.") || crate::operations::sidecar::supports(method)
}

impl Call {
    pub(crate) fn new(method: &str, params: &Value) -> Self {
        let policy = policy(method, params);
        Self {
            method: method.to_owned(),
            policy,
            limit: blockers::limit(policy, params),
            ends_itself: ends_itself(method),
        }
    }

    pub(crate) fn catalog(request: &eoka_protocol::Request) -> Self {
        let command = request.cmd();
        let arguments = request.args_json();
        let policy = blockers::policy(command, &arguments);
        Self {
            method: eoka_protocol::operation_by_cmd(command)
                .map_or(command, |operation| operation.path)
                .to_owned(),
            policy,
            limit: blockers::limit(policy, &arguments),
            ends_itself: false,
        }
    }
}

pub(crate) fn page_blocker(blocker: &Blocker) -> PageBlocker {
    match blocker {
        Blocker::Dialog(dialog) => PageBlocker::Dialog {
            dialog_type: dialog.kind.clone(),
            message: dialog.message.chars().take(BLOCKER_MESSAGE_LIMIT).collect(),
            url: dialog.url.clone(),
            elapsed_ms: dialog.since.elapsed().as_millis() as u64,
        },
        Blocker::Navigation(navigation) => PageBlocker::Navigation {
            url: navigation.url.clone(),
            elapsed_ms: navigation.since.elapsed().as_millis() as u64,
        },
    }
}

pub(crate) fn blocked_detail(detail: ErrorDetail, page: &Page) -> ServerError {
    let blocker = blockers::current(page)
        .filter(|blocker| blocker.code() == detail.code)
        .map(|blocker| page_blocker(&blocker));
    ServerError::blocked(detail, blocker)
}

pub(crate) fn named(code: &str, description: &str, method: &str, page: &Page) -> ServerError {
    match blockers::current(page).filter(|blocker| blocker.code() == code) {
        Some(blocker) => blocked(&blocker, method, Stage::ReadInterrupted),
        None => ServerError::blocked(ErrorDetail::new(code, description).retryable(false), None),
    }
}

fn stopped(stop: &blockers::Stop, method: &str, stage: Stage) -> ServerError {
    ServerError::blocked(
        stop.detail(&Remedies::new(None), method, stage),
        stop.blocker().map(page_blocker),
    )
}

fn blocked(blocker: &Blocker, method: &str, stage: Stage) -> ServerError {
    ServerError::blocked(
        blocker.detail(&Remedies::new(None), method, stage),
        Some(page_blocker(blocker)),
    )
}

fn timed_out(error: &ServerError) -> bool {
    let message = error.message.to_ascii_lowercase();
    error.code == eoka_protocol::sidecar::ErrorCode::Timeout
        || message.contains("timed out")
        || message.contains("deadline")
}

pub(crate) async fn guarded<F>(
    operations: &OperationManager,
    page: &Page,
    call: Call,
    work: F,
) -> Result<Value, ServerError>
where
    F: Future<Output = Result<Value, ServerError>>,
{
    let Call {
        method,
        policy,
        limit,
        ends_itself,
    } = call;
    if policy == Policy::Free {
        return work.await;
    }
    if let Some(blocker) = blockers::admit(operations, page, policy).await {
        return Err(blocked(&blocker, &method, Stage::NotStarted));
    }
    let answered = match blockers::race(page, policy, limit, !ends_itself, work).await {
        Ok(answered) => answered,
        Err(stop) => return Err(stopped(&stop, &method, policy.interrupted())),
    };
    let Err(error) = answered else {
        return answered;
    };
    if !timed_out(&error) {
        return Err(error);
    }
    match blockers::after_timeout(page, true).await {
        Some(stop) => Err(stopped(&stop, &method, policy.interrupted())),
        None => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn every_registered_page_method_has_a_deliberate_policy() {
        let free = [
            "server.hello",
            "server.shutdown",
            "browser.launch",
            "browser.connect",
            "browser.new_page",
            "browser.tabs",
            "browser.close_tab",
            "browser.close",
            "operations.get",
            "operations.list",
            "operations.wait",
            "operations.cancel",
            "recording.start",
            "recording.get",
            "recording.stop",
            "recording.cancel",
            "page.restore_state",
        ];
        let registered = eoka_protocol::sidecar::method_names()
            .into_iter()
            .filter(|method| eoka_protocol::sidecar::catalog_operation(method).is_none());
        for method in registered {
            let policy = policy(method, &json!({"action":{"kind":"click"}}));
            assert_eq!(
                policy == Policy::Free,
                free.contains(&method),
                "{method} is {policy:?}"
            );
        }
    }

    #[test]
    fn navigation_actions_are_admitted_past_a_pending_navigation() {
        for kind in ["navigate", "reload", "back", "forward", "history"] {
            for method in ["actions.perform", "agent.step"] {
                assert_eq!(
                    policy(method, &json!({"action":{"kind":kind}})),
                    Policy::Navigate
                );
            }
        }
        assert_eq!(
            policy("actions.perform", &json!({"action":{"kind":"fill"}})),
            Policy::Gate
        );
        assert_eq!(policy("page.capture_state", &json!({})), Policy::Read);
        assert_eq!(policy("console", &json!({})), Policy::Free);
    }

    #[test]
    fn admissions_and_agent_calls_end_themselves_when_a_dialog_opens() {
        for method in ["actions.perform", "agent.step", "agent.observe"] {
            assert!(ends_itself(method), "{method}");
        }
        for method in ["page.capture_state", "eval", "screenshot"] {
            assert!(!ends_itself(method), "{method}");
        }
    }

    #[test]
    fn a_blocker_becomes_its_wire_code_with_what_blocks_the_page() {
        let dialog = Blocker::Dialog(blockers::Dialog {
            kind: "confirm".into(),
            message: "are you sure?".into(),
            url: "https://example.test/".into(),
            since: Instant::now(),
        });
        let error = blocked(&dialog, "eval", Stage::NotStarted);
        assert_eq!(error.code, eoka_protocol::sidecar::ErrorCode::DialogOpen);
        assert!(
            error.message.contains("dialog is open"),
            "{}",
            error.message
        );
        assert!(error.message.contains("are you sure?"), "{}", error.message);
        assert!(error.message.contains("`eval` was not started"));
        let data = *error.data.unwrap();
        let detail = data.detail.unwrap();
        assert_eq!(detail.code, "dialog_open");
        assert!(detail.hint.unwrap().contains("dialog.accept"));
        assert!(matches!(
            data.blocker,
            Some(PageBlocker::Dialog { ref dialog_type, ref message, .. })
                if dialog_type == "confirm" && message == "are you sure?"
        ));

        let navigation = Blocker::Navigation(blockers::Navigation {
            url: Some("https://slow.test/hang".into()),
            since: Instant::now(),
        });
        let error = blocked(&navigation, "eval", Stage::ActionInterrupted);
        assert_eq!(
            error.code,
            eoka_protocol::sidecar::ErrorCode::NavigationPending
        );
        assert!(error.message.contains("https://slow.test/hang"));
        assert!(error.message.contains("`eval` returned without a result"));
        assert!(matches!(
            error.data.unwrap().blocker,
            Some(PageBlocker::Navigation { url: Some(ref url), .. }) if url == "https://slow.test/hang"
        ));

        let long = Blocker::Dialog(blockers::Dialog {
            kind: "alert".into(),
            message: "x".repeat(BLOCKER_MESSAGE_LIMIT + 10),
            url: String::new(),
            since: Instant::now(),
        });
        assert!(matches!(
            page_blocker(&long),
            PageBlocker::Dialog { message, .. } if message.len() == BLOCKER_MESSAGE_LIMIT
        ));
    }

    #[test]
    fn only_deadline_failures_are_checked_for_a_blocked_page() {
        use eoka_protocol::sidecar::ErrorCode;
        assert!(timed_out(&ServerError::new(ErrorCode::Timeout, "slow")));
        assert!(timed_out(&ServerError::internal(
            "Transport error: CDP command 'Runtime.evaluate' timed out after 30s"
        )));
        assert!(timed_out(&ServerError::internal(
            "CDP command Runtime.evaluate exceeded its deadline"
        )));
        assert!(!timed_out(&ServerError::new(
            ErrorCode::ElementNotFound,
            "Element not found: #missing"
        )));
        assert!(!timed_out(&ServerError::invalid_page("gone")));
    }
}
