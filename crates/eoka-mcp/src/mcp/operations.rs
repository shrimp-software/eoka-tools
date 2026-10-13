use super::*;
use eoka_server::eoka::{LocatorQuery, LocatorSpec};
use eoka_server::operations::commands;
use eoka_server::Target;

impl EokaServer {
    pub(super) async fn perform_command(
        &self,
        cmd: &str,
        args: Value,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let page = tab.page.clone();
        let (id, action, options) = if cmd == "action_perform" {
            let request: eoka_protocol::ActionPerformArgs =
                serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?;
            (request.operation_id, request.action, request.options)
        } else {
            let id = args["operation_id"]
                .as_str()
                .ok_or_else(|| {
                    invalid("operation_id is required; generate a UUID before requesting an action")
                })?
                .to_owned();
            let target = if let Some(target) = args["target"].as_str() {
                Some(match Target::parse(target) {
                    Target::Ref(label) => LocatorSpec {
                        query: LocatorQuery::BackendNode {
                            id: *tab
                                .snapshot_refs
                                .get(&label)
                                .ok_or_else(|| invalid("Stale reference; take a new snapshot"))?,
                        },
                        frame_id: None,
                        index: None,
                    },
                    Target::Index(index) => LocatorSpec::css(
                        &tab.elements
                            .get(index)
                            .ok_or_else(|| invalid("Index unavailable; observe first"))?
                            .selector,
                    ),
                    Target::Live(_) => {
                        commands::live_locator(target).map_err(|e| invalid(e.to_string()))?
                    }
                })
            } else {
                None
            };
            (
                id,
                commands::action(cmd, &args, target).map_err(|e| invalid(e.to_string()))?,
                commands::options(&args).map_err(|e| invalid(e.to_string()))?,
            )
        };
        drop(guard);
        protocol_tool_response(commands::response(
            self.operations.submit(page, id, action, options).await,
        ))
    }
}
