use crate::{protocol::ServerError, state::AppState};
use eoka::{Action, ActionOptions, LocatorSpec};
use serde_json::{json, Value};

pub async fn dispatch(
    state: &mut AppState,
    method: &str,
    params: &Value,
) -> Option<Result<Value, ServerError>> {
    let mapped = method.replace("operations.", "operation_");
    if mapped.starts_with("operation_") {
        let response = state.operations.query(&mapped, params).await?;
        return Some(if response.ok {
            Ok(response.data.unwrap_or(Value::Null))
        } else {
            Err(ServerError::internal(response.error.unwrap_or_default()))
        });
    }
    let supported = matches!(
        method,
        "actions.perform"
            | "page.goto"
            | "page.click"
            | "page.click_text"
            | "page.human_click"
            | "page.human_click_text"
            | "page.fill"
            | "page.human_fill"
            | "page.type_into"
            | "page.select"
            | "page.hover"
            | "page.press_key"
    );
    if !supported {
        return None;
    }
    Some(
        async {
            let page = state
                .page(
                    params["pageId"]
                        .as_str()
                        .ok_or_else(|| ServerError::invalid_params("pageId required"))?,
                )?
                .clone();
            let id = params["operation_id"]
                .as_str()
                .ok_or_else(|| {
                    ServerError::invalid_params("operation_id is required; upgrade the client")
                })?
                .to_owned();
            let options: ActionOptions =
                serde_json::from_value(params.get("options").cloned().unwrap_or(json!({})))
                    .map_err(|e| ServerError::invalid_params(e.to_string()))?;
            let string = |key: &str| {
                params[key]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| ServerError::invalid_params(format!("{key} required")))
            };
            let target = || string("selector").map(LocatorSpec::css);
            let action = match method {
                "actions.perform" => serde_json::from_value(params["action"].clone())
                    .map_err(|e| ServerError::invalid_params(e.to_string()))?,
                "page.goto" => Action::Navigate {
                    url: string("url")?,
                    referrer: None,
                },
                "page.click" | "page.human_click" => Action::Click {
                    target: target()?,
                    human: method == "page.human_click",
                    double: false,
                },
                "page.click_text" | "page.human_click_text" => Action::Click {
                    target: LocatorSpec::text(string("text")?),
                    human: method == "page.human_click_text",
                    double: false,
                },
                "page.fill" | "page.human_fill" => Action::Fill {
                    target: target()?,
                    value: string("value")?,
                    human: method == "page.human_fill",
                },
                "page.type_into" => Action::Type {
                    target: target()?,
                    text: string("text")?,
                    human: false,
                },
                "page.select" => Action::SelectOption {
                    target: target()?,
                    value: string("value")?,
                },
                "page.hover" => Action::Hover {
                    target: target()?,
                    human: false,
                },
                "page.press_key" => Action::PressKey {
                    key: string("key")?,
                },
                _ => return Err(ServerError::unknown_method(method)),
            };
            let record = state
                .operations
                .submit(page, id, action, options)
                .await
                .map_err(|e| ServerError::internal(e.to_string()))?;
            serde_json::to_value(record).map_err(|e| ServerError::internal(e.to_string()))
        }
        .await,
    )
}
