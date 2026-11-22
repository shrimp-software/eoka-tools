use crate::state::AppState;
use eoka_protocol::sidecar::{
    validate_safe_numbers, ActionPerformParams, ErrorData, Method, ServerError,
};
use eoka_protocol::{ErrorDetail, OperationIdArgs, OperationListArgs, OperationWaitArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationControl {
    Get,
    List,
    Wait,
    Cancel,
}

impl OperationControl {
    pub(crate) fn of(method: Method) -> Option<Self> {
        match method {
            Method::OperationsGet => Some(Self::Get),
            Method::OperationsList => Some(Self::List),
            Method::OperationsWait => Some(Self::Wait),
            Method::OperationsCancel => Some(Self::Cancel),
            _ => None,
        }
    }
}

pub(crate) async fn existing_control(
    manager: &super::OperationManager,
    control: OperationControl,
    params: &Value,
) -> Option<Result<Value, ServerError>> {
    validate_safe_numbers(params).ok()?;
    let args = OperationIdArgs::deserialize(params).ok()?;
    let record = manager.get(&args.id).await.ok()?;
    Some(if control == OperationControl::Get {
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

pub async fn perform(state: &AppState, params: Value) -> Result<Value, ServerError> {
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
    control: OperationControl,
    params: Value,
) -> Result<Value, ServerError> {
    validate_safe_numbers(&params)?;
    match control {
        OperationControl::Get | OperationControl::Cancel => {
            let args: OperationIdArgs = parse(params)?;
            let record = if control == OperationControl::Get {
                manager.get(&args.id).await
            } else {
                manager.cancel(&args.id).await
            }
            .map_err(|error| operation_error(error, Some(args.id)))?;
            encode(record)
        }
        OperationControl::List => {
            let args: OperationListArgs = parse(params)?;
            encode(
                manager
                    .list(args)
                    .await
                    .map_err(|error| operation_error(error, None))?,
            )
        }
        OperationControl::Wait => wait_inner(manager, params).await,
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
