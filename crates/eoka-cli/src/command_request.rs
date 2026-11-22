mod network;

use crate::captcha_cmd::captcha_inject_request;
use crate::cli::{CaptchaAction, Command, JsAction, TabAction, WasmAction};
use crate::protocol::{
    CloneFromArgs, FakeCameraArgs, FetchArgs, FillArgs, FrameEvalArgs, LoadStateArgs, MouseButton,
    MouseButtonArgs, ObserveArgs, OpenArgs, PathArgs, Request, ScreenshotArgs, ScriptArgs,
    TargetArgs, WaitArgs,
};
use network::network_action_to_request;

const STEP_SHAPES: &str = r##"A step is {"action":{"kind":"click","target":"text:Submit"},"expect":{...}}. target is a ref from view, text:..., css:... or id:....
expect is one of: {"kind":"url","value":"/done","contains":true}, {"kind":"visible","target":{"query":{"kind":"css","selector":"#id"}}}, {"kind":"text","target":{"query":{"kind":"css","selector":"#id"}},"value":"Saved"}, {"kind":"value","target":{"query":{"kind":"css","selector":"#id"}},"value":"x"}.
fill and select verify themselves and need no expect. To act without verifying, leave expect out and add "mode":"explore"."##;

fn parse_mouse_button(button: &str) -> MouseButton {
    serde_json::from_value(serde_json::Value::String(button.to_owned())).unwrap_or_else(|error| {
        eprintln!("Error: invalid mouse button: {error}");
        std::process::exit(2);
    })
}

fn parse_headers_json(raw: &str) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid --headers JSON: {}", e);
            std::process::exit(2);
        }
    }
}

pub(crate) fn command_to_request(cmd: &Command, agent_mode: bool) -> Request {
    match cmd.clone() {
        Command::View(args) => Request::AgentObserve(args),
        Command::Step { request } => {
            Request::AgentStep(serde_json::from_str(&request).unwrap_or_else(|error| {
                eprintln!("Invalid step request: {error}\n{STEP_SHAPES}");
                std::process::exit(2)
            }))
        }
        Command::Action { request } => {
            Request::ActionPerform(serde_json::from_str(&request).unwrap_or_else(|e| {
                eprintln!("Invalid action request: {e}");
                std::process::exit(2)
            }))
        }
        Command::Operations { command } => match command {
            crate::cli::OperationCommand::Get(args) => Request::OperationGet(args),
            crate::cli::OperationCommand::List { limit } => {
                Request::OperationList(eoka_protocol::OperationListArgs {
                    limit: Some(limit),
                    before_ms: None,
                })
            }
            crate::cli::OperationCommand::Wait(args) => Request::OperationWait(args),
            crate::cli::OperationCommand::Cancel(args) => Request::OperationCancel(args),
        },
        Command::Download(args) => args.request().unwrap_or_else(|error| {
            eprintln!("Error: {error}");
            std::process::exit(2);
        }),
        Command::Open {
            url,
            headers,
            user_agent,
            bypass_csp,
            inject_js,
            load_state,
        } => Request::Open(OpenArgs {
            operation: Default::default(),
            url,
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            user_agent,
            bypass_csp,
            inject_js,
            load_state: load_state
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Back => Request::Back(Default::default()),
        Command::Forward => Request::Forward(Default::default()),
        Command::Reload => Request::Reload,
        Command::Snapshot(args) => Request::Snapshot(args),
        Command::Observe {
            filter,
            max,
            structured,
        } => Request::Observe(ObserveArgs {
            filter,
            max,
            structured: structured || agent_mode,
        }),
        Command::Screenshot {
            output,
            annotate,
            format,
            quality,
            full_page,
            clip,
        } => Request::Screenshot(ScreenshotArgs {
            output: output.map(|path| {
                std::path::absolute(&path)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string()
            }),
            annotate,
            format: format.as_deref().map(|format| match format {
                "jpeg" => eoka_protocol::sidecar::ScreenshotFormat::Jpeg,
                "webp" => eoka_protocol::sidecar::ScreenshotFormat::Webp,
                _ => eoka_protocol::sidecar::ScreenshotFormat::Png,
            }),
            quality,
            clip,
            full_page,
            inline: false,
        }),
        Command::Recording { action } => {
            let id = |id: &String| eoka_protocol::RecordingIdArgs {
                recording_id: id.to_string(),
            };
            match action {
                crate::cli::RecordingAction::Start {
                    path,
                    id,
                    fps,
                    max_width,
                    max_height,
                } => Request::RecordingStart(eoka_protocol::RecordingStartArgs {
                    path: std::path::absolute(&path)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string(),
                    recording_id: id,
                    max_width,
                    max_height,
                    fps,
                }),
                crate::cli::RecordingAction::Status { id: value } => {
                    Request::RecordingGet(id(&value))
                }
                crate::cli::RecordingAction::Stop { id: value } => {
                    Request::RecordingStop(id(&value))
                }
                crate::cli::RecordingAction::Cancel { id: value } => {
                    Request::RecordingCancel(id(&value))
                }
            }
        }
        Command::Emulate(args) => Request::Emulate(args),
        Command::Info => Request::Info,
        Command::Text => Request::Text,
        Command::Find(args) => Request::Find(args),
        Command::Content => Request::Content,
        Command::Dialog { action } => match action {
            crate::cli::DialogAction::Accept(args) => Request::DialogAccept(args),
            crate::cli::DialogAction::Dismiss => Request::DialogDismiss,
        },
        Command::Captcha {
            action: CaptchaAction::Detect,
        } => Request::CaptchaDetect,
        Command::GetText(args) => Request::GetText(args),
        Command::GetAttribute(args) => Request::GetAttribute(args),
        Command::Exists(args) => Request::Exists(args),
        Command::ClickShadow(args) => Request::ClickShadow(args),
        Command::Click { target } => Request::Click(eoka_protocol::ClickArgs {
            element: TargetArgs {
                operation: Default::default(),
                target,
            },
            intercept_nav: None,
        }),
        Command::DoubleClick(args) => Request::DblClick(args),
        Command::Fill {
            target,
            text,
            input_mode,
        } => Request::Fill(FillArgs {
            input_mode: if input_mode == "bulk" {
                eoka_protocol::InputMode::Bulk
            } else {
                eoka_protocol::InputMode::Sequential
            },
            operation: Default::default(),
            target,
            text,
        }),
        Command::Select(args) => Request::Select(args),
        Command::Hover(args) => Request::Hover(args),
        Command::Key(args) => Request::Key(args),
        Command::MouseDown { x, y, button } => Request::MouseDown(MouseButtonArgs {
            x,
            y,
            button: parse_mouse_button(&button),
        }),
        Command::MouseMove(args) => Request::MouseMove(args),
        Command::MouseUp { x, y, button } => Request::MouseUp(MouseButtonArgs {
            x,
            y,
            button: parse_mouse_button(&button),
        }),
        Command::KeyDown(args) => Request::KeyDown(args),
        Command::KeyUp(args) => Request::KeyUp(args),
        Command::ReleaseAllInputs => Request::ReleaseAllInputs,
        Command::Scroll(args) => Request::Scroll(args),
        Command::Eval {
            args,
            raw,
            code,
            file,
            no_return,
            max_size,
            no_await,
        } => {
            let args = ScriptArgs {
                arguments: serde_json::from_str(&args).unwrap_or_else(|error| {
                    eprintln!("Invalid --args: {error}");
                    std::process::exit(2)
                }),
                raw,
                code,
                file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
                max_size,
                no_await,
                function: false,
                json: false,
            };
            if no_return {
                Request::Exec(args)
            } else {
                Request::Eval(args)
            }
        }
        Command::Exec {
            args,
            raw,
            code,
            file,
            no_await,
        } => Request::Exec(ScriptArgs {
            arguments: serde_json::from_str(&args).unwrap_or_else(|error| {
                eprintln!("Invalid --args: {error}");
                std::process::exit(2)
            }),
            raw,
            code,
            file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
            max_size: None,
            no_await,
            function: false,
            json: false,
        }),
        Command::Frames => Request::Frames,
        Command::FrameEval { frame, code, file } => Request::FrameEval(FrameEvalArgs {
            frame,
            code,
            file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Tack { .. } | Command::Tools { .. } => {
            unreachable!("command should be handled before daemon request conversion")
        }
        Command::Fetch {
            url,
            method,
            headers,
            body,
            redirect,
            body_only,
            max_body,
        } => Request::Fetch(FetchArgs {
            url,
            method,
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            body,
            redirect,
            body_only,
            max_body,
        }),
        Command::Cookies => Request::Cookies,
        Command::SetCookie(args) => Request::SetCookie(args),
        Command::DeleteCookie(args) => Request::DeleteCookie(args),
        Command::ClearCookies => Request::ClearCookies,
        Command::Storage(args) => Request::Storage(args),
        Command::SetStorage(args) => Request::SetStorage(args),
        Command::DumpStorage => Request::DumpStorage,
        Command::SaveState { path } => Request::SaveState(PathArgs {
            path: path.to_string_lossy().to_string(),
        }),
        Command::LoadState { path, no_navigate } => Request::LoadState(LoadStateArgs {
            path: path.to_string_lossy().to_string(),
            no_navigate,
        }),
        Command::Headers(args) => Request::Headers(args),
        Command::Console(args) => Request::Console(args),
        Command::Errors(args) => Request::Errors(args),
        Command::Tab { action } => match action {
            TabAction::List => Request::TabList,
            TabAction::New(args) => Request::TabNew(args),
            TabAction::Switch(args) => Request::TabSwitch(args),
            TabAction::Close(args) => Request::TabClose(args),
            TabAction::Attach(args) => Request::TabAttach(args),
        },
        Command::Wait {
            ms,
            text,
            url,
            load,
            selector,
            state,
            timeout,
        } => Request::Wait(WaitArgs {
            expect: None,
            ms,
            text,
            url,
            load,
            selector,
            state: state.as_deref().map(|state| match state {
                "attached" => eoka_protocol::WaitSelectorState::Attached,
                "hidden" => eoka_protocol::WaitSelectorState::Hidden,
                _ => eoka_protocol::WaitSelectorState::Visible,
            }),
            network_idle: None,
            timeout,
        }),
        Command::SpaInfo => Request::SpaInfo,
        Command::SpaNavigate(args) => Request::SpaNavigate(args),
        Command::FakeCamera { file, loop_video } => Request::FakeCamera(FakeCameraArgs {
            file: file.to_string_lossy().to_string(),
            loop_video,
        }),
        Command::Wasm { action } => match action {
            WasmAction::Info => Request::WasmInfo,
            WasmAction::Read(args) => Request::WasmRead(args),
            WasmAction::Write(args) => Request::WasmWrite(args),
            WasmAction::Find(args) => Request::WasmFind(args),
        },

        Command::Network { action } => network_action_to_request(&action),
        Command::Js { action } => match action {
            JsAction::Mode(args) => Request::JsMode(args),
            JsAction::Allow(args) => Request::JsAllow(args),
            JsAction::Block(args) => Request::JsBlock(args),
            JsAction::Remove(args) => Request::JsRemove(args),
            JsAction::List => Request::JsList,
        },
        Command::Close => Request::Close,
        Command::CloneFrom { source, to } => Request::CloneFrom(CloneFromArgs {
            source,
            to: to.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Captcha {
            action: CaptchaAction::Datadome(args),
        } => Request::CaptchaDatadome(args),
        Command::Captcha {
            action: CaptchaAction::Turnstile(args),
        } => Request::CaptchaTurnstile(args),
        Command::Captcha {
            action:
                CaptchaAction::Inject {
                    token,
                    captcha_type,
                    callback,
                    click_after,
                },
        } => captcha_inject_request(
            &token,
            &captcha_type,
            callback.as_deref(),
            click_after.as_deref(),
        ),
        Command::Captcha {
            action: CaptchaAction::Solve(_),
        }
        | Command::Mcp
        | Command::Sessions
        | Command::Status
        | Command::Doctor
        | Command::Kill
        | Command::Update(_)
        | Command::Skill { .. }
        | Command::CdpUrl { .. }
        | Command::Batch { .. } => {
            unreachable!(
                "BUG: command should have been handled before command_to_request — see main()'s early-dispatch match"
            )
        }
    }
}

#[cfg(test)]
mod tests;
