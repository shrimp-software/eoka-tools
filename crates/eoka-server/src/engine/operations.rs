use super::*;
use crate::operations::{commands, OperationManager};

impl Engine {
    pub fn operation_manager(&self) -> OperationManager {
        self.operations.clone()
    }

    pub(super) async fn handle_action(&mut self, cmd: &str, args: &Value) -> Response {
        // Every action is a durable operation. A caller that did not name one
        // gets a fresh ID, returned in the record.
        let mut args = args.clone();
        if !args.is_object() {
            args = serde_json::json!({});
        }
        if args["operation_id"].is_null() {
            args["operation_id"] = Value::String(uuid::Uuid::new_v4().to_string());
        }
        let args = &args;
        let result: anyhow::Result<_> = async {
            let page = &self.require_tab().map_err(anyhow::Error::msg)?.page;
            let budget = commands::options(args)?.timeout_ms.min(60_000);
            let record = self
                .agent
                .lock()
                .await
                .command(&self.operations, page, cmd, args.clone())
                .await?;
            if matches!(record.state, eoka_protocol::OperationState::Running) {
                self.operations
                    .wait(&record.id, budget)
                    .await
                    .map_err(commands::uncertain)
            } else {
                Ok(record)
            }
        }
        .await;
        match result {
            Ok(record) => Response::action_result(record),
            Err(error) => {
                commands::failure(error, args["operation_id"].as_str().map(str::to_owned))
            }
        }
    }
}
