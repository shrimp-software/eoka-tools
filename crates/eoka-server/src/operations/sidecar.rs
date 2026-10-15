use crate::state::AppState;
use eoka::{Action, ActionOptions, LocatorSpec};
use eoka_protocol::sidecar::{
    validate_safe_numbers, ActionPerformParams, ClickParams, ClickTextParams, ErrorData,
    FillParams, GotoParams, PressKeyParams, ServerError, TypeIntoParams,
};
use eoka_protocol::{ErrorDetail, OperationIdArgs, OperationListArgs, OperationWaitArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

pub fn supports(method: &str) -> bool {
    supports_control(method)
        || matches!(
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
        )
}

pub(crate) fn supports_control(method: &str) -> bool {
    matches!(
        method,
        "operations.get" | "operations.list" | "operations.wait" | "operations.cancel"
    )
}

pub(crate) async fn existing_control(
    manager: &super::OperationManager,
    method: &str,
    params: &Value,
) -> Option<Result<Value, ServerError>> {
    validate_safe_numbers(params).ok()?;
    let args = OperationIdArgs::deserialize(params).ok()?;
    let record = manager.get(&args.id).await.ok()?;
    Some(if method == "operations.get" {
        encode(record)
    } else {
        manager
            .cancel_admitted(&args.id, record)
            .map_err(|error| operation_error(error, Some(args.id)))
            .and_then(encode)
    })
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, ServerError> {
    serde_json::from_value(params).map_err(|error| ServerError::invalid_params(error.to_string()))
}

fn operation_error(error: anyhow::Error, id: Option<String>) -> ServerError {
    let mut error = ServerError::from_detail(
        ErrorDetail::new("operation_error", error.to_string()).retryable(false),
    );
    error
        .data
        .get_or_insert_with(Box::<ErrorData>::default)
        .operation_id = id;
    error
}

fn encode<T: Serialize>(value: T) -> Result<Value, ServerError> {
    serde_json::to_value(value).map_err(|error| ServerError::internal(error.to_string()))
}

pub async fn dispatch(
    state: &mut AppState,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    if supports_control(method) {
        return control(&state.operations, method, params).await;
    }
    let (page_id, id, action, options) = admission(method, params)?;
    let page = state.page(&page_id)?.clone();
    let record = state
        .operations
        .submit(page, id.clone(), action, options)
        .await
        .map_err(|error| operation_error(error, Some(id)))?;
    encode(record)
}

fn admission(
    method: &str,
    params: Value,
) -> Result<(String, String, Action, ActionOptions), ServerError> {
    Ok(match method {
        "actions.perform" => {
            let args: ActionPerformParams = parse(params)?;
            (args.page_id, args.operation_id, args.action, args.options)
        }
        "page.goto" => {
            let args: GotoParams = parse(params)?;
            (
                args.page_id,
                args.operation_id,
                Action::Navigate {
                    url: args.url,
                    referrer: None,
                },
                args.options,
            )
        }
        "page.click" | "page.human_click" | "page.hover" => {
            let args: ClickParams = parse(params)?;
            let target = LocatorSpec::css(args.selector);
            let action = if method == "page.hover" {
                Action::Hover {
                    target,
                    human: false,
                }
            } else {
                Action::Click {
                    target,
                    human: method == "page.human_click",
                    double: false,
                }
            };
            (args.page_id, args.operation_id, action, args.options)
        }
        "page.click_text" | "page.human_click_text" => {
            let args: ClickTextParams = parse(params)?;
            (
                args.page_id,
                args.operation_id,
                Action::Click {
                    target: LocatorSpec::text(args.text),
                    human: method == "page.human_click_text",
                    double: false,
                },
                args.options,
            )
        }
        "page.fill" | "page.human_fill" | "page.select" => {
            let args: FillParams = parse(params)?;
            let target = LocatorSpec::css(args.selector);
            let action = if method == "page.select" {
                Action::SelectOption {
                    target,
                    value: args.value,
                }
            } else {
                Action::Fill {
                    target,
                    value: args.value,
                    human: method == "page.human_fill",
                }
            };
            (args.page_id, args.operation_id, action, args.options)
        }
        "page.type_into" => {
            let args: TypeIntoParams = parse(params)?;
            (
                args.page_id,
                args.operation_id,
                Action::Type {
                    target: LocatorSpec::css(args.selector),
                    text: args.text,
                    human: false,
                },
                args.options,
            )
        }
        "page.press_key" => {
            let args: PressKeyParams = parse(params)?;
            (
                args.page_id,
                args.operation_id,
                Action::PressKey { key: args.key },
                args.options,
            )
        }
        _ => return Err(ServerError::unknown_method(method)),
    })
}

pub async fn wait(manager: &super::OperationManager, params: Value) -> Result<Value, ServerError> {
    validate_safe_numbers(&params)?;
    wait_inner(manager, params).await
}

pub(crate) async fn control(
    manager: &super::OperationManager,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    if method == "operations.wait" {
        return wait(manager, params).await;
    }
    validate_safe_numbers(&params)?;
    match method {
        "operations.get" | "operations.cancel" => {
            let args: OperationIdArgs = parse(params)?;
            let record = if method == "operations.get" {
                manager.get(&args.id).await
            } else {
                manager.cancel(&args.id).await
            }
            .map_err(|error| operation_error(error, Some(args.id)))?;
            encode(record)
        }
        "operations.list" => {
            let args: OperationListArgs = parse(params)?;
            encode(
                manager
                    .list(args)
                    .await
                    .map_err(|error| operation_error(error, None))?,
            )
        }
        _ => Err(ServerError::unknown_method(method)),
    }
}

async fn wait_inner(
    manager: &super::OperationManager,
    params: Value,
) -> Result<Value, ServerError> {
    let args: OperationWaitArgs = parse(params)?;
    if args.timeout_ms > 86_400_000 {
        return Err(ServerError::invalid_params("Wait timeout exceeds 24 hours"));
    }
    let record = manager
        .wait(&args.id, args.timeout_ms)
        .await
        .map_err(|error| operation_error(error, Some(args.id)))?;
    encode(record)
}
