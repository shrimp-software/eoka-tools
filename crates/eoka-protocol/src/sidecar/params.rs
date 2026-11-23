use super::*;

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
    #[serde(default = "default_sandbox")] sandbox: bool,
    #[serde(default)] aggressive_cdp_evasion: bool,
    #[serde(default)] geo_align: bool,
    #[serde(default)] extra_args: Vec<String>,
    #[serde(default)]
    #[schemars(schema_with = "optional_positive_scale_schema")]
    device_scale_factor: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_solve_cloudflare: Option<CloudflareAutoSolve>,
});
dto!(ProxyParams { server: String, username: Option<String>, password: Option<String> });
fn default_headless() -> bool {
    true
}

fn default_sandbox() -> bool {
    true
}

pub(super) fn positive_scale_schema(_: &mut SchemaGenerator) -> Schema {
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

dto!(PageIdParams { page_id: String });
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
dto!(CaptureStateResult {
    state: BrowserState
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
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* }) => {
        dto!($name {
            page_id: String,
            #[serde(rename = "operation_id")] operation_id: String,
            #[serde(default)] options: crate::ActionOptions,
            $($(#[$attr])* $field: $ty),*
        });
    };
}
action_params!(ActionPerformParams {
    action: crate::Action
});
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
