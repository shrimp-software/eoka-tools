use super::*;

impl Handler {
    pub(super) async fn handle_agent(&mut self, cmd: &str, args: &Value) -> Response {
        let result: anyhow::Result<Value> = async {
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
                "agent_observe" => Ok(serde_json::to_value(
                    self.agent
                        .observe(&state.browser, &page, serde_json::from_value(args.clone())?)
                        .await?,
                )?),
                "agent_step" => Ok(serde_json::to_value(
                    self.agent
                        .step(
                            &state.browser,
                            &page,
                            &self.operations,
                            serde_json::from_value(args.clone())?,
                        )
                        .await?,
                )?),
                _ => anyhow::bail!("Unknown agent tool"),
            }
        }
        .await;
        if let Some(tab) = self
            .state
            .as_mut()
            .and_then(|state| state.current_tab_mut())
        {
            tab.snapshot_refs = self.agent.references(tab.page.target_id());
        }
        match result {
            Ok(value) => Response::ok(value),
            Err(error) => Response::err_detail(
                eoka_protocol::ErrorDetail::new("agent_error", format!("{error:#}"))
                    .retryable(false),
            ),
        }
    }
}
