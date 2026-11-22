use serde::{Deserialize, Serialize};

#[cfg(feature = "clap")]
pub const TARGET_HELP: &str =
    "text:Submit (or bare text), placeholder:Email, css:#id, id:login, role:button, or a target from view";
#[cfg(feature = "clap")]
const WASM_ADDR_HELP: &str = "Address, decimal or 0x-prefixed hex";
#[cfg(feature = "clap")]
const WASM_MEMORY_HELP: &str =
    "JavaScript expression for the WebAssembly.Memory [default: auto-detect]";
#[cfg(feature = "clap")]
pub(crate) const OPERATION_ID_HELP: &str = "Operation ID returned by the action";

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct OpenArgs {
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    pub url: String,
    pub headers: Option<serde_json::Value>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub bypass_csp: bool,
    pub inject_js: Option<String>,
    pub load_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct SnapshotArgs {
    #[cfg_attr(feature = "clap", arg(short, long, help = "Only interactive nodes"))]
    #[serde(default)]
    pub interactive: bool,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Keep decorative and empty text nodes")
    )]
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ObserveArgs {
    #[schemars(
        description = "Filter: 'inputs' for form elements, 'buttons' for buttons and links, or 'all' for all observed elements."
    )]
    pub filter: Option<String>,
    #[schemars(description = "Maximum elements to return.")]
    pub max: Option<usize>,
    #[serde(default)]
    pub structured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ScreenshotArgs {
    pub output: Option<String>,
    #[serde(default)]
    pub annotate: bool,
    #[serde(default)]
    #[schemars(description = "Image format; PNG when omitted.")]
    pub format: Option<crate::sidecar::ScreenshotFormat>,
    #[serde(default)]
    #[schemars(range(max = 100), description = "JPEG or WebP quality, 0 to 100.")]
    pub quality: Option<u8>,
    #[serde(default)]
    #[schemars(description = "Document-relative CSS pixel region; conflicts with full_page.")]
    pub clip: Option<crate::sidecar::ScreenshotClip>,
    #[serde(default)]
    #[schemars(description = "Capture the whole scrollable page instead of the viewport.")]
    pub full_page: bool,
    #[serde(default)]
    #[schemars(
        description = "Return the image as base64 dataBase64 instead of writing a file; conflicts with output."
    )]
    pub inline: bool,
}

impl ScreenshotArgs {
    pub fn validate(&self) -> Result<(), crate::sidecar::ServerError> {
        crate::sidecar::validate_screenshot(
            self.format,
            self.quality,
            self.clip.as_ref(),
            Some(self.full_page),
        )?;
        if self.inline && self.output.is_some() {
            return Err(crate::sidecar::ServerError::invalid_params(
                "inline cannot be combined with output",
            ));
        }
        if self.annotate
            && (self.format.is_some()
                || self.quality.is_some()
                || self.clip.is_some()
                || self.full_page)
        {
            return Err(crate::sidecar::ServerError::invalid_params(
                "annotate cannot be combined with format, quality, clip or full page",
            ));
        }
        Ok(())
    }

    pub fn options(&self) -> eoka::ScreenshotOptions {
        crate::sidecar::screenshot_options(
            self.format,
            self.quality,
            self.clip.as_ref(),
            Some(self.full_page),
        )
    }

    pub fn mime_type(&self) -> &'static str {
        match self.format.unwrap_or(crate::sidecar::ScreenshotFormat::Png) {
            crate::sidecar::ScreenshotFormat::Png => "image/png",
            crate::sidecar::ScreenshotFormat::Jpeg => "image/jpeg",
            crate::sidecar::ScreenshotFormat::Webp => "image/webp",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self.format.unwrap_or(crate::sidecar::ScreenshotFormat::Png) {
            crate::sidecar::ScreenshotFormat::Png => "png",
            crate::sidecar::ScreenshotFormat::Jpeg => "jpg",
            crate::sidecar::ScreenshotFormat::Webp => "webp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct RecordingStartArgs {
    #[schemars(description = "Absolute path of the MP4 file to write.")]
    pub path: String,
    #[serde(default)]
    #[schemars(description = "UUID naming this recording; generated when omitted.")]
    pub recording_id: Option<String>,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
    #[serde(default)]
    #[schemars(range(min = 1, max = 120))]
    pub fps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct RecordingIdArgs {
    #[schemars(description = "Recording UUID returned by recording_start.")]
    pub recording_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct EmulateArgs {
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "390", help = "Viewport width in CSS pixels")
    )]
    #[serde(default = "default_viewport_width")]
    pub width: u32,
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "844", help = "Viewport height in CSS pixels")
    )]
    #[serde(default = "default_viewport_height")]
    pub height: u32,
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "2", help = "Device pixel ratio")
    )]
    #[serde(default = "default_device_pixel_ratio")]
    pub dpr: f64,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Emulate a desktop instead of a touch mobile device")
    )]
    #[serde(default)]
    pub desktop: bool,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Clear emulation and ignore the other flags")
    )]
    #[serde(default)]
    pub reset: bool,
}

impl Default for EmulateArgs {
    fn default() -> Self {
        Self {
            width: default_viewport_width(),
            height: default_viewport_height(),
            dpr: default_device_pixel_ratio(),
            desktop: false,
            reset: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct TextArgs {
    #[cfg_attr(feature = "clap", arg(help = "Text to search for"))]
    #[schemars(description = "Text substring to search for, matched case-insensitively.")]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct TargetArgs {
    #[cfg_attr(feature = "clap", arg(skip))]
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    #[cfg_attr(feature = "clap", arg(help = TARGET_HELP))]
    #[schemars(
        description = "Target element. Supports scoped refs from view, text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ClickArgs {
    #[serde(flatten)]
    pub element: TargetArgs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intercept_nav: Option<NavIntercept>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(
    description = "Install a history.pushState/replaceState diagnostic before the click and record the attempted navigation in window.__eoka_nav. block keeps the page where it is; allow lets the history change through. Requires expect; location navigation cannot be intercepted."
)]
pub enum NavIntercept {
    Block,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct HistoryGoArgs {
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    #[schemars(description = "History delta: -1 for back, 1 for forward, -2 for back twice, etc.")]
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FillArgs {
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    #[schemars(
        description = "Target input. Supports scoped refs from view, text:Email, placeholder:Enter code, css:input.search, id:email-field, or plain text search."
    )]
    pub target: String,
    #[schemars(description = "Text to type into the element.")]
    pub text: String,
    #[serde(default)]
    pub input_mode: crate::InputMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct SelectArgs {
    #[cfg_attr(feature = "clap", arg(skip))]
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    #[cfg_attr(feature = "clap", arg(help = TARGET_HELP))]
    #[schemars(
        description = "Target select element by scoped reference, text, CSS selector, id, or role."
    )]
    pub target: String,
    #[cfg_attr(feature = "clap", arg(help = "Option value or visible label"))]
    #[schemars(description = "Option value or visible text to select.")]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct KeyArgs {
    #[cfg_attr(feature = "clap", arg(skip))]
    #[serde(flatten)]
    pub operation: crate::ActionSettings,
    #[cfg_attr(
        feature = "clap",
        arg(help = "Key name or chord such as Enter, a, Shift+o, Control+a")
    )]
    #[schemars(
        description = "Key to press, for example Enter, Tab, Escape, ArrowDown, or Backspace."
    )]
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct HeldKeyArgs {
    #[cfg_attr(feature = "clap", arg(help = "Key name such as Shift, Control or a"))]
    #[schemars(description = "Key to hold or release, for example Shift, Control, or a.")]
    pub key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MouseButtonArgs {
    #[schemars(description = "Viewport horizontal coordinate in CSS pixels.")]
    pub x: f64,
    #[schemars(description = "Viewport vertical coordinate in CSS pixels.")]
    pub y: f64,
    #[schemars(description = "Mouse button to hold or release.")]
    pub button: MouseButton,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct MouseMoveArgs {
    #[cfg_attr(feature = "clap", arg(help = "Viewport X in CSS pixels"))]
    #[schemars(description = "Viewport horizontal coordinate in CSS pixels.")]
    pub x: f64,
    #[cfg_attr(feature = "clap", arg(help = "Viewport Y in CSS pixels"))]
    #[schemars(description = "Viewport vertical coordinate in CSS pixels.")]
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct ScriptArgs {
    #[serde(default)]
    pub arguments: Vec<serde_json::Value>,
    #[serde(default)]
    pub raw: bool,
    pub code: Option<String>,
    pub file: Option<String>,
    pub max_size: Option<usize>,
    #[serde(default)]
    pub no_await: bool,
    #[serde(default)]
    #[schemars(
        description = "Treat code as function source and call it with arguments as written, without expression or statement wrapping; conflicts with raw."
    )]
    pub function: bool,
    #[serde(default)]
    #[schemars(
        description = "With raw, return the expression's value as strict JSON, failing on values JSON cannot represent."
    )]
    pub json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FrameEvalArgs {
    #[schemars(
        description = "CSS selector matching exactly one iframe/frame in the top document, or id:<frame-id> from frames for explicit/nested selection. Bare URLs and indices are not supported. Evaluation uses an isolated world, not page-owned JavaScript globals."
    )]
    pub frame: String,
    pub code: Option<String>,
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FetchArgs {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub body: Option<String>,
    pub redirect: Option<String>,
    #[serde(default)]
    pub body_only: bool,
    #[schemars(
        description = "Raw text byte limit: default 8192, maximum 16777216; zero inspects headers only.",
        range(max = 16777216)
    )]
    pub max_body: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct SetCookieArgs {
    #[cfg_attr(feature = "clap", arg(help = "Cookie name"))]
    pub name: String,
    #[cfg_attr(feature = "clap", arg(help = "Cookie value"))]
    pub value: String,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Cookie domain [default: current page]")
    )]
    pub domain: Option<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Cookie path"))]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct DeleteCookieArgs {
    #[cfg_attr(feature = "clap", arg(help = "Cookie name"))]
    pub name: String,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Cookie domain [default: current page hostname]")
    )]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct StorageArgs {
    #[cfg_attr(feature = "clap", arg(help = "Key to read; omit for every key"))]
    pub key: Option<String>,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Use sessionStorage instead of localStorage")
    )]
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct SetStorageArgs {
    #[cfg_attr(feature = "clap", arg(help = "Storage key"))]
    pub key: String,
    #[cfg_attr(feature = "clap", arg(help = "Value to store"))]
    pub value: String,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Use sessionStorage instead of localStorage")
    )]
    #[serde(default)]
    pub session_storage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PathArgs {
    #[schemars(description = "File path.")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct LoadStateArgs {
    pub path: String,
    #[serde(default)]
    pub no_navigate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct HeadersArgs {
    #[cfg_attr(feature = "clap", arg(help = "Headers as a JSON object"))]
    pub headers_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct ConsoleArgs {
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Clear captured output after reading")
    )]
    #[serde(default)]
    pub clear: bool,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Only this level: log, warn, error, info or debug")
    )]
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct ClearFlagArgs {
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Clear captured errors after reading")
    )]
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct TabNewArgs {
    #[cfg_attr(feature = "clap", arg(help = "URL to open [default: blank page]"))]
    #[schemars(description = "Optional URL to navigate to. If omitted, opens a blank tab.")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct TabIdArgs {
    #[cfg_attr(feature = "clap", arg(help = "Tab ID from tab list"))]
    #[schemars(description = "Tab ID from list_tabs.")]
    pub tab_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct CloneFromArgs {
    pub source: String,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct WaitArgs {
    #[serde(default)]
    pub expect: Option<crate::Expectation>,
    pub ms: Option<u64>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub load: Option<String>,
    #[serde(default)]
    #[schemars(description = "CSS selector to wait for; see `state`.")]
    pub selector: Option<String>,
    #[serde(default)]
    #[schemars(description = "What the selector must become. Defaults to visible.")]
    pub state: Option<WaitSelectorState>,
    #[serde(default)]
    #[schemars(
        description = "Wait until no XHR or fetch request has run for this many milliseconds."
    )]
    pub network_idle: Option<u64>,
    pub timeout: Option<u64>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
#[schemars(description = "The condition a `wait` selector must reach.")]
pub enum WaitSelectorState {
    #[schemars(description = "Present in the DOM, rendered or not.")]
    Attached,
    #[schemars(description = "Rendered with visible CSS and a nonzero box.")]
    #[default]
    Visible,
    #[schemars(description = "Absent or no longer rendered.")]
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct DialogAcceptArgs {
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Text to enter for a prompt() dialog")
    )]
    #[serde(default)]
    #[schemars(
        description = "Text to enter for a prompt() dialog; ignored for alert and confirm."
    )]
    pub prompt_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct SelectorArgs {
    #[cfg_attr(feature = "clap", arg(help = "CSS selector"))]
    #[schemars(description = "CSS selector of the element.")]
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct AttributeArgs {
    #[cfg_attr(feature = "clap", arg(help = "CSS selector"))]
    #[schemars(description = "CSS selector of the element.")]
    pub selector: String,
    #[cfg_attr(feature = "clap", arg(help = "Attribute name, for example href"))]
    #[schemars(description = "Attribute name, for example href.")]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct ClickShadowArgs {
    #[cfg_attr(
        feature = "clap",
        arg(help = "CSS selector, matched through shadow roots")
    )]
    #[serde(deserialize_with = "shadow_selector")]
    #[schemars(description = "CSS selector, matched through open and closed shadow roots.")]
    pub selector: String,
    #[cfg_attr(
        feature = "clap",
        arg(
            long = "frame",
            value_name = "FRAME",
            help = "Frame ID from `eoka frames`; the top frame when omitted"
        )
    )]
    #[serde(default)]
    #[schemars(description = "Frame ID from `frames`; the top frame when omitted.")]
    pub frame_id: Option<String>,
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "10000", help = "Deadline in milliseconds")
    )]
    #[serde(
        default = "default_click_shadow_timeout_ms",
        deserialize_with = "shadow_timeout"
    )]
    pub timeout_ms: u64,
}

fn default_click_shadow_timeout_ms() -> u64 {
    10_000
}

fn shadow_selector<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let selector = String::deserialize(d)?;
    if selector.trim().is_empty() || selector.len() > 4096 {
        return Err(serde::de::Error::custom(
            "selector must be 1..=4096 characters",
        ));
    }
    Ok(selector)
}

fn shadow_timeout<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let timeout = u64::deserialize(d)?;
    if !(1..=60_000).contains(&timeout) {
        return Err(serde::de::Error::custom(
            "timeout_ms must be between 1 and 60000",
        ));
    }
    Ok(timeout)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct PathStringArgs {
    #[cfg_attr(feature = "clap", arg(help = "Route path such as /dashboard"))]
    #[schemars(description = "Target path, for example /docs or /about.")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct FakeCameraArgs {
    pub file: String,
    #[serde(default)]
    pub loop_video: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct WasmReadArgs {
    #[cfg_attr(feature = "clap", arg(help = WASM_ADDR_HELP))]
    pub addr: String,
    #[cfg_attr(feature = "clap", arg(help = "Number of bytes"))]
    pub len: usize,
    #[cfg_attr(feature = "clap", arg(long, help = WASM_MEMORY_HELP))]
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct WasmWriteArgs {
    #[cfg_attr(feature = "clap", arg(help = WASM_ADDR_HELP))]
    pub addr: String,
    #[cfg_attr(feature = "clap", arg(help = "Bytes as hex, such as deadbeef"))]
    pub hex: String,
    #[cfg_attr(feature = "clap", arg(long, help = WASM_MEMORY_HELP))]
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct WasmFindArgs {
    #[cfg_attr(
        feature = "clap",
        arg(help = "Hex bytes to find; spaces allowed, such as \"ff d8 ff e0\"")
    )]
    pub pattern: String,
    #[cfg_attr(feature = "clap", arg(long, help = "Start address [default: 0]"))]
    pub start: Option<String>,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "End address [default: end of memory]")
    )]
    pub end: Option<String>,
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "20", help = "Maximum matches")
    )]
    #[serde(default = "default_wasm_find_max")]
    pub max: usize,
    #[cfg_attr(feature = "clap", arg(long, help = WASM_MEMORY_HELP))]
    pub memory: Option<String>,
}

impl Default for WasmFindArgs {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            start: None,
            end: None,
            max: default_wasm_find_max(),
            memory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InterceptAddArgs {
    pub url_pattern: String,
    pub capture: Option<String>,
    pub respond: Option<String>,
    #[serde(default = "default_intercept_status")]
    pub status: u16,
}

impl Default for InterceptAddArgs {
    fn default() -> Self {
        Self {
            url_pattern: String::new(),
            capture: None,
            respond: None,
            status: default_intercept_status(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct ModeArgs {
    #[cfg_attr(feature = "clap", arg(help = "block-all or allow-all"))]
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct DomainArgs {
    #[cfg_attr(feature = "clap", arg(help = "Domain such as example.com"))]
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct CaptchaInjectArgs {
    pub token: String,
    #[serde(default = "default_captcha_type")]
    pub captcha_type: String,
    pub callback: Option<String>,
    pub click_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct NetworkRecordStartArgs {
    #[cfg_attr(
        feature = "clap",
        arg(long = "pattern", help = "URL glob to record; repeat for several")
    )]
    #[serde(default)]
    pub patterns: Vec<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Record metadata only"))]
    #[serde(default)]
    pub no_bodies: bool,
    #[cfg_attr(
        feature = "clap",
        arg(long, default_value = "10485760", help = "Per-response body cap")
    )]
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    #[cfg_attr(feature = "clap", arg(long, help = "Discard earlier entries first"))]
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct NetworkLogArgs {
    #[cfg_attr(feature = "clap", arg(long, help = "Maximum entries to return"))]
    pub limit: Option<usize>,
    #[cfg_attr(feature = "clap", arg(long, help = "URL glob such as */api/*"))]
    pub pattern: Option<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Only this HTTP method"))]
    pub method: Option<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Only this response status"))]
    pub status: Option<u16>,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Only entries after this request ID")
    )]
    pub since: Option<u64>,
    #[cfg_attr(feature = "clap", arg(long, help = "One line per request"))]
    #[serde(default)]
    pub compact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct NetworkShowArgs {
    #[cfg_attr(feature = "clap", arg(help = "Request ID from network log"))]
    pub id: u64,
    #[cfg_attr(feature = "clap", arg(long, help = "Include the response body"))]
    #[serde(default)]
    pub body: bool,
    #[cfg_attr(feature = "clap", arg(long, help = "Maximum body bytes to print"))]
    pub max_body: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct NetworkWaitArgs {
    #[cfg_attr(feature = "clap", arg(long, help = "URL glob such as */api/*"))]
    pub pattern: Option<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Only this HTTP method"))]
    pub method: Option<String>,
    #[cfg_attr(feature = "clap", arg(long, help = "Only this response status"))]
    pub status: Option<u16>,
    #[cfg_attr(feature = "clap", arg(long, help = "Timeout in milliseconds"))]
    pub timeout: Option<u64>,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Only match requests after this request ID")
    )]
    pub since: Option<u64>,
    #[cfg_attr(
        feature = "clap",
        arg(long, help = "Also match requests recorded before the wait started")
    )]
    #[serde(default)]
    pub include_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NetworkExportArgs {
    pub path: String,
    #[serde(default = "default_network_export_format")]
    pub format: String,
    pub settle_ms: Option<u64>,
}

impl Default for NetworkExportArgs {
    fn default() -> Self {
        Self {
            path: String::new(),
            format: default_network_export_format(),
            settle_ms: None,
        }
    }
}

fn default_max_body_bytes() -> usize {
    10 * 1024 * 1024
}

fn default_network_export_format() -> String {
    "har".to_string()
}

fn default_viewport_width() -> u32 {
    390
}

fn default_viewport_height() -> u32 {
    844
}

fn default_device_pixel_ratio() -> f64 {
    2.0
}

fn default_wasm_find_max() -> usize {
    20
}

fn default_intercept_status() -> u16 {
    200
}

fn default_captcha_type() -> String {
    "auto".to_string()
}
