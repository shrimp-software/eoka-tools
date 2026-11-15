use super::*;

impl EokaServer {
    pub(super) async fn agent_tool(
        &self,
        tool: &str,
        request: Operation,
    ) -> Result<CallToolResult, ErrorData> {
        let (mut response, _) = self.respond(tool, request).await?;
        let destination = response
            .data
            .as_ref()
            .and_then(|data| data["observation"]["tab_id"].as_str())
            .map(str::to_owned);
        if let Some(id) = destination {
            if let Some(state) = self.state.lock().await.as_mut() {
                if state.current_tab_id.as_deref() != Some(id.as_str()) {
                    if let Err(error) = state.switch_tab(&id).await {
                        if let Some(data) = response.data.as_mut() {
                            data["observation_error"] =
                                serde_json::json!(format!("Destination selection failed: {error}"));
                        }
                    }
                }
            }
        }
        protocol_tool_response(response)
    }
}
