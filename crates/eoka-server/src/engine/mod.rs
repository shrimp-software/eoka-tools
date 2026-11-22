mod agent;
pub mod blockers;
mod captcha;
mod commands;
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
use eoka_protocol::{Request, Response, SessionHello, SessionMode};
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

fn typed(value: impl serde::Serialize) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => Response::ok(value),
        Err(error) => Response::err(error.to_string()),
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

    pub fn hello(&self) -> SessionHello {
        let mode = match self.spec {
            LaunchSpec::Launch { headless: true, .. } => SessionMode::Headless,
            LaunchSpec::Launch {
                headless: false, ..
            } => SessionMode::Headed,
            LaunchSpec::Connect { .. } => SessionMode::Connect,
            LaunchSpec::Reuse => SessionMode::Reuse,
        };
        SessionHello {
            protocol: eoka_protocol::sidecar::PROTOCOL_VERSION,
            server_version: host_version().to_owned(),
            session: self.session_name.clone(),
            mode,
            configuration_fingerprint: self.spec.configuration_fingerprint(),
            command_limit_ms: None,
            activity: None,
        }
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
            return typed(self.hello());
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
            Request::ActionBatch(batch) => self.action_batch(batch).await,
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
            | Request::ActionBatch(_)
            | Request::Back(_)
            | Request::Forward(_)
            | Request::HistoryGo(_)
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
