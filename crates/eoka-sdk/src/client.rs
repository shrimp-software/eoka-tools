use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::net::UnixStream;

use crate::launch_spec::LaunchSpec;
use crate::session;
use eoka_protocol::{
    read_msg, write_msg, CaptchaDatadomeArgs, CaptchaInjectArgs, CaptchaTurnstileArgs,
    ClearFlagArgs, CloneFromArgs, ConsoleArgs, DeleteCookieArgs, DomainArgs, EmulateArgs,
    FakeCameraArgs, FetchArgs, FillArgs, FrameEvalArgs, HeadersArgs, IdArgs, InterceptAddArgs,
    KeyArgs, LoadStateArgs, ModeArgs, MouseButtonArgs, MouseMoveArgs, NetworkExportArgs,
    NetworkLogArgs, NetworkRecordStartArgs, NetworkShowArgs, NetworkWaitArgs, ObserveArgs,
    OpenArgs, PathArgs, PathStringArgs, Request, Response, ScreenshotArgs, ScriptArgs, SelectArgs,
    SetCookieArgs, SetStorageArgs, SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs, TargetArgs,
    TextArgs, WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
};

macro_rules! forward_requests {
    ($(fn $name:ident($($arg:ident: $ty:ty)?) => $variant:ident;)*) => {
        $(
            pub async fn $name(&self $(, $arg: $ty)?) -> anyhow::Result<Response> {
                self.call(Request::$variant $(($arg))?).await
            }
        )*
    };
}

#[derive(Debug, Clone)]
pub struct EokaClient {
    session_name: String,
    spec: LaunchSpec,
}

impl EokaClient {
    pub fn new(session_name: impl Into<String>, spec: LaunchSpec) -> Self {
        Self {
            session_name: session_name.into(),
            spec,
        }
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    pub fn launch_spec(&self) -> &LaunchSpec {
        &self.spec
    }

    pub async fn call(&self, request: Request) -> anyhow::Result<Response> {
        send_command(&self.session_name, request, self.spec.clone()).await
    }

    pub async fn agent_observe(
        &self,
        args: eoka_protocol::AgentObserveArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::AgentObserve(args)).await
    }
    pub async fn operation(&self, id: impl Into<String>) -> anyhow::Result<Response> {
        self.call(Request::OperationGet(eoka_protocol::OperationIdArgs {
            id: id.into(),
        }))
        .await
    }
    pub async fn wait_operation(
        &self,
        id: impl Into<String>,
        timeout_ms: u64,
    ) -> anyhow::Result<Response> {
        self.call(Request::OperationWait(eoka_protocol::OperationWaitArgs {
            id: id.into(),
            timeout_ms,
        }))
        .await
    }
    pub async fn cancel_operation(&self, id: impl Into<String>) -> anyhow::Result<Response> {
        self.call(Request::OperationCancel(eoka_protocol::OperationIdArgs {
            id: id.into(),
        }))
        .await
    }

    pub async fn captcha_inject(&self, args: CaptchaInjectArgs) -> anyhow::Result<Response> {
        self.call(Request::CaptchaInject(args)).await
    }

    forward_requests! {
        fn perform(request: eoka_protocol::ActionPerformArgs) => ActionPerform;
        fn agent_step(args: eoka_protocol::AgentStepArgs) => AgentStep;
        fn open(args: OpenArgs) => Open;
        fn back() => Back;
        fn forward() => Forward;
        fn reload() => Reload;
        fn snapshot(args: SnapshotArgs) => Snapshot;
        fn observe(args: ObserveArgs) => Observe;
        fn screenshot(args: ScreenshotArgs) => Screenshot;
        fn emulate(args: EmulateArgs) => Emulate;
        fn info() => Info;
        fn frames() => Frames;
        fn frame_eval(args: FrameEvalArgs) => FrameEval;
        fn text() => Text;
        fn find(args: TextArgs) => Find;
        fn click(args: TargetArgs) => Click;
        fn double_click(args: TargetArgs) => DblClick;
        fn fill(args: FillArgs) => Fill;
        fn select(args: SelectArgs) => Select;
        fn hover(args: TargetArgs) => Hover;
        fn key(args: KeyArgs) => Key;
        fn mouse_down(args: MouseButtonArgs) => MouseDown;
        fn mouse_move(args: MouseMoveArgs) => MouseMove;
        fn mouse_up(args: MouseButtonArgs) => MouseUp;
        fn key_down(args: KeyArgs) => KeyDown;
        fn key_up(args: KeyArgs) => KeyUp;
        fn release_all_inputs() => ReleaseAllInputs;
        fn scroll(args: TargetArgs) => Scroll;
        fn eval(args: ScriptArgs) => Eval;
        fn exec(args: ScriptArgs) => Exec;
        fn fetch(args: FetchArgs) => Fetch;
        fn download_start(args: eoka_protocol::DownloadStartArgs) => DownloadStart;
        fn download_list() => DownloadList;
        fn download_get(args: eoka_protocol::DownloadIdArgs) => DownloadGet;
        fn download_wait(args: eoka_protocol::DownloadWaitArgs) => DownloadWait;
        fn download_cancel(args: eoka_protocol::DownloadIdArgs) => DownloadCancel;
        fn cookies() => Cookies;
        fn set_cookie(args: SetCookieArgs) => SetCookie;
        fn delete_cookie(args: DeleteCookieArgs) => DeleteCookie;
        fn clear_cookies() => ClearCookies;
        fn storage(args: StorageArgs) => Storage;
        fn set_storage(args: SetStorageArgs) => SetStorage;
        fn dump_storage() => DumpStorage;
        fn save_state(args: PathArgs) => SaveState;
        fn load_state(args: LoadStateArgs) => LoadState;
        fn headers(args: HeadersArgs) => Headers;
        fn console(args: ConsoleArgs) => Console;
        fn errors(args: ClearFlagArgs) => Errors;
        fn tab_list() => TabList;
        fn tab_new(args: TabNewArgs) => TabNew;
        fn tab_switch(args: TabIdArgs) => TabSwitch;
        fn tab_close(args: TabIdArgs) => TabClose;
        fn tab_attach(args: TabIdArgs) => TabAttach;
        fn clone_from(args: CloneFromArgs) => CloneFrom;
        fn wait(args: WaitArgs) => Wait;
        fn spa_info() => SpaInfo;
        fn spa_navigate(args: PathStringArgs) => SpaNavigate;
        fn fake_camera(args: FakeCameraArgs) => FakeCamera;
        fn wasm_info() => WasmInfo;
        fn wasm_read(args: WasmReadArgs) => WasmRead;
        fn wasm_write(args: WasmWriteArgs) => WasmWrite;
        fn wasm_find(args: WasmFindArgs) => WasmFind;
        fn intercept_add(args: InterceptAddArgs) => InterceptAdd;
        fn intercept_list() => InterceptList;
        fn intercept_remove(args: IdArgs) => InterceptRemove;
        fn intercept_log(args: ClearFlagArgs) => InterceptLog;
        fn network_record_start(args: NetworkRecordStartArgs) => NetworkRecordStart;
        fn network_record_stop() => NetworkRecordStop;
        fn network_record_status() => NetworkRecordStatus;
        fn network_log(args: NetworkLogArgs) => NetworkLog;
        fn network_show(args: NetworkShowArgs) => NetworkShow;
        fn network_wait(args: NetworkWaitArgs) => NetworkWait;
        fn network_export(args: NetworkExportArgs) => NetworkExport;
        fn network_clear() => NetworkClear;
        fn js_mode(args: ModeArgs) => JsMode;
        fn js_allow(args: DomainArgs) => JsAllow;
        fn js_block(args: DomainArgs) => JsBlock;
        fn js_remove(args: DomainArgs) => JsRemove;
        fn js_list() => JsList;
        fn close() => Close;
        fn shutdown() => Shutdown;
        fn captcha_datadome(args: CaptchaDatadomeArgs) => CaptchaDatadome;
        fn captcha_turnstile(args: CaptchaTurnstileArgs) => CaptchaTurnstile;
    }
}
pub async fn send_command(
    session_name: &str,
    mut request: Request,
    spec: LaunchSpec,
) -> anyhow::Result<Response> {
    let mut reply_limit = None;
    if !matches!(request, Request::SessionHello) {
        if let Ok(stream) = connect_session(session_name).await {
            let hello = match exchange(stream, &hello_wire(), HELLO_LIMIT).await {
                Ok(response) if response.ok => response.data.unwrap_or_default(),
                Ok(_) => return Ok(non_retryable_error("protocol_mismatch", "Daemon compatibility could not be verified. Upgrade explicitly; no browser was restarted and no action was sent.")),
                Err(_) => return Ok(silent_daemon(session_name)),
            };
            reply_limit = hello["command_limit_ms"].as_u64();
            if hello["protocol_major"] != eoka_protocol::sidecar::PROTOCOL_VERSION.major {
                return Ok(non_retryable_error("protocol_mismatch", "Client and daemon protocols differ. Upgrade explicitly; the existing browser was retained."));
            }
            let required_mode = match &spec {
                LaunchSpec::Launch { headless: true, .. } => Some("headless"),
                LaunchSpec::Launch {
                    headless: false, ..
                } => Some("headed"),
                LaunchSpec::Connect { .. } => Some("connect"),
                LaunchSpec::Reuse => None,
            };
            if spec.has_creation_settings()
                && hello["configuration_fingerprint"] != spec.configuration_fingerprint()
            {
                return Ok(non_retryable_error("session_configuration_conflict", "Creation settings differ from the existing session. Omit creation flags to reuse it or choose a new session name."));
            }
            if required_mode.is_some_and(|mode| hello["mode"] != mode) {
                return Ok(non_retryable_error("session_mode_conflict", "Existing session mode conflicts with the explicit launch mode. Omit launch flags to reuse it or choose a different session name."));
            }
        }
    }
    if let Request::Open(args) = &mut request {
        args.operation
            .operation_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string());
    }
    if let Request::DownloadStart(args) = &mut request {
        args.operation_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string());
    }
    if let Request::AgentStep(args) = &mut request {
        args.operation_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string());
    }
    if request.is_convenience_action()
        || matches!(
            request,
            Request::AgentStep(_)
                | Request::AgentObserve(_)
                | Request::ActionPerform(_)
                | Request::ActionCommand(_)
                | Request::OperationGet(_)
                | Request::OperationList(_)
                | Request::OperationWait(_)
                | Request::OperationCancel(_)
        )
    {
        return send_operation(session_name, request, &spec, reply_limit).await;
    }
    if matches!(
        request,
        Request::DownloadStart(_)
            | Request::DownloadList
            | Request::DownloadGet(_)
            | Request::DownloadWait(_)
            | Request::DownloadCancel(_)
    ) {
        return Ok(send_download(session_name, &request, &spec, reply_limit).await);
    }
    if let Request::CaptchaDatadome(args) = request {
        return Ok(send_datadome(session_name, args, reply_limit).await);
    }
    if matches!(request, Request::Frames | Request::FrameEval(_)) {
        return Ok(send_existing_session(session_name, &request, "frame_transport_error",
            "This daemon does not support frame operations. Upgrade the daemon explicitly; the existing browser was not restarted.", reply_limit).await);
    }
    send_command_once(session_name, request, &spec, reply_limit).await
}

#[cfg(not(test))]
const HELLO_LIMIT: Duration = Duration::from_secs(5);
#[cfg(test)]
const HELLO_LIMIT: Duration = Duration::from_millis(1000);
const STATUS_LIMIT: Duration = Duration::from_millis(1500);
#[cfg(not(test))]
const REPLY_MARGIN_MS: u64 = 15_000;
#[cfg(test)]
const REPLY_MARGIN_MS: u64 = 200;
const LAUNCH_MARGIN_MS: u64 = 30_000;
const UNKNOWN_COMMAND_LIMIT_MS: u64 = 90_000;

enum Lost {
    Timeout(Duration),
    Disconnected,
}

fn hello_wire() -> serde_json::Value {
    serde_json::json!({"cmd":"session_hello"})
}

fn request_wire(request: &Request) -> anyhow::Result<serde_json::Value> {
    let mut wire = serde_json::to_value(request)?;
    wire["protocol_major"] = serde_json::json!(eoka_protocol::sidecar::PROTOCOL_VERSION.major);
    Ok(wire)
}

async fn exchange(
    stream: UnixStream,
    wire: &serde_json::Value,
    limit: Duration,
) -> Result<Response, Lost> {
    let (mut reader, mut writer) = stream.into_split();
    let reply = async {
        write_msg(&mut writer, wire).await?;
        Ok::<Response, anyhow::Error>(read_msg(&mut reader).await?)
    };
    match tokio::time::timeout(limit, reply).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) => Err(Lost::Disconnected),
        Err(_) => Err(Lost::Timeout(limit)),
    }
}

fn configured_reply_ms() -> Option<u64> {
    std::env::var("EOKA_REPLY_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|limit| *limit > 0)
}

fn reply_budget(request: &Request, command_limit_ms: Option<u64>) -> Duration {
    let args = request.args_json();
    let number = |value: &serde_json::Value| value.as_u64();
    let own = match request {
        Request::SessionHello => return HELLO_LIMIT,
        Request::AgentStep(_) | Request::AgentObserve(_) => {
            return Duration::from_millis(
                number(&args["timeout_ms"]).unwrap_or(10_000).min(60_000) + 1_000,
            )
        }
        Request::Wait(_) => number(&args["ms"])
            .max(number(&args["timeout"]))
            .or(Some(10_000)),
        Request::NetworkWait(_) => number(&args["timeout"]).or(Some(10_000)),
        Request::OperationWait(_) | Request::DownloadWait(_) => {
            number(&args["timeout_ms"]).or(Some(30_000))
        }
        Request::CaptchaDatadome(_) => number(&args["timeout_ms"])
            .map(|limit| limit.saturating_mul(number(&args["max_attempts"]).unwrap_or(1).max(1))),
        Request::CaptchaTurnstile(_) => number(&args["timeout_ms"]),
        Request::Open(_) => Some(
            number(&args["timeout_ms"])
                .unwrap_or(30_000)
                .saturating_add(LAUNCH_MARGIN_MS),
        ),
        Request::TabAttach(_) | Request::TabNew(_) => Some(30_000 + LAUNCH_MARGIN_MS),
        Request::ActionCommand(command) => Some(
            number(&command.arguments["timeout_ms"])
                .max(number(&command.arguments["start_timeout_ms"]))
                .unwrap_or(30_000),
        ),
        Request::ActionPerform(_) => Some(number(&args["options"]["timeout_ms"]).unwrap_or(30_000)),
        Request::Click(_)
        | Request::DblClick(_)
        | Request::Fill(_)
        | Request::Select(_)
        | Request::Hover(_)
        | Request::Key(_)
        | Request::Back
        | Request::Forward
        | Request::Reload => Some(number(&args["timeout_ms"]).unwrap_or(30_000)),
        Request::DownloadStart(_) => number(&args["start_timeout_ms"]).or(Some(30_000)),
        _ => None,
    };
    let limit = own
        .or_else(configured_reply_ms)
        .or(command_limit_ms)
        .unwrap_or(UNKNOWN_COMMAND_LIMIT_MS);
    Duration::from_millis(limit.saturating_add(REPLY_MARGIN_MS))
}

fn log_path(session_name: &str) -> String {
    session::socket_path(session_name)
        .with_extension("log")
        .to_string_lossy()
        .into_owned()
}

fn session_flag(session_name: &str) -> String {
    if session_name == "default" {
        String::new()
    } else {
        format!("--session {session_name} ")
    }
}

fn silent_daemon(session_name: &str) -> Response {
    let flag = session_flag(session_name);
    Response::err_detail(
        eoka_protocol::ErrorDetail::new(
            "daemon_unresponsive",
            format!(
                "Session `{session_name}` accepted the connection but did not answer within {:.1} s; its daemon is suspended, overloaded or shutting down. No command was sent.",
                HELLO_LIMIT.as_secs_f64()
            ),
        )
        .retryable(true)
        .hint(format!(
            "Check it with `eoka {flag}status`; the daemon log is {}. If it stays silent, `eoka {flag}kill` force-stops the daemon and closes its browser, losing page state.",
            log_path(session_name)
        ))
        .context(eoka_protocol::ErrorContext {
            action_status: Some(eoka_protocol::ActionStatus::NotAttempted),
            ..Default::default()
        }),
    )
}

pub async fn session_status(session_name: &str) -> Option<serde_json::Value> {
    let stream = connect_session(session_name).await.ok()?;
    exchange(stream, &hello_wire(), STATUS_LIMIT)
        .await
        .ok()
        .filter(|response| response.ok)
        .and_then(|response| response.data)
}

async fn daemon_activity(session_name: &str) -> (String, String) {
    let flag = session_flag(session_name);
    let status = format!("`eoka {flag}status` shows what the session is doing");
    let last_resort = format!(
        "As a last resort `eoka {flag}kill` force-stops the daemon and closes its browser, losing page state; the daemon log is {}.",
        log_path(session_name)
    );
    let Some(hello) = session_status(session_name).await else {
        return (
            "The daemon does not answer status probes either, so its process is suspended or overloaded.".into(),
            last_resort,
        );
    };
    let activity = &hello["activity"];
    let blocker = activity["blocker"]["hint"].as_str();
    let state = match activity["busy"]["cmd"].as_str() {
        Some(command) => format!(
            "The daemon is still running `{command}` ({:.1} s so far).",
            activity["busy"]["for_ms"].as_u64().unwrap_or_default() as f64 / 1000.0
        ),
        None if activity.is_object() => {
            "The daemon is idle again, so the reply was lost after the command ended.".into()
        }
        None => {
            "The daemon still answers status probes but does not report what it is running.".into()
        }
    };
    let hint = match blocker {
        Some(blocker) => format!("{blocker} {status}."),
        None => format!("{status}. {last_resort}"),
    };
    (state, hint)
}

async fn unanswered(session_name: &str, request: &Request, lost: Lost) -> Response {
    let command = request.cmd();
    let read_only = eoka_protocol::operation_by_cmd(command).is_some_and(|spec| spec.read_only);
    let (state, hint) = daemon_activity(session_name).await;
    let (code, cause) = match lost {
        Lost::Timeout(limit) => (
            "reply_timeout",
            format!(
                "No reply to `{command}` from session `{session_name}` within {:.1} s.",
                limit.as_secs_f64()
            ),
        ),
        Lost::Disconnected => (
            "transport_error",
            format!("Session `{session_name}` closed the connection before answering `{command}`."),
        ),
    };
    let consequence = if read_only {
        "Nothing was sent again."
    } else {
        "The command may have run; it was not sent again."
    };
    let detail = eoka_protocol::ErrorDetail::new(code, format!("{cause} {state} {consequence}"))
        .retryable(read_only)
        .hint(hint);
    Response::err_detail(if read_only {
        detail
    } else {
        detail.context(eoka_protocol::ErrorContext {
            action_status: Some(eoka_protocol::ActionStatus::MayHaveExecuted),
            ..Default::default()
        })
    })
}

fn no_session(session_name: &str) -> Response {
    let flag = session_flag(session_name);
    Response::err_detail(
        eoka_protocol::ErrorDetail::new(
            "session_unavailable",
            format!("No session named `{session_name}` is running; nothing was sent."),
        )
        .retryable(false)
        .hint(format!(
            "Start it with `eoka {flag}open URL`. `eoka sessions` lists the sessions that are running."
        )),
    )
}

fn non_retryable_error(code: &str, message: &str) -> Response {
    Response::err_detail(eoka_protocol::ErrorDetail::new(code, message).retryable(false))
}

async fn send_download(
    session_name: &str,
    request: &Request,
    spec: &LaunchSpec,
    reply_limit: Option<u64>,
) -> Response {
    use eoka_protocol::{ActionStatus, ErrorContext};
    let id = match request {
        Request::DownloadStart(args) => args.operation_id.as_deref(),
        Request::DownloadWait(args) => Some(args.id.as_str()),
        Request::DownloadGet(args) | Request::DownloadCancel(args) => Some(args.id.as_str()),
        _ => None,
    };
    let disconnected = |code, message, action_status| {
        Response::err_detail(
            eoka_protocol::ErrorDetail::new(code, message)
                .retryable(false)
                .context(ErrorContext {
                    download_id: id.map(str::to_owned),
                    action_status: Some(action_status),
                    ..Default::default()
                }),
        )
    };
    let stream = match UnixStream::connect(session::socket_path(session_name)).await {
        Ok(stream) => stream,
        Err(_) if !matches!(request, Request::DownloadStart(_)) => {
            match async {
                launch_daemon(session_name, spec)?;
                wait_for_socket(session_name).await
            }
            .await
            {
                Ok(stream) => stream,
                Err(_) => {
                    return disconnected(
                        "session_unavailable",
                        "Could not open the download history service",
                        ActionStatus::NotAttempted,
                    )
                }
            }
        }
        Err(_) => return disconnected(
            "session_unavailable",
            "Existing session unavailable; open the intended page first. No daemon was launched.",
            ActionStatus::NotAttempted,
        ),
    };
    let wire = if let Request::DownloadStart(args) = request {
        Request::ActionCommand(eoka_protocol::ActionCommandArgs {
            operation_id: args.operation_id.clone().unwrap_or_default(),
            command: "download_start".into(),
            arguments: request.args_json(),
        })
    } else {
        request.clone()
    };
    let exchanged = match request_wire(&wire) {
        Ok(message) => exchange(stream, &message, reply_budget(request, reply_limit))
            .await
            .ok(),
        Err(_) => None,
    };
    let mut response = match exchanged {
        Some(response) if response.error.as_deref().is_some_and(|message| {
            message.contains("unknown variant") || message.contains("unknown eoka protocol command")
        }) => disconnected(
            "unsupported_operation",
            "This daemon does not support downloads. Restart it explicitly after upgrading; no action was replayed.",
            ActionStatus::NotAttempted,
        ),
        Some(response) => response,
        None => disconnected(
            "download_transport_error",
            "Session transport failed; the action may have executed. Inspect download list before retrying. No restart or replay was attempted.",
            ActionStatus::MayHaveExecuted,
        ),
    };
    if let (Some(error), Some(id)) = (response.error_detail.as_mut(), id) {
        error
            .context
            .get_or_insert_with(Default::default)
            .download_id
            .get_or_insert_with(|| id.to_owned());
    }
    response
}

async fn send_datadome(
    session_name: &str,
    args: CaptchaDatadomeArgs,
    reply_limit: Option<u64>,
) -> Response {
    if let Err(message) = args.validate() {
        return non_retryable_error("invalid_input", message);
    }
    send_existing_session(session_name, &Request::CaptchaDatadome(args), "datadome_transport_error",
        "This daemon does not support DataDome. Upgrade the daemon explicitly; the existing browser was not restarted.", reply_limit).await
}

async fn send_existing_session(
    session_name: &str,
    request: &Request,
    transport_error: &str,
    unsupported_message: &str,
    reply_limit: Option<u64>,
) -> Response {
    let Ok(stream) = connect_session(session_name).await else {
        return non_retryable_error(
            "session_unavailable",
            "Existing session unavailable; open the intended page first. No browser was launched.",
        );
    };
    let exchanged = match request_wire(request) {
        Ok(wire) => exchange(stream, &wire, reply_budget(request, reply_limit))
            .await
            .ok(),
        Err(_) => None,
    };
    match exchanged {
        Some(response) if response.error.as_deref().is_some_and(|error| error.contains(request.cmd()) && (error.contains("unknown variant") || error.contains("unknown eoka protocol command"))) =>
            non_retryable_error("unsupported_operation", unsupported_message),
        Some(response) => response,
        None => non_retryable_error(transport_error, "Session transport failed; the request may have executed. Inspect the existing page before retrying. No replay was attempted."),
    }
}

async fn send_command_once(
    session_name: &str,
    request: Request,
    spec: &LaunchSpec,
    reply_limit: Option<u64>,
) -> anyhow::Result<Response> {
    let stream = match connect_session(session_name).await {
        Ok(s) => s,
        Err(_)
            if matches!(request, Request::Open(_))
                || (matches!(request, Request::TabAttach(_))
                    && matches!(spec, LaunchSpec::Connect { .. })) =>
        {
            let legacy = [
                format!("{session_name}-headed"),
                format!("{session_name}-live"),
            ];
            if legacy.iter().any(|name| is_daemon_running(name)) {
                return Ok(non_retryable_error("legacy_session_present", "A legacy suffixed session exists. Select it explicitly or migrate it; no replacement browser was created."));
            }
            launch_daemon(session_name, spec)?;
            wait_for_socket(session_name).await?
        }
        Err(_) => return Ok(no_session(session_name)),
    };

    let wire = request_wire(&request)?;
    Ok(
        match exchange(stream, &wire, reply_budget(&request, reply_limit)).await {
            Ok(response) => response,
            Err(lost) => unanswered(session_name, &request, lost).await,
        },
    )
}

async fn connect_session(session_name: &str) -> std::io::Result<UnixStream> {
    UnixStream::connect(session::trusted_socket_path(session_name)?).await
}

async fn wait_for_socket(session_name: &str) -> anyhow::Result<UnixStream> {
    let sock = session::socket_path(session_name);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() > deadline {
            anyhow::bail!(
                "Daemon failed to start (socket {} not found after 5s)",
                sock.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(s) = connect_session(session_name).await {
            return Ok(s);
        }
    }
}
static DAEMON_PROGRAM: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn serve_daemons_from(program: std::path::PathBuf) {
    let _ = DAEMON_PROGRAM.set(program);
}

fn daemon_program(
    host: Option<&std::path::PathBuf>,
    configured: Option<std::ffi::OsString>,
) -> std::path::PathBuf {
    host.cloned()
        .or_else(|| {
            configured
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
        })
        .unwrap_or_else(|| "eoka".into())
}

fn launch_daemon(session_name: &str, spec: &LaunchSpec) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let program = daemon_program(DAEMON_PROGRAM.get(), std::env::var_os("EOKA_BIN"));
    let mut cmd = StdCommand::new(&program);
    cmd.arg("--daemon").arg("--session").arg(session_name);
    let mut proxy_for_stdin = None;

    match spec {
        LaunchSpec::Reuse => {}
        LaunchSpec::Launch {
            headless,
            from_profile,
            clone_state_from,
            no_stealth,
            proxy,
            no_js,
            js_allow,
            js_block,
            persist,
            geo_align,
            auto_solve_cloudflare,
            sandbox,
        } => {
            if !*headless {
                cmd.arg("--headed");
            }
            if let Some(p) = from_profile {
                cmd.arg("--from-profile").arg(p);
            }
            if let Some(s) = clone_state_from {
                cmd.arg("--clone-state-from").arg(s);
            }
            if *no_stealth {
                cmd.arg("--no-stealth");
            }
            if *persist {
                cmd.arg("--persist");
            }
            if !*geo_align {
                cmd.arg("--no-geo-align");
            }
            if *auto_solve_cloudflare {
                cmd.arg("--auto-solve-cloudflare");
            }
            if !*sandbox {
                cmd.arg("--no-sandbox");
            }
            if let Some(p) = proxy {
                cmd.arg("--proxy-stdin");
                proxy_for_stdin = Some(p.clone());
            }
            if *no_js {
                cmd.arg("--no-js");
            }
            for domain in js_allow {
                cmd.arg("--js-allow").arg(domain);
            }
            for domain in js_block {
                cmd.arg("--js-block").arg(domain);
            }
        }
        LaunchSpec::Connect { ws_url } => {
            cmd.arg("--cdp").arg(ws_url);
        }
    }

    let (log_file, log_path) = session::open_daemon_log(session_name)?;

    cmd.stdin(if proxy_for_stdin.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    })
    .stdout(std::process::Stdio::null())
    .stderr(log_file);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|error| {
        anyhow::anyhow!(
            "Could not start the Eoka daemon with `{}`: {error}. Install the eoka CLI or set EOKA_BIN to its path.",
            program.display()
        )
    })?;
    if let Some(proxy) = proxy_for_stdin {
        use std::io::Write;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Daemon stdin was not captured"))?;
        stdin.write_all(format!("{proxy}\n").as_bytes())?;
    }
    eprintln!(
        "[eoka] daemon starting (session={}, log={})",
        session_name,
        log_path.display()
    );
    Ok(())
}
pub fn is_daemon_running(session_name: &str) -> bool {
    session::trusted_socket_path(session_name)
        .is_ok_and(|sock| std::os::unix::net::UnixStream::connect(sock).is_ok())
}
pub fn kill_daemon(session_name: &str) -> anyhow::Result<bool> {
    let sock = session::trusted_socket_path(session_name)?;
    let daemon_is_listening = std::os::unix::net::UnixStream::connect(&sock).is_ok();
    let pid_path = session::pid_path(session_name);
    let original_pid = std::fs::read_to_string(&pid_path).ok();
    let killed = if let Some(pid_str) = original_pid.as_ref().filter(|_| daemon_is_listening) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            StdCommand::new("kill")
                .arg(pid.to_string())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    };
    if killed {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::fs::read_to_string(&pid_path).ok() == original_pid {
            if let Some(pid) = original_pid.as_deref() {
                let alive = StdCommand::new("kill")
                    .args(["-0", pid.trim()])
                    .output()?
                    .status
                    .success();
                if !alive {
                    break;
                }
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!("Daemon is still shutting down; its socket was retained");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if let Some(pid) = original_pid.as_deref() {
            let exited = std::time::Instant::now() + Duration::from_secs(5);
            while std::time::Instant::now() < exited
                && StdCommand::new("kill")
                    .args(["-0", pid.trim()])
                    .output()
                    .is_ok_and(|probe| probe.status.success())
            {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    if std::fs::read_to_string(&pid_path).ok() == original_pid {
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(&pid_path);
    }
    Ok(killed)
}

async fn send_operation(
    session_name: &str,
    request: Request,
    spec: &LaunchSpec,
    reply_limit: Option<u64>,
) -> anyhow::Result<Response> {
    let args = request.args_json();
    let id = args["operation_id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut wire = serde_json::to_value(&request)?;
    if request.is_convenience_action() {
        wire = serde_json::json!({"cmd":"action_command","args":{"operation_id":id,"command":request.cmd(),"arguments":args}});
    }
    if !matches!(request, Request::AgentObserve(_)) {
        wire["operation_id"] = serde_json::json!(id);
    }
    wire["protocol_major"] = serde_json::json!(eoka_protocol::sidecar::PROTOCOL_VERSION.major);
    let stream = match connect_session(session_name).await {
        Ok(stream) => stream,
        Err(_) if request.cmd().starts_with("operation_") => {
            launch_daemon(session_name, spec)?;
            wait_for_socket(session_name).await?
        }
        Err(_) if matches!(request, Request::AgentObserve(_)) => {
            return Ok(no_session(session_name))
        }
        Err(_) => {
            let mut response = no_session(session_name);
            response.data = Some(serde_json::json!({"operation_id":id,"status":"not_dispatched"}));
            return Ok(response);
        }
    };
    let lost = match exchange(stream, &wire, reply_budget(&request, reply_limit)).await {
        Ok(response) => return Ok(response),
        Err(lost) => lost,
    };
    let (state, hint) = daemon_activity(session_name).await;
    let cause = match lost {
        Lost::Timeout(limit) => format!("No reply within {:.1} s", limit.as_secs_f64()),
        Lost::Disconnected => "Connection lost".to_owned(),
    };
    if matches!(request, Request::AgentObserve(_)) {
        return Ok(Response::err_detail(
            eoka_protocol::ErrorDetail::new(
                "observation_transport_error",
                format!(
                    "{cause}: the observation is unavailable. {state} No action was requested."
                ),
            )
            .retryable(false)
            .hint(hint),
        ));
    }
    let mut response = Response::err_detail(eoka_protocol::ErrorDetail::new("operation_transport_error", format!("{cause}; inspect operation {id}. Its outcome is unknown. {state} No action was replayed.")).retryable(false).hint(hint).context(eoka_protocol::ErrorContext { operation_id: Some(id.clone()), phase: Some("transport".into()), action_status: Some(eoka_protocol::ActionStatus::MayHaveExecuted), ..Default::default() }));
    response.data = Some(serde_json::json!({"operation_id":id,"status":"outcome_unknown"}));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    async fn handshake(listener: &UnixListener) {
        handshake_with(listener, serde_json::json!({})).await;
    }

    async fn handshake_with(listener: &UnixListener, extra: serde_json::Value) {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        let request: Request = read_msg(&mut reader).await.unwrap();
        assert!(matches!(request, Request::SessionHello));
        let mut hello = serde_json::json!({"protocol_major":2,"mode":"headless","configuration_fingerprint":test_client("fixture").spec.configuration_fingerprint()});
        for (key, value) in extra.as_object().into_iter().flatten() {
            hello[key] = value.clone();
        }
        write_msg(&mut writer, &Response::ok(hello)).await.unwrap();
    }

    fn fake_daemon(prefix: &str) -> (String, std::path::PathBuf, UnixListener) {
        let session = format!("{prefix}-{}", uuid::Uuid::new_v4());
        session::ensure_runtime_dir().unwrap();
        let socket = session::socket_path(&session);
        let listener = UnixListener::bind(&socket).unwrap();
        (session, socket, listener)
    }

    #[tokio::test]
    async fn a_daemon_that_accepts_but_stays_silent_is_reported_as_unresponsive() {
        let (session, socket, listener) = fake_daemon("silent-daemon");
        let server = tokio::spawn(async move {
            let (held, _) = listener.accept().await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(1500), listener.accept())
                    .await
                    .is_err(),
                "nothing may be sent after an unanswered handshake"
            );
            drop(held);
        });
        let started = std::time::Instant::now();
        let response = test_client(&session).info().await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "daemon_unresponsive");
        assert!(detail.message.contains("No command was sent"));
        let hint = detail.hint.unwrap();
        assert!(hint.contains(&format!("eoka --session {session} status")));
        assert!(hint.contains(&format!("eoka --session {session} kill")));
        assert_eq!(
            detail.context.unwrap().action_status,
            Some(eoka_protocol::ActionStatus::NotAttempted)
        );
        server.await.unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[tokio::test]
    async fn an_unanswered_read_times_out_with_the_daemon_activity_and_is_not_replayed() {
        let (session, socket, listener) = fake_daemon("reply-timeout");
        let server = tokio::spawn(async move {
            handshake_with(&listener, serde_json::json!({"command_limit_ms":100})).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _held) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(request.cmd(), "info");
            handshake_with(
                &listener,
                serde_json::json!({"activity":{"busy":{"cmd":"info","for_ms":400},"queued":0,"blocker":null}}),
            )
            .await;
            assert!(
                tokio::time::timeout(Duration::from_millis(300), listener.accept())
                    .await
                    .is_err(),
                "an unanswered command must not be sent again"
            );
        });
        let started = std::time::Instant::now();
        let response = test_client(&session).info().await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!response.ok);
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "reply_timeout");
        assert!(
            detail.message.contains("No reply to `info`"),
            "{}",
            detail.message
        );
        assert!(
            detail.message.contains("still running `info`"),
            "{}",
            detail.message
        );
        assert!(detail.message.contains("Nothing was sent again"));
        assert_eq!(detail.retryable, Some(true));
        assert!(detail.context.is_none());
        assert!(detail.hint.unwrap().contains("status"));
        server.await.unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[tokio::test]
    async fn an_unanswered_script_reports_that_it_may_have_run() {
        let (session, socket, listener) = fake_daemon("script-timeout");
        let server = tokio::spawn(async move {
            handshake_with(&listener, serde_json::json!({"command_limit_ms":100})).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, held) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(request.cmd(), "exec");
            let (probe, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(1700)).await;
            drop((probe, held));
        });
        let response = test_client(&session)
            .exec(serde_json::from_value(serde_json::json!({"code":"() => 1"})).unwrap())
            .await
            .unwrap();
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "reply_timeout");
        assert!(detail.message.contains("does not answer status probes"));
        assert!(detail
            .message
            .contains("may have run; it was not sent again"));
        assert_eq!(detail.retryable, Some(false));
        assert_eq!(
            detail.context.unwrap().action_status,
            Some(eoka_protocol::ActionStatus::MayHaveExecuted)
        );
        assert!(detail.hint.unwrap().contains("kill"));
        server.await.unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[tokio::test]
    async fn a_busy_refusal_is_returned_as_is_and_never_replayed() {
        let (session, socket, listener) = fake_daemon("busy-daemon");
        let server = tokio::spawn(async move {
            handshake(&listener).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let _: Request = read_msg(&mut reader).await.unwrap();
            let busy = Response::err_detail(
                eoka_protocol::ErrorDetail::new(
                    "daemon_busy",
                    "Session `work` is busy: `wait` has been running for 9.0 s. `text` was not started.",
                )
                .retryable(true)
                .hint("Run it again once the running command returns."),
            );
            write_msg(&mut writer, &busy).await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(200), listener.accept())
                    .await
                    .is_err(),
                "a refused command must not be retried by the client"
            );
        });
        let response = test_client(&session).text().await.unwrap();
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "daemon_busy");
        assert!(detail.message.contains("`wait` has been running"));
        server.await.unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[tokio::test]
    async fn a_missing_session_names_the_command_that_starts_it() {
        let name = format!("absent-{}", uuid::Uuid::new_v4());
        let client = test_client(&name);
        let read = client.info().await.unwrap();
        let detail = read.error_detail.unwrap();
        assert_eq!(detail.code, "session_unavailable");
        assert!(detail.message.contains(&name));
        assert!(detail
            .hint
            .unwrap()
            .contains(&format!("eoka --session {name} open URL")));
        let observed = client
            .agent_observe(Default::default())
            .await
            .unwrap()
            .error_detail
            .unwrap();
        assert_eq!(observed.code, "session_unavailable");
        let step = client
            .agent_step(
                serde_json::from_value(
                    serde_json::json!({"action":{"kind":"press_key","key":"Tab"}}),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(step.data.unwrap()["status"], "not_dispatched");
        assert!(!session::socket_path(&name).exists());
    }

    #[test]
    fn every_request_has_a_reply_deadline_derived_from_its_own_timeout() {
        let request = |cmd: &str, args: serde_json::Value| {
            eoka_protocol::request_from_cmd(cmd, args).unwrap()
        };
        let budget = |cmd: &str, args: serde_json::Value, limit: Option<u64>| {
            reply_budget(&request(cmd, args), limit).as_millis() as u64
        };
        let none = serde_json::json!({});
        assert_eq!(
            budget("info", none.clone(), Some(30_000)),
            30_000 + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget("info", none.clone(), Some(90_000)),
            90_000 + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget("info", none.clone(), None),
            UNKNOWN_COMMAND_LIMIT_MS + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget(
                "wait",
                serde_json::json!({"text":"Done","timeout":45_000}),
                Some(30_000)
            ),
            45_000 + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget("wait", serde_json::json!({"ms":2_000}), Some(30_000)),
            2_000 + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget(
                "open",
                serde_json::json!({"url":"https://example.test/","timeout_ms":5_000}),
                None
            ),
            5_000 + LAUNCH_MARGIN_MS + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget(
                "agent_observe",
                serde_json::json!({"timeout_ms":3_000}),
                None
            ),
            4_000
        );
        assert_eq!(
            budget(
                "agent_step",
                serde_json::json!({"action":{"kind":"back"},"timeout_ms":120_000}),
                None
            ),
            61_000
        );
        assert_eq!(
            budget(
                "operation_wait",
                serde_json::json!({"id":"x","timeout_ms":70_000}),
                None
            ),
            70_000 + REPLY_MARGIN_MS
        );
        assert_eq!(
            budget("session_hello", none, None),
            HELLO_LIMIT.as_millis() as u64
        );
    }

    #[tokio::test]
    async fn agent_step_allocates_id_before_transport_and_retains_it_on_disconnect() {
        let session = format!("agent-step-{}", uuid::Uuid::new_v4());
        session::ensure_runtime_dir().unwrap();
        let socket = session::socket_path(&session);
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            handshake(&listener).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, _) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            let Request::AgentStep(args) = request else {
                panic!("wrong request")
            };
            args.operation_id.unwrap()
        });
        let request =
            serde_json::from_value(serde_json::json!({"action":{"kind":"press_key","key":"Tab"}}))
                .unwrap();
        let response = test_client(&session).agent_step(request).await.unwrap();
        let id = server.await.unwrap();
        assert!(!response.ok);
        assert!(response.error.unwrap().contains(&id));
        assert_eq!(response.error_detail.unwrap().retryable, Some(false));
        std::fs::remove_file(socket).unwrap();
    }

    #[test]
    fn the_daemon_is_the_eoka_cli_not_whatever_program_uses_the_sdk() {
        let cli = std::path::PathBuf::from("/opt/tools/eoka-renamed");
        assert_eq!(
            daemon_program(Some(&cli), Some("/elsewhere/eoka".into())),
            cli
        );
        assert_eq!(
            daemon_program(None, Some("/elsewhere/eoka".into())),
            std::path::PathBuf::from("/elsewhere/eoka")
        );
        assert_eq!(
            daemon_program(None, Some("".into())),
            std::path::PathBuf::from("eoka")
        );
        assert_eq!(daemon_program(None, None), std::path::PathBuf::from("eoka"));
    }

    #[test]
    fn disabling_the_sandbox_is_a_creation_setting() {
        let spec = |sandbox| LaunchSpec::Launch {
            headless: true,
            from_profile: None,
            clone_state_from: None,
            no_stealth: false,
            proxy: None,
            no_js: false,
            js_allow: Vec::new(),
            js_block: Vec::new(),
            persist: false,
            geo_align: true,
            auto_solve_cloudflare: false,
            sandbox,
        };
        assert!(!spec(true).has_creation_settings());
        assert!(spec(false).has_creation_settings());
        assert_ne!(
            spec(true).configuration_fingerprint(),
            spec(false).configuration_fingerprint()
        );
    }

    fn test_client(session: &str) -> EokaClient {
        EokaClient::new(
            session,
            LaunchSpec::Launch {
                headless: true,
                from_profile: None,
                clone_state_from: None,
                no_stealth: false,
                proxy: None,
                no_js: false,
                js_allow: Vec::new(),
                js_block: Vec::new(),
                persist: false,
                geo_align: false,
                auto_solve_cloudflare: false,
                sandbox: true,
            },
        )
    }

    #[tokio::test]
    async fn typed_mouse_down_helper_sends_protocol_request() {
        let session = format!("eoka-sdk-held-input-test-{}", std::process::id());
        session::ensure_runtime_dir().unwrap();
        let sock = session::socket_path(&session);
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            handshake(&listener).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(request.cmd(), "mouse_down");
            assert_eq!(request.args_json()["x"], 10.0);
            assert_eq!(request.args_json()["y"], 20.0);
            assert_eq!(request.args_json()["button"], "left");
            write_msg(&mut writer, &Response::ok_text("ok"))
                .await
                .unwrap();
        });

        let response = test_client(&session)
            .mouse_down(MouseButtonArgs {
                x: 10.0,
                y: 20.0,
                button: eoka_protocol::MouseButton::Left,
            })
            .await
            .unwrap();

        assert!(response.ok);
        server.await.unwrap();
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn existing_session_requests_never_launch_a_daemon() {
        let name = format!("datadome-missing-{}", std::process::id());
        let response = test_client(&name)
            .captcha_datadome(CaptchaDatadomeArgs::default())
            .await
            .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
        assert!(!session::socket_path(&name).exists());
        assert!(!session::pid_path(&name).exists());
        for request in [
            Request::Frames,
            Request::FrameEval(FrameEvalArgs {
                frame: "id:fixture-frame".into(),
                code: Some("1+1".into()),
                file: None,
            }),
        ] {
            let response = test_client(&name).call(request).await.unwrap();
            assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
            assert!(!session::socket_path(&name).exists());
            assert!(!session::pid_path(&name).exists());
        }
        let invalid = test_client(&name)
            .captcha_datadome(CaptchaDatadomeArgs {
                max_attempts: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(invalid.error_detail.unwrap().code, "invalid_input");
    }

    #[tokio::test]
    async fn existing_session_transport_failure_and_old_daemon_are_not_replayed() {
        for request in [
            Request::CaptchaDatadome(CaptchaDatadomeArgs::default()),
            Request::Frames,
            Request::FrameEval(FrameEvalArgs {
                frame: "id:fixture-frame".into(),
                code: Some("1+1".into()),
                file: None,
            }),
        ] {
            let cmd = request.cmd();
            for (index, error) in [
                None,
                Some("channel is empty and sending half is closed".to_string()),
                Some(format!("Invalid command request: unknown variant {cmd}")),
            ]
            .into_iter()
            .enumerate()
            {
                let name = format!("{cmd}-transport-{}-{index}", std::process::id());
                session::ensure_runtime_dir().unwrap();
                let sock = session::socket_path(&name);
                let listener = UnixListener::bind(&sock).unwrap();
                let server = tokio::spawn(async move {
                    handshake(&listener).await;
                    let (stream, _) = listener.accept().await.unwrap();
                    let (mut reader, mut writer) = stream.into_split();
                    let request: Request = read_msg(&mut reader).await.unwrap();
                    assert_eq!(request.cmd(), cmd);
                    if let Some(message) = error {
                        write_msg(&mut writer, &Response::err(message))
                            .await
                            .unwrap();
                    }
                    drop(reader);
                    drop(writer);
                    assert!(
                        tokio::time::timeout(Duration::from_millis(150), listener.accept())
                            .await
                            .is_err()
                    );
                });
                let client = test_client(&name);
                let response = client.call(request.clone()).await.unwrap();
                assert!(!response.ok);
                if index == 0 {
                    assert_eq!(
                        response.error_detail.as_ref().unwrap().code,
                        if cmd == "captcha_datadome" {
                            "datadome_transport_error"
                        } else {
                            "frame_transport_error"
                        }
                    );
                }
                if index == 2 {
                    assert_eq!(
                        response.error_detail.as_ref().unwrap().code,
                        "unsupported_operation"
                    );
                }
                server.await.unwrap();
                std::fs::remove_file(sock).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn downloads_require_existing_sessions_and_never_replay() {
        let name = format!("download-missing-{}", std::process::id());
        let response = test_client(&name)
            .download_start(eoka_protocol::DownloadStartArgs {
                operation_id: None,
                click: Some("css:#download".into()),
                url: None,
                output: "/tmp/not-created".into(),
                expect_bytes: None,
                start_timeout_ms: 1000,
            })
            .await
            .unwrap();
        assert_eq!(response.error_detail.unwrap().code, "session_unavailable");
        assert!(!session::pid_path(&name).exists());
        for (index, error) in [
            None,
            Some("Invalid command request: unknown variant download_start"),
        ]
        .into_iter()
        .enumerate()
        {
            let name = format!("download-transport-{}-{index}", std::process::id());
            session::ensure_runtime_dir().unwrap();
            let sock = session::socket_path(&name);
            let listener = UnixListener::bind(&sock).unwrap();
            let server = tokio::spawn(async move {
                handshake(&listener).await;
                let (stream, _) = listener.accept().await.unwrap();
                let (mut reader, mut writer) = stream.into_split();
                let request: Request = read_msg(&mut reader).await.unwrap();
                assert!(
                    matches!(request,Request::ActionCommand(ref a) if a.command=="download_start" && !a.operation_id.is_empty())
                );
                if let Some(message) = error {
                    write_msg(&mut writer, &Response::err(message))
                        .await
                        .unwrap();
                }
                drop(reader);
                drop(writer);
                assert!(
                    tokio::time::timeout(Duration::from_millis(150), listener.accept())
                        .await
                        .is_err()
                );
            });
            let response = test_client(&name)
                .download_start(eoka_protocol::DownloadStartArgs {
                    operation_id: None,
                    click: Some("css:#download".into()),
                    url: None,
                    output: "/tmp/not-created".into(),
                    expect_bytes: None,
                    start_timeout_ms: 1000,
                })
                .await
                .unwrap();
            assert_eq!(
                response.error_detail.unwrap().code,
                if error.is_some() {
                    "unsupported_operation"
                } else {
                    "download_transport_error"
                }
            );
            server.await.unwrap();
            std::fs::remove_file(sock).unwrap();
        }
    }

    #[tokio::test]
    async fn typed_open_helper_sends_protocol_request() {
        let session = format!("eoka-sdk-helper-test-{}", std::process::id());
        session::ensure_runtime_dir().unwrap();
        let sock = session::socket_path(&session);
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();

        let server = tokio::spawn(async move {
            handshake(&listener).await;
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = stream.into_split();
            let request: Request = read_msg(&mut reader).await.unwrap();
            assert_eq!(request.cmd(), "open");
            assert_eq!(request.args_json()["url"], "https://example.com");
            write_msg(&mut writer, &Response::ok_text("ok"))
                .await
                .unwrap();
        });

        let response = test_client(&session)
            .open(OpenArgs {
                url: "https://example.com".to_string(),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(response.ok);
        server.await.unwrap();
        let _ = std::fs::remove_file(&sock);
    }
}
