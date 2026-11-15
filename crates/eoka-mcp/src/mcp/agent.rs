use super::*;

impl EokaServer {
    pub(super) async fn agent_command(
        &self,
        cmd: &str,
        mut args: Value,
    ) -> Result<CallToolResult, ErrorData> {
        if cmd == "agent_step" && args["operation_id"].is_null() {
            args["operation_id"] = Value::String(uuid::Uuid::new_v4().to_string());
        }
        let id = args["operation_id"].as_str().map(str::to_owned);
        let budget = args["timeout_ms"]
            .as_u64()
            .unwrap_or(if cmd == "agent_step" { 10000 } else { 3000 })
            .min(60000);
        match tokio::time::timeout(std::time::Duration::from_millis(budget), self.agent_command_inner(cmd, args)).await {
            Ok(result) => result,
            Err(_) => protocol_tool_response(match id {
                Some(id) => eoka_server::operations::commands::failure(
                    eoka_server::operations::commands::uncertain(anyhow::anyhow!("Response deadline reached; inspect this operation ID. Admission or execution may still be running; do not replay.")),
                    Some(id),
                ),
                None => eoka_protocol::Response::err_detail(eoka_protocol::ErrorDetail::new("observation_timeout", "Observation deadline reached; no action was requested.")),
            }),
        }
    }

    async fn agent_command_inner(
        &self,
        cmd: &str,
        args: Value,
    ) -> Result<CallToolResult, ErrorData> {
        let (browser, page) = {
            let state = self.state.lock().await;
            let state = state
                .as_ref()
                .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
            let page = state
                .current_tab()
                .ok_or_else(|| ErrorData::from(AgentError::NoTab))?
                .page
                .clone();
            (state.browser.clone(), page)
        };
        let mut agent = self.agent.lock().await;
        let mut response = match cmd {
            "agent_observe" => eoka_protocol::Response::ok(
                serde_json::to_value(
                    agent
                        .observe(
                            &browser,
                            &page,
                            serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?,
                        )
                        .await
                        .map_err(internal)?,
                )
                .map_err(internal)?,
            ),
            "agent_step" => eoka_protocol::Response::agent_result(
                agent
                    .step_selected(
                        &browser,
                        &page,
                        &self.operations,
                        serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?,
                    )
                    .await
                    .map_err(internal)?,
            ),
            _ => return Err(invalid("Unknown agent tool")),
        };
        drop(agent);
        let destination = response
            .data
            .as_ref()
            .and_then(|v| v["observation"]["tab_id"].as_str())
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
