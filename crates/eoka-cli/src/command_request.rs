mod network;

use crate::cli::{CaptchaAction, Command, JsAction, RecordingAction, TabAction, WasmAction};
use crate::protocol::{MouseButton, MouseButtonArgs, Request, ScriptArgs};
use network::network_action_to_request;

const STEP_SHAPES: &str = r##"A step is {"action":{"kind":"click","target":"text:Submit"},"expect":{...}}. target is a ref from view, text:..., css:... or id:....
expect is one of: {"kind":"url","value":"/done","contains":true}, {"kind":"visible","target":{"query":{"kind":"css","selector":"#id"}}}, {"kind":"text","target":{"query":{"kind":"css","selector":"#id"}},"value":"Saved"}, {"kind":"value","target":{"query":{"kind":"css","selector":"#id"}},"value":"x"}.
fill and select verify themselves and need no expect. To act without verifying, leave expect out and add "mode":"explore"."##;

fn with_button(position: MouseButtonArgs, button: &str) -> MouseButtonArgs {
    let button: MouseButton = serde_json::from_value(serde_json::Value::String(button.to_owned()))
        .unwrap_or_else(|error| {
            eprintln!("Error: invalid mouse button: {error}");
            std::process::exit(2);
        });
    MouseButtonArgs { button, ..position }
}

fn parse_script_arguments(raw: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        eprintln!("Invalid --args: {error}");
        std::process::exit(2)
    })
}

fn absolute_path(path: String) -> String {
    std::path::absolute(&path)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or(path)
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
            crate::cli::OperationCommand::List(args) => Request::OperationList(args),
            crate::cli::OperationCommand::Wait(args) => Request::OperationWait(args),
            crate::cli::OperationCommand::Cancel(args) => Request::OperationCancel(args),
        },
        Command::Download(args) => args.request().unwrap_or_else(|error| {
            eprintln!("Error: {error}");
            std::process::exit(2);
        }),
        Command::Open { headers, mut args } => {
            args.headers = headers.as_deref().map(parse_headers_json);
            Request::Open(args)
        }
        Command::Back => Request::Back(Default::default()),
        Command::Forward => Request::Forward(Default::default()),
        Command::Reload => Request::Reload,
        Command::Snapshot(args) => Request::Snapshot(args),
        Command::Observe(mut args) => {
            args.structured |= agent_mode;
            Request::Observe(args)
        }
        Command::Screenshot(mut args) => {
            args.output = args.output.map(absolute_path);
            Request::Screenshot(args)
        }
        Command::Recording { action } => match action {
            RecordingAction::Start(mut args) => {
                args.path = absolute_path(args.path);
                Request::RecordingStart(args)
            }
            RecordingAction::Status(args) => Request::RecordingGet(args),
            RecordingAction::Stop(args) => Request::RecordingStop(args),
            RecordingAction::Cancel(args) => Request::RecordingCancel(args),
        },
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
        Command::Click(args) => Request::Click(args),
        Command::DoubleClick(args) => Request::DblClick(args),
        Command::Fill(args) => Request::Fill(args),
        Command::Select(args) => Request::Select(args),
        Command::Hover(args) => Request::Hover(args),
        Command::Key(args) => Request::Key(args),
        Command::MouseDown { position, button } => {
            Request::MouseDown(with_button(position, &button))
        }
        Command::MouseMove(args) => Request::MouseMove(args),
        Command::MouseUp { position, button } => Request::MouseUp(with_button(position, &button)),
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
                arguments: parse_script_arguments(&args),
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
        Command::Exec { args, mut script } => {
            script.arguments = parse_script_arguments(&args);
            Request::Exec(script)
        }
        Command::Frames => Request::Frames,
        Command::FrameEval(args) => Request::FrameEval(args),
        Command::Tack { .. } | Command::Tools { .. } => {
            unreachable!("command should be handled before daemon request conversion")
        }
        Command::Fetch { headers, mut args } => {
            args.headers = headers.as_deref().map(parse_headers_json);
            Request::Fetch(args)
        }
        Command::Cookies => Request::Cookies,
        Command::SetCookie(args) => Request::SetCookie(args),
        Command::DeleteCookie(args) => Request::DeleteCookie(args),
        Command::ClearCookies => Request::ClearCookies,
        Command::Storage(args) => Request::Storage(args),
        Command::SetStorage(args) => Request::SetStorage(args),
        Command::DumpStorage => Request::DumpStorage,
        Command::SaveState(args) => Request::SaveState(args),
        Command::LoadState(args) => Request::LoadState(args),
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
        Command::Wait(args) => Request::Wait(args),
        Command::SpaInfo => Request::SpaInfo,
        Command::SpaNavigate(args) => Request::SpaNavigate(args),
        Command::FakeCamera(args) => Request::FakeCamera(args),
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
        Command::CloneFrom(args) => Request::CloneFrom(args),
        Command::Captcha {
            action: CaptchaAction::Datadome(args),
        } => Request::CaptchaDatadome(args),
        Command::Captcha {
            action: CaptchaAction::Turnstile(args),
        } => Request::CaptchaTurnstile(args),
        Command::Captcha {
            action: CaptchaAction::Inject(args),
        } => Request::CaptchaInject(args),
        Command::Captcha {
            action: CaptchaAction::Solve(_),
        }
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
