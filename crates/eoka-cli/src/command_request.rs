mod network;

use crate::captcha_cmd::captcha_inject_request;
use crate::cli::{CaptchaAction, Command, JsAction, TabAction, WasmAction};
use crate::protocol::{
    CaptchaDatadomeArgs, CaptchaTurnstileArgs, ClearFlagArgs, CloneFromArgs, ConsoleArgs,
    DeleteCookieArgs, DomainArgs, EmulateArgs, FakeCameraArgs, FetchArgs, FillArgs, FrameEvalArgs,
    HeadersArgs, HeldKeyArgs, KeyArgs, LoadStateArgs, ModeArgs, MouseButton, MouseButtonArgs,
    MouseMoveArgs, ObserveArgs, OpenArgs, PathArgs, PathStringArgs, Request, ScreenshotArgs,
    ScriptArgs, SelectArgs, SetCookieArgs, SetStorageArgs, SnapshotArgs, StorageArgs, TabIdArgs,
    TabNewArgs, TargetArgs, TextArgs, WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
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
        Command::View {
            cursor,
            role,
            name,
            max_controls,
            timeout_ms,
        } => Request::AgentObserve(eoka_protocol::AgentObserveArgs {
            cursor,
            role,
            name,
            max_controls,
            timeout_ms,
        }),
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
            crate::cli::OperationCommand::Get { id } => {
                Request::OperationGet(eoka_protocol::OperationIdArgs { id })
            }
            crate::cli::OperationCommand::List { limit } => {
                Request::OperationList(eoka_protocol::OperationListArgs {
                    limit: Some(limit),
                    before_ms: None,
                })
            }
            crate::cli::OperationCommand::Wait { id, timeout_ms } => {
                Request::OperationWait(eoka_protocol::OperationWaitArgs { id, timeout_ms })
            }
            crate::cli::OperationCommand::Cancel { id } => {
                Request::OperationCancel(eoka_protocol::OperationIdArgs { id })
            }
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
        Command::Snapshot { interactive, all } => {
            Request::Snapshot(SnapshotArgs { interactive, all })
        }
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
        Command::Emulate {
            width,
            height,
            dpr,
            desktop,
            reset,
        } => Request::Emulate(EmulateArgs {
            width,
            height,
            dpr,
            desktop,
            reset,
        }),
        Command::Info => Request::Info,
        Command::Text => Request::Text,
        Command::Find { text } => Request::Find(TextArgs { text }),
        Command::Content => Request::Content,
        Command::Dialog { action } => match action {
            crate::cli::DialogAction::Accept { prompt_text } => {
                Request::DialogAccept(eoka_protocol::DialogAcceptArgs { prompt_text })
            }
            crate::cli::DialogAction::Dismiss => Request::DialogDismiss,
        },
        Command::Captcha {
            action: CaptchaAction::Detect,
        } => Request::CaptchaDetect,
        Command::GetText { selector } => Request::GetText(eoka_protocol::SelectorArgs { selector }),
        Command::GetAttribute { selector, name } => {
            Request::GetAttribute(eoka_protocol::AttributeArgs { selector, name })
        }
        Command::Exists { selector } => Request::Exists(eoka_protocol::SelectorArgs { selector }),
        Command::ClickShadow {
            selector,
            frame,
            timeout_ms,
        } => Request::ClickShadow(eoka_protocol::ClickShadowArgs {
            selector,
            frame_id: frame,
            timeout_ms,
        }),
        Command::Click { target } => Request::Click(eoka_protocol::ClickArgs {
            element: TargetArgs {
                operation: Default::default(),
                target,
            },
            intercept_nav: None,
        }),
        Command::DoubleClick { target } => Request::DblClick(TargetArgs {
            operation: Default::default(),
            target,
        }),
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
        Command::Select { target, value } => Request::Select(SelectArgs {
            operation: Default::default(),
            target,
            value,
        }),
        Command::Hover { target } => Request::Hover(TargetArgs {
            operation: Default::default(),
            target,
        }),
        Command::Key { key } => Request::Key(KeyArgs {
            operation: Default::default(),
            key,
        }),
        Command::MouseDown { x, y, button } => Request::MouseDown(MouseButtonArgs {
            x,
            y,
            button: parse_mouse_button(&button),
        }),
        Command::MouseMove { x, y } => Request::MouseMove(MouseMoveArgs { x, y }),
        Command::MouseUp { x, y, button } => Request::MouseUp(MouseButtonArgs {
            x,
            y,
            button: parse_mouse_button(&button),
        }),
        Command::KeyDown { key } => Request::KeyDown(HeldKeyArgs { key }),
        Command::KeyUp { key } => Request::KeyUp(HeldKeyArgs { key }),
        Command::ReleaseAllInputs => Request::ReleaseAllInputs,
        Command::Scroll { target } => Request::Scroll(TargetArgs {
            operation: Default::default(),
            target,
        }),
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
        Command::SetCookie {
            name,
            value,
            domain,
            path,
        } => Request::SetCookie(SetCookieArgs {
            name,
            value,
            domain,
            path,
        }),
        Command::DeleteCookie { name, domain } => {
            Request::DeleteCookie(DeleteCookieArgs { name, domain })
        }
        Command::ClearCookies => Request::ClearCookies,
        Command::Storage {
            key,
            session_storage,
        } => Request::Storage(StorageArgs {
            key,
            session_storage,
        }),
        Command::SetStorage {
            key,
            value,
            session_storage,
        } => Request::SetStorage(SetStorageArgs {
            key,
            value,
            session_storage,
        }),
        Command::DumpStorage => Request::DumpStorage,
        Command::SaveState { path } => Request::SaveState(PathArgs {
            path: path.to_string_lossy().to_string(),
        }),
        Command::LoadState { path, no_navigate } => Request::LoadState(LoadStateArgs {
            path: path.to_string_lossy().to_string(),
            no_navigate,
        }),
        Command::Headers { headers_json } => Request::Headers(HeadersArgs { headers_json }),
        Command::Console { clear, level } => Request::Console(ConsoleArgs { clear, level }),
        Command::Errors { clear } => Request::Errors(ClearFlagArgs { clear }),
        Command::Tab { action } => match action {
            TabAction::List => Request::TabList,
            TabAction::New { url } => Request::TabNew(TabNewArgs { url }),
            TabAction::Switch { tab_id } => Request::TabSwitch(TabIdArgs { tab_id }),
            TabAction::Close { tab_id } => Request::TabClose(TabIdArgs { tab_id }),
            TabAction::Attach { tab_id } => Request::TabAttach(TabIdArgs { tab_id }),
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
        Command::SpaNavigate { path } => Request::SpaNavigate(PathStringArgs { path }),
        Command::FakeCamera { file, loop_video } => Request::FakeCamera(FakeCameraArgs {
            file: file.to_string_lossy().to_string(),
            loop_video,
        }),
        Command::Wasm { action } => match action {
            WasmAction::Info => Request::WasmInfo,
            WasmAction::Read { addr, len, memory } => {
                Request::WasmRead(WasmReadArgs { addr, len, memory })
            }
            WasmAction::Write { addr, hex, memory } => {
                Request::WasmWrite(WasmWriteArgs { addr, hex, memory })
            }
            WasmAction::Find {
                pattern,
                start,
                end,
                max,
                memory,
            } => Request::WasmFind(WasmFindArgs {
                pattern,
                start,
                end,
                max,
                memory,
            }),
        },

        Command::Network { action } => network_action_to_request(&action),
        Command::Js { action } => match action {
            JsAction::Mode { mode } => Request::JsMode(ModeArgs { mode }),
            JsAction::Allow { domain } => Request::JsAllow(DomainArgs { domain }),
            JsAction::Block { domain } => Request::JsBlock(DomainArgs { domain }),
            JsAction::Remove { domain } => Request::JsRemove(DomainArgs { domain }),
            JsAction::List => Request::JsList,
        },
        Command::Close => Request::Close,
        Command::CloneFrom { source, to } => Request::CloneFrom(CloneFromArgs {
            source,
            to: to.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Captcha {
            action:
                CaptchaAction::Datadome {
                    max_attempts,
                    timeout_ms,
                },
        } => Request::CaptchaDatadome(CaptchaDatadomeArgs {
            max_attempts,
            timeout_ms,
        }),
        Command::Captcha {
            action: CaptchaAction::Turnstile { timeout_ms },
        } => Request::CaptchaTurnstile(CaptchaTurnstileArgs { timeout_ms }),
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
