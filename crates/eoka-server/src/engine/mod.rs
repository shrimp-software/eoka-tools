mod agent;
pub mod blockers;
mod captcha;
mod downloads;
mod eval;
mod fetch_drain;
#[cfg(test)]
mod fetch_tests;
mod intercept;
mod intercept_files;
mod network;
mod network_ops;
mod operations;
pub mod page_ops;
mod persist;
pub mod profile;
mod proxy_forward;
mod script_policy;
mod state;
mod storage;
mod tabs;
mod target;
mod wasm;

use fetch_drain::*;
use storage::*;

use eoka_protocol::{
    ActionCommandArgs, CaptchaInjectArgs, ClearFlagArgs, CloneFromArgs, ConsoleArgs,
    DeleteCookieArgs, EmulateArgs, FakeCameraArgs, FetchArgs, HeadersArgs, IdArgs,
    InterceptAddArgs, LoadStateArgs, NetworkExportArgs, NetworkLogArgs, NetworkRecordStartArgs,
    NetworkShowArgs, NetworkWaitArgs, ObserveArgs, OpenArgs, PathArgs, PathStringArgs,
    RecordingIdArgs, RecordingStartArgs, ScreenshotArgs, SetCookieArgs, SetStorageArgs,
    SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs, TargetArgs, TextArgs,
};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

use crate::{annotate, observe, snapshot};
use ::eoka_solvers::{build_captcha_inject_js, parse_captcha_inject_kind};
use base64::Engine as _;
use eoka::cdp::{
    transport::{CommandDispatch, RequestPause},
    Session as CdpSession,
};
use serde_json::{json, Value};

use eoka_protocol::launch_spec::LaunchSpec;
use eoka_protocol::{Request, Response};
use intercept::{InterceptLog, InterceptLogEntry, InterceptRule, InterceptState};
use network::{NetworkConfig, NetworkRecorder};
use persist::{capture_state, ensure_console_capture, restore_cookies, restore_state, SavedState};
use script_policy::{ScriptPolicyMode, ScriptPolicyState};
use state::{BrowserState, TabState};
use target::{click_once, json_str, title_nonblocking, wait_for_stable};

#[doc(hidden)]
pub mod test_support {
    pub use super::intercept_files::FILE_SLOT;
}

fn recording_response(
    result: Result<Value, eoka_protocol::sidecar::ServerError>,
) -> Result<Response, EngineError> {
    match result {
        Ok(record) => Ok(Response::ok(record)),
        Err(error) => {
            let code = serde_json::to_value(error.code)
                .ok()
                .and_then(|code| code.as_str().map(snake_case))
                .unwrap_or_else(|| "recording_failed".to_owned());
            let mut response =
                Response::err_detail(eoka_protocol::ErrorDetail::new(code, error.message.clone()));
            response.data = serde_json::to_value(&error.data)
                .ok()
                .filter(|v| !v.is_null());
            Ok(response)
        }
    }
}

fn snake_case(pascal: &str) -> String {
    let mut snake = String::with_capacity(pascal.len() + 4);
    for (index, character) in pascal.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(character.to_ascii_lowercase());
    }
    snake
}

#[derive(Debug)]
pub struct EngineError {
    message: String,
    kind: Option<eoka_protocol::sidecar::ErrorCode>,
}

impl EngineError {
    fn into_response(self) -> Response {
        match self
            .kind
            .and_then(eoka_protocol::sidecar::core_error_detail_code)
        {
            Some(code) => Response::err_detail(eoka_protocol::ErrorDetail::new(code, self.message)),
            None => Response::err(self.message),
        }
    }
}

impl From<eoka::Error> for EngineError {
    fn from(error: eoka::Error) -> Self {
        eoka_protocol::sidecar::ServerError::from(error).into()
    }
}

impl From<eoka_protocol::sidecar::ServerError> for EngineError {
    fn from(error: eoka_protocol::sidecar::ServerError) -> Self {
        Self {
            message: error.message,
            kind: Some(error.code),
        }
    }
}

impl From<String> for EngineError {
    fn from(message: String) -> Self {
        Self {
            message,
            kind: None,
        }
    }
}

impl From<anyhow::Error> for EngineError {
    fn from(error: anyhow::Error) -> Self {
        match error.downcast::<eoka::Error>() {
            Ok(error) => error.into(),
            Err(error) => error.to_string().into(),
        }
    }
}

impl From<serde_json::Error> for EngineError {
    fn from(error: serde_json::Error) -> Self {
        error.to_string().into()
    }
}

impl From<std::io::Error> for EngineError {
    fn from(error: std::io::Error) -> Self {
        error.to_string().into()
    }
}

impl From<&str> for EngineError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

static HOST_VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

pub fn set_host_version(version: &'static str) {
    let _ = HOST_VERSION.set(version);
}

pub(crate) fn host_version() -> &'static str {
    HOST_VERSION
        .get()
        .copied()
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub struct Engine {
    spoken: Option<String>,
    agent: crate::agent::SharedAgentSession,
    operations: crate::operations::OperationManager,
    downloads: crate::downloads::DownloadManager,
    state: Option<BrowserState>,
    spec: LaunchSpec,
    session_name: String,
    intercept: InterceptState,
    script_policy: ScriptPolicyState,
    network: Option<NetworkRecorder>,
    idle_fetch_drain: Option<FetchDrainHandle>,
    fetch_events: Option<FetchQueue>,
    fetch_sender: Option<tokio::sync::mpsc::Sender<RequestPause>>,
    fetch_dropped: std::sync::Arc<std::sync::atomic::AtomicU64>,
    fetch_sessions: HashMap<String, CdpSession>,
    cloudflare: Option<crate::cloudflare::CloudflareWatcher>,
    recordings: crate::recordings::RecordingManager,
    patch_binary: Option<bool>,
}

impl Engine {
    pub fn new(session_name: impl Into<String>, spec: LaunchSpec) -> Self {
        let script_policy = match &spec {
            LaunchSpec::Launch {
                no_js,
                js_allow,
                js_block,
                ..
            } => ScriptPolicyState::new(
                if *no_js {
                    ScriptPolicyMode::BlockAll
                } else {
                    ScriptPolicyMode::AllowAll
                },
                js_allow.clone(),
                js_block.clone(),
            ),
            LaunchSpec::Connect { .. } | LaunchSpec::Reuse => ScriptPolicyState::default(),
        };
        Self {
            spoken: None,
            agent: Default::default(),
            operations: Default::default(),
            downloads: Default::default(),
            state: None,
            spec,
            session_name: session_name.into(),
            intercept: InterceptState::new(),
            script_policy,
            network: None,
            idle_fetch_drain: None,
            fetch_events: None,
            fetch_sender: None,
            fetch_dropped: Default::default(),
            fetch_sessions: HashMap::new(),
            cloudflare: None,
            recordings: Default::default(),
            patch_binary: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.is_some()
    }

    pub fn patch_binary(&mut self, patch: bool) {
        self.patch_binary = Some(patch);
    }

    pub async fn launch_with(&mut self, spec: LaunchSpec) -> Result<(), String> {
        self.spec = spec;
        self.ensure_browser().await
    }

    pub fn adopt(
        session_name: impl Into<String>,
        browser: std::sync::Arc<eoka::Browser>,
        operations: crate::operations::OperationManager,
        recordings: crate::recordings::RecordingManager,
        agent: crate::agent::SharedAgentSession,
    ) -> Self {
        let mut engine = Self::new(session_name, LaunchSpec::Reuse);
        engine.operations = operations;
        engine.recordings = recordings;
        engine.agent = agent;
        engine.state = Some(BrowserState::adopted(browser));
        engine
    }

    pub async fn handle_on(
        &mut self,
        tab_id: &str,
        page: &eoka::Page,
        request: Request,
        spoken: Option<&str>,
    ) -> Response {
        let mut switched = false;
        if let Some(state) = self.state.as_mut() {
            let tab = state
                .tabs
                .entry(tab_id.to_owned())
                .or_insert_with(|| TabState::new(page.clone()));
            tab.invalidate();
            switched = state.current_tab_id.as_deref() != Some(tab_id);
            state.current_tab_id = Some(tab_id.to_owned());
        }
        if switched {
            if let Err(error) = self.sync_current_target_features().await {
                return Response::err(error);
            }
        }
        self.handle_as(spoken, request).await
    }

    pub async fn handle_as(&mut self, spoken: Option<&str>, request: Request) -> Response {
        if let Some(tab) = self
            .state
            .as_mut()
            .and_then(|state| state.current_tab_mut())
        {
            tab.invalidate();
        }
        self.spoken = spoken.map(str::to_owned);
        let response = self.handle(request).await;
        self.spoken = None;
        self.service_idle_fetch();
        response
    }

    pub async fn release_tab(&mut self, tab_id: &str) {
        let session = self
            .state
            .as_ref()
            .and_then(|state| state.tabs.get(tab_id))
            .map(|tab| tab.page.session().clone());
        if let Some(session) = session {
            if self.fetch_sessions.contains_key(session.session_id()) {
                self.disable_fetch_session(&session).await;
            }
        }
        self.forget_tab(tab_id);
        self.service_idle_fetch();
    }

    pub fn forget_tab(&mut self, tab_id: &str) {
        if let Some(state) = self.state.as_mut() {
            state.tabs.remove(tab_id);
            if state.current_tab_id.as_deref() == Some(tab_id) {
                state.current_tab_id = None;
            }
        }
    }

    pub async fn release(&mut self) {
        self.downloads.stop().await;
        if let Some(recorder) = self.network.take() {
            let session = self
                .state
                .as_ref()
                .and_then(|state| state.current_tab())
                .map(|tab| tab.page.session().clone());
            let _ = recorder.stop(session).await;
        }
        self.disable_all_fetch_sessions().await;
        self.state = None;
    }

    pub fn is_recording(&self) -> bool {
        self.recordings.is_active()
    }

    async fn ensure_browser(&mut self) -> Result<(), String> {
        if self.state.is_some() {
            return Ok(());
        }
        let mut state = match &self.spec {
            LaunchSpec::Reuse => {
                return Err("Daemon creation requires explicit launch configuration".into())
            }
            LaunchSpec::Connect { ws_url } => BrowserState::connected(ws_url)
                .await
                .map_err(|e| e.to_string())?,
            LaunchSpec::Launch {
                headless,
                from_profile,
                clone_state_from,
                no_stealth,
                proxy,
                persist,
                geo_align,
                sandbox,
                ..
            } => {
                let persist_dir = if *persist {
                    Some(eoka_protocol::launch_spec::profile_dir(&self.session_name))
                } else {
                    None
                };
                let mut s = BrowserState::launched(
                    state::LaunchFlags {
                        headless: *headless,
                        no_stealth: *no_stealth,
                        geo_align: *geo_align,
                        sandbox: *sandbox,
                        patch_binary: self.patch_binary,
                    },
                    from_profile.as_deref(),
                    proxy.clone(),
                    persist_dir.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?;

                if let Some(source) = clone_state_from {
                    let saved = persist::clone_state_from_source(source).await?;
                    let url = saved.url.clone();
                    let tab = if !url.is_empty() {
                        s.ensure_tab(&url).await.map_err(|e| e.to_string())?
                    } else {
                        s.ensure_blank_tab().await.map_err(|e| e.to_string())?
                    };
                    let _ = wait_for_stable(&tab.page).await;
                    let restored =
                        restore_state_and_maybe_reload(tab, &saved, state_url_can_reload(&url))
                            .await?;
                    eprintln!("[eoka]{}", restored.notice(&saved.url));
                }
                s
            }
        };

        if state.is_live && state.current_tab_id.is_none() {
            if let Ok(tabs) = state.browser.tabs().await {
                if let Some(t) = tabs.into_iter().find(|t| !t.url.starts_with("devtools://")) {
                    let _ = state.attach_existing_tab(&t.id).await;
                }
            }
        }

        self.state = Some(state);
        if matches!(
            self.spec,
            LaunchSpec::Launch {
                auto_solve_cloudflare: true,
                ..
            }
        ) && self.cloudflare.is_none()
        {
            self.cloudflare = Some(crate::cloudflare::CloudflareWatcher::start(
                crate::cloudflare::DEFAULT_WATCH_INTERVAL,
                crate::cloudflare::DEFAULT_SOLVE_TIMEOUT,
            ));
        }
        Ok(())
    }

    fn publish_cloudflare_pages(&self) {
        if let Some(watcher) = &self.cloudflare {
            let tabs = self.state.as_ref().map(|state| &state.tabs);
            watcher.set_pages(tabs.into_iter().flatten().map(|(id, tab)| (id, &tab.page)));
        }
    }

    fn script_enabled_for(&self, url: &str) -> Option<bool> {
        self.script_policy.is_active().then(|| {
            let host = url::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned));
            self.script_policy.resolve(host.as_deref().unwrap_or(""))
        })
    }

    fn require_tab(&self) -> Result<&TabState, String> {
        self.state
            .as_ref()
            .ok_or("No browser open. Use 'open' first.")?
            .current_tab()
            .ok_or_else(|| "No tab open. Use 'open' first.".to_string())
    }

    fn require_tab_mut(&mut self) -> Result<&mut TabState, String> {
        self.state
            .as_mut()
            .ok_or("No browser open. Use 'open' first.")?
            .current_tab_mut()
            .ok_or_else(|| "No tab open. Use 'open' first.".to_string())
    }

    fn require_state_mut(&mut self) -> Result<&mut BrowserState, String> {
        self.state
            .as_mut()
            .ok_or_else(|| "No browser open. Use 'open' first.".to_string())
    }

    pub async fn handle(&mut self, request: Request) -> Response {
        let cmd = request.cmd();
        let args = &request.args_json();
        let policy = blockers::policy(cmd, args);
        let response = match self.refuse_blocked(cmd, args, policy).await {
            Some(refusal) => refusal,
            None => {
                let mut response = Box::pin(self.handle_command(request, args)).await;
                self.diagnose_blocked(cmd, args, policy, &mut response)
                    .await;
                response
            }
        };
        self.publish_cloudflare_pages();
        response
    }

    async fn handle_command(&mut self, request: Request, args: &Value) -> Response {
        if matches!(request, Request::SessionHello) {
            let mode = match self.spec {
                LaunchSpec::Launch { headless: true, .. } => "headless",
                LaunchSpec::Launch {
                    headless: false, ..
                } => "headed",
                LaunchSpec::Connect { .. } => "connect",
                LaunchSpec::Reuse => "reuse",
            };
            return Response::ok(
                json!({"protocol_major": eoka_protocol::sidecar::PROTOCOL_VERSION.major, "daemon_version":host_version(), "session":self.session_name, "mode":mode, "configuration_fingerprint":self.spec.configuration_fingerprint()}),
            );
        }
        if let Err(error) = crate::url_policy::ensure_args_allowed(args) {
            return Response::err(error);
        }
        let cmd = request.cmd();
        match request {
            Request::AgentObserve(observe) => {
                self.handle_agent(agent::AgentCall::Observe(observe)).await
            }
            Request::AgentStep(step) => self.handle_agent(agent::AgentCall::Step(step)).await,
            Request::ActionCommand(envelope) => self.action_command(envelope).await,
            Request::OperationGet(_)
            | Request::OperationList(_)
            | Request::OperationWait(_)
            | Request::OperationCancel(_) => match self.operations.query(cmd, args).await {
                Some(response) => response,
                None => Response::err(format!("Unknown command: {cmd}")),
            },
            query @ (Request::DownloadList
            | Request::DownloadGet(_)
            | Request::DownloadWait(_)
            | Request::DownloadCancel(_)) => match self.downloads.query(&query).await {
                Some(response) => response,
                None => Response::err(format!("Unknown command: {cmd}")),
            },
            Request::ActionPerform(_) => self.handle_action(cmd, args).await,
            action if action.is_convenience_action() => self.handle_action(cmd, args).await,
            Request::DownloadStart(start) => {
                if let Err(error) = self.downloads.restore().await {
                    return Response::err_detail(error);
                }
                self.cmd_download_start(start).await
            }
            Request::CaptchaDatadome(datadome) => self.cmd_captcha_datadome(datadome).await,
            page_command => self.page_command(page_command, args).await,
        }
    }

    async fn action_command(&mut self, envelope: ActionCommandArgs) -> Response {
        let command = envelope.command.as_str();
        if (!crate::operations::commands::is_action(command) && command != "download_start")
            || command == "action_perform"
        {
            return Response::err("Invalid convenience action");
        }
        let mut arguments = envelope.arguments;
        if !arguments.is_object() {
            arguments = serde_json::json!({});
        }
        arguments["operation_id"] = Value::String(envelope.operation_id);
        if command == "download_start" {
            return match serde_json::from_value(arguments) {
                Ok(start) => self.cmd_download_start(start).await,
                Err(error) => crate::downloads::rejected(crate::downloads::failure(
                    "invalid_input",
                    error.to_string(),
                )),
            };
        }
        self.handle_action(command, &arguments).await
    }

    async fn page_command(&mut self, request: Request, args: &Value) -> Response {
        if matches!(request, Request::Close | Request::TabClose(_)) {
            self.operations.stop().await;
        }
        let idle = self.idle_fetch_drain.take();
        self.stop_fetch_drain(idle).await;
        self.drain_fetch_events().await;
        let response = match self.dispatch_guarded(request, args).await {
            Ok(resp) => resp,
            Err(e) => e.into_response(),
        };
        self.drain_fetch_events().await;
        response
    }

    async fn dispatch(&mut self, request: Request) -> Result<Response, EngineError> {
        match request {
            request @ (page_ops::page_request!()) => self.on_current_page(request).await,
            query @ (Request::DownloadList
            | Request::DownloadGet(_)
            | Request::DownloadWait(_)
            | Request::DownloadCancel(_)) => Ok(self.downloads.answer(&query).await),
            Request::Open(args) => self.cmd_open(&args).await,
            Request::Snapshot(args) => self.cmd_snapshot(&args).await,
            Request::Observe(args) => self.cmd_observe(&args).await,
            Request::Screenshot(args) => self.cmd_screenshot(args).await,
            Request::Emulate(args) => self.cmd_emulate(&args).await,
            Request::Find(args) => self.cmd_find(&args).await,
            Request::RecordingStart(args) => self.cmd_recording_start(args).await,
            Request::RecordingGet(args) => self.cmd_recording_control("recording.get", &args).await,
            Request::RecordingStop(args) => {
                self.cmd_recording_control("recording.stop", &args).await
            }
            Request::RecordingCancel(args) => {
                self.cmd_recording_control("recording.cancel", &args).await
            }
            Request::Scroll(args) => self.cmd_scroll(&args).await,
            Request::Eval(args) => self.script("eval", eval::Script::Eval(args)).await,
            Request::Exec(args) => self.script("exec", eval::Script::Exec(args)).await,
            Request::FrameEval(args) => {
                self.script("frame_eval", eval::Script::FrameEval(args))
                    .await
            }
            Request::CaptchaInject(args) => self.cmd_captcha_inject(&args).await,
            Request::CaptchaDetect => self.cmd_captcha_detect().await,
            Request::DialogAccept(args) => self.cmd_dialog(Some(args.prompt_text.as_deref())).await,
            Request::DialogDismiss => self.cmd_dialog(None).await,
            Request::Fetch(args) => self.cmd_fetch(args).await,
            Request::Cookies => self.cmd_cookies().await,
            Request::SetCookie(args) => self.cmd_set_cookie(&args).await,
            Request::DeleteCookie(args) => self.cmd_delete_cookie(&args).await,
            Request::ClearCookies => self.cmd_clear_cookies().await,
            Request::Storage(args) => self.cmd_storage(&args).await,
            Request::SetStorage(args) => self.cmd_set_storage(&args).await,
            Request::DumpStorage => self.cmd_dump_storage().await,
            Request::SaveState(args) => self.cmd_save_state(&args).await,
            Request::LoadState(args) => self.cmd_load_state(&args).await,
            Request::Headers(args) => self.cmd_headers(&args).await,
            Request::Console(args) => self.cmd_console(&args).await,
            Request::Errors(args) => self.cmd_errors(&args).await,
            Request::TabList => self.cmd_tab_list().await,
            Request::TabNew(args) => self.cmd_tab_new(&args).await,
            Request::TabSwitch(args) => self.cmd_tab_switch(&args).await,
            Request::TabClose(args) => self.cmd_tab_close(&args).await,
            Request::TabAttach(args) => self.cmd_tab_attach(&args).await,
            Request::CloneFrom(args) => self.cmd_clone_from(&args).await,
            Request::SpaInfo => self.cmd_spa_info().await,
            Request::SpaNavigate(args) => self.cmd_spa_navigate(&args).await,
            Request::FakeCamera(args) => self.cmd_fake_camera(&args).await,
            Request::WasmInfo => self.cmd_wasm_info().await,
            Request::WasmRead(args) => self.cmd_wasm_read(&args).await,
            Request::WasmWrite(args) => self.cmd_wasm_write(&args).await,
            Request::WasmFind(args) => self.cmd_wasm_find(&args).await,
            Request::InterceptAdd(args) => self.cmd_intercept_add(&args).await,
            Request::InterceptList => self.cmd_intercept_list().await,
            Request::InterceptRemove(args) => self.cmd_intercept_remove(&args).await,
            Request::InterceptLog(args) => self.cmd_intercept_log(&args).await,
            Request::JsMode(args) => self.cmd_js_mode(&args).await,
            Request::JsAllow(args) => self.cmd_js_allow(&args).await,
            Request::JsBlock(args) => self.cmd_js_block(&args).await,
            Request::JsRemove(args) => self.cmd_js_remove(&args).await,
            Request::JsList => self.cmd_js_list().await,
            Request::NetworkRecordStart(args) => self.cmd_network_record_start(&args).await,
            Request::NetworkRecordStop => self.cmd_network_record_stop().await,
            Request::NetworkRecordStatus => self.cmd_network_record_status().await,
            Request::NetworkLog(args) => self.cmd_network_log(&args).await,
            Request::NetworkShow(args) => self.cmd_network_show(&args).await,
            Request::NetworkExport(args) => self.cmd_network_export(&args).await,
            Request::NetworkWait(args) => self.cmd_network_wait(&args).await,
            Request::NetworkClear => self.cmd_network_clear().await,
            Request::Close => self.cmd_close().await,
            before_the_page @ (Request::SessionHello
            | Request::AgentObserve(_)
            | Request::AgentStep(_)
            | Request::ActionCommand(_)
            | Request::ActionPerform(_)
            | Request::OperationGet(_)
            | Request::OperationList(_)
            | Request::OperationWait(_)
            | Request::OperationCancel(_)
            | Request::Click(_)
            | Request::DblClick(_)
            | Request::Fill(_)
            | Request::Select(_)
            | Request::Hover(_)
            | Request::Key(_)
            | Request::Back
            | Request::Forward
            | Request::Reload
            | Request::DownloadStart(_)
            | Request::CaptchaDatadome(_)
            | Request::Shutdown) => {
                Err(format!("Unknown command: {}", before_the_page.cmd()).into())
            }
        }
    }

    async fn on_current_page(&mut self, request: Request) -> Result<Response, EngineError> {
        let page = self.require_tab()?.page.clone();
        let drain = match &request {
            Request::Wait(args) if args.ms.is_none() => self.start_fetch_drain(),
            Request::CaptchaTurnstile(_) => self.start_fetch_drain(),
            _ => None,
        };
        let result = page_ops::execute(&page, request).await;
        if !self.stop_fetch_drain(drain).await {
            return Err("Request interception cleanup failed".into());
        }
        Ok(result?)
    }

    fn tab_with_config(&mut self) -> Result<(&mut TabState, bool), String> {
        let state = self.require_state_mut()?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or("No tab open. Use 'open' first.")?;
        Ok((tab, vp))
    }

    async fn cmd_open(&mut self, args: &OpenArgs) -> Result<Response, EngineError> {
        let requested_url = args.url.clone();
        let open_state = args
            .load_state
            .as_deref()
            .map(read_saved_state_file)
            .transpose()?;
        let url = resolve_open_url_for_state(&requested_url, open_state.as_ref())?;
        let state_init_js = open_state
            .as_ref()
            .filter(|saved| state_should_prime_open(saved, &url))
            .and_then(build_storage_seed_js);
        self.ensure_browser().await?;

        let headers: Option<HashMap<String, String>> = args
            .headers
            .as_ref()
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let user_agent = args.user_agent.as_deref();
        let bypass_csp = args.bypass_csp;
        let inject_js = args.inject_js.as_deref();
        let script_enabled = self.script_enabled_for(&url);
        let _has_extras = headers.is_some()
            || user_agent.is_some()
            || bypass_csp
            || inject_js.is_some()
            || open_state.is_some()
            || state_init_js.is_some()
            || script_enabled.is_some();

        let operations = self.operations.clone();
        let operation_id = args
            .operation
            .operation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let options =
            crate::operations::commands::options(&serde_json::to_value(&args.operation)?)?;
        let budget = options.timeout_ms;
        let drain = self.start_fetch_drain();
        let active_recorder = self.network.clone();
        let result = async {
            let state = self.require_state_mut()?;

            if state.is_live {
                state.new_tab(None).await.map_err(|e| e.to_string())?;
                if let Some(recorder) = active_recorder.as_ref() {
                    if recorder.is_active().await {
                        let session = state.current_tab().ok_or("No tab")?.page.session().clone();
                        recorder.update_session(&session).await?;
                    }
                }
            }

            {
                let tab = if state.current_tab_id.is_some() {
                    state.current_tab_mut().ok_or("No tab")?
                } else {
                    state.ensure_blank_tab().await.map_err(|e| e.to_string())?
                };
                blockers::watch(&tab.page);
                if let Some(saved) = open_state.as_ref() {
                    restore_cookies(&tab.page, saved).await?;
                }
                if let Some(ua) = user_agent {
                    tab.page
                        .set_user_agent(ua)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if bypass_csp {
                    tab.page
                        .set_bypass_csp(true)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if let Some(ref h) = headers {
                    tab.page
                        .set_extra_headers(h.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if let Some(js) = inject_js {
                    let source = if js.ends_with(".js") {
                        std::fs::read_to_string(js)
                            .map_err(|e| format!("inject_js read error: {}", e))?
                    } else {
                        js.to_string()
                    };
                    tab.page
                        .session()
                        .send::<_, serde_json::Value>(
                            "Page.addScriptToEvaluateOnNewDocument",
                            &serde_json::json!({ "source": source }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                }
                let state_script_id = if let Some(source) = state_init_js.as_deref() {
                    let response: Value = tab
                        .page
                        .session()
                        .send(
                            "Page.addScriptToEvaluateOnNewDocument",
                            &json!({ "source": source }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    response
                        .get("identifier")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                } else {
                    None
                };
                if let Some(enabled) = script_enabled {
                    tab.page
                        .set_javascript_enabled(enabled)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                tab.invalidate();
                let record = operations
                    .submit(
                        tab.page.clone(),
                        operation_id.clone(),
                        eoka::Action::Navigate {
                            url: url.clone(),
                            referrer: None,
                        },
                        options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let nav_result = if matches!(record.state, eoka_protocol::OperationState::Running) {
                    blockers::wait_unless_dialog(&operations, &tab.page, &operation_id, budget)
                        .await
                } else {
                    Ok(record)
                };
                if let Some(identifier) = state_script_id {
                    let _ = tab
                        .page
                        .session()
                        .send::<_, Value>(
                            "Page.removeScriptToEvaluateOnNewDocument",
                            &json!({ "identifier": identifier }),
                        )
                        .await;
                }
                if headers.is_some() {
                    let _ = tab.page.clear_extra_headers().await;
                }
                nav_result.map_err(|e| e.to_string())
            }
        }
        .await;
        self.stop_fetch_drain(drain).await;
        if let Some(enabled) = script_enabled {
            self.script_policy.note_applied(enabled);
        }
        let record = result?;
        self.sync_current_target_features().await?;
        Ok(Response::action_result(record))
    }

    async fn cmd_snapshot(&mut self, args: &SnapshotArgs) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let include_all = args.all;
        let tab = self.require_tab_mut()?;

        let result = snapshot::snapshot(&tab.page, include_all).await?;

        Ok(Response::ok_text(result.tree_text))
    }

    async fn cmd_observe(&mut self, args: &ObserveArgs) -> Result<Response, EngineError> {
        let (tab, viewport_only) = self.tab_with_config()?;
        let (elements, data) = observe::inspect(&tab.page, viewport_only, args).await?;
        tab.elements = elements;
        Ok(Response::ok(data))
    }

    async fn cmd_screenshot(&mut self, capture: ScreenshotArgs) -> Result<Response, EngineError> {
        let annotate_flag = capture.annotate;
        let output = capture.output.as_deref();

        let (tab, viewport_only) = self.tab_with_config()?;

        let params = eoka_protocol::sidecar::ScreenshotParams {
            page_id: String::new(),
            format: capture.format,
            quality: capture.quality,
            clip: capture.clip,
            full_page: capture.full_page.then_some(true),
        };
        params.validate().map_err(|error| error.message)?;
        let plain = params.format.is_none()
            && params.quality.is_none()
            && params.clip.is_none()
            && params.full_page.is_none();
        if annotate_flag && !plain {
            return Err(
                "--annotate cannot be combined with format, quality, clip or full page".into(),
            );
        }
        let extension = match params.format {
            Some(eoka_protocol::sidecar::ScreenshotFormat::Jpeg) => "jpg",
            Some(eoka_protocol::sidecar::ScreenshotFormat::Webp) => "webp",
            _ => "png",
        };

        let png = if annotate_flag {
            if tab.elements.is_empty() {
                tab.elements = observe::observe(&tab.page, viewport_only).await?;
            }
            annotate::annotated_screenshot(&tab.page, &tab.elements).await?
        } else {
            crate::capture::screenshot(&tab.page, &params.options()).await?
        };

        let path = match output {
            Some(p) => std::path::PathBuf::from(p),
            None => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                std::env::temp_dir().join(format!("eoka-screenshot-{ts}.{extension}"))
            }
        };

        std::fs::write(&path, &png)?;

        let mut result = json!({ "path": path.to_string_lossy(), "size": png.len() });
        if annotate_flag {
            let mut list = String::new();
            for el in &tab.elements {
                let _ = writeln!(list, "{}", el);
            }
            result["elements"] = Value::String(list);
        }
        Ok(Response::ok(result))
    }

    async fn cmd_emulate(&mut self, args: &EmulateArgs) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let reset = args.reset;
        let tab = self.require_tab_mut()?;
        let session = tab.page.session();

        if reset {
            session
                .send::<_, Value>("Emulation.clearDeviceMetricsOverride", &json!({}))
                .await?;
            let _ = session
                .send::<_, Value>(
                    "Emulation.setTouchEmulationEnabled",
                    &json!({ "enabled": false }),
                )
                .await;
            return Ok(Response::ok_text("Device emulation cleared"));
        }

        let width = args.width;
        let height = args.height;
        let dpr = args.dpr;
        let mobile = !args.desktop;

        session
            .send::<_, Value>(
                "Emulation.setDeviceMetricsOverride",
                &json!({
                    "width": width,
                    "height": height,
                    "deviceScaleFactor": dpr,
                    "mobile": mobile,
                }),
            )
            .await?;

        let _ = session
            .send::<_, Value>(
                "Emulation.setTouchEmulationEnabled",
                &json!({ "enabled": mobile, "maxTouchPoints": 5 }),
            )
            .await;

        Ok(Response::ok(json!({
            "width": width,
            "height": height,
            "dpr": dpr,
            "mobile": mobile,
        })))
    }

    async fn cmd_recording_start(
        &mut self,
        args: RecordingStartArgs,
    ) -> Result<Response, EngineError> {
        let state = self
            .state
            .as_ref()
            .ok_or("No browser open. Use 'open' first.")?;
        let page_id = state.current_tab_id.clone().ok_or("Open a tab first")?;
        let page = self.require_tab()?.page.clone();
        let params = eoka_protocol::sidecar::RecordingStartParams {
            page_id,
            recording_id: args
                .recording_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            path: args.path,
            max_width: args.max_width,
            max_height: args.max_height,
            fps: args.fps,
        };
        let record = self.recordings.start(params, Ok(page)).await;
        recording_response(record.and_then(|record| {
            serde_json::to_value(record)
                .map_err(|e| eoka_protocol::sidecar::ServerError::internal(e.to_string()))
        }))
    }

    async fn cmd_recording_control(
        &mut self,
        method: &str,
        args: &RecordingIdArgs,
    ) -> Result<Response, EngineError> {
        let params = json!({"recordingId": args.recording_id});
        recording_response(crate::recordings::control(&self.recordings, method, params).await)
    }

    async fn cmd_dialog(&mut self, accept: Option<Option<&str>>) -> Result<Response, EngineError> {
        let tab = self.require_tab_mut()?;
        tab.invalidate();
        let page = tab.page.clone();
        let reply = blockers::answer(&self.operations, &page, accept).await?;
        Ok(Response::ok_text(reply))
    }

    async fn cmd_captcha_detect(&mut self) -> Result<Response, EngineError> {
        let page = &self.require_tab()?.page;
        Ok(Response::ok(
            match crate::captcha::detect_captcha_on_page(page).await {
                Some(info) => {
                    json!({"detected": true, "type": info.captcha_type, "sitekey": info.sitekey})
                }
                None => json!({"detected": false}),
            },
        ))
    }

    async fn cmd_find(&mut self, args: &TextArgs) -> Result<Response, EngineError> {
        let text = args.text.as_str();
        let (tab, viewport_only) = self.tab_with_config()?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, viewport_only).await?;
        }

        let needle = text.to_lowercase();
        let matches: Vec<_> = tab
            .elements
            .iter()
            .filter(|e| {
                e.text.to_lowercase().contains(&needle)
                    || e.placeholder
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(&needle))
            })
            .collect();

        if matches.is_empty() {
            Ok(Response::ok_text(format!(
                "No elements found matching \"{}\"",
                text
            )))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            Ok(Response::ok_text(out))
        }
    }

    async fn cmd_scroll(&mut self, args: &TargetArgs) -> Result<Response, EngineError> {
        let target_str = args.target.as_str();
        let tab = self.require_tab()?;

        match target_str {
            "up" => {
                tab.page
                    .execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)")
                    .await
            }
            "down" => {
                tab.page
                    .execute_sync("window.scrollBy(0, window.innerHeight * 0.8)")
                    .await
            }
            "top" => tab.page.execute_sync("window.scrollTo(0, 0)").await,
            "bottom" => {
                tab.page
                    .execute_sync("window.scrollTo(0, document.body.scrollHeight)")
                    .await
            }
            _ => {
                let target = self
                    .agent
                    .lock()
                    .await
                    .resolve_target(&tab.page, target_str)
                    .await?;
                let id = uuid::Uuid::new_v4().to_string();
                self.operations
                    .submit(
                        tab.page.clone(),
                        id.clone(),
                        eoka::Action::ScrollIntoView { target },
                        Default::default(),
                    )
                    .await?;
                return Ok(Response::action_result(
                    self.operations.wait(&id, 30_000).await?,
                ));
            }
        }?;
        Ok(Response::ok_text(format!("Scrolled {}", target_str)))
    }

    async fn cmd_captcha_inject(
        &mut self,
        args: &CaptchaInjectArgs,
    ) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let token = args.token.as_str();
        let captcha_type = args.captcha_type.as_str();
        let callback = args.callback.as_deref();
        let click_after = args.click_after.as_deref().map(str::to_string);
        let kind = parse_captcha_inject_kind(captcha_type).map_err(|error| error.to_string())?;
        let js = build_captcha_inject_js(token, kind, callback);
        let result: String = {
            let tab = self.require_tab()?;
            tab.page.evaluate(&js).await?
        };
        let mut parsed: Value = serde_json::from_str(&result)?;
        if let Some(target) = click_after {
            let (tab, viewport_only) = self.tab_with_config()?;
            let desc = click_once(tab, &target, viewport_only).await?;
            if let Value::Object(ref mut object) = parsed {
                object.insert(
                    "click_after".into(),
                    json!({ "target": target, "clicked": desc }),
                );
            }
        } else if let Value::Object(ref mut object) = parsed {
            object.insert(
                "next_action".into(),
                Value::String(
                    "If the page does not advance after callbacks run, click the continuation control again or pass --click-after.".into(),
                ),
            );
        }
        Ok(Response::ok(parsed))
    }

    async fn cmd_fetch(&mut self, args: FetchArgs) -> Result<Response, EngineError> {
        let page = self.require_tab()?.page.clone();
        let drain = self.start_fetch_drain();
        let remedies = blockers::Remedies::new(Some(&self.session_name));
        let name = self.named(&remedies, "fetch", &Value::Null);
        let response = blockers::guard(
            Some(&page),
            remedies,
            &name,
            blockers::Policy::Script,
            None,
            async { Ok::<_, EngineError>(crate::browser_fetch::fetch(&page, args).await) },
        )
        .await;
        self.stop_fetch_drain(drain).await;
        response
    }

    async fn cmd_headers(&mut self, args: &HeadersArgs) -> Result<Response, EngineError> {
        let headers: HashMap<String, String> =
            serde_json::from_str(&args.headers_json).map_err(|_| "Invalid headers JSON")?;

        let tab = self.require_tab()?;
        tab.page.set_extra_headers(headers.clone()).await?;
        Ok(Response::ok_text(format!(
            "Set {} extra headers",
            headers.len()
        )))
    }

    async fn cmd_console(&mut self, args: &ConsoleArgs) -> Result<Response, EngineError> {
        let clear = args.clear;
        let level = args.level.as_deref();

        let tab = self.require_tab_mut()?;
        ensure_console_capture(tab).await?;

        let js = match level {
            Some(l) => format!(
                "JSON.stringify((__eoka_console || []).filter(e => e.level === {}))",
                json_str(l)
            ),
            None => "JSON.stringify(__eoka_console || [])".into(),
        };
        let parsed = eval_json_or_string(&tab.page, &js).await?;
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_console = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

    async fn cmd_errors(&mut self, args: &ClearFlagArgs) -> Result<Response, EngineError> {
        let clear = args.clear;
        let tab = self.require_tab_mut()?;
        ensure_console_capture(tab).await?;

        let parsed = eval_json_or_string(&tab.page, "JSON.stringify(__eoka_errors || [])").await?;
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_errors = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

    async fn cmd_spa_info(&mut self) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        let info = crate::spa::detect_router(&tab.page)
            .await
            .map_err(|e: eoka::Error| e.to_string())?;
        Ok(Response::ok(json!({
            "router_type": format!("{:?}", info.router_type),
            "current_path": info.current_path,
            "can_navigate": info.can_navigate,
        })))
    }

    async fn cmd_spa_navigate(&mut self, args: &PathStringArgs) -> Result<Response, EngineError> {
        let path = args.path.as_str();
        let drain = self.start_fetch_drain();
        let result = async {
            let tab = self.require_tab_mut()?;
            let info = crate::spa::detect_router(&tab.page)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            crate::spa::spa_navigate(&tab.page, &info.router_type, path)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            let _ = wait_for_stable(&tab.page).await;
            tab.invalidate();
            tab.page.url().await.map_err(|e| e.to_string())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let url = result?;
        Ok(Response::ok_text(format!("SPA navigated to: {}", url)))
    }

    async fn cmd_fake_camera(&mut self, args: &FakeCameraArgs) -> Result<Response, EngineError> {
        let file_path = args.file.as_str();
        let loop_video = args.loop_video;

        let data = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read video file '{}': {}", file_path, e))?;

        let mime = if file_path.ends_with(".webm") {
            "video/webm"
        } else {
            "video/mp4"
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        let template = include_str!("js/fake_camera.js");
        let js = template
            .replace("/*VIDEO_DATA*/", &b64)
            .replace("/*MIME_TYPE*/", mime)
            .replace("/*LOOP*/", if loop_video { "true" } else { "false" });

        self.ensure_browser().await?;
        let tab = self.require_tab_mut()?;

        tab.page
            .session()
            .send::<_, Value>(
                "Page.addScriptToEvaluateOnNewDocument",
                &json!({ "source": js }),
            )
            .await?;
        let _: String = tab.page.evaluate_sync(&js).await.unwrap_or_default();

        let _ = tab
            .page
            .session()
            .send::<_, Value>(
                "Browser.grantPermissions",
                &json!({ "permissions": ["videoCapture", "audioCapture"] }),
            )
            .await;

        Ok(Response::ok_text(format!(
            "Fake camera injected ({}KB {}, loop={})",
            data.len() / 1024,
            mime,
            loop_video
        )))
    }

    async fn cmd_close(&mut self) -> Result<Response, EngineError> {
        self.downloads.stop().await;
        if let Some(recorder) = self.network.take() {
            let session = self
                .state
                .as_ref()
                .and_then(|state| state.current_tab())
                .map(|tab| tab.page.session().clone());
            let _ = recorder.stop(session).await;
        }
        self.disable_all_fetch_sessions().await;
        if let Err(error) = self.recordings.shutdown().await {
            eprintln!("[eoka] recording cleanup failed: {}", error.message);
        }
        if let Some(state) = self.state.take() {
            crate::capture::quiet().await;
            state.close().await?;
        }
        Ok(Response::ok_text("Browser closed"))
    }
}
async fn eval_json_or_string(page: &eoka::Page, js: &str) -> Result<Value, String> {
    let result: String = page.evaluate_sync(js).await.map_err(|e| e.to_string())?;
    Ok(serde_json::from_str(&result).unwrap_or(Value::String(result)))
}
async fn tab_summary(tab: &TabState) -> Result<(String, String), String> {
    let url = tab.page.url().await.map_err(|e| e.to_string())?;
    let title = title_nonblocking(&tab.page).await;
    Ok((url, title))
}
