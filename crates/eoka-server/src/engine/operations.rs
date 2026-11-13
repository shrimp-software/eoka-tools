use super::*;
use crate::operations::{commands, OperationManager};

impl Engine {
    pub fn operation_manager(&self) -> OperationManager {
        self.operations.clone()
    }

    pub(super) async fn handle_action(&mut self, cmd: &str, args: &Value) -> Response {
        let result: anyhow::Result<_> = async {
            let page = &self.require_tab().map_err(anyhow::Error::msg)?.page;
            let budget = commands::options(args)?.timeout_ms.min(60_000);
            let record = self
                .agent
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
