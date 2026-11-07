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

#[cfg(test)]
mod tests {
    use super::*;

    fn data(result: CallToolResult) -> Value {
        let value = serde_json::to_value(result).unwrap();
        let envelope: Value =
            serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(envelope["ok"], true);
        envelope["data"].clone()
    }

    #[tokio::test]
    #[ignore = "requires Chrome; MCP agent loop"]
    async fn actions_work_without_ids_and_find_fields_by_label() {
        let directory = tempfile_directory();
        let mut server = EokaServer::new();
        server.operations =
            eoka_server::operations::OperationManager::in_directory(directory.clone());
        server.ensure_browser().await.unwrap();
        let page = {
            let mut guard = server.state.lock().await;
            let state = guard.as_mut().unwrap();
            state
                .new_tab(Some("about:blank"))
                .await
                .unwrap()
                .1
                .page
                .clone()
        };
        page.execute("document.body.innerHTML='<label>Name<input id=name></label><button id=go onclick=\\'document.body.insertAdjacentHTML(`beforeend`,`<p id=done>Done</p>`)\\'>Join</button>'")
            .await
            .unwrap();

        let fill = data(
            server
                .fill(Parameters(
                    serde_json::from_value(serde_json::json!({"target":"Name","text":"Ada"}))
                        .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(fill["outcome"], "verified_complete");
        assert!(uuid::Uuid::parse_str(fill["id"].as_str().unwrap()).is_ok());
        assert_eq!(
            page.evaluate::<String>("document.querySelector('#name').value")
                .await
                .unwrap(),
            "Ada"
        );

        let unverified = data(
            server
                .click(Parameters(
                    serde_json::from_value(serde_json::json!({"target":"Join"})).unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(unverified["status"], "outcome_unknown");
        assert!(unverified["note"]
            .as_str()
            .unwrap()
            .contains("not verified"));
        assert!(page
            .evaluate::<bool>("!!document.querySelector('#done')")
            .await
            .unwrap());
        page.execute("document.querySelector('#done').remove()")
            .await
            .unwrap();

        let click = data(
            server
                .click(Parameters(
                    serde_json::from_value(serde_json::json!({
                        "target":"Join",
                        "expect":{"kind":"visible","target":{"query":{"kind":"css","selector":"#done"}}}
                    }))
                    .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(click["outcome"], "verified_complete");
        let step = data(
            server
                .agent_step(Parameters(
                    serde_json::from_value(serde_json::json!({
                        "action":{"kind":"fill","target":"Name","text":"Grace"}
                    }))
                    .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(step["operation"]["outcome"], "verified_complete");
        server.close().await.unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome; MCP agent loop"]
    async fn mcp_agent_step_returns_fresh_state_and_preserves_unchanged_targets() {
        let directory = tempfile_directory();
        let mut server = EokaServer::new();
        server.operations =
            eoka_server::operations::OperationManager::in_directory(directory.clone());
        server.ensure_browser().await.unwrap();
        let page = {
            let mut guard = server.state.lock().await;
            let state = guard.as_mut().unwrap();
            state
                .new_tab(Some("about:blank"))
                .await
                .unwrap()
                .1
                .page
                .clone()
        };
        page.execute("document.body.innerHTML='<label>Name<input id=name></label><button id=go>Save</button><p id=status></p>';window.hits=0;document.querySelector('#go').onclick=()=>{window.hits++;document.querySelector('#status').textContent='Saved'}").await.unwrap();
        let view = data(
            server
                .agent_observe(Parameters(Default::default()))
                .await
                .unwrap(),
        );
        let controls = view["controls"].as_array().unwrap();
        let field = controls
            .iter()
            .find(|control| control["name"] == "Name")
            .unwrap()["target"]
            .clone();
        let button = controls
            .iter()
            .find(|control| control["name"] == "Save")
            .unwrap()["target"]
            .clone();
        let fill = data(
            server
                .agent_step(Parameters(
                    serde_json::from_value(
                        serde_json::json!({"action":{"kind":"fill","target":field,"text":"Ada"}}),
                    )
                    .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(fill["operation"]["outcome"], "verified_complete");
        assert!(fill["observation"]["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|control| control["target"] == button));
        let click = data(
            server
                .agent_step(Parameters(
                    serde_json::from_value(
                        serde_json::json!({"action":{"kind":"click","target":button},"expect":{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}}),
                    )
                    .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(click["operation"]["outcome"], "verified_complete");
        assert!(click["operation_id"].as_str().is_some());
        assert!(click["observation"]["page"]["text_excerpt"]
            .as_str()
            .unwrap()
            .contains("Saved"));
        assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
        page.execute("document.querySelector('#go').onclick=()=>{window.hits++;const child=window.open('about:blank');child.document.body.innerHTML='<button id=ready>Ready</button>'}").await.unwrap();
        let request: eoka_protocol::AgentStepArgs = serde_json::from_value(serde_json::json!({
            "operation_id":uuid::Uuid::new_v4().to_string(),
            "action":{"kind":"click","target":button},
            "expect":{"kind":"visible","target":{"query":{"kind":"css","selector":"#ready"}}}
        }))
        .unwrap();
        let first = data(
            server
                .agent_step(Parameters(request.clone()))
                .await
                .unwrap(),
        );
        let second = data(
            server
                .agent_step(Parameters(request.clone()))
                .await
                .unwrap(),
        );
        assert_eq!(first["operation"], second["operation"]);
        assert_eq!(
            first["observation"]["tab_id"],
            second["observation"]["tab_id"]
        );
        assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 2);
        let (browser, selected) = {
            let guard = server.state.lock().await;
            let state = guard.as_ref().unwrap();
            (
                state.browser.clone(),
                state.current_tab().unwrap().page.clone(),
            )
        };
        assert_ne!(selected.target_id(), page.target_id());
        let explicit = server
            .agent
            .lock()
            .await
            .step(&browser, &selected, &server.operations, request)
            .await
            .unwrap();
        assert!(explicit
            .operation_error
            .unwrap()
            .message
            .contains("operation_id_conflict"));
        drop(browser);
        server.close().await.unwrap();
        drop(server);
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn tempfile_directory() -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("eoka-mcp-agent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        directory
    }
}
