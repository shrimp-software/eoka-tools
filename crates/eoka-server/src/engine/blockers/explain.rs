use super::*;

fn seconds(since: Instant) -> String {
    let elapsed = since.elapsed();
    if elapsed < Duration::from_secs(10) {
        format!("{:.1}", elapsed.as_secs_f64())
    } else {
        elapsed.as_secs().to_string()
    }
}

fn quoted(text: &str) -> String {
    let mut short: String = text.chars().take(200).collect();
    if short.len() < text.len() {
        short.push('…');
    }
    format!("{short:?}")
}

impl Blocker {
    pub fn code(&self) -> &'static str {
        match self {
            Blocker::Dialog(_) => "dialog_open",
            Blocker::Navigation(_) => "navigation_pending",
        }
    }

    pub fn summary(&self) -> Value {
        match self {
            Blocker::Dialog(dialog) => json!({
                "kind": "dialog",
                "dialog_type": dialog.kind,
                "message": dialog.message.chars().take(200).collect::<String>(),
                "url": dialog.url,
                "for_ms": dialog.since.elapsed().as_millis() as u64,
            }),
            Blocker::Navigation(navigation) => json!({
                "kind": "navigation",
                "url": navigation.url,
                "for_ms": navigation.since.elapsed().as_millis() as u64,
            }),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Blocker::Dialog(dialog) if dialog.kind == "beforeunload" => {
                "A \"leave this page?\" (beforeunload) dialog is open on this tab".into()
            }
            Blocker::Dialog(dialog) => format!(
                "A JavaScript {} dialog is open on this tab ({})",
                dialog.kind,
                quoted(&dialog.message)
            ),
            Blocker::Navigation(navigation) => format!(
                "This tab has been waiting {} s for a response from {}",
                seconds(navigation.since),
                navigation.url.as_deref().unwrap_or("a navigation")
            ),
        }
    }

    pub fn remedy(&self, remedies: &Remedies<'_>) -> String {
        match self {
            Blocker::Dialog(dialog) if dialog.kind == "beforeunload" => format!(
                "Run {} to leave the page or {} to stay, then {}.",
                remedies.accept(),
                remedies.dismiss(),
                remedies.view()
            ),
            Blocker::Dialog(_) => format!(
                "Run {} or {}, then {}.",
                remedies.accept(),
                remedies.dismiss(),
                remedies.view()
            ),
            Blocker::Navigation(_) => format!(
                "Keep waiting with {}, or abandon the load: {} returns to the page that is still displayed and {} goes elsewhere.",
                remedies.wait(),
                remedies.reload(),
                remedies.open()
            ),
        }
    }

    pub fn detail(&self, remedies: &Remedies<'_>, command: &str, stage: Stage) -> ErrorDetail {
        let consequence = if stage == Stage::NotStarted {
            format!("`{command}` was not started")
        } else {
            format!("`{command}` returned without a result")
        };
        let reason = match self {
            Blocker::Dialog(_) => {
                "Chrome answers no page commands until the dialog is accepted or dismissed"
            }
            Blocker::Navigation(_) => {
                "Chrome answers no page commands until that navigation commits or is abandoned"
            }
        };
        let mut hint = self.remedy(remedies);
        if stage == Stage::ActionInterrupted {
            hint.push_str(
                " Do not repeat the interrupted command blindly: its page effects may already have happened.",
            );
        }
        let detail = ErrorDetail::new(
            self.code(),
            format!("{}. {reason}, so {consequence}.", self.describe()),
        )
        .retryable(false)
        .hint(hint);
        match stage {
            Stage::NotStarted => detail.context(ErrorContext {
                action_status: Some(ActionStatus::NotAttempted),
                ..Default::default()
            }),
            Stage::ActionInterrupted => detail.context(ErrorContext {
                action_status: Some(ActionStatus::MayHaveExecuted),
                ..Default::default()
            }),
            Stage::ReadInterrupted => detail,
        }
    }
}

pub(super) fn stalled(remedies: &Remedies<'_>, observed: String) -> ErrorDetail {
    ErrorDetail::new(
        "page_unresponsive",
        format!("{observed} No dialog or pending navigation is known, so the page's main thread is busy or frozen."),
    )
    .retryable(false)
    .hint(format!(
        "Retry once the page settles. If it stays frozen, continue in a fresh tab with {}. {} clears a dialog that was already open before this session attached.",
        remedies.new_tab(),
        remedies.dismiss()
    ))
}

pub fn navigation_timeout(
    remedies: &Remedies<'_>,
    navigation: &Navigation,
    operation_id: Option<&str>,
) -> ErrorDetail {
    let timeout = match remedies.surface {
        Surface::Cli => "`--action-timeout-ms`",
        _ => "`timeout_ms`",
    };
    ErrorDetail::new(
        "navigation_timeout",
        format!(
            "No response from {} after {} s. The pending load was stopped, so the tab still shows the page it had before and accepts commands.",
            navigation.url.as_deref().unwrap_or("the requested URL"),
            seconds(navigation.since)
        ),
    )
    .retryable(false)
    .hint(format!(
        "Retry with a larger {timeout} if the site is slow, or check the URL, network and proxy. {} shows the current page.",
        remedies.view()
    ))
    .context(ErrorContext {
        operation_id: operation_id.map(str::to_owned),
        phase: Some("navigation".into()),
        ..Default::default()
    })
}

pub fn policy(command: &str, arguments: &Value) -> Policy {
    use eoka_protocol::OperationId as Id;
    let Some(operation) = eoka_protocol::operation_by_cmd(command) else {
        return Policy::Free;
    };
    let navigates = |kind: Option<&str>| {
        matches!(
            kind,
            Some("navigate" | "navigate_with_headers" | "reload" | "back" | "forward" | "history")
        )
    };
    match operation.id {
        Id::Open | Id::Back | Id::Forward | Id::Reload => Policy::Navigate,
        Id::ActionCommand => match arguments["command"].as_str() {
            Some("back" | "forward" | "reload") => Policy::Navigate,
            _ => Policy::Gate,
        },
        Id::ActionPerform | Id::AgentStep if navigates(arguments["action"]["kind"].as_str()) => {
            Policy::Navigate
        }
        Id::Wait if arguments["ms"].is_u64() => Policy::Free,
        Id::Wait => Policy::Wait,
        Id::Snapshot
        | Id::Observe
        | Id::Info
        | Id::Frames
        | Id::Text
        | Id::Find
        | Id::Content
        | Id::GetText
        | Id::GetAttribute
        | Id::Exists
        | Id::Storage
        | Id::DumpStorage
        | Id::Console
        | Id::Errors
        | Id::SpaInfo
        | Id::WasmInfo
        | Id::WasmRead
        | Id::WasmFind => Policy::Read,
        Id::Eval | Id::Exec | Id::FrameEval | Id::Fetch => Policy::Script,
        Id::Screenshot if arguments["annotate"].as_bool().unwrap_or(false) => Policy::Gate,
        Id::Screenshot => Policy::Capture,
        Id::AgentObserve => Policy::Observe,
        Id::AgentStep
        | Id::ActionPerform
        | Id::Click
        | Id::DblClick
        | Id::Fill
        | Id::Select
        | Id::Hover
        | Id::Key
        | Id::ClickShadow
        | Id::MouseDown
        | Id::MouseMove
        | Id::MouseUp
        | Id::KeyDown
        | Id::KeyUp
        | Id::Scroll
        | Id::SetStorage
        | Id::SaveState
        | Id::SpaNavigate
        | Id::WasmWrite
        | Id::CaptchaDetect
        | Id::CaptchaInject
        | Id::DownloadStart => Policy::Gate,
        Id::SessionHello
        | Id::OperationGet
        | Id::OperationList
        | Id::OperationWait
        | Id::OperationCancel
        | Id::Emulate
        | Id::RecordingStart
        | Id::RecordingGet
        | Id::RecordingStop
        | Id::RecordingCancel
        | Id::ReleaseAllInputs
        | Id::DownloadList
        | Id::DownloadGet
        | Id::DownloadWait
        | Id::DownloadCancel
        | Id::Cookies
        | Id::SetCookie
        | Id::DeleteCookie
        | Id::ClearCookies
        | Id::LoadState
        | Id::Headers
        | Id::TabList
        | Id::TabNew
        | Id::TabSwitch
        | Id::TabClose
        | Id::TabAttach
        | Id::CloneFrom
        | Id::FakeCamera
        | Id::InterceptAdd
        | Id::InterceptList
        | Id::InterceptRemove
        | Id::InterceptLog
        | Id::JsMode
        | Id::JsAllow
        | Id::JsBlock
        | Id::JsRemove
        | Id::JsList
        | Id::NetworkRecordStart
        | Id::NetworkRecordStop
        | Id::NetworkRecordStatus
        | Id::NetworkLog
        | Id::NetworkShow
        | Id::NetworkWait
        | Id::NetworkExport
        | Id::NetworkClear
        | Id::Close
        | Id::Shutdown
        | Id::CaptchaDatadome
        | Id::CaptchaTurnstile
        | Id::DialogAccept
        | Id::DialogDismiss => Policy::Free,
    }
}

pub fn limit(policy: Policy, arguments: &Value) -> Option<Duration> {
    match policy {
        Policy::Read => Some(READ_LIMIT),
        Policy::Wait => Some(Duration::from_millis(
            arguments["timeout"]
                .as_u64()
                .unwrap_or(10_000)
                .saturating_add(2_000),
        )),
        _ => None,
    }
}

pub async fn guard<E>(
    page: Option<&Page>,
    remedies: Remedies<'_>,
    command: &str,
    policy: Policy,
    limit: Option<Duration>,
    work: impl Future<Output = Result<Response, E>>,
) -> Result<Response, E> {
    let Some(page) = page.filter(|_| policy != Policy::Free) else {
        return tokio::select! {
            biased;
            result = work => result,
            _ = abandoned() => Ok(abandoned_response(command)),
        };
    };
    tokio::select! {
        biased;
        raced = race(page, policy, limit, true, work) => match raced {
            Ok(result) => result,
            Err(stop) => Ok(Response::err_detail(stop.detail(
                &remedies,
                command,
                policy.interrupted(),
            ))),
        },
        _ = abandoned() => Ok(abandoned_response(command)),
    }
}

pub fn explain(response: &mut Response, remedies: &Remedies<'_>) {
    let Some(detail) = response.error_detail.as_mut() else {
        return;
    };
    if detail.hint.is_some() {
        return;
    }
    let hint = match detail.code.as_str() {
        "expectation_required" if remedies.surface == Surface::Mcp => return,
        "expectation_required" => match remedies.surface {
            Surface::Cli => "Nothing was sent to the page. State the result you expect and run it again, for example `--expect '{\"kind\":\"url\",\"value\":\"/next\",\"contains\":true}'` (or `\"expect\":{...}` inside `step`). For deliberate unverified input use `step` with `\"mode\":\"explore\"`, then read the page with `view`.".to_owned(),
            _ => "Nothing was sent to the page. Pass an `expect` condition that is false now and true after the action, or use explore mode for deliberate unverified input and observe afterwards.".to_owned(),
        },
        _ => return,
    };
    detail.hint = Some(hint);
}
