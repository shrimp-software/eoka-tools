use super::*;
use crate::operations::{commands, OperationManager};

const FAILURE_REPORT_GRACE_MS: u64 = 2_000;

impl Engine {
    pub fn operation_manager(&self) -> OperationManager {
        self.operations.clone()
    }

    pub(super) async fn handle_action(&mut self, cmd: &str, args: &Value) -> Response {
        let mut args = args.clone();
        if !args.is_object() {
            args = serde_json::json!({});
        }
        if args["operation_id"].is_null() {
            args["operation_id"] = Value::String(uuid::Uuid::new_v4().to_string());
        }
        let args = &args;
        let reloads = cmd == "reload" || args["action"]["kind"] == "reload";
        let mut script_applied = None;
        let result: anyhow::Result<_> = async {
            let page = &self.require_tab().map_err(anyhow::Error::msg)?.page;
            if reloads && self.script_policy.is_active() {
                let enabled = self.script_enabled_for(&page.url().await?);
                if let Some(enabled) = enabled {
                    page.set_javascript_enabled(enabled).await?;
                    script_applied = Some(enabled);
                }
            }
            let budget = commands::options(args)?
                .timeout_ms
                .min(60_000)
                .saturating_add(FAILURE_REPORT_GRACE_MS);
            let record = self
                .agent
                .lock()
                .await
                .command(&self.operations, page, cmd, args.clone())
                .await?;
            if matches!(record.state, eoka_protocol::OperationState::Running) {
                blockers::wait_unless_dialog(&self.operations, page, &record.id, budget)
                    .await
                    .map_err(commands::uncertain)
            } else {
                Ok(record)
            }
        }
        .await;
        if let Some(enabled) = script_applied {
            self.script_policy.note_applied(enabled);
        }
        match result {
            Ok(record) => Response::action_result(record),
            Err(error) => {
                commands::failure(error, args["operation_id"].as_str().map(str::to_owned))
            }
        }
    }
}
