use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AttributeValue {
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Existence {
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FrameSummary {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShadowClick {
    pub frame_id: String,
    pub selector: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    Headless,
    Headed,
    Connect,
    Reuse,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionHello {
    pub protocol: crate::sidecar::ProtocolVersion,
    pub server_version: String,
    pub session: String,
    pub mode: SessionMode,
    pub configuration_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_limit_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<SessionActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionActivity {
    pub busy: Option<BusyCommand>,
    pub queued: usize,
    pub blocker: Option<SessionBlocker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BusyCommand {
    pub cmd: String,
    pub for_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionBlocker {
    #[serde(flatten)]
    pub blocker: BlockerSummary,
    pub description: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockerSummary {
    Dialog {
        dialog_type: String,
        message: String,
        url: String,
        for_ms: u64,
    },
    Navigation {
        url: Option<String>,
        for_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ActionCommandResult {
    Action(crate::ActionResult),
    Download(crate::DownloadResult),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ScrollResult {
    Scrolled(String),
    Action(Box<crate::ActionResult>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ObserveResult {
    Text(String),
    Structured(StructuredObservation),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StructuredObservation {
    pub text: String,
    pub elements: Vec<ObservedElement>,
    pub page: Option<crate::PageContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObservedElement {
    pub index: usize,
    pub selector: String,
    pub text: String,
    pub role: Option<String>,
    pub tag: String,
    pub visible: bool,
    pub disabled: bool,
    pub checked: bool,
    pub placeholder: Option<String>,
    pub input_type: Option<String>,
    pub value: Option<String>,
    pub bbox: ElementBox,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElementBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ScreenshotResult {
    Inline(InlineScreenshot),
    File(ScreenshotFile),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InlineScreenshot {
    pub data_base64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotFile {
    pub path: String,
    pub size: usize,
    pub mime_type: String,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elements: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum EmulateResult {
    Viewport(Viewport),
    Cleared(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
    pub mobile: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FetchResult {
    Body(String),
    Response(FetchedResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FetchedResponse {
    pub url: String,
    pub status: u16,
    pub status_text: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub redirected: bool,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub truncated: bool,
    pub bytes_read: usize,
    pub body_omitted: Option<String>,
}

impl From<eoka::BoundedFetchResponse> for FetchedResponse {
    fn from(response: eoka::BoundedFetchResponse) -> Self {
        Self {
            url: response.url,
            status: response.status,
            status_text: response.status_text,
            kind: response.r#type,
            redirected: response.redirected,
            headers: response.headers.into_iter().collect(),
            body: response.body,
            truncated: response.truncated,
            bytes_read: response.bytes_read,
            body_omitted: response.body_omitted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: Option<String>,
    pub expires: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum StorageRead {
    Item(Option<String>),
    Entries(BTreeMap<String, String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorageDump {
    pub local_storage: BTreeMap<String, String>,
    pub session_storage: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub text: String,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageError {
    pub message: String,
    pub source: String,
    pub line: u64,
    pub col: u64,
    pub stack: String,
    pub ts: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SpaRouter {
    ReactRouter,
    NextJs,
    VueRouter,
    AngularRouter,
    HistoryApi,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpaInfo {
    pub router_type: SpaRouter,
    pub current_path: String,
    pub can_navigate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WasmMemory {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: u64,
    pub pages: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InterceptRuleSummary {
    pub id: usize,
    pub pattern: String,
    pub capture: Option<String>,
    pub respond: Option<String>,
    pub status: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InterceptLogRecord {
    pub rule_id: usize,
    pub url: String,
    pub method: String,
    pub has_body: bool,
    pub action: String,
    pub session_id: Option<String>,
    pub network_id: Option<String>,
    pub network_entry_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptMode {
    AllowAll,
    BlockAll,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptPolicy {
    pub mode: ScriptMode,
    pub allow: Vec<String>,
    pub block: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptchaDetection {
    pub detected: bool,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub captcha_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sitekey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptchaInjectResult {
    #[serde(flatten)]
    pub injection: crate::sidecar::CaptchaInjection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click_after: Option<ClickAfter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClickAfter {
    pub target: String,
    pub clicked: String,
}
