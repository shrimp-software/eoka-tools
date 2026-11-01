use super::*;
use eoka_server::operations::commands;

impl EokaServer {
    pub(super) async fn perform_command(
        &self,
        cmd: &str,
        args: Value,
    ) -> Result<CallToolResult, ErrorData> {
        let page = {
            let guard = self.state.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?
                .current_tab()
                .ok_or_else(|| ErrorData::from(AgentError::NoTab))?
                .page
                .clone()
        };
        let budget = commands::options(&args)
            .map_err(internal)?
            .timeout_ms
            .min(60_000);
        let id = args["operation_id"].as_str().map(str::to_owned);
        let record = self
            .agent
            .lock()
            .await
            .command(&self.operations, &page, cmd, args)
            .await;
        let record = match record {
            Ok(record) => record,
            Err(error) => return protocol_tool_response(commands::failure(error, id)),
        };
        let record = if matches!(record.state, eoka_protocol::OperationState::Running) {
            match self.operations.wait(&record.id, budget).await {
                Ok(record) => record,
                Err(error) => {
                    return protocol_tool_response(commands::failure(
                        commands::uncertain(error),
                        Some(record.id),
                    ))
                }
            }
        } else {
            record
        };
        protocol_tool_response(eoka_protocol::Response::action_result(record))
    }
}
