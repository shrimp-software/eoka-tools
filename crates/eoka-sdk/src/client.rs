use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::net::UnixStream;

use crate::launch_spec::LaunchSpec;
use crate::session;
use eoka_protocol::{
    read_msg, write_msg, CaptchaDatadomeArgs, CaptchaInjectArgs, ClearFlagArgs, CloneFromArgs,
    ConsoleArgs, DeleteCookieArgs, DomainArgs, EmulateArgs, FakeCameraArgs, FetchArgs, FillArgs,
    FrameEvalArgs, HeadersArgs, IdArgs, InterceptAddArgs, KeyArgs, LoadStateArgs, ModeArgs,
    MouseButtonArgs, MouseMoveArgs, NetworkExportArgs, NetworkLogArgs, NetworkRecordStartArgs,
    NetworkShowArgs, NetworkWaitArgs, ObserveArgs, OpenArgs, PathArgs, PathStringArgs, Request,
    Response, ScreenshotArgs, ScriptArgs, SelectArgs, SetCookieArgs, SetStorageArgs, SnapshotArgs,
    StorageArgs, TabIdArgs, TabNewArgs, TargetArgs, TextArgs, WaitArgs, WasmFindArgs, WasmReadArgs,
    WasmWriteArgs,
};

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

    pub async fn perform(
        &self,
        request: eoka_protocol::ActionPerformArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::ActionPerform(request)).await
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
    pub async fn open(&self, args: OpenArgs) -> anyhow::Result<Response> {
        self.call(Request::Open(args)).await
    }

    pub async fn back(&self) -> anyhow::Result<Response> {
        self.call(Request::Back).await
    }

    pub async fn forward(&self) -> anyhow::Result<Response> {
        self.call(Request::Forward).await
    }

    pub async fn reload(&self) -> anyhow::Result<Response> {
        self.call(Request::Reload).await
    }

    pub async fn snapshot(&self, args: SnapshotArgs) -> anyhow::Result<Response> {
        self.call(Request::Snapshot(args)).await
    }

    pub async fn observe(&self, args: ObserveArgs) -> anyhow::Result<Response> {
        self.call(Request::Observe(args)).await
    }

    pub async fn screenshot(&self, args: ScreenshotArgs) -> anyhow::Result<Response> {
        self.call(Request::Screenshot(args)).await
    }

    pub async fn emulate(&self, args: EmulateArgs) -> anyhow::Result<Response> {
        self.call(Request::Emulate(args)).await
    }

    pub async fn info(&self) -> anyhow::Result<Response> {
        self.call(Request::Info).await
    }

    pub async fn frames(&self) -> anyhow::Result<Response> {
        self.call(Request::Frames).await
    }

    pub async fn frame_eval(&self, args: FrameEvalArgs) -> anyhow::Result<Response> {
        self.call(Request::FrameEval(args)).await
    }

    pub async fn text(&self) -> anyhow::Result<Response> {
        self.call(Request::Text).await
    }

    pub async fn find(&self, args: TextArgs) -> anyhow::Result<Response> {
        self.call(Request::Find(args)).await
    }

    pub async fn click(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Click(args)).await
    }

    pub async fn double_click(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::DblClick(args)).await
    }

    pub async fn fill(&self, args: FillArgs) -> anyhow::Result<Response> {
        self.call(Request::Fill(args)).await
    }

    pub async fn select(&self, args: SelectArgs) -> anyhow::Result<Response> {
        self.call(Request::Select(args)).await
    }

    pub async fn hover(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Hover(args)).await
    }

    pub async fn key(&self, args: KeyArgs) -> anyhow::Result<Response> {
        self.call(Request::Key(args)).await
    }

    pub async fn mouse_down(&self, args: MouseButtonArgs) -> anyhow::Result<Response> {
        self.call(Request::MouseDown(args)).await
    }

    pub async fn mouse_move(&self, args: MouseMoveArgs) -> anyhow::Result<Response> {
        self.call(Request::MouseMove(args)).await
    }

    pub async fn mouse_up(&self, args: MouseButtonArgs) -> anyhow::Result<Response> {
        self.call(Request::MouseUp(args)).await
    }

    pub async fn key_down(&self, args: KeyArgs) -> anyhow::Result<Response> {
        self.call(Request::KeyDown(args)).await
    }

    pub async fn key_up(&self, args: KeyArgs) -> anyhow::Result<Response> {
        self.call(Request::KeyUp(args)).await
    }

    pub async fn release_all_inputs(&self) -> anyhow::Result<Response> {
        self.call(Request::ReleaseAllInputs).await
    }

    pub async fn scroll(&self, args: TargetArgs) -> anyhow::Result<Response> {
        self.call(Request::Scroll(args)).await
    }

    pub async fn eval(&self, args: ScriptArgs) -> anyhow::Result<Response> {
        self.call(Request::Eval(args)).await
    }

    pub async fn exec(&self, args: ScriptArgs) -> anyhow::Result<Response> {
        self.call(Request::Exec(args)).await
    }

    pub async fn fetch(&self, args: FetchArgs) -> anyhow::Result<Response> {
        self.call(Request::Fetch(args)).await
    }

    pub async fn download_start(
        &self,
        args: eoka_protocol::DownloadStartArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::DownloadStart(args)).await
    }

    pub async fn download_list(&self) -> anyhow::Result<Response> {
        self.call(Request::DownloadList).await
    }

    pub async fn download_get(
        &self,
        args: eoka_protocol::DownloadIdArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::DownloadGet(args)).await
    }

    pub async fn download_wait(
        &self,
        args: eoka_protocol::DownloadWaitArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::DownloadWait(args)).await
    }

    pub async fn download_cancel(
        &self,
        args: eoka_protocol::DownloadIdArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::DownloadCancel(args)).await
    }

    pub async fn cookies(&self) -> anyhow::Result<Response> {
        self.call(Request::Cookies).await
    }

    pub async fn set_cookie(&self, args: SetCookieArgs) -> anyhow::Result<Response> {
        self.call(Request::SetCookie(args)).await
    }

    pub async fn delete_cookie(&self, args: DeleteCookieArgs) -> anyhow::Result<Response> {
        self.call(Request::DeleteCookie(args)).await
    }

    pub async fn clear_cookies(&self) -> anyhow::Result<Response> {
        self.call(Request::ClearCookies).await
    }

    pub async fn storage(&self, args: StorageArgs) -> anyhow::Result<Response> {
        self.call(Request::Storage(args)).await
    }

    pub async fn set_storage(&self, args: SetStorageArgs) -> anyhow::Result<Response> {
        self.call(Request::SetStorage(args)).await
    }

    pub async fn dump_storage(&self) -> anyhow::Result<Response> {
        self.call(Request::DumpStorage).await
    }

    pub async fn save_state(&self, args: PathArgs) -> anyhow::Result<Response> {
        self.call(Request::SaveState(args)).await
    }

    pub async fn load_state(&self, args: LoadStateArgs) -> anyhow::Result<Response> {
        self.call(Request::LoadState(args)).await
    }

    pub async fn headers(&self, args: HeadersArgs) -> anyhow::Result<Response> {
        self.call(Request::Headers(args)).await
    }

    pub async fn console(&self, args: ConsoleArgs) -> anyhow::Result<Response> {
        self.call(Request::Console(args)).await
    }

    pub async fn errors(&self, args: ClearFlagArgs) -> anyhow::Result<Response> {
        self.call(Request::Errors(args)).await
    }

    pub async fn tab_list(&self) -> anyhow::Result<Response> {
        self.call(Request::TabList).await
    }

    pub async fn tab_new(&self, args: TabNewArgs) -> anyhow::Result<Response> {
        self.call(Request::TabNew(args)).await
    }

    pub async fn tab_switch(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabSwitch(args)).await
    }

    pub async fn tab_close(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabClose(args)).await
    }

    pub async fn tab_attach(&self, args: TabIdArgs) -> anyhow::Result<Response> {
        self.call(Request::TabAttach(args)).await
    }

    pub async fn clone_from(&self, args: CloneFromArgs) -> anyhow::Result<Response> {
        self.call(Request::CloneFrom(args)).await
    }

    pub async fn wait(&self, args: WaitArgs) -> anyhow::Result<Response> {
        self.call(Request::Wait(args)).await
    }

    pub async fn spa_info(&self) -> anyhow::Result<Response> {
        self.call(Request::SpaInfo).await
    }

    pub async fn spa_navigate(&self, args: PathStringArgs) -> anyhow::Result<Response> {
        self.call(Request::SpaNavigate(args)).await
    }

    pub async fn fake_camera(&self, args: FakeCameraArgs) -> anyhow::Result<Response> {
        self.call(Request::FakeCamera(args)).await
    }

    pub async fn wasm_info(&self) -> anyhow::Result<Response> {
        self.call(Request::WasmInfo).await
    }

    pub async fn wasm_read(&self, args: WasmReadArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmRead(args)).await
    }

    pub async fn wasm_write(&self, args: WasmWriteArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmWrite(args)).await
    }

    pub async fn wasm_find(&self, args: WasmFindArgs) -> anyhow::Result<Response> {
        self.call(Request::WasmFind(args)).await
    }

    pub async fn intercept_add(&self, args: InterceptAddArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptAdd(args)).await
    }

    pub async fn intercept_list(&self) -> anyhow::Result<Response> {
        self.call(Request::InterceptList).await
    }

    pub async fn intercept_remove(&self, args: IdArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptRemove(args)).await
    }

    pub async fn intercept_log(&self, args: ClearFlagArgs) -> anyhow::Result<Response> {
        self.call(Request::InterceptLog(args)).await
    }

    pub async fn network_record_start(
        &self,
        args: NetworkRecordStartArgs,
    ) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStart(args)).await
    }

    pub async fn network_record_stop(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStop).await
    }

    pub async fn network_record_status(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkRecordStatus).await
    }

    pub async fn network_log(&self, args: NetworkLogArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkLog(args)).await
    }

    pub async fn network_show(&self, args: NetworkShowArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkShow(args)).await
    }

    pub async fn network_wait(&self, args: NetworkWaitArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkWait(args)).await
    }

    pub async fn network_export(&self, args: NetworkExportArgs) -> anyhow::Result<Response> {
        self.call(Request::NetworkExport(args)).await
    }

    pub async fn network_clear(&self) -> anyhow::Result<Response> {
        self.call(Request::NetworkClear).await
    }

    pub async fn js_mode(&self, args: ModeArgs) -> anyhow::Result<Response> {
        self.call(Request::JsMode(args)).await
    }

    pub async fn js_allow(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsAllow(args)).await
    }

    pub async fn js_block(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsBlock(args)).await
    }

    pub async fn js_remove(&self, args: DomainArgs) -> anyhow::Result<Response> {
        self.call(Request::JsRemove(args)).await
    }

    pub async fn js_list(&self) -> anyhow::Result<Response> {
        self.call(Request::JsList).await
    }

    pub async fn close(&self) -> anyhow::Result<Response> {
        self.call(Request::Close).await
    }

    pub async fn shutdown(&self) -> anyhow::Result<Response> {
        self.call(Request::Shutdown).await
    }

    pub async fn captcha_datadome(&self, args: CaptchaDatadomeArgs) -> anyhow::Result<Response> {
        self.call(Request::CaptchaDatadome(args)).await
    }

    pub async fn captcha_inject(&self, args: CaptchaInjectArgs) -> anyhow::Result<Response> {
        self.call(Request::CaptchaInject(args)).await
    }
}
pub async fn send_command(
    session_name: &str,
    mut request: Request,
    spec: LaunchSpec,
) -> anyhow::Result<Response> {
    if let Request::DownloadStart(args) = &mut request {
        args.operation_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string());
    }
    if matches!(
        request.cmd(),
        "click"
            | "dblclick"
            | "fill"
            | "select"
            | "hover"
            | "key"
            | "back"
            | "forward"
            | "reload"
            | "action_perform"
            | "action_command"
            | "operation_get"
            | "operation_list"
            | "operation_wait"
            | "operation_cancel"
    ) {
        return send_operation(session_name, request, &spec).await;
    }
    if matches!(
        request,
        Request::DownloadStart(_)
            | Request::DownloadList
            | Request::DownloadGet(_)
            | Request::DownloadWait(_)
            | Request::DownloadCancel(_)
    ) {
        return Ok(send_download(session_name, &request, &spec).await);
    }
    if let Request::CaptchaDatadome(args) = request {
        return Ok(send_datadome(session_name, args).await);
    }
    if matches!(request, Request::Frames | Request::FrameEval(_)) {
        return Ok(send_existing_session(session_name, &request, "frame_transport_error",
            "This daemon does not support frame operations. Upgrade the daemon explicitly; the existing browser was not restarted.").await);
    }
    let response = send_command_once(session_name, request.clone(), &spec).await?;
    if should_restart_headed_daemon(&response, &spec) {
        let _ = kill_daemon(session_name);
        let retry = send_command_once(session_name, request, &spec).await?;
        return Ok(retry);
    }
    Ok(response)
}

fn non_retryable_error(code: &str, message: &str) -> Response {
    Response::err_detail(eoka_protocol::ErrorDetail::new(code, message).retryable(false))
}

async fn send_download(session_name: &str, request: &Request, spec: &LaunchSpec) -> Response {
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
    let mut response = match exchange_request(stream, &wire).await {
        Ok(response) if response.error.as_deref().is_some_and(|message| {
            message.contains("unknown variant") || message.contains("unknown eoka protocol command")
        }) => disconnected(
            "unsupported_operation",
            "This daemon does not support downloads. Restart it explicitly after upgrading; no action was replayed.",
            ActionStatus::NotAttempted,
        ),
        Ok(response) => response,
        Err(_) => disconnected(
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

async fn send_datadome(session_name: &str, args: CaptchaDatadomeArgs) -> Response {
    if let Err(message) = args.validate() {
        return non_retryable_error("invalid_input", message);
    }
    send_existing_session(session_name, &Request::CaptchaDatadome(args), "datadome_transport_error",
        "This daemon does not support DataDome. Upgrade the daemon explicitly; the existing browser was not restarted.").await
}

async fn send_existing_session(
    session_name: &str,
    request: &Request,
    transport_error: &str,
    unsupported_message: &str,
) -> Response {
    let Ok(stream) = UnixStream::connect(session::socket_path(session_name)).await else {
        return non_retryable_error(
            "session_unavailable",
            "Existing session unavailable; open the intended page first. No browser was launched.",
        );
    };
    match exchange_request(stream, request).await {
        Ok(response) if response.error.as_deref().is_some_and(|error| error.contains(request.cmd()) && (error.contains("unknown variant") || error.contains("unknown eoka protocol command"))) =>
            non_retryable_error("unsupported_operation", unsupported_message),
        Ok(response) => response,
        Err(_) => non_retryable_error(transport_error, "Session transport failed; the request may have executed. Inspect the existing page before retrying. No replay was attempted."),
    }
}

async fn send_command_once(
    session_name: &str,
    request: Request,
    spec: &LaunchSpec,
) -> anyhow::Result<Response> {
    let sock = session::socket_path(session_name);
    let stream = match UnixStream::connect(&sock).await {
        Ok(s) => s,
        Err(_) => {
            launch_daemon(session_name, spec)?;
            wait_for_socket(session_name).await?
        }
    };

    exchange_request(stream, &request).await
}

async fn exchange_request(stream: UnixStream, request: &Request) -> anyhow::Result<Response> {
    let (mut reader, mut writer) = stream.into_split();
    write_msg(&mut writer, request).await?;
    Ok(read_msg(&mut reader).await?)
}

fn should_restart_headed_daemon(response: &Response, spec: &LaunchSpec) -> bool {
    let LaunchSpec::Launch {
        headless: false, ..
    } = spec
    else {
        return false;
    };
    if std::env::var_os("DISPLAY").is_none() {
        return false;
    }
    let Some(error) = response.error.as_deref() else {
        return false;
    };
    error.contains("Missing X server or $DISPLAY")
        || error.contains("channel is empty and sending half is closed")
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
        if let Ok(s) = UnixStream::connect(&sock).await {
            return Ok(s);
        }
    }
}
fn launch_daemon(session_name: &str, spec: &LaunchSpec) -> anyhow::Result<()> {
    session::ensure_runtime_dir()?;

    let exe = std::env::current_exe()?;
    let mut cmd = StdCommand::new(exe);
    cmd.arg("--daemon").arg("--session").arg(session_name);

    match spec {
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
            if let Some(p) = proxy {
                cmd.arg("--proxy").arg(p);
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

    let log_path = session::socket_path(session_name).with_extension("log");
    let log_file = std::fs::File::create(&log_path)?;

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log_file);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn()?;
    eprintln!(
        "[eoka] daemon starting (session={}, log={})",
        session_name,
        log_path.display()
    );
    Ok(())
}
pub fn is_daemon_running(session_name: &str) -> bool {
    let sock = session::socket_path(session_name);
    std::os::unix::net::UnixStream::connect(&sock).is_ok()
}
pub fn kill_daemon(session_name: &str) -> anyhow::Result<bool> {
    let sock = session::socket_path(session_name);
    if let Ok(stream) = std::os::unix::net::UnixStream::connect(&sock) {
        drop(stream);
    }
    let pid_path = session::pid_path(session_name);
    let original_pid = std::fs::read_to_string(&pid_path).ok();
    let killed = if let Some(pid_str) = &original_pid {
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
) -> anyhow::Result<Response> {
    let args = request.args_json();
    let id = args["operation_id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut wire = serde_json::to_value(&request)?;
    if matches!(
        request.cmd(),
        "click" | "dblclick" | "fill" | "select" | "hover" | "key" | "back" | "forward" | "reload"
    ) {
        wire = serde_json::json!({"cmd":"action_command","args":{"operation_id":id,"command":request.cmd(),"arguments":args}});
    }
    wire["operation_id"] = serde_json::json!(id);
    let stream = match UnixStream::connect(session::socket_path(session_name)).await {
        Ok(stream) => stream,
        Err(_) if request.cmd().starts_with("operation_") => {
            launch_daemon(session_name, spec)?;
            wait_for_socket(session_name).await?
        }
        Err(_) => {
            return Ok(non_retryable_error(
                "session_unavailable",
                &format!("Existing session unavailable; operation {id} was not sent"),
            ))
        }
    };
    let (mut reader, mut writer) = stream.into_split();
    let result: anyhow::Result<Response> = async {
        write_msg(&mut writer, &wire).await?;
        Ok(read_msg(&mut reader).await?)
    }
    .await;
    Ok(match result {
        Ok(response)=>response,
        Err(_)=>non_retryable_error("operation_transport_error",&format!("Connection lost; inspect operation {id}. Its outcome is unknown. No action was replayed.")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

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
                let mut client = test_client(&name);
                if let LaunchSpec::Launch { headless, .. } = &mut client.spec {
                    *headless = false;
                }
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
