use std::future::Future;
use std::time::Duration;

use eoka::Page;
use eoka_protocol::sidecar::{PageBlocker, ServerError};
use eoka_protocol::ErrorDetail;
use serde_json::{json, Value};

use crate::engine::blockers::{self, Blocker, Policy, Remedies, Stage};
use crate::operations::OperationManager;

const LIVENESS_PROBE: Duration = Duration::from_millis(400);
const WAIT_MARGIN: Duration = Duration::from_secs(2);
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
        "page.title" | "page.url" | "page.text" | "page.content" | "page.get_text"
        | "page.get_attribute" | "page.exists" | "page.frames" | "page.capture_state" => {
            Policy::Read
        }
        "page.wait_for" | "page.wait_for_visible" | "page.wait_for_text" => Policy::Wait,
        "page.evaluate"
        | "page.execute"
        | "page.evaluate_raw"
        | "page.execute_raw"
        | "page.evaluate_in_frame"
        | "page.fetch"
        | "page.fetch_bounded" => Policy::Script,
        "page.screenshot" | "page.screenshot_to_file" => Policy::Capture,
        "agent.observe" => Policy::Observe,
        "page.goto" => Policy::Navigate,
        "actions.perform" | "agent.step" if navigates(&params["action"]) => Policy::Navigate,
        "actions.perform"
        | "agent.step"
        | "page.click"
        | "page.click_text"
        | "page.human_click"
        | "page.human_click_text"
        | "page.fill"
        | "page.human_fill"
        | "page.type_into"
        | "page.select"
        | "page.hover"
        | "page.press_key"
        | "page.click_shadow"
        | "page.mouse_down"
        | "page.mouse_move"
        | "page.mouse_up"
        | "page.key_down"
        | "page.key_up"
        | "page.solve_captcha" => Policy::Gate,
        _ => Policy::Free,
    }
}

fn limit(policy: Policy, params: &Value) -> Option<Duration> {
    match policy {
        Policy::Read => Some(blockers::READ_LIMIT),
        Policy::Wait => params["timeoutMs"]
            .as_u64()
            .map(|timeout| Duration::from_millis(timeout).saturating_add(WAIT_MARGIN)),
        _ => None,
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
            limit: limit(policy, params),
            ends_itself: ends_itself(method),
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

async fn responsive(page: &Page) -> bool {
    tokio::time::timeout(
        LIVENESS_PROBE,
        page.session().send::<_, Value>(
            "Runtime.evaluate",
            &json!({"expression":"1","returnByValue":true}),
        ),
    )
    .await
    .is_ok_and(|answer| answer.is_ok())
}

async fn interrupted(page: &Page, policy: Policy, ends_itself: bool) -> Blocker {
    if ends_itself {
        return std::future::pending().await;
    }
    blockers::interrupted(page, policy != Policy::Wait).await
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
    blockers::watch(page);
    let grace = if policy == Policy::Navigate {
        Duration::ZERO
    } else {
        blockers::NAVIGATION_GRACE
    };
    match blockers::confirmed(page, grace).await {
        Some(Blocker::Navigation(_)) if policy == Policy::Navigate => {
            blockers::supersede_navigation(operations, page).await;
        }
        Some(Blocker::Navigation(_)) if matches!(policy, Policy::Wait | Policy::Capture) => {}
        Some(blocker) => return Err(blocked(&blocker, &method, Stage::NotStarted)),
        None => {}
    }
    let deadline = async {
        match limit {
            Some(limit) => tokio::time::sleep(limit).await,
            None => std::future::pending().await,
        }
    };
    let answered = tokio::select! {
        biased;
        answered = work => answered,
        blocker = interrupted(page, policy, ends_itself) => {
            return Err(blocked(&blocker, &method, policy.interrupted()));
        }
        _ = deadline => {
            return Err(match blockers::current(page) {
                Some(blocker) => blocked(&blocker, &method, policy.interrupted()),
                None => ServerError::blocked(
                    blockers::unresponsive(&Remedies::new(None), &method, limit.unwrap_or_default()),
                    None,
                ),
            });
        }
    };
    let Err(error) = answered else {
        return answered;
    };
    if !timed_out(&error) {
        return Err(error);
    }
    match blockers::confirmed(page, Duration::ZERO).await {
        Some(blocker) => Err(blocked(&blocker, &method, policy.interrupted())),
        None if !responsive(page).await => Err(ServerError::blocked(
            blockers::frozen(&Remedies::new(None), &method),
            None,
        )),
        None => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            "page.close",
            "page.release_all_inputs",
            "page.set_viewport",
            "page.reset_viewport",
            "page.restore_state",
            "page.solve_turnstile",
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
        assert_eq!(policy("page.goto", &json!({})), Policy::Navigate);
        assert_eq!(
            policy("actions.perform", &json!({"action":{"kind":"fill"}})),
            Policy::Gate
        );
        assert_eq!(policy("page.title", &json!({})), Policy::Read);
        assert_eq!(policy("page.evaluate", &json!({})), Policy::Script);
        assert_eq!(policy("page.screenshot", &json!({})), Policy::Capture);
        assert_eq!(policy("page.wait_for", &json!({})), Policy::Wait);
        assert_eq!(policy("console", &json!({})), Policy::Free);
    }

    #[test]
    fn admissions_and_agent_calls_end_themselves_when_a_dialog_opens() {
        for method in [
            "actions.perform",
            "page.goto",
            "page.click",
            "page.fill",
            "page.press_key",
            "agent.step",
            "agent.observe",
        ] {
            assert!(ends_itself(method), "{method}");
        }
        for method in [
            "page.title",
            "page.evaluate",
            "page.wait_for",
            "page.screenshot",
            "page.mouse_up",
            "page.click_shadow",
        ] {
            assert!(!ends_itself(method), "{method}");
        }
    }

    #[test]
    fn reads_and_waits_are_bounded_and_scripts_are_not() {
        assert_eq!(limit(Policy::Read, &json!({})), Some(blockers::READ_LIMIT));
        assert_eq!(
            limit(Policy::Wait, &json!({"timeoutMs": 1500})),
            Some(Duration::from_millis(3500))
        );
        assert_eq!(limit(Policy::Wait, &json!({})), None);
        assert_eq!(limit(Policy::Script, &json!({})), None);
        assert_eq!(limit(Policy::Capture, &json!({})), None);
    }

    #[test]
    fn a_blocker_becomes_its_wire_code_with_what_blocks_the_page() {
        let dialog = Blocker::Dialog(blockers::Dialog {
            kind: "confirm".into(),
            message: "are you sure?".into(),
            url: "https://example.test/".into(),
            since: Instant::now(),
        });
        let error = blocked(&dialog, "page.title", Stage::NotStarted);
        assert_eq!(error.code, eoka_protocol::sidecar::ErrorCode::DialogOpen);
        assert!(
            error.message.contains("dialog is open"),
            "{}",
            error.message
        );
        assert!(error.message.contains("are you sure?"), "{}", error.message);
        assert!(error.message.contains("`page.title` was not started"));
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
        let error = blocked(&navigation, "page.evaluate", Stage::ActionInterrupted);
        assert_eq!(
            error.code,
            eoka_protocol::sidecar::ErrorCode::NavigationPending
        );
        assert!(error.message.contains("https://slow.test/hang"));
        assert!(error
            .message
            .contains("`page.evaluate` returned without a result"));
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
