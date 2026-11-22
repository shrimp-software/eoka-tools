use crate::state::AppState;
use eoka_protocol::sidecar::{validate_safe_numbers, ActionPerformParams, ErrorData, ServerError};
use eoka_protocol::{ErrorDetail, OperationIdArgs, OperationListArgs, OperationWaitArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

pub fn supports(method: &str) -> bool {
    method == "actions.perform" || supports_control(method)
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

pub async fn dispatch(state: &AppState, method: &str, params: Value) -> Result<Value, ServerError> {
    if supports_control(method) {
        return control(&state.operations, method, params).await;
    }
    if method != "actions.perform" {
        return Err(ServerError::unknown_method(method));
    }
    let args: ActionPerformParams = parse(params)?;
    let page = state.page(&args.page_id)?;
    let id = args.operation_id;
    let record = state
        .operations
        .submit(page, id.clone(), args.action, args.options)
        .await
        .map_err(|error| operation_error(error, Some(id)))?;
    encode(record)
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
