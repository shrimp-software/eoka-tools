use super::*;
use crate::operations::{commands, OperationManager};

const FAILURE_REPORT_GRACE_MS: u64 = 2_000;

impl Engine {
    pub fn operation_manager(&self) -> OperationManager {
        self.operations.clone()
    }

    pub(super) async fn handle_action(&mut self, cmd: &str, args: &Value) -> Response {
        let mut args = args.clone();
        if !args.is_object() {
            args = serde_json::json!({});
        }
        if args["operation_id"].is_null() {
            args["operation_id"] = Value::String(uuid::Uuid::new_v4().to_string());
        }
        let args = &args;
        let reloads = cmd == "reload" || args["action"]["kind"] == "reload";
        let intercept = match nav_intercept(cmd, args) {
            Ok(intercept) => intercept,
            Err(error) => {
                return commands::failure(error, args["operation_id"].as_str().map(str::to_owned))
            }
        };
        let mut script_applied = None;
        let result: anyhow::Result<_> = async {
            let page = &self.require_tab().map_err(anyhow::Error::msg)?.page;
            if reloads && self.script_policy.is_active() {
                let enabled = self.script_enabled_for(&page.url().await?);
                if let Some(enabled) = enabled {
                    page.set_javascript_enabled(enabled).await?;
                    script_applied = Some(enabled);
                }
            }
            if let Some(allow) = intercept {
                let id = args["operation_id"].as_str().unwrap_or_default();
                if self.operations.get(id).await.is_err() {
                    page.execute_sync(&nav_intercept_script(allow)).await?;
                }
            }
            let budget = commands::options(args)?
                .timeout_ms
                .min(60_000)
                .saturating_add(FAILURE_REPORT_GRACE_MS);
            let record = self
                .agent
                .lock()
                .await
                .command(&self.operations, page, cmd, args.clone())
                .await?;
            if matches!(record.state, eoka_protocol::OperationState::Running) {
                blockers::wait_unless_dialog(&self.operations, page, &record.id, budget)
                    .await
                    .map_err(commands::uncertain)
            } else {
                Ok(record)
            }
        }
        .await;
        if let Some(enabled) = script_applied {
            self.script_policy.note_applied(enabled);
        }
        match result {
            Ok(record) => Response::action_result(record),
            Err(error) => {
                commands::failure(error, args["operation_id"].as_str().map(str::to_owned))
            }
        }
    }

    pub(super) async fn action_batch(&mut self, batch: eoka_protocol::ActionBatchArgs) -> Response {
        let mut records = Vec::new();
        for action in batch.actions {
            let args = match serde_json::to_value(action) {
                Ok(args) => args,
                Err(error) => return Response::err(error.to_string()),
            };
            let response = Box::pin(self.handle_action("action_perform", &args)).await;
            let Some(record) = response
                .data
                .clone()
                .filter(|data| data.get("id").is_some())
            else {
                return response;
            };
            records.push(record);
            if !response.ok {
                break;
            }
        }
        Response::ok(Value::Array(records))
    }
}

fn nav_intercept(cmd: &str, args: &Value) -> anyhow::Result<Option<bool>> {
    if cmd != "click" || args["intercept_nav"].is_null() {
        return Ok(None);
    }
    if args["expect"].is_null() {
        anyhow::bail!("intercept_nav requires expect to verify the click outcome");
    }
    let mode: eoka_protocol::NavIntercept = serde_json::from_value(args["intercept_nav"].clone())?;
    Ok(Some(mode == eoka_protocol::NavIntercept::Allow))
}

fn nav_intercept_script(allow: bool) -> String {
    format!(
        r#"(() => {{
            window.__eoka_nav = null;
            window.__eoka_allow_nav = {allow};
            if (window.__eoka_nav_installed) return;
            window.__eoka_nav_installed = true;
            for (const method of ['pushState', 'replaceState']) {{
                const original = history[method].bind(history);
                history[method] = function(state, title, url) {{
                    window.__eoka_nav = {{ method: 'history.' + method, url: String(url) }};
                    if (window.__eoka_allow_nav) return original(state, title, url);
                }};
            }}
        }})()"#
    )
}
