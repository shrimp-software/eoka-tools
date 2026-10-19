use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub use crate::{OperationIdArgs, OperationListArgs, OperationWaitArgs};

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_IN_FLIGHT: usize = 128;
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 3 };
pub const MAX_BOUNDED_FETCH_BYTES: usize = 1024 * 1024;
pub const MAX_SCREENSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RECORDING_FPS: u32 = 120;
pub const CAPABILITY_SCREENSHOT_OPTIONS: &str = "screenshotOptions";
pub const CAPABILITY_SCREENSHOT_FILE: &str = "screenshotFile";
pub const CAPABILITY_NATIVE_RECORDING: &str = "nativeRecording";
pub const CAPABILITY_CLOUDFLARE_AUTO_SOLVE: &str = "cloudflareAutoSolve";

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
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ServerError {}

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

dto!(LaunchParams {
    #[serde(default = "default_headless")] headless: bool,
    user_agent: Option<String>, timezone: Option<String>,
    #[schemars(range(min = 1, max = 2147483647))] viewport_width: Option<u32>,
    #[schemars(range(min = 1, max = 2147483647))] viewport_height: Option<u32>,
    user_data_dir: Option<String>, proxy: Option<ProxyParams>,
    executable_path: Option<String>, runtime_dir: Option<String>,
    #[serde(default)] sandbox: bool,
    #[serde(default)] aggressive_cdp_evasion: bool,
    #[serde(default)] geo_align: bool,
    #[serde(default)] extra_args: Vec<String>,
    #[schemars(schema_with = "optional_positive_scale_schema")]
    device_scale_factor: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_solve_cloudflare: Option<CloudflareAutoSolve>,
});
dto!(ProxyParams { server: String, username: Option<String>, password: Option<String> });
fn default_headless() -> bool {
    true
}
fn default_device_scale_factor() -> f64 {
    1.0
}

fn positive_scale_schema(_: &mut SchemaGenerator) -> Schema {
    json!({"type":"number","exclusiveMinimum":0})
        .try_into()
        .unwrap()
}

fn optional_positive_scale_schema(_: &mut SchemaGenerator) -> Schema {
    json!({"type":["number","null"],"exclusiveMinimum":0})
        .try_into()
        .unwrap()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectParams {
    pub ws_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_solve_cloudflare: Option<CloudflareAutoSolve>,
}

impl ConnectParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        self.auto_solve_cloudflare
            .as_ref()
            .map_or(Ok(()), CloudflareAutoSolve::validate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BrowserOwnership {
    Owned,
    Borrowed,
}

dto!(BrowserSessionResult {
    ownership: BrowserOwnership,
    browser_version: String,
});
dto!(SetViewportParams {
    page_id: String,
    #[schemars(range(min = 1, max = 2147483647))]
    width: u32,
    #[schemars(range(min = 1, max = 2147483647))]
    height: u32,
    #[serde(default = "default_device_scale_factor")]
    #[schemars(schema_with = "positive_scale_schema")]
    device_scale_factor: f64,
});

pub fn validate_viewport(width: u32, height: u32, scale: f64) -> Result<(), ServerError> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(ServerError::invalid_params(
            "viewport dimensions must be positive and no greater than 2147483647",
        ));
    }
    if !scale.is_finite() || scale <= 0.0 {
        return Err(ServerError::invalid_params(
            "deviceScaleFactor must be finite and positive",
        ));
    }
    Ok(())
}

impl LaunchParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        if self.viewport_width.is_some() != self.viewport_height.is_some() {
            return Err(ServerError::invalid_params(
                "viewportWidth and viewportHeight must be provided together",
            ));
        }
        validate_viewport(
            self.viewport_width.unwrap_or(1),
            self.viewport_height.unwrap_or(1),
            self.device_scale_factor.unwrap_or(1.0),
        )?;
        for (name, path) in [
            ("userDataDir", self.user_data_dir.as_deref()),
            ("executablePath", self.executable_path.as_deref()),
            ("runtimeDir", self.runtime_dir.as_deref()),
        ] {
            if path.is_some_and(|path| !std::path::Path::new(path).is_absolute()) {
                return Err(ServerError::invalid_params(format!(
                    "{name} must be an absolute path",
                )));
            }
        }
        if let Some(auto_solve) = &self.auto_solve_cloudflare {
            auto_solve.validate()?;
        }
        Ok(())
    }
}
dto!(NewPageParams { url: Option<String> });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

dto!(ScreenshotClip {
    #[schemars(range(min = 0))]
    x: f64,
    #[schemars(range(min = 0))]
    y: f64,
    #[schemars(schema_with = "positive_scale_schema")]
    width: f64,
    #[schemars(schema_with = "positive_scale_schema")]
    height: f64,
});
dto!(ScreenshotParams {
    page_id: String,
    #[serde(skip_serializing_if = "Option::is_none")] format: Option<ScreenshotFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 100))] quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")] clip: Option<ScreenshotClip>,
    #[serde(skip_serializing_if = "Option::is_none")] full_page: Option<bool>,
});
dto!(ScreenshotToFileParams {
    page_id: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")] format: Option<ScreenshotFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 100))] quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")] clip: Option<ScreenshotClip>,
    #[serde(skip_serializing_if = "Option::is_none")] full_page: Option<bool>,
});

fn screenshot_options(
    format: Option<ScreenshotFormat>,
    quality: Option<u8>,
    clip: Option<&ScreenshotClip>,
    full_page: Option<bool>,
) -> eoka::ScreenshotOptions {
    eoka::ScreenshotOptions {
        format: match format.unwrap_or(ScreenshotFormat::Png) {
            ScreenshotFormat::Png => eoka::ScreenshotFormat::Png,
            ScreenshotFormat::Jpeg => eoka::ScreenshotFormat::Jpeg,
            ScreenshotFormat::Webp => eoka::ScreenshotFormat::Webp,
        },
        quality,
        clip: clip.map(|clip| eoka::ScreenshotClip {
            x: clip.x,
            y: clip.y,
            width: clip.width,
            height: clip.height,
        }),
        full_page: full_page.unwrap_or(false),
    }
}

fn validate_screenshot(
    format: Option<ScreenshotFormat>,
    quality: Option<u8>,
    clip: Option<&ScreenshotClip>,
    full_page: Option<bool>,
) -> Result<(), ServerError> {
    if quality.is_some_and(|quality| quality > 100)
        || (quality.is_some() && format.unwrap_or(ScreenshotFormat::Png) == ScreenshotFormat::Png)
    {
        return Err(ServerError::invalid_params(
            "quality must be between 0 and 100 and is only supported for JPEG or WebP",
        ));
    }
    if let Some(clip) = clip {
        if full_page.unwrap_or(false) {
            return Err(ServerError::invalid_params(
                "clip and fullPage cannot be combined",
            ));
        }
        if !clip.x.is_finite()
            || !clip.y.is_finite()
            || clip.x < 0.0
            || clip.y < 0.0
            || !clip.width.is_finite()
            || !clip.height.is_finite()
            || clip.width <= 0.0
            || clip.height <= 0.0
        {
            return Err(ServerError::invalid_params(
                "clip requires finite nonnegative x/y and finite positive width/height",
            ));
        }
    }
    Ok(())
}

fn validate_output_path(path: &str) -> Result<(), ServerError> {
    if !std::path::Path::new(path).is_absolute() || path.as_bytes().contains(&0) {
        return Err(ServerError::invalid_params(
            "path must be an absolute file path without NUL bytes",
        ));
    }
    Ok(())
}

impl ScreenshotParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_screenshot(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
    pub fn options(&self) -> eoka::ScreenshotOptions {
        screenshot_options(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
}

impl ScreenshotToFileParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_screenshot(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )?;
        validate_output_path(&self.path)
    }
    pub fn options(&self) -> eoka::ScreenshotOptions {
        screenshot_options(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactContainer {
    Mp4,
}

dto!(Artifact {
    path: String,
    bytes: u64,
    mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")] container: Option<ArtifactContainer>,
});

impl Artifact {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_output_path(&self.path)?;
        if self.bytes > MAX_SAFE_INTEGER {
            return Err(ServerError::invalid_params(
                "artifact bytes exceed the JavaScript-safe integer limit",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingStartParams {
    pub page_id: String,
    #[schemars(regex(
        pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    ))]
    pub recording_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2147483647))]
    pub max_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2147483647))]
    pub max_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 120))]
    pub fps: Option<u32>,
}

dto!(RecordingIdParams {
    #[schemars(regex(
        pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    ))]
    recording_id: String,
});

fn validate_recording_id(id: &str) -> Result<(), ServerError> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return Err(ServerError::invalid_params("recordingId must be a UUID"));
    }
    Ok(())
}

impl RecordingStartParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)?;
        validate_output_path(&self.path)?;
        if self
            .max_width
            .into_iter()
            .chain(self.max_height)
            .any(|dimension| dimension == 0 || dimension > i32::MAX as u32)
        {
            return Err(ServerError::invalid_params(
                "maxWidth/maxHeight must be positive and no greater than 2147483647",
            ));
        }
        if self
            .fps
            .is_some_and(|fps| fps == 0 || fps > MAX_RECORDING_FPS)
        {
            return Err(ServerError::invalid_params(
                "fps must be an integer between 1 and 120",
            ));
        }
        Ok(())
    }
}

impl RecordingIdParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    Running,
    Finalizing,
    Completed,
    Cancelled,
    Failed,
}

impl RecordingState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordingFailureStage {
    Start,
    Stop,
    Read,
    Write,
    Flush,
    Close,
    Publish,
    Cancel,
    Disconnect,
}

dto!(RecordingFailure {
    stage: RecordingFailureStage,
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")] partial_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] cleanup_uncertain: Option<bool>,
});
dto!(RecordingRecord {
    recording_id: String,
    page_id: String,
    state: RecordingState,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")] artifact: Option<Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")] failure: Option<RecordingFailure>,
});

impl RecordingRecord {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)?;
        validate_output_path(&self.path)?;
        if self.state == RecordingState::Completed {
            let artifact = self.artifact.as_ref().ok_or_else(|| {
                ServerError::invalid_params("completed recording requires a published artifact")
            })?;
            artifact.validate()?;
            if artifact.path != self.path
                || artifact.container != Some(ArtifactContainer::Mp4)
                || artifact.mime_type != "video/mp4"
                || self.failure.is_some()
            {
                return Err(ServerError::invalid_params(
                    "completed recording requires its destination MP4 artifact and no failure",
                ));
            }
        } else if self.artifact.is_some() {
            return Err(ServerError::invalid_params(
                "only completed recordings may contain an artifact",
            ));
        }
        if self.state == RecordingState::Failed && self.failure.is_none() {
            return Err(ServerError::invalid_params(
                "failed recording requires failure evidence",
            ));
        }
        if self.failure.is_some() && !self.state.is_terminal() {
            return Err(ServerError::invalid_params(
                "nonterminal recording cannot contain failure evidence",
            ));
        }
        if let Some(partial) = self
            .failure
            .as_ref()
            .and_then(|failure| failure.partial_path.as_ref())
        {
            validate_output_path(partial)?;
            if partial == &self.path {
                return Err(ServerError::invalid_params(
                    "partial artifact cannot use the destination success path",
                ));
            }
        }
        Ok(())
    }
}
dto!(PageIdParams { page_id: String });
dto!(FramesParams { page_id: String });
dto!(FrameJsParams {
    page_id: String,
    frame_id: String,
    #[schemars(
        description = "JavaScript expression evaluated in the frame's isolated world. Frame IDs come from `page.frames`; detached or unrelated IDs are rejected."
    )]
    js: String,
});
dto!(FrameInfo {
    id: String,
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] name: Option<String>,
});
dto!(ClickShadowParams {
    page_id: String,
    selector: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] frame_id: Option<String>,
    #[serde(default = "default_shadow_timeout_ms")] timeout_ms: u64,
});
impl ClickShadowParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        if self.selector.trim().is_empty() || self.selector.len() > 4096 {
            return Err(ServerError::invalid_params(
                "selector must be 1..=4096 characters",
            ));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 60_000 {
            return Err(ServerError::invalid_params(
                "timeoutMs must be between 1 and 60000",
            ));
        }
        Ok(())
    }
}
dto!(ClickShadowResult {
    frame_id: String,
    selector: String,
    x: f64,
    y: f64,
});
fn default_shadow_timeout_ms() -> u64 {
    10_000
}
dto!(SolveTurnstileParams {
    page_id: String,
    #[serde(default = "default_turnstile_timeout_ms")]
    #[schemars(range(min = 1, max = 120000))]
    timeout_ms: u64,
});
impl SolveTurnstileParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_turnstile_timeout(self.timeout_ms)
    }
}
dto!(SolveTurnstileResult {
    clicks: u32,
    elapsed_ms: u64
});
// Server-side watcher that solves full-page Cloudflare challenges in every tab.
dto!(CloudflareAutoSolve {
    #[serde(default = "default_cloudflare_interval_ms")]
    #[schemars(range(min = 100, max = 60000))]
    interval_ms: u64,
    #[serde(default = "default_turnstile_timeout_ms")]
    #[schemars(range(min = 1, max = 120000))]
    timeout_ms: u64,
});
impl CloudflareAutoSolve {
    pub fn validate(&self) -> Result<(), ServerError> {
        if !(100..=60_000).contains(&self.interval_ms) {
            return Err(ServerError::invalid_params(
                "autoSolveCloudflare.intervalMs must be between 100 and 60000",
            ));
        }
        validate_turnstile_timeout(self.timeout_ms)
    }
}
fn validate_turnstile_timeout(timeout_ms: u64) -> Result<(), ServerError> {
    if timeout_ms == 0 || timeout_ms > 120_000 {
        return Err(ServerError::invalid_params(
            "timeoutMs must be between 1 and 120000",
        ));
    }
    Ok(())
}
fn default_turnstile_timeout_ms() -> u64 {
    30_000
}
fn default_cloudflare_interval_ms() -> u64 {
    500
}
dto!(SelectorParams {
    page_id: String,
    selector: String
});
dto!(WaitForParams {
    page_id: String,
    selector: String,
    timeout_ms: u64
});
dto!(WaitForTextParams {
    page_id: String,
    text: String,
    timeout_ms: u64
});
dto!(JsParams {
    page_id: String,
    #[schemars(
        description = "JavaScript expression, awaited. Evaluation accepts only JSON data: null, booleans, strings, finite numbers, arrays and plain objects. Undefined, BigInt, functions, symbols, cycles and non-JSON object types are rejected. Execute discards its return value."
    )]
    js: String,
});
dto!(FetchParams {
    page_id: String, url: String,
    #[serde(default = "default_fetch_method")] method: String,
    #[serde(default)] headers: BTreeMap<String, String>,
    #[serde(default)] body: Option<String>,
    #[serde(default = "default_fetch_redirect")]
    #[schemars(regex(pattern = "^(follow|error|manual)$"))] redirect: String,
});
dto!(BoundedFetchParams {
    #[serde(flatten)]
    request: FetchParams,
    #[serde(default = "default_bounded_fetch_bytes")]
    #[schemars(range(max = 1048576))]
    max_bytes: usize,
});

impl BoundedFetchParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        if self.max_bytes > MAX_BOUNDED_FETCH_BYTES {
            return Err(ServerError::invalid_params(
                "maxBytes must not exceed 1048576 bytes",
            ));
        }
        if !matches!(
            self.request.redirect.as_str(),
            "follow" | "manual" | "error"
        ) {
            return Err(ServerError::invalid_params(
                "redirect must be follow, manual, or error",
            ));
        }
        Ok(())
    }
}

fn default_bounded_fetch_bytes() -> usize {
    8192
}

fn default_fetch_method() -> String {
    "GET".into()
}
fn default_fetch_redirect() -> String {
    "follow".into()
}
dto!(MouseButtonParams {
    page_id: String,
    #[serde(flatten)]
    mouse: crate::MouseButtonArgs
});
dto!(MouseMoveParams {
    page_id: String,
    #[serde(flatten)]
    mouse: crate::MouseMoveArgs
});
dto!(HeldKeyParams {
    page_id: String,
    #[serde(flatten)]
    key: crate::KeyArgs
});
dto!(RestoreStateParams {
    page_id: String,
    state: BrowserState
});
dto!(SolveCaptchaParams {
    page_id: String, api_key: String,
    #[schemars(regex(pattern = "^recaptcha_v2_enterprise$"))] captcha_type: String,
    #[schemars(regex(pattern = "^anti_captcha_proxyless$"))] captcha_mode: String,
    #[serde(rename = "websiteURL")] website_url: String,
    website_key: String,
    #[serde(default)] enterprise_payload: Option<Value>,
    #[serde(default)] api_domain: Option<String>,
    #[serde(default)] callback: Option<String>,
});

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires: Option<f64>,
}
dto!(BrowserState {
    cookies: Vec<SessionCookie>,
    #[serde(default)] local_storage: HashMap<String, String>,
    #[serde(default)] session_storage: HashMap<String, String>,
    user_agent: String, url: String,
});

impl From<eoka::BrowserState> for BrowserState {
    fn from(state: eoka::BrowserState) -> Self {
        Self {
            cookies: state
                .cookies
                .into_iter()
                .map(|cookie| SessionCookie {
                    name: cookie.name,
                    value: cookie.value,
                    domain: cookie.domain,
                    path: cookie.path,
                    secure: cookie.secure,
                    http_only: cookie.http_only,
                    same_site: cookie.same_site,
                    expires: cookie.expires,
                })
                .collect(),
            local_storage: state.local_storage,
            session_storage: state.session_storage,
            user_agent: state.user_agent,
            url: state.url,
        }
    }
}
impl From<BrowserState> for eoka::BrowserState {
    fn from(state: BrowserState) -> Self {
        Self {
            cookies: state
                .cookies
                .into_iter()
                .map(|cookie| eoka::SessionCookie {
                    name: cookie.name,
                    value: cookie.value,
                    domain: cookie.domain,
                    path: cookie.path,
                    secure: cookie.secure,
                    http_only: cookie.http_only,
                    same_site: cookie.same_site,
                    expires: cookie.expires,
                })
                .collect(),
            local_storage: state.local_storage,
            session_storage: state.session_storage,
            user_agent: state.user_agent,
            url: state.url,
        }
    }
}

dto!(NewPageResult { page_id: String });
dto!(Tab {
    id: String,
    title: String,
    url: String
});
dto!(TabsResult { tabs: Vec<Tab> });
dto!(CloseTabResult { #[serde(skip_serializing_if = "Option::is_none")] cleanup_error: Option<String> });
dto!(TextResult { text: String });
dto!(ContentResult { html: String });
dto!(TitleResult { title: String });
dto!(UrlResult { url: String });
dto!(AttributeResult { value: Option<String> });
dto!(ExistsResult { exists: bool });
dto!(EvaluateResult { result: Value });
dto!(CaptureStateResult {
    state: BrowserState
});
dto!(ScreenshotResult {
    data_base64: String
});
dto!(FetchResult { url: String, status: u16, ok: bool, headers: BTreeMap<String, String>, body: String });
dto!(BoundedFetchResult {
    url: String,
    status: u16,
    status_text: String,
    r#type: String,
    redirected: bool,
    headers: HashMap<String, String>,
    body: String,
    truncated: bool,
    bytes_read: usize,
    body_omitted: Option<String>,
});

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptchaInjection {
    pub kind: String,
    pub updated_count: usize,
    pub created: Vec<String>,
    pub callbacks: Vec<String>,
    pub errors: Vec<String>,
    #[serde(rename = "solverUserAgent", skip_serializing_if = "Option::is_none")]
    pub solver_user_agent: Option<String>,
}
dto!(SolveCaptchaResult {
    injection: CaptchaInjection
});

macro_rules! action_params {
    ($name:ident { $($field:ident: $ty:ty),* }) => {
        dto!($name {
            page_id: String,
            #[serde(rename = "operation_id")] operation_id: String,
            #[serde(default)] options: crate::ActionOptions,
            $($field: $ty),*
        });
    };
}
action_params!(ActionPerformParams {
    action: crate::Action
});
action_params!(GotoParams { url: String });
action_params!(ClickParams { selector: String });
action_params!(ClickTextParams { text: String });
action_params!(FillParams {
    selector: String,
    value: String
});
action_params!(TypeIntoParams {
    selector: String,
    text: String
});
action_params!(PressKeyParams { key: String });
dto!(AgentObserveParams {
    page_id: String,
    #[serde(flatten)]
    args: crate::AgentObserveArgs
});
dto!(AgentStepParams {
    page_id: String,
    #[serde(flatten)]
    args: crate::AgentStepArgs
});

pub fn validate_safe_numbers(value: &Value) -> Result<(), ServerError> {
    match value {
        Value::Number(number) => {
            let unsafe_integer = number.as_u64().is_some_and(|n| n > MAX_SAFE_INTEGER)
                || number
                    .as_i64()
                    .is_some_and(|n| n < -(MAX_SAFE_INTEGER as i64))
                || number
                    .as_f64()
                    .is_some_and(|n| n.fract() == 0.0 && n.abs() > MAX_SAFE_INTEGER as f64);
            if unsafe_integer {
                return Err(ServerError::invalid_params(
                    "integer exceeds JavaScript's exact safe integer range",
                ));
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_safe_numbers(value)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_safe_numbers(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn constrain_integers(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let integer = object.get("type").is_some_and(|ty| {
                ty == "integer"
                    || ty
                        .as_array()
                        .is_some_and(|types| types.iter().any(|ty| ty == "integer"))
            });
            if integer {
                let (rust_minimum, rust_maximum) =
                    match object.get("format").and_then(Value::as_str) {
                        Some("int8") => (i8::MIN as f64, i8::MAX as f64),
                        Some("int16") => (i16::MIN as f64, i16::MAX as f64),
                        Some("int32") => (i32::MIN as f64, i32::MAX as f64),
                        Some("uint8") => (0.0, u8::MAX as f64),
                        Some("uint16") => (0.0, u16::MAX as f64),
                        Some("uint32") => (0.0, u32::MAX as f64),
                        Some("uint") => (0.0, (usize::MAX as f64).min(MAX_SAFE_INTEGER as f64)),
                        Some("uint64" | "uint128") => (0.0, MAX_SAFE_INTEGER as f64),
                        _ => (-(MAX_SAFE_INTEGER as f64), MAX_SAFE_INTEGER as f64),
                    };
                let maximum = object
                    .get("maximum")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::INFINITY)
                    .min(rust_maximum);
                let minimum = object
                    .get("minimum")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::NEG_INFINITY)
                    .max(rust_minimum);
                object.insert("maximum".into(), json!(maximum));
                object.insert("minimum".into(), json!(minimum));
            }
            for child in object.values_mut() {
                constrain_integers(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                constrain_integers(child);
            }
        }
        _ => {}
    }
}

fn schema<T: JsonSchema>() -> Value {
    let mut value =
        serde_json::to_value(schemars::schema_for!(T)).expect("JSON schemas are serializable");
    constrain_integers(&mut value);
    value
}

#[derive(Debug, Clone, Copy)]
pub struct MethodSpec {
    pub name: &'static str,
    pub params_schema: fn() -> Value,
    pub result_schema: fn() -> Value,
    pub validate: fn(&Value) -> Result<(), ServerError>,
}

macro_rules! methods {
    ($($name:literal => $params:ty $(, $check:expr)? => $result:ty),* $(,)?) => {
        pub const METHODS: &[MethodSpec] = &[$(MethodSpec {
            name: $name,
            params_schema: schema::<$params>,
            result_schema: schema::<$result>,
            validate: |params| {
                let _params = <$params>::deserialize(params)
                    .map_err(|error| ServerError::invalid_params(error.to_string()))?;
                $(($check)(&_params)?;)?
                Ok(())
            },
        }),*];
    };
}

methods! {
    "server.hello" => HelloParams => HelloResult,
    "server.shutdown" => Empty => Empty,
    "browser.launch" => LaunchParams, LaunchParams::validate => BrowserSessionResult,
    "browser.connect" => ConnectParams, ConnectParams::validate => BrowserSessionResult,
    "browser.new_page" => NewPageParams => NewPageResult,
    "browser.tabs" => Empty => TabsResult,
    "browser.close_tab" => PageIdParams => CloseTabResult,
    "browser.close" => Empty => Empty,
    "actions.perform" => ActionPerformParams => crate::OperationRecord,
    "operations.get" => OperationIdArgs => crate::OperationRecord,
    "operations.list" => OperationListArgs => Vec<crate::OperationRecord>,
    "operations.wait" => OperationWaitArgs => crate::OperationRecord,
    "operations.cancel" => OperationIdArgs => crate::OperationRecord,
    "agent.observe" => AgentObserveParams => crate::AgentObservation,
    "agent.step" => AgentStepParams => crate::AgentStepResult,
    "page.goto" => GotoParams => crate::OperationRecord,
    "page.click" => ClickParams => crate::OperationRecord,
    "page.click_text" => ClickTextParams => crate::OperationRecord,
    "page.human_click" => ClickParams => crate::OperationRecord,
    "page.human_click_text" => ClickTextParams => crate::OperationRecord,
    "page.fill" => FillParams => crate::OperationRecord,
    "page.human_fill" => FillParams => crate::OperationRecord,
    "page.type_into" => TypeIntoParams => crate::OperationRecord,
    "page.select" => FillParams => crate::OperationRecord,
    "page.hover" => ClickParams => crate::OperationRecord,
    "page.press_key" => PressKeyParams => crate::OperationRecord,
    "page.text" => PageIdParams => TextResult,
    "page.content" => PageIdParams => ContentResult,
    "page.title" => PageIdParams => TitleResult,
    "page.url" => PageIdParams => UrlResult,
    "page.get_text" => SelectorParams => TextResult,
    "page.get_attribute" => GetAttributeParams => AttributeResult,
    "page.exists" => SelectorParams => ExistsResult,
    "page.wait_for" => WaitForParams => Empty,
    "page.wait_for_visible" => WaitForParams => Empty,
    "page.wait_for_text" => WaitForTextParams => Empty,
    "page.evaluate" => JsParams => EvaluateResult,
    "page.execute" => JsParams => Empty,
    "page.frames" => FramesParams => Vec<FrameInfo>,
    "page.evaluate_in_frame" => FrameJsParams => EvaluateResult,
    "page.click_shadow" => ClickShadowParams, ClickShadowParams::validate => ClickShadowResult,
    "page.solve_turnstile" => SolveTurnstileParams, SolveTurnstileParams::validate => SolveTurnstileResult,
    "page.fetch" => FetchParams => FetchResult,
    "page.fetch_bounded" => BoundedFetchParams, BoundedFetchParams::validate => BoundedFetchResult,
    "page.capture_state" => PageIdParams => CaptureStateResult,
    "page.restore_state" => RestoreStateParams => Empty,
    "page.screenshot" => ScreenshotParams, ScreenshotParams::validate => ScreenshotResult,
    "page.screenshot_to_file" => ScreenshotToFileParams, ScreenshotToFileParams::validate => Artifact,
    "recording.start" => RecordingStartParams, RecordingStartParams::validate => RecordingRecord,
    "recording.get" => RecordingIdParams, RecordingIdParams::validate => RecordingRecord,
    "recording.stop" => RecordingIdParams, RecordingIdParams::validate => RecordingRecord,
    "recording.cancel" => RecordingIdParams, RecordingIdParams::validate => RecordingRecord,
    "page.set_viewport" => SetViewportParams, |params: &SetViewportParams| {
        validate_viewport(params.width, params.height, params.device_scale_factor)
    } => Empty,
    "page.reset_viewport" => PageIdParams => Empty,
    "page.mouse_down" => MouseButtonParams => Empty,
    "page.mouse_move" => MouseMoveParams => Empty,
    "page.mouse_up" => MouseButtonParams => Empty,
    "page.key_down" => HeldKeyParams => Empty,
    "page.key_up" => HeldKeyParams => Empty,
    "page.release_all_inputs" => PageIdParams => Empty,
    "page.solve_captcha" => SolveCaptchaParams => SolveCaptchaResult,
    "page.close" => PageIdParams => Empty,
}
dto!(GetAttributeParams {
    page_id: String,
    selector: String,
    name: String
});

pub fn method_names() -> Vec<&'static str> {
    METHODS.iter().map(|method| method.name).collect()
}

pub fn validate_params(method: &str, params: &Value) -> Result<(), ServerError> {
    validate_safe_numbers(params)?;
    let spec = METHODS
        .iter()
        .find(|spec| spec.name == method)
        .ok_or_else(|| ServerError::unknown_method(method))?;
    (spec.validate)(params)
}

pub fn schema_bundle() -> Value {
    let methods: BTreeMap<_, _> = METHODS
        .iter()
        .map(|method| {
            (
                method.name,
                json!({"params": (method.params_schema)(), "result": (method.result_schema)()}),
            )
        })
        .collect();
    let mut bundle = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "methods": methods,
        "types": {
            "Request": schema::<Request>(), "Response": schema::<Response>(),
            "ServerError": schema::<ServerError>(), "ErrorData": schema::<ErrorData>(),
            "ErrorDetail": schema::<crate::ErrorDetail>(), "HelloResult": schema::<HelloResult>(),
            "ProtocolVersion": schema::<ProtocolVersion>(), "HelloParams": schema::<HelloParams>(),
            "TransportLimits": schema::<TransportLimits>(), "ErrorCode": schema::<ErrorCode>(),
            "CdpErrorDetail": schema::<CdpErrorDetail>(),
            "Action": schema::<crate::Action>(), "ActionOptions": schema::<crate::ActionOptions>(),
            "ActionReceipt": schema::<crate::ActionReceipt>(), "LocatorSpec": schema::<crate::LocatorSpec>(),
            "ActionOutcome": schema::<crate::ActionOutcome>(), "DispatchState": schema::<crate::DispatchState>(),
            "LocatorQuery": schema::<crate::LocatorQuery>(),
            "Expectation": schema::<crate::Expectation>(), "OperationRecord": schema::<crate::OperationRecord>(),
            "OperationState": schema::<crate::OperationState>(), "AgentObservation": schema::<crate::AgentObservation>(),
            "AgentStepResult": schema::<crate::AgentStepResult>(), "BrowserState": schema::<BrowserState>(),
            "AgentAction": schema::<crate::AgentAction>(), "AgentMode": schema::<crate::AgentMode>(),
            "ScreenshotFormat": schema::<ScreenshotFormat>(), "ScreenshotClip": schema::<ScreenshotClip>(),
            "ScreenshotParams": schema::<ScreenshotParams>(), "ScreenshotToFileParams": schema::<ScreenshotToFileParams>(),
            "Artifact": schema::<Artifact>(), "ArtifactContainer": schema::<ArtifactContainer>(),
            "RecordingStartParams": schema::<RecordingStartParams>(), "RecordingIdParams": schema::<RecordingIdParams>(),
            "RecordingRecord": schema::<RecordingRecord>(), "RecordingState": schema::<RecordingState>(),
            "RecordingFailure": schema::<RecordingFailure>(), "RecordingFailureStage": schema::<RecordingFailureStage>()
        }
    });
    // Added outside `json!` to stay under its macro recursion limit.
    bundle["types"]["CloudflareAutoSolve"] = schema::<CloudflareAutoSolve>();
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDING_ID: &str = "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb";

    #[test]
    fn capture_validation_preserves_defaults_and_rejects_conflicts() {
        assert!(validate_params("page.screenshot", &json!({"pageId":"p"})).is_ok());
        assert_eq!(
            serde_json::to_value(ScreenshotResult {
                data_base64: "AA==".into()
            })
            .unwrap(),
            json!({"dataBase64":"AA=="})
        );
        for format in ["jpeg", "webp"] {
            assert!(validate_params("page.screenshot", &json!({"pageId":"p","format":format,"quality":0,"clip":{"x":0,"y":1,"width":1.5,"height":2}})).is_ok());
        }
        for params in [
            json!({"pageId":"p","quality":10}),
            json!({"pageId":"p","format":"jpeg","quality":101}),
            json!({"pageId":"p","format":"gif"}),
            json!({"pageId":"p","clip":{"x":-1,"y":0,"width":1,"height":1}}),
            json!({"pageId":"p","clip":{"x":0,"y":0,"width":0,"height":1}}),
            json!({"pageId":"p","clip":{"x":0,"y":0,"width":1,"height":1},"fullPage":true}),
        ] {
            assert_eq!(
                validate_params("page.screenshot", &params)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidParams
            );
        }
        assert!(validate_params(
            "page.screenshot_to_file",
            &json!({"pageId":"p","path":"/tmp/capture.png","fullPage":true})
        )
        .is_ok());
        assert!(validate_params(
            "page.screenshot_to_file",
            &json!({"pageId":"p","path":"capture.png"})
        )
        .is_err());
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let clip = ScreenshotClip {
                x: 0.0,
                y: 0.0,
                width: invalid,
                height: 1.0,
            };
            assert!(validate_screenshot(None, None, Some(&clip), None).is_err());
        }
    }

    #[test]
    fn recording_ids_paths_and_native_bounds_are_validated_before_dispatch() {
        let valid = json!({"pageId":"p","recordingId":RECORDING_ID,"path":"/tmp/capture.mp4","fps":120,"maxWidth":2147483647_u32,"maxHeight":1});
        assert!(validate_params("recording.start", &valid).is_ok());
        for (field, value) in [
            ("recordingId", json!("unknown")),
            ("path", json!("capture.mp4")),
            ("fps", json!(0)),
            ("fps", json!(120.5)),
            ("fps", json!(1.5)),
            ("maxWidth", json!(0)),
            ("maxHeight", json!(2147483648_u32)),
        ] {
            let mut params = valid.clone();
            params[field] = value;
            assert_eq!(
                validate_params("recording.start", &params)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidParams
            );
        }
        for method in ["recording.get", "recording.stop", "recording.cancel"] {
            assert!(validate_params(method, &json!({"recordingId":RECORDING_ID})).is_ok());
            assert!(validate_params(method, &json!({"recordingId":"not-a-uuid"})).is_err());
        }
    }

    #[test]
    fn recording_terminal_evidence_never_labels_partial_files_complete() {
        let mut record = RecordingRecord {
            recording_id: RECORDING_ID.into(),
            page_id: "p".into(),
            state: RecordingState::Running,
            path: "/tmp/capture.mp4".into(),
            artifact: None,
            failure: None,
        };
        assert!(record.validate().is_ok());
        record.state = RecordingState::Completed;
        assert!(record.validate().is_err());
        record.artifact = Some(Artifact {
            path: record.path.clone(),
            bytes: MAX_SAFE_INTEGER,
            mime_type: "video/mp4".into(),
            container: Some(ArtifactContainer::Mp4),
        });
        assert!(record.validate().is_ok());
        assert!(serde_json::to_value(&record).unwrap()["artifact"]
            .get("codec")
            .is_none());
        record.artifact.as_mut().unwrap().bytes += 1;
        assert!(record.validate().is_err());
        record.artifact = None;
        record.state = RecordingState::Failed;
        assert!(record.validate().is_err());
        record.failure = Some(RecordingFailure {
            stage: RecordingFailureStage::Write,
            code: ErrorCode::RecordingFailed,
            message: "disk full".into(),
            partial_path: Some("/tmp/.capture.partial".into()),
            cleanup_uncertain: Some(true),
        });
        assert!(record.validate().is_ok());
        let error =
            ServerError::new(ErrorCode::RecordingFailed, "disk full").with_data(ErrorData {
                recording_id: Some(RECORDING_ID.into()),
                recording: Some(record.clone()),
                ..Default::default()
            });
        let wire = serde_json::to_value(error).unwrap();
        assert_eq!(wire["data"]["recordingId"], RECORDING_ID);
        assert_eq!(
            wire["data"]["recording"]["failure"]["partialPath"],
            "/tmp/.capture.partial"
        );
        assert_eq!(
            wire["data"]["recording"]["failure"]["cleanupUncertain"],
            true
        );
        record.failure.as_mut().unwrap().partial_path = Some(record.path.clone());
        assert!(record.validate().is_err());
        assert!(RecordingState::Cancelled.is_terminal());
        assert!(!RecordingState::Finalizing.is_terminal());
    }

    #[test]
    fn bounded_fetch_limits_reject_unsafe_retention_before_browser_work() {
        for max_bytes in [0, MAX_BOUNDED_FETCH_BYTES] {
            validate_params(
                "page.fetch_bounded",
                &json!({"pageId":"page","url":"/text","maxBytes":max_bytes}),
            )
            .unwrap();
        }
        for params in [
            json!({"pageId":"page","url":"/text","maxBytes":MAX_BOUNDED_FETCH_BYTES + 1}),
            json!({"pageId":"page","url":"/text","maxBytes":-1}),
            json!({"pageId":"page","url":"/text","maxBytes":1.5}),
            json!({"pageId":"page","url":"/text","redirect":"unbounded"}),
        ] {
            assert_eq!(
                validate_params("page.fetch_bounded", &params)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidParams
            );
        }
    }

    #[test]
    fn request_ids_are_positive_exact_integers() {
        for id in [json!(1), json!(MAX_SAFE_INTEGER)] {
            let request: Request =
                serde_json::from_value(json!({"id":id,"method":"browser.tabs"})).unwrap();
            assert_eq!(request.id, id);
            assert_eq!(request.params, json!({}));
        }
        for id in [
            json!(0),
            json!(-1),
            json!(MAX_SAFE_INTEGER + 1),
            json!(1.5),
            json!(1.0),
            json!("1"),
            Value::Null,
        ] {
            assert!(
                serde_json::from_value::<Request>(json!({"id":id,"method":"browser.tabs"}))
                    .is_err()
            );
        }
    }

    #[test]
    fn response_requires_exactly_one_outcome_and_preserves_null() {
        let success = serde_json::to_value(Response::ok(1_u64, Value::Null)).unwrap();
        assert_eq!(success, json!({"id":1,"result":null}));
        let decoded: Response = serde_json::from_value(success).unwrap();
        assert!(matches!(
            decoded,
            Response::Success {
                result: Value::Null,
                ..
            }
        ));
        for invalid in [
            json!({"id":1}),
            json!({"id":1,"result":null,"error":{"code":"Internal","message":"failure"}}),
        ] {
            assert!(serde_json::from_value::<Response>(invalid).is_err());
        }
        let failure = serde_json::to_value(Response::err(
            Value::Null,
            ServerError::invalid_params("bad input"),
        ))
        .unwrap();
        assert!(failure.get("result").is_none());
        assert_eq!(failure["error"]["code"], "InvalidParams");
    }

    #[test]
    fn cdp_and_action_errors_preserve_evidence_through_wire_round_trip() {
        let receipt = crate::ActionReceipt {
            started_at_ms: 1,
            finished_at_ms: 2,
            download_guid: None,
            action: "click".into(),
            target_id: "target".into(),
            outcome: crate::ActionOutcome::OutcomeUnknown,
            dispatch: crate::DispatchState::MayHaveBeenSent,
            preparation_may_have_executed: true,
            verification: None,
            elapsed_ms: 1,
            already_satisfied: false,
            cleanup_pending: true,
        };
        let error = ServerError::from(eoka::Error::Action {
            code: "outcome_unknown".into(),
            receipt: Box::new(receipt),
            source: Box::new(eoka::Error::Cdp {
                method: Some("Input.dispatchMouseEvent".into()),
                code: Some(-32000),
                message: "connection interrupted".into(),
            }),
        });
        let wire = serde_json::to_value(Response::err(1_u64, error)).unwrap();
        let decoded: Response = serde_json::from_value(wire).unwrap();
        let Response::Failure { error, .. } = decoded else {
            panic!("expected error")
        };
        assert_eq!(error.code, ErrorCode::Action);
        let data = error.data.unwrap();
        assert_eq!(data.detail.unwrap().code, "outcome_unknown");
        let receipt = data.receipt.unwrap();
        assert_eq!(receipt.outcome, crate::ActionOutcome::OutcomeUnknown);
        assert_eq!(receipt.dispatch, crate::DispatchState::MayHaveBeenSent);
        assert!(receipt.cleanup_pending);
        let cdp = data.source.unwrap().data.unwrap().cdp.unwrap();
        assert_eq!(cdp.method.as_deref(), Some("Input.dispatchMouseEvent"));
        assert_eq!(cdp.code, Some(-32000));
    }

    #[test]
    fn click_shadow_validates_selector_and_deadline() {
        assert!(validate_params(
            "page.click_shadow",
            &json!({"pageId":"p","selector":"input[type=\"checkbox\"]"})
        )
        .is_ok());
        assert!(validate_params(
            "page.click_shadow",
            &json!({"pageId":"p","selector":"button","frameId":"frame","timeoutMs":1000})
        )
        .is_ok());
        assert!(
            validate_params("page.click_shadow", &json!({"pageId":"p","selector":""})).is_err()
        );
        assert!(validate_params(
            "page.click_shadow",
            &json!({"pageId":"p","selector":"button","timeoutMs":0})
        )
        .is_err());
        assert!(validate_params(
            "page.click_shadow",
            &json!({"pageId":"p","selector":"button","timeoutMs":60001})
        )
        .is_err());
    }

    #[test]
    fn canonical_params_reject_missing_actions_and_unsafe_nested_integers() {
        assert!(validate_params("page.click", &json!({"pageId":"p","selector":"button"})).is_err());
        let params = json!({"pageId":"p","selector":"button","operation_id":"operation","options":{"timeout_ms":30000}});
        assert!(validate_params("page.click", &params).is_ok());
        assert!(validate_params("actions.perform", &json!({"pageId":"p","operation_id":"operation","action":{"kind":"navigate","url":"https://example.test","referrer":null},"options":{"timeout_ms":MAX_SAFE_INTEGER + 1}})).is_err());
        assert!(
            validate_safe_numbers(&json!({"nested":[{"value":-(MAX_SAFE_INTEGER as i64)-1}]}))
                .is_err()
        );
        assert!(validate_safe_numbers(&json!({"value":9007199254740992.0})).is_err());
        assert!(validate_safe_numbers(
            &json!({"values":[MAX_SAFE_INTEGER, -(MAX_SAFE_INTEGER as i64), 1.25]})
        )
        .is_ok());
        assert!(validate_params(
            "operations.wait",
            &json!({"id":"operation","timeout_ms":MAX_SAFE_INTEGER + 1})
        )
        .is_err());
        assert_eq!(
            validate_params("unknown", &json!({})).unwrap_err().code,
            ErrorCode::UnknownMethod
        );
        for method in ["unknown", "browser.launch"] {
            let error = validate_params(
                method,
                &json!({"sandbox":"false","viewportWidth":MAX_SAFE_INTEGER + 1}),
            )
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidParams);
            assert_eq!(
                error.message,
                "integer exceeds JavaScript's exact safe integer range"
            );
        }
    }

    #[test]
    fn managed_params_validate_paths_dimensions_and_scale_before_browser_work() {
        for params in [
            json!({"executablePath":"relative/chrome"}),
            json!({"runtimeDir":"relative/runtime"}),
            json!({"viewportWidth":640}),
            json!({"viewportWidth":0,"viewportHeight":360}),
            json!({"viewportWidth":2147483648_u64,"viewportHeight":360}),
            json!({"deviceScaleFactor":0}),
            json!({"deviceScaleFactor":-1}),
            json!({"sandbox":"false"}),
        ] {
            assert_eq!(
                validate_params("browser.launch", &params).unwrap_err().code,
                ErrorCode::InvalidParams
            );
        }
        let params = json!({"viewportWidth":0,"sandbox":"false"});
        assert_eq!(
            validate_params("browser.launch", &params)
                .unwrap_err()
                .message,
            LaunchParams::deserialize(&params).unwrap_err().to_string()
        );
        assert!(validate_params(
            "browser.launch",
            &json!({
                "viewportWidth":640,"viewportHeight":360,"deviceScaleFactor":2.0,"sandbox":true
            })
        )
        .is_ok());
        assert!(validate_params("browser.launch", &json!({"deviceScaleFactor":1.5})).is_ok());
        for params in [
            json!({"pageId":"p","width":0,"height":360}),
            json!({"pageId":"p","width":640,"height":2147483648_u64}),
            json!({"pageId":"p","width":640,"height":360,"deviceScaleFactor":-1}),
        ] {
            assert_eq!(
                validate_params("page.set_viewport", &params)
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidParams
            );
        }
        for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                validate_viewport(640, 360, scale).unwrap_err().code,
                ErrorCode::InvalidParams
            );
        }
        assert!(validate_params(
            "page.set_viewport",
            &json!({"pageId":"p","width":640,"height":360})
        )
        .is_ok());
        assert!(validate_params(
            "browser.connect",
            &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","sandbox":true})
        )
        .is_err());
    }

    #[test]
    fn canonical_state_conversion_preserves_cookie_and_storage_shapes() {
        let state: BrowserState = serde_json::from_value(json!({
            "cookies":[{"name":"session","value":"private","domain":"example.test","path":"/","secure":true,"http_only":true,"same_site":"Lax","expires":null}],
            "localStorage":{"local":"value"},"sessionStorage":{"tab":"value"},
            "userAgent":"agent","url":"https://example.test/"
        })).unwrap();
        let before = serde_json::to_value(&state).unwrap();
        let core: eoka::BrowserState = state.into();
        assert_eq!(serde_json::to_value(&core).unwrap(), before);
        assert_eq!(
            serde_json::to_value(BrowserState::from(core)).unwrap(),
            before
        );
    }

    #[test]
    fn actual_result_schemas_preserve_array_dynamic_json_and_numeric_bounds() {
        let bundle = schema_bundle();
        let list = &bundle["methods"]["operations.list"]["result"];
        assert_eq!(list["type"], "array");
        assert!(list["items"]["$ref"]
            .as_str()
            .unwrap()
            .ends_with("/OperationRecord"));
        let dynamic = &bundle["methods"]["page.evaluate"]["result"];
        let decoded: EvaluateResult =
            serde_json::from_value(json!({"result":[null,false,{"value":3}]})).unwrap();
        assert_eq!(decoded.result, json!([null,false,{"value":3}]));
        assert!(
            dynamic["properties"]["result"].is_boolean()
                || dynamic["properties"]["result"].is_object()
        );
        let launch = &bundle["methods"]["browser.launch"]["params"];
        assert_eq!(
            launch["properties"]["viewportWidth"]["maximum"].as_f64(),
            Some(i32::MAX as f64)
        );
        let wait = &bundle["methods"]["operations.wait"]["params"];
        assert_eq!(
            wait["properties"]["timeout_ms"]["maximum"].as_f64(),
            Some(86_400_000.0)
        );
        let methods = method_names();
        let unique: std::collections::BTreeSet<_> = methods.iter().collect();
        assert_eq!(unique.len(), methods.len());
        assert!(validate_params("browser.connect", &json!({})).is_err());
        assert!(validate_params("operation_get", &json!({"id":"operation"})).is_err());
    }

    #[test]
    fn turnstile_and_cloudflare_auto_solve_validate_deadlines() {
        assert!(validate_params("page.solve_turnstile", &json!({"pageId":"p"})).is_ok());
        for timeout in [0, 120_001] {
            assert!(validate_params(
                "page.solve_turnstile",
                &json!({"pageId":"p","timeoutMs":timeout})
            )
            .is_err());
        }
        let launch: LaunchParams =
            serde_json::from_value(json!({"autoSolveCloudflare":{}})).unwrap();
        assert_eq!(
            launch
                .auto_solve_cloudflare
                .map(|o| (o.interval_ms, o.timeout_ms)),
            Some((500, 30_000))
        );
        assert!(validate_params(
            "browser.launch",
            &json!({"autoSolveCloudflare":{"intervalMs":99}})
        )
        .is_err());
        assert!(validate_params(
            "browser.connect",
            &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","autoSolveCloudflare":{"timeoutMs":5000}})
        )
        .is_ok());
        assert!(validate_params(
            "browser.connect",
            &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","autoSolveCloudflare":{"intervalMs":60001}})
        )
        .is_err());
    }
}
