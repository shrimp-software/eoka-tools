use std::time::{Duration, Instant};

use eoka_server::engine::blockers::{self, Policy, Remedies, Stage, Surface};
use eoka_server::eoka::Page;
use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock, ErrorData};
use serde_json::Value;

use super::{routes, EokaServer};

pub(super) const BUSY_LIMIT: Duration = Duration::from_secs(60);

pub(super) fn policy_for_tool(tool: &str, arguments: &Value) -> Policy {
    routes::route(tool).map_or(Policy::Free, |route| {
        blockers::policy(route.operation.cmd, &route.arguments(arguments))
    })
}

fn remedies() -> Remedies<'static> {
    Remedies::for_surface(Surface::Mcp, None)
}

pub(super) fn stopped(tool: &str, policy: Policy, stop: &blockers::Stop) -> CallToolResult {
    refusal(stop.detail(&remedies(), tool, policy.interrupted()))
}

pub(super) fn blocked_detail(
    error: &eoka_protocol::sidecar::ServerError,
) -> Option<eoka_protocol::ErrorDetail> {
    eoka_protocol::sidecar::ErrorCode::for_blocker_detail(
        &error.data.as_ref()?.detail.as_ref()?.code,
    )?;
    error.data.as_ref()?.detail.clone()
}

pub(super) fn refusal(detail: eoka_protocol::ErrorDetail) -> CallToolResult {
    let text = serde_json::to_string(&eoka_protocol::Response::err_detail(detail))
        .unwrap_or_else(|_| "The page is blocked".into());
    CallToolResult::error(vec![ContentBlock::text(text)])
}

fn failure_text(result: &Result<CallToolResponse, ErrorData>) -> Option<String> {
    match result {
        Err(error) => Some(error.message.to_string()),
        Ok(CallToolResponse::Complete(result)) if result.is_error == Some(true) => {
            Some(serde_json::to_string(&result.content).unwrap_or_default())
        }
        Ok(_) => None,
    }
}

fn timed_out(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    ["timed out", "timeout", "deadline", "operation_pending"]
        .iter()
        .any(|needle| text.contains(needle))
}

impl EokaServer {
    pub(super) fn started(&self, tool: &str) {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((tool.to_owned(), Instant::now()));
    }

    pub(super) fn finished(&self) {
        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub(super) async fn take_turn(
        &self,
        tool: &str,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, CallToolResult> {
        match tokio::time::timeout(self.busy_limit, self.turn.clone().lock_owned()).await {
            Ok(turn) => Ok(turn),
            Err(_) => {
                let occupant = self
                    .running
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map_or_else(
                        || "another tool call holds the browser".to_owned(),
                        |(running, since)| {
                            format!(
                                "`{running}` has been running for {} s",
                                since.elapsed().as_secs()
                            )
                        },
                    );
                Err(refusal(
                    eoka_protocol::ErrorDetail::new(
                        "daemon_busy",
                        format!(
                            "The browser session is busy: {occupant}. `{tool}` was not started."
                        ),
                    )
                    .retryable(true)
                    .hint("Call it again once the running tool call returns.")
                    .context(eoka_protocol::ErrorContext {
                        action_status: Some(eoka_protocol::ActionStatus::NotAttempted),
                        ..Default::default()
                    }),
                ))
            }
        }
    }

    pub(super) async fn refuse_blocked(
        &self,
        tool: &str,
        policy: Policy,
        page: &Page,
    ) -> Option<CallToolResult> {
        let blocker = blockers::admit(&self.operations, page, policy).await?;
        Some(refusal(blocker.detail(
            &remedies(),
            tool,
            Stage::NotStarted,
        )))
    }

    pub(super) async fn explain_blocked(
        &self,
        tool: &str,
        policy: Policy,
        result: Result<CallToolResponse, ErrorData>,
    ) -> Result<CallToolResponse, ErrorData> {
        if policy == Policy::Free {
            return result;
        }
        let Some(text) = failure_text(&result) else {
            return result;
        };
        let Some(page) = self.engine.lock().await.current_page() else {
            return result;
        };
        let navigation_action = policy == Policy::Navigate;
        let timed_out = timed_out(&text);
        if let Some(navigation) =
            blockers::finish_navigation(&self.operations, &page, navigation_action, timed_out).await
        {
            return Ok(
                refusal(blockers::navigation_timeout(&remedies(), &navigation, None)).into(),
            );
        }
        if !timed_out {
            return result;
        }
        match blockers::after_timeout(&page, false).await {
            Some(stop) => Ok(stopped(tool, policy, &stop).into()),
            None => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tools_map_to_the_page_access_they_need() {
        let none = Value::Null;
        for free in [
            "accept_dialog",
            "dismiss_dialog",
            "list_tabs",
            "close_tab",
            "close",
        ] {
            assert_eq!(policy_for_tool(free, &none), Policy::Free, "{free}");
        }
        assert_eq!(policy_for_tool("wait_ms", &json!({"ms":100})), Policy::Free);
        assert_eq!(
            policy_for_tool("wait_for_text", &json!({"text":"Done"})),
            Policy::Wait
        );
        for navigation in ["navigate", "back", "forward", "history_go", "reload"] {
            assert_eq!(
                policy_for_tool(navigation, &none),
                Policy::Navigate,
                "{navigation}"
            );
        }
        for read in ["page_info", "page_text", "find_text", "snapshot", "observe"] {
            assert_eq!(policy_for_tool(read, &none), Policy::Read, "{read}");
        }
        assert_eq!(policy_for_tool("extract", &none), Policy::Script);
        assert_eq!(policy_for_tool("agent_observe", &none), Policy::Observe);
        assert_eq!(policy_for_tool("screenshot", &none), Policy::Gate);
        for action in ["click", "fill", "type_key", "batch", "agent_step"] {
            assert_eq!(policy_for_tool(action, &none), Policy::Gate, "{action}");
        }
    }

    #[test]
    fn a_refusal_is_a_readable_tool_error_with_mcp_tool_names() {
        let detail = eoka_protocol::ErrorDetail::new("dialog_open", "A dialog is open.")
            .hint(format!("Run {}.", remedies().accept()));
        let result = refusal(detail);
        assert_eq!(result.is_error, Some(true));
        let text = serde_json::to_string(&result.content).unwrap();
        assert!(text.contains("dialog_open"), "{text}");
        assert!(text.contains("accept_dialog"), "{text}");
        assert!(timed_out(
            "Transport error: CDP command 'Runtime.evaluate' timed out after 30s"
        ));
        assert!(!timed_out("ElementNotFound: #absent"));
    }
}
