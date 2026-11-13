use super::*;

impl Engine {
    pub(super) async fn handle_agent(&mut self, cmd: &str, args: &Value) -> Response {
        let mut result: anyhow::Result<Response> = async {
            let state = self
                .state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Open a browser page first"))?;
            let page = state
                .current_tab()
                .ok_or_else(|| anyhow::anyhow!("Open a tab first"))?
                .page
                .clone();
            match cmd {
                "agent_observe" => Ok(Response::ok(serde_json::to_value(
                    self.agent
                        .observe(&state.browser, &page, serde_json::from_value(args.clone())?)
                        .await?,
                )?)),
                "agent_step" => Ok(Response::agent_result(
                    self.agent
                        .step_selected(
                            &state.browser,
                            &page,
                            &self.operations,
                            serde_json::from_value(args.clone())?,
                        )
                        .await?,
                )),
                _ => anyhow::bail!("Unknown agent tool"),
            }
        }
        .await;
        if let Ok(response) = &mut result {
            if let Some(id) = response
                .data
                .as_ref()
                .and_then(|v| v["observation"]["tab_id"].as_str())
                .map(str::to_owned)
            {
                if let Some(state) = self.state.as_mut() {
                    if state.current_tab_id.as_deref() != Some(id.as_str()) {
                        if let Err(error) = state.attach_existing_tab(&id).await {
                            if let Some(data) = response.data.as_mut() {
                                data["observation_error"] =
                                    json!(format!("Destination selection failed: {error}"));
                            }
                        }
                    }
                }
            }
        }
        match result {
            Ok(response) => response,
            Err(error) => Response::err_detail(
                eoka_protocol::ErrorDetail::new("agent_error", format!("{error:#}"))
                    .retryable(false),
            ),
        }
    }
}
