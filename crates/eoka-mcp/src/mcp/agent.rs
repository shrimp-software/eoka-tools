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
        tokio::time::timeout(std::time::Duration::from_millis(budget), self.agent_command_inner(cmd, args)).await.unwrap_or_else(|_| {
            Err(internal(match id {
                Some(id) => format!("Response deadline reached; inspect operation {id}. Admission or execution may still be running; do not replay."),
                None => "Observation deadline reached; no action was requested.".into(),
            }))
        })
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
        let result = match cmd {
            "agent_observe" => serde_json::to_value(
                agent
                    .observe(
                        &browser,
                        &page,
                        serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?,
                    )
                    .await
                    .map_err(internal)?,
            ),
            "agent_step" => serde_json::to_value(
                agent
                    .step(
                        &browser,
                        &page,
                        &self.operations,
                        serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?,
                    )
                    .await
                    .map_err(internal)?,
            ),
            _ => return Err(invalid("Unknown agent tool")),
        }
        .map_err(internal)?;
        let references = agent.references(page.target_id());
        drop(agent);
        if let Some(tab) = self
            .state
            .lock()
            .await
            .as_mut()
            .and_then(|state| state.tabs.get_mut(page.target_id()))
        {
            tab.snapshot_refs = references;
        }
        protocol_tool_response(eoka_protocol::Response::ok(result))
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
                        serde_json::json!({"action":{"kind":"click","target":button}}),
                    )
                    .unwrap(),
                ))
                .await
                .unwrap(),
        );
        assert_eq!(click["operation"]["outcome"], "outcome_unknown");
        assert!(click["operation_id"].as_str().is_some());
        assert!(click["observation"]["page"]["text_excerpt"]
            .as_str()
            .unwrap()
            .contains("Saved"));
        assert_eq!(page.evaluate::<u32>("window.hits").await.unwrap(), 1);
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
