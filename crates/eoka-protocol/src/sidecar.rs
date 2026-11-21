use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub use crate::{OperationIdArgs, OperationListArgs, OperationWaitArgs};

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_IN_FLIGHT: usize = 128;
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 3, minor: 0 };
pub const MAX_BOUNDED_FETCH_BYTES: usize = 1024 * 1024;
pub const MAX_SCREENSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RECORDING_FPS: u32 = 120;
pub const CAPABILITY_SCREENSHOT_OPTIONS: &str = "screenshotOptions";
pub const CAPABILITY_SCREENSHOT_FILE: &str = "screenshotFile";
pub const CAPABILITY_NATIVE_RECORDING: &str = "nativeRecording";
pub const CAPABILITY_CLOUDFLARE_AUTO_SOLVE: &str = "cloudflareAutoSolve";
pub const CAPABILITY_OPERATION_CATALOG: &str = "operationCatalog";
pub const CAPABILITY_PAGE_BLOCKERS: &str = "pageBlockers";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HelloParams {
    pub protocol: ProtocolVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TransportLimits {
    pub max_message_bytes: usize,
    pub max_in_flight: usize,
}

impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: MAX_MESSAGE_BYTES,
            max_in_flight: MAX_IN_FLIGHT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub protocol: ProtocolVersion,
    pub server_version: String,
    pub core_version: String,
    pub methods: Vec<String>,
    pub capabilities: Vec<String>,
    pub limits: TransportLimits,
}

pub fn valid_request_id(id: &Value) -> bool {
    id.as_u64()
        .is_some_and(|id| id > 0 && id <= MAX_SAFE_INTEGER)
}

fn deserialize_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Value, D::Error> {
    let id = Value::deserialize(deserializer)?;
    if valid_request_id(&id) {
        Ok(id)
    } else {
        Err(serde::de::Error::custom(
            "id must be a positive integer no greater than 9007199254740991",
        ))
    }
}

fn request_id_schema(_: &mut SchemaGenerator) -> Schema {
    json!({"type":"integer", "minimum":1, "maximum":MAX_SAFE_INTEGER})
        .try_into()
        .unwrap()
}

fn response_id_schema(_: &mut SchemaGenerator) -> Schema {
    json!({"anyOf":[{"type":"integer","minimum":1,"maximum":MAX_SAFE_INTEGER},{"type":"null"}]})
        .try_into()
        .unwrap()
}

fn deserialize_response_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Value, D::Error> {
    let id = Value::deserialize(deserializer)?;
    if id.is_null() || valid_request_id(&id) {
        Ok(id)
    } else {
        Err(serde::de::Error::custom(
            "response id must be null or a safe positive integer",
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    #[serde(deserialize_with = "deserialize_id")]
    #[schemars(schema_with = "request_id_schema")]
    pub id: Value,
    pub method: String,
    #[serde(default = "empty_params")]
    pub params: Value,
}

fn empty_params() -> Value {
    json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum Response {
    Success {
        #[schemars(schema_with = "response_id_schema")]
        #[serde(deserialize_with = "deserialize_response_id")]
        id: Value,
        result: Value,
    },
    Failure {
        #[schemars(schema_with = "response_id_schema")]
        #[serde(deserialize_with = "deserialize_response_id")]
        id: Value,
        error: ServerError,
    },
}

impl Response {
    pub fn ok(id: impl Into<Value>, result: Value) -> Self {
        Self::Success {
            id: response_id(id.into()),
            result,
        }
    }

    pub fn err(id: impl Into<Value>, error: ServerError) -> Self {
        Self::Failure {
            id: response_id(id.into()),
            error,
        }
    }
}

fn response_id(id: Value) -> Value {
    if valid_request_id(&id) {
        id
    } else {
        Value::Null
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ErrorCode {
    ElementNotFound,
    ElementNotVisible,
    Timeout,
    RetryExhausted,
    Cdp,
    InvalidPage,
    InvalidParams,
    UnknownMethod,
    Internal,
    ProtocolMismatch,
    MessageTooLarge,
    ServerBusy,
    ShuttingDown,
    Cancelled,
    Action,
    Operation,
    ScreenshotTooLarge,
    FileOutput,
    RecordingUnsupported,
    RecordingNotFound,
    RecordingConflict,
    RecordingAlreadyActive,
    RecordingInvalidState,
    RecordingFailed,
    DialogOpen,
    NavigationPending,
    PageUnresponsive,
}

impl ErrorCode {
    pub fn blocker_detail_code(self) -> Option<&'static str> {
        BLOCKER_ERROR_DETAIL_CODES
            .iter()
            .find(|(code, _)| *code == self)
            .map(|(_, detail)| *detail)
    }

    pub fn for_blocker_detail(detail_code: &str) -> Option<Self> {
        BLOCKER_ERROR_DETAIL_CODES
            .iter()
            .find(|(_, detail)| *detail == detail_code)
            .map(|(code, _)| *code)
    }
}

const BLOCKER_ERROR_DETAIL_CODES: &[(ErrorCode, &str)] = &[
    (ErrorCode::DialogOpen, "dialog_open"),
    (ErrorCode::NavigationPending, "navigation_pending"),
    (ErrorCode::PageUnresponsive, "page_unresponsive"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageBlocker {
    #[serde(rename_all = "camelCase")]
    Dialog {
        dialog_type: String,
        message: String,
        url: String,
        elapsed_ms: u64,
    },
    #[serde(rename_all = "camelCase")]
    Navigation {
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        elapsed_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CdpErrorDetail {
    pub method: Option<String>,
    pub code: Option<i64>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ErrorData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<crate::ActionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<crate::ErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdp: Option<CdpErrorDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Box<ServerError>>,
    #[serde(rename = "recordingId", skip_serializing_if = "Option::is_none")]
    pub recording_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording: Option<RecordingRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "partialPath", skip_serializing_if = "Option::is_none")]
    pub partial_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<PageBlocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Box<ErrorData>>,
}

impl ServerError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
    pub fn with_data(mut self, data: ErrorData) -> Self {
        self.data = Some(Box::new(data));
        self
    }
    pub fn from_detail(detail: crate::ErrorDetail) -> Self {
        Self::new(ErrorCode::Operation, detail.message.clone()).with_data(ErrorData {
            detail: Some(detail),
            ..Default::default()
        })
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }
    pub fn invalid_page(page_id: &str) -> Self {
        Self::new(
            ErrorCode::InvalidPage,
            format!("unknown pageId \"{page_id}\""),
        )
    }
    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            ErrorCode::UnknownMethod,
            format!("unknown method \"{method}\""),
        )
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn blocked(detail: crate::ErrorDetail, blocker: Option<PageBlocker>) -> Self {
        let code = ErrorCode::for_blocker_detail(&detail.code).unwrap_or(ErrorCode::Operation);
        Self::new(code, detail.message.clone()).with_data(ErrorData {
            detail: Some(detail),
            blocker,
            ..Default::default()
        })
    }
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ServerError {}

const CORE_ERROR_DETAIL_CODES: &[(ErrorCode, &str)] = &[
    (ErrorCode::ElementNotFound, "element_not_found"),
    (ErrorCode::ElementNotVisible, "element_not_visible"),
    (ErrorCode::Timeout, "timeout"),
    (ErrorCode::RetryExhausted, "retry_exhausted"),
    (ErrorCode::Cdp, "cdp_error"),
    (ErrorCode::MessageTooLarge, "message_too_large"),
];

pub fn core_error_detail_code(code: ErrorCode) -> Option<&'static str> {
    CORE_ERROR_DETAIL_CODES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, detail)| *detail)
}

pub fn core_error_code(detail_code: &str) -> Option<ErrorCode> {
    CORE_ERROR_DETAIL_CODES
        .iter()
        .find(|(_, candidate)| *candidate == detail_code)
        .map(|(code, _)| *code)
}

impl From<eoka::Error> for ServerError {
    fn from(error: eoka::Error) -> Self {
        let message = error.to_string();
        match error {
            eoka::Error::Action {
                code,
                receipt,
                source,
            } => Self::new(ErrorCode::Action, message).with_data(ErrorData {
                detail: Some(crate::ErrorDetail::new(code, source.to_string())),
                receipt: Some(*receipt),
                source: Some(Box::new(Self::from(*source))),
                ..Default::default()
            }),
            eoka::Error::Cdp {
                method,
                code,
                message: cdp_message,
            } => Self::new(ErrorCode::Cdp, message).with_data(ErrorData {
                cdp: Some(CdpErrorDetail {
                    method,
                    code,
                    message: cdp_message,
                }),
                ..Default::default()
            }),
            eoka::Error::ElementNotFound(_) => Self::new(ErrorCode::ElementNotFound, message),
            eoka::Error::ElementNotVisible { .. } => {
                Self::new(ErrorCode::ElementNotVisible, message)
            }
            eoka::Error::Timeout(_) => Self::new(ErrorCode::Timeout, message),
            eoka::Error::EvaluationTooLarge { .. } => {
                Self::new(ErrorCode::MessageTooLarge, message)
            }
            eoka::Error::RetryExhausted { .. } => Self::new(ErrorCode::RetryExhausted, message),
            eoka::Error::InvalidScreenshotOptions(_) => {
                Self::new(ErrorCode::InvalidParams, message)
            }
            _ => Self::internal(message),
        }
    }
}

macro_rules! dto {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
        #[serde(rename_all = "camelCase")]
        pub struct $name { $($(#[$attr])* pub $field: $ty),* }
    };
}

mod capture;
mod methods;
mod params;

pub use capture::*;
pub use methods::*;
pub use params::*;

#[cfg(test)]
mod tests;
