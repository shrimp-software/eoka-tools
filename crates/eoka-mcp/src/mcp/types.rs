use serde::Deserialize;
use std::collections::HashMap;

pub type NewTabRequest = eoka_protocol::TabNewArgs;
pub type ObserveRequest = eoka_protocol::ObserveArgs;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NavigateRequest {
    #[serde(flatten)]
    pub operation: eoka_protocol::ActionSettings,
    #[schemars(description = "URL to navigate to")]
    pub url: String,
    #[schemars(
        description = "Extra HTTP request headers (e.g. {\"x-middleware-subrequest\": \"middleware\"} for CVE-2025-29927)"
    )]
    pub headers: Option<HashMap<String, String>>,
    #[schemars(description = "Override User-Agent string for this navigation")]
    pub user_agent: Option<String>,
    #[schemars(description = "Disable CSP enforcement before navigating (useful for XSS testing)")]
    pub bypass_csp: Option<bool>,
}

pub type JsRequest = eoka_protocol::ScriptArgs;

pub type DownloadStartRequest = eoka_protocol::DownloadStartArgs;
pub type DownloadIdRequest = eoka_protocol::DownloadIdArgs;
pub type DownloadWaitRequest = eoka_protocol::DownloadWaitArgs;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCookieRequest {
    #[schemars(description = "Cookie name")]
    pub name: String,
    #[schemars(description = "Cookie value")]
    pub value: String,
    #[schemars(
        description = "Cookie domain (e.g. '.example.com'). If omitted, uses current page domain."
    )]
    pub domain: Option<String>,
    #[schemars(description = "Cookie path (default: '/')")]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryGoRequest {
    #[serde(flatten)]
    pub operation: eoka_protocol::ActionSettings,
    #[schemars(description = "History delta: -1 for back, 1 for forward, -2 for back twice, etc.")]
    pub delta: i32,
}

pub type BatchRequest = eoka_protocol::ActionBatchArgs;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(not(feature = "anti-captcha"), allow(dead_code))]
pub struct SolveCaptchaRequest {
    #[schemars(description = "Anti-captcha.com API key")]
    pub api_key: String,
    #[schemars(
        description = "Captcha type: 'hcaptcha', 'recaptcha_v2', 'recaptcha_v2_enterprise', 'recaptcha_v3', or 'amazon_waf'"
    )]
    pub captcha_type: String,
    #[schemars(description = "Website/page URL")]
    pub website_url: String,
    #[schemars(description = "Site key for the captcha")]
    pub website_key: String,
    #[schemars(description = "Page action (for reCAPTCHA v3)")]
    pub page_action: Option<String>,
    #[schemars(description = "Minimum score (for reCAPTCHA v3, default 0.3)")]
    pub min_score: Option<f32>,
    #[schemars(
        description = "Optional object passed to grecaptcha.enterprise.render for reCAPTCHA v2 Enterprise"
    )]
    pub enterprise_payload: Option<serde_json::Value>,
    #[schemars(
        description = "Optional reCAPTCHA script domain: www.google.com or www.recaptcha.net"
    )]
    pub api_domain: Option<String>,
    #[schemars(description = "AWS WAF iv value from window.gokuProps (required for amazon_waf)")]
    pub iv: Option<String>,
    #[schemars(
        description = "AWS WAF context value from window.gokuProps (required for amazon_waf)"
    )]
    pub context: Option<String>,
    #[schemars(description = "Optional AWS WAF captcha.js URL")]
    pub captcha_script: Option<String>,
    #[schemars(description = "Optional AWS WAF challenge.js URL")]
    pub challenge_script: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DetectCaptchaRequest {
    #[schemars(description = "Auto-detect hCaptcha or reCAPTCHA on current page")]
    #[allow(dead_code)]
    pub auto_detect: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitMsRequest {
    #[schemars(description = "Milliseconds to wait (max 30000)")]
    pub ms: u64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClickInterceptNavRequest {
    #[schemars(
        description = "Canonical target from agent.observe, or a live text:, placeholder:, role:, css:, id: query. Numeric and legacy snapshot labels are not action targets."
    )]
    pub target: String,
    #[serde(flatten)]
    pub operation: eoka_protocol::ActionSettings,
    #[schemars(
        description = "If true, allow the navigation to proceed after capturing the URL (default: false — blocks navigation so you can read the page)"
    )]
    pub allow_nav: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteCookieRequest {
    #[schemars(description = "Cookie name to delete")]
    pub name: String,
    #[schemars(
        description = "Cookie domain (e.g. '.example.com'). If omitted, uses current page domain."
    )]
    pub domain: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AcceptDialogRequest {
    #[schemars(
        description = "Text to type for prompt() dialogs (optional, ignored for alert/confirm)"
    )]
    pub prompt_text: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForTextRequest {
    #[schemars(description = "Text substring to wait for (case-insensitive)")]
    pub text: String,
    #[schemars(description = "Timeout in milliseconds (default: 10000)")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitNetworkIdleRequest {
    #[schemars(description = "Milliseconds without requests to consider idle (default: 500)")]
    pub idle_ms: Option<u64>,
    #[schemars(description = "Maximum wait time in milliseconds (default: 10000)")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StorageKeyRequest {
    #[schemars(description = "Storage key to read")]
    pub key: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct StorageSetRequest {
    #[schemars(description = "Storage key")]
    pub key: String,
    #[schemars(description = "Value to store")]
    pub value: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConsoleRequest {
    #[schemars(description = "Clear the console buffer after reading (default: false)")]
    pub clear: Option<bool>,
    #[schemars(
        description = "Filter by level: 'log', 'warn', 'error', 'info', 'debug' (default: all)"
    )]
    pub level: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ErrorsRequest {
    #[schemars(description = "Clear the errors buffer after reading (default: false)")]
    pub clear: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoadStateRequest {
    #[schemars(description = "File path to load state from (JSON). Absolute path required.")]
    pub path: String,
    #[schemars(
        description = "Navigate to the saved URL before restoring (default: true). Set false to restore into the current tab. localStorage and sessionStorage are only restored when the tab is on the saved origin; otherwise they are skipped and the result says so."
    )]
    pub navigate: Option<bool>,
}
