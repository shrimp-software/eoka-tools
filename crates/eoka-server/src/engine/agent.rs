use super::*;

pub(super) enum AgentCall {
    Observe(eoka_protocol::AgentObserveArgs),
    Step(eoka_protocol::AgentStepArgs),
}

impl Engine {
    pub(super) async fn handle_agent(&mut self, call: AgentCall) -> Response {
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
            match call {
                AgentCall::Observe(args) => Ok(Response::ok(serde_json::to_value(
                    self.agent
                        .lock()
                        .await
                        .observe(&state.browser, &page, args)
                        .await?,
                )?)),
                AgentCall::Step(args) => Ok(Response::agent_result(
                    self.agent
                        .lock()
                        .await
                        .step_selected(&state.browser, &page, &self.operations, args)
                        .await?,
                )),
            }
        }
        .await;
        let mut moved = false;
        if let Ok(response) = &mut result {
            if let Some(id) = response
                .data
                .as_ref()
                .and_then(|v| v["observation"]["tab_id"].as_str())
                .map(str::to_owned)
            {
                if let Some(state) = self.state.as_mut() {
                    if state.current_tab_id.as_deref() != Some(id.as_str()) {
                        match state.attach_existing_tab(&id).await {
                            Ok(()) => moved = true,
                            Err(error) => {
                                if let Some(data) = response.data.as_mut() {
                                    data["observation_error"] =
                                        json!(format!("Destination selection failed: {error}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        if moved {
            if let Err(error) = self.sync_current_target_features().await {
                eprintln!("[eoka] could not carry interception to the new tab: {error}");
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
