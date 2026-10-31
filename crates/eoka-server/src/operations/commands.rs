use crate::target::{LivePattern, Target};
use anyhow::{anyhow, Result};
use eoka::{Action, ActionOptions, LocatorQuery, LocatorSpec};
use eoka_protocol::{ErrorDetail, Response};
use serde_json::Value;

#[derive(Debug)]
struct OutcomeUnknown(anyhow::Error);

impl std::fmt::Display for OutcomeUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.0)
    }
}

impl std::error::Error for OutcomeUnknown {}

pub fn uncertain(error: anyhow::Error) -> anyhow::Error {
    OutcomeUnknown(error).into()
}

pub fn failure(error: anyhow::Error, id: Option<String>) -> Response {
    let unknown = error.downcast_ref::<OutcomeUnknown>().is_some();
    let status = if unknown {
        eoka_protocol::OperationStatus::OutcomeUnknown
    } else {
        eoka_protocol::OperationStatus::NotDispatched
    };
    let mut response = Response::err_detail(
        ErrorDetail::new(
            if unknown {
                "outcome_unknown"
            } else {
                "invalid_action"
            },
            format!("{error:#}"),
        )
        .retryable(false)
        .context(eoka_protocol::ErrorContext {
            operation_id: id.clone(),
            phase: Some(
                if unknown {
                    "admission_or_execution"
                } else {
                    "validation"
                }
                .into(),
            ),
            ..Default::default()
        }),
    );
    response.data = Some(serde_json::json!({"operation_id":id,"status":status}));
    response
}

pub fn live_locator(target: &str) -> Result<LocatorSpec> {
    let query = match Target::parse(target) {
        Target::Live(LivePattern::Css(selector)) => LocatorQuery::Css { selector },
        Target::Live(LivePattern::Id(id)) => LocatorQuery::Css {
            selector: format!("[id={}]", serde_json::to_string(&id)?),
        },
        Target::Live(LivePattern::Text(text)) => LocatorQuery::Text { text, exact: false },
        Target::Live(LivePattern::Placeholder(text)) => LocatorQuery::Placeholder { text },
        Target::Live(LivePattern::Role(role)) => LocatorQuery::Role { role },
        _ => {
            return Err(anyhow!(
                "Snapshot references and indices require the originating observation"
            ))
        }
    };
    Ok(LocatorSpec {
        query,
        frame_id: None,
        index: None,
    })
}

pub fn is_action(cmd: &str) -> bool {
    matches!(
        cmd,
        "click"
            | "dblclick"
            | "fill"
            | "select"
            | "hover"
            | "key"
            | "back"
            | "forward"
            | "reload"
            | "action_perform"
    )
}

pub fn action(cmd: &str, args: &Value, target: Option<LocatorSpec>) -> Result<Action> {
    let required = || target.clone().ok_or_else(|| anyhow!("Target is required"));
    let string = |key: &str| {
        args[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("Missing {key}"))
    };
    Ok(match cmd {
        "click" | "dblclick" => Action::Click {
            target: required()?,
            human: false,
            double: cmd == "dblclick",
        },
        "fill" => Action::Fill {
            target: required()?,
            value: string("text")?,
            input_mode: serde_json::from_value(
                args.get("input_mode")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("sequential")),
            )?,
        },
        "select" => Action::SelectOption {
            target: required()?,
            value: string("value")?,
        },
        "hover" => Action::Hover {
            target: required()?,
            human: false,
        },
        "key" => Action::PressKey {
            key: string("key")?,
        },
        "back" => Action::Back,
        "forward" => Action::Forward,
        "reload" => Action::Reload,
        _ => return Err(anyhow!("Unsupported action")),
    })
}

pub fn options(args: &Value) -> Result<ActionOptions> {
    let mut options: ActionOptions = serde_json::from_value(
        args.get("options")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )?;
    if let Some(expect) = args.get("expect").filter(|v| !v.is_null()) {
        options.expect = Some(serde_json::from_value(expect.clone())?);
    }
    if let Some(timeout) = args["timeout_ms"].as_u64() {
        options.timeout_ms = timeout;
    }
    Ok(options)
}

pub fn response(result: Result<impl serde::Serialize>) -> Response {
    match result {
        Ok(value) => match serde_json::to_value(value) {
            Ok(value) => Response::ok(value),
            Err(_) => Response::err_detail(ErrorDetail::new(
                "operation_error",
                "Could not encode operation record",
            )),
        },
        Err(error) => Response::err_detail(
            ErrorDetail::new("operation_error", error.to_string()).retryable(false),
        ),
    }
}

impl super::OperationManager {
    pub async fn query(&self, cmd: &str, args: &Value) -> Option<Response> {
        let id = args["id"].as_str().unwrap_or_default();
        Some(match cmd {
            "operation_get" => response(self.get(id).await),
            "operation_cancel" => response(self.cancel(id).await),
            "operation_wait" => match self
                .wait(id, args["timeout_ms"].as_u64().unwrap_or(30_000))
                .await
            {
                Ok(record) => Response::action_result(record),
                Err(error) => Response::err_detail(
                    ErrorDetail::new("operation_error", error.to_string()).retryable(false),
                ),
            },
            "operation_list" => match serde_json::from_value(args.clone()) {
                Ok(args) => response(self.list(args).await),
                Err(error) => Response::err(error.to_string()),
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_admission_preserves_recovery_id_and_uncertainty() {
        let id = uuid::Uuid::new_v4().to_string();
        let response = failure(
            uncertain(anyhow!("storage acknowledgement lost")),
            Some(id.clone()),
        );
        assert_eq!(response.exit_code(), 4);
        assert_eq!(response.data.as_ref().unwrap()["operation_id"], id);
        assert_eq!(
            response.error_detail.unwrap().context.unwrap().operation_id,
            Some(id)
        );
        let rejected = failure(anyhow!("Target is required"), None);
        assert_eq!(rejected.exit_code(), 1);
        assert_eq!(rejected.data.unwrap()["status"], "not_dispatched");
    }
}
