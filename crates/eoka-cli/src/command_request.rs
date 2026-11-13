mod network;

use crate::captcha_cmd::captcha_inject_request;
use crate::cli::{CaptchaAction, Command, JsAction, TabAction, WasmAction};
use crate::protocol::{
    CaptchaDatadomeArgs, CaptchaTurnstileArgs, ClearFlagArgs, CloneFromArgs, ConsoleArgs,
    DeleteCookieArgs, DomainArgs, EmulateArgs, FakeCameraArgs, FetchArgs, FillArgs, FrameEvalArgs,
    HeadersArgs, KeyArgs, LoadStateArgs, ModeArgs, MouseButton, MouseButtonArgs, MouseMoveArgs,
    ObserveArgs, OpenArgs, PathArgs, PathStringArgs, Request, ScreenshotArgs, ScriptArgs,
    SelectArgs, SetCookieArgs, SetStorageArgs, SnapshotArgs, StorageArgs, TabIdArgs, TabNewArgs,
    TargetArgs, TextArgs, WaitArgs, WasmFindArgs, WasmReadArgs, WasmWriteArgs,
};
use network::network_action_to_request;

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
    match cmd {
        Command::View {
            cursor,
            role,
            name,
            max_controls,
            timeout_ms,
        } => Request::AgentObserve(eoka_protocol::AgentObserveArgs {
            cursor: cursor.clone(),
            role: role.clone(),
            name: name.clone(),
            max_controls: *max_controls,
            timeout_ms: *timeout_ms,
        }),
        Command::Step { request } => {
            Request::AgentStep(serde_json::from_str(request).unwrap_or_else(|error| {
                eprintln!("Invalid step request: {error}");
                std::process::exit(2)
            }))
        }
        Command::Action { request } => {
            Request::ActionPerform(serde_json::from_str(request).unwrap_or_else(|e| {
                eprintln!("Invalid action request: {e}");
                std::process::exit(2)
            }))
        }
        Command::Operations { command } => match command {
            crate::cli::OperationCommand::Get { id } => {
                Request::OperationGet(eoka_protocol::OperationIdArgs { id: id.clone() })
            }
            crate::cli::OperationCommand::List { limit } => {
                Request::OperationList(eoka_protocol::OperationListArgs {
                    limit: Some(*limit),
                    before_ms: None,
                })
            }
            crate::cli::OperationCommand::Wait { id, timeout_ms } => {
                Request::OperationWait(eoka_protocol::OperationWaitArgs {
                    id: id.clone(),
                    timeout_ms: *timeout_ms,
                })
            }
            crate::cli::OperationCommand::Cancel { id } => {
                Request::OperationCancel(eoka_protocol::OperationIdArgs { id: id.clone() })
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
            url: url.clone(),
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            user_agent: user_agent.clone(),
            bypass_csp: *bypass_csp,
            inject_js: inject_js.clone(),
            load_state: load_state
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Back => Request::Back,
        Command::Forward => Request::Forward,
        Command::Reload => Request::Reload,
        Command::Snapshot { interactive, all } => Request::Snapshot(SnapshotArgs {
            interactive: *interactive,
            all: *all,
        }),
        Command::Observe {
            filter,
            max,
            structured,
        } => Request::Observe(ObserveArgs {
            filter: filter.clone(),
            max: *max,
            structured: *structured || agent_mode,
        }),
        Command::Screenshot {
            output,
            annotate,
            format,
            quality,
            full_page,
            clip,
        } => Request::Screenshot(ScreenshotArgs {
            output: output
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            annotate: *annotate,
            format: format.as_deref().map(|format| match format {
                "jpeg" => eoka_protocol::sidecar::ScreenshotFormat::Jpeg,
                "webp" => eoka_protocol::sidecar::ScreenshotFormat::Webp,
                _ => eoka_protocol::sidecar::ScreenshotFormat::Png,
            }),
            quality: *quality,
            clip: clip.as_deref().map(parse_clip),
            full_page: *full_page,
        }),
        Command::Recording { action } => {
            let id = |id: &String| eoka_protocol::RecordingIdArgs {
                recording_id: id.clone(),
            };
            match action {
                crate::cli::RecordingAction::Start {
                    path,
                    id,
                    fps,
                    max_width,
                    max_height,
                } => Request::RecordingStart(eoka_protocol::RecordingStartArgs {
                    path: std::path::absolute(path)
                        .unwrap_or_else(|_| path.clone())
                        .to_string_lossy()
                        .to_string(),
                    recording_id: id.clone(),
                    max_width: *max_width,
                    max_height: *max_height,
                    fps: *fps,
                }),
                crate::cli::RecordingAction::Status { id: value } => {
                    Request::RecordingGet(id(value))
                }
                crate::cli::RecordingAction::Stop { id: value } => {
                    Request::RecordingStop(id(value))
                }
                crate::cli::RecordingAction::Cancel { id: value } => {
                    Request::RecordingCancel(id(value))
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
            width: *width,
            height: *height,
            dpr: *dpr,
            desktop: *desktop,
            reset: *reset,
        }),
        Command::Info => Request::Info,
        Command::Text => Request::Text,
        Command::Find { text } => Request::Find(TextArgs { text: text.clone() }),
        Command::Content => Request::Content,
        Command::GetText { selector } => Request::GetText(eoka_protocol::SelectorArgs {
            selector: selector.clone(),
        }),
        Command::GetAttribute { selector, name } => {
            Request::GetAttribute(eoka_protocol::AttributeArgs {
                selector: selector.clone(),
                name: name.clone(),
            })
        }
        Command::Exists { selector } => Request::Exists(eoka_protocol::SelectorArgs {
            selector: selector.clone(),
        }),
        Command::ClickShadow {
            selector,
            frame,
            timeout_ms,
        } => Request::ClickShadow(eoka_protocol::ClickShadowArgs {
            selector: selector.clone(),
            frame_id: frame.clone(),
            timeout_ms: *timeout_ms,
        }),
        Command::Click { target } => Request::Click(TargetArgs {
            operation: Default::default(),
            target: target.clone(),
        }),
        Command::DoubleClick { target } => Request::DblClick(TargetArgs {
            operation: Default::default(),
            target: target.clone(),
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
            target: target.clone(),
            text: text.clone(),
        }),
        Command::Select { target, value } => Request::Select(SelectArgs {
            operation: Default::default(),
            target: target.clone(),
            value: value.clone(),
        }),
        Command::Hover { target } => Request::Hover(TargetArgs {
            operation: Default::default(),
            target: target.clone(),
        }),
        Command::Key { key } => Request::Key(KeyArgs {
            operation: Default::default(),
            key: key.clone(),
        }),
        Command::MouseDown { x, y, button } => Request::MouseDown(MouseButtonArgs {
            x: *x,
            y: *y,
            button: parse_mouse_button(button),
        }),
        Command::MouseMove { x, y } => Request::MouseMove(MouseMoveArgs { x: *x, y: *y }),
        Command::MouseUp { x, y, button } => Request::MouseUp(MouseButtonArgs {
            x: *x,
            y: *y,
            button: parse_mouse_button(button),
        }),
        Command::KeyDown { key } => Request::KeyDown(KeyArgs {
            operation: Default::default(),
            key: key.clone(),
        }),
        Command::KeyUp { key } => Request::KeyUp(KeyArgs {
            operation: Default::default(),
            key: key.clone(),
        }),
        Command::ReleaseAllInputs => Request::ReleaseAllInputs,
        Command::Scroll { target } => Request::Scroll(TargetArgs {
            operation: Default::default(),
            target: target.clone(),
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
                arguments: serde_json::from_str(args).unwrap_or_else(|error| {
                    eprintln!("Invalid --args: {error}");
                    std::process::exit(2)
                }),
                raw: *raw,
                code: code.clone(),
                file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
                max_size: *max_size,
                no_await: *no_await,
            };
            if *no_return {
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
            arguments: serde_json::from_str(args).unwrap_or_else(|error| {
                eprintln!("Invalid --args: {error}");
                std::process::exit(2)
            }),
            raw: *raw,
            code: code.clone(),
            file: file.as_ref().map(|path| path.to_string_lossy().to_string()),
            max_size: None,
            no_await: *no_await,
        }),
        Command::Frames => Request::Frames,
        Command::FrameEval { frame, code, file } => Request::FrameEval(FrameEvalArgs {
            frame: frame.clone(),
            code: code.clone(),
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
            url: url.clone(),
            method: method.clone(),
            headers: headers.as_ref().map(|h| parse_headers_json(h)),
            body: body.clone(),
            redirect: redirect.clone(),
            body_only: *body_only,
            max_body: *max_body,
        }),
        Command::Cookies => Request::Cookies,
        Command::SetCookie {
            name,
            value,
            domain,
            path,
        } => Request::SetCookie(SetCookieArgs {
            name: name.clone(),
            value: value.clone(),
            domain: domain.clone(),
            path: path.clone(),
        }),
        Command::DeleteCookie { name, domain } => Request::DeleteCookie(DeleteCookieArgs {
            name: name.clone(),
            domain: domain.clone(),
        }),
        Command::ClearCookies => Request::ClearCookies,
        Command::Storage {
            key,
            session_storage,
        } => Request::Storage(StorageArgs {
            key: key.clone(),
            session_storage: *session_storage,
        }),
        Command::SetStorage {
            key,
            value,
            session_storage,
        } => Request::SetStorage(SetStorageArgs {
            key: key.clone(),
            value: value.clone(),
            session_storage: *session_storage,
        }),
        Command::DumpStorage => Request::DumpStorage,
        Command::SaveState { path } => Request::SaveState(PathArgs {
            path: path.to_string_lossy().to_string(),
        }),
        Command::LoadState { path, no_navigate } => Request::LoadState(LoadStateArgs {
            path: path.to_string_lossy().to_string(),
            no_navigate: *no_navigate,
        }),
        Command::Headers { headers_json } => Request::Headers(HeadersArgs {
            headers_json: headers_json.clone(),
        }),
        Command::Console { clear, level } => Request::Console(ConsoleArgs {
            clear: *clear,
            level: level.clone(),
        }),
        Command::Errors { clear } => Request::Errors(ClearFlagArgs { clear: *clear }),
        Command::Tab { action } => match action {
            TabAction::List => Request::TabList,
            TabAction::New { url } => Request::TabNew(TabNewArgs { url: url.clone() }),
            TabAction::Switch { tab_id } => Request::TabSwitch(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
            TabAction::Close { tab_id } => Request::TabClose(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
            TabAction::Attach { tab_id } => Request::TabAttach(TabIdArgs {
                tab_id: tab_id.clone(),
            }),
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
            ms: *ms,
            text: text.clone(),
            url: url.clone(),
            load: load.clone(),
            selector: selector.clone(),
            state: state.as_deref().map(|state| match state {
                "attached" => eoka_protocol::WaitSelectorState::Attached,
                "hidden" => eoka_protocol::WaitSelectorState::Hidden,
                _ => eoka_protocol::WaitSelectorState::Visible,
            }),
            timeout: *timeout,
        }),
        Command::SpaInfo => Request::SpaInfo,
        Command::SpaNavigate { path } => {
            Request::SpaNavigate(PathStringArgs { path: path.clone() })
        }
        Command::FakeCamera { file, loop_video } => Request::FakeCamera(FakeCameraArgs {
            file: file.to_string_lossy().to_string(),
            loop_video: *loop_video,
        }),
        Command::Wasm { action } => match action {
            WasmAction::Info => Request::WasmInfo,
            WasmAction::Read { addr, len, memory } => Request::WasmRead(WasmReadArgs {
                addr: addr.clone(),
                len: *len,
                memory: memory.clone(),
            }),
            WasmAction::Write { addr, hex, memory } => Request::WasmWrite(WasmWriteArgs {
                addr: addr.clone(),
                hex: hex.clone(),
                memory: memory.clone(),
            }),
            WasmAction::Find {
                pattern,
                start,
                end,
                max,
                memory,
            } => Request::WasmFind(WasmFindArgs {
                pattern: pattern.clone(),
                start: start.clone(),
                end: end.clone(),
                max: *max,
                memory: memory.clone(),
            }),
        },

        Command::Network { action } => network_action_to_request(action),
        Command::Js { action } => match action {
            JsAction::Mode { mode } => Request::JsMode(ModeArgs { mode: mode.clone() }),
            JsAction::Allow { domain } => Request::JsAllow(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::Block { domain } => Request::JsBlock(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::Remove { domain } => Request::JsRemove(DomainArgs {
                domain: domain.clone(),
            }),
            JsAction::List => Request::JsList,
        },
        Command::Close => Request::Close,
        Command::CloneFrom { source, to } => Request::CloneFrom(CloneFromArgs {
            source: source.clone(),
            to: to.as_ref().map(|path| path.to_string_lossy().to_string()),
        }),
        Command::Captcha {
            action:
                CaptchaAction::Datadome {
                    max_attempts,
                    timeout_ms,
                },
        } => Request::CaptchaDatadome(CaptchaDatadomeArgs {
            max_attempts: *max_attempts,
            timeout_ms: *timeout_ms,
        }),
        Command::Captcha {
            action: CaptchaAction::Turnstile { timeout_ms },
        } => Request::CaptchaTurnstile(CaptchaTurnstileArgs {
            timeout_ms: *timeout_ms,
        }),
        Command::Captcha {
            action:
                CaptchaAction::Inject {
                    token,
                    captcha_type,
                    callback,
                    click_after,
                },
        } => captcha_inject_request(
            token,
            captcha_type,
            callback.as_deref(),
            click_after.as_deref(),
        ),
        Command::Captcha {
            action: CaptchaAction::Solve(_),
        }
        | Command::Sessions
        | Command::Status
        | Command::Doctor
        | Command::Kill
        | Command::Update(_)
        | Command::CdpUrl { .. }
        | Command::Batch { .. } => {
            unreachable!(
                "BUG: command should have been handled before command_to_request — see main()'s early-dispatch match"
            )
        }
    }
}

/// Parse `X,Y,WIDTH,HEIGHT`; a malformed value becomes an empty region, which
/// the engine rejects with the usual validation error.
fn parse_clip(value: &str) -> eoka_protocol::sidecar::ScreenshotClip {
    let numbers: Vec<f64> = value
        .split(',')
        .map(|part| part.trim().parse().unwrap_or(f64::NAN))
        .collect();
    let at = |index: usize| match numbers.len() {
        4 => numbers[index],
        _ => f64::NAN,
    };
    eoka_protocol::sidecar::ScreenshotClip {
        x: at(0),
        y: at(1),
        width: at(2),
        height: at(3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::test_support::parsed_command;
    use crate::cli::Cli;
    use clap::Parser;
    use eoka_protocol::operation_by_cmd;
    use serde_json::json;

    fn assert_command_maps_to_cataloged_operation(args: &[&str]) -> &'static str {
        let (_cli, command) = parsed_command(args);
        let request = command_to_request(&command, false);
        assert!(
            operation_by_cmd(request.cmd()).is_some(),
            "command {:?} mapped to uncataloged protocol command {}",
            args,
            request.cmd()
        );
        request.cmd()
    }

    #[test]
    fn click_accepts_bracketed_observe_target_at_cli_layer() {
        let (_cli, command) = parsed_command(&["eoka", "click", "[38]"]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "click");
        assert_eq!(request.args_json()["target"], "[38]");
    }

    #[test]
    fn fetch_body_only_sets_body_only_and_preserves_max_body() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "fetch",
            "https://example.com/app.js",
            "--body-only",
            "--max-body",
            "16",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "fetch");
        assert_eq!(request.args_json()["body_only"], true);
        assert_eq!(request.args_json()["max_body"], 16);
    }

    #[test]
    fn open_load_state_maps_to_daemon_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "open",
            "/camping/campsites/71576",
            "--load-state",
            "auth.json",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "open");
        assert_eq!(request.args_json()["url"], "/camping/campsites/71576");
        assert_eq!(request.args_json()["load_state"], "auth.json");
    }

    #[test]
    fn agent_mode_requests_structured_observe() {
        let (_cli, command) = parsed_command(&["eoka", "observe"]);
        let request = command_to_request(&command, true);

        assert_eq!(request.cmd(), "observe");
        assert_eq!(request.args_json()["structured"], true);
    }

    #[test]
    fn turnstile_maps_to_catalog_operation_with_timeout() {
        let (_, command) =
            parsed_command(&["eoka", "captcha", "turnstile", "--timeout-ms", "5000"]);
        let request = command_to_request(&command, false);
        assert_eq!(request.cmd(), "captcha_turnstile");
        assert_eq!(request.args_json(), json!({"timeout_ms":5000}));
        assert!(
            Cli::try_parse_from(["eoka", "captcha", "turnstile", "--timeout-ms", "0"]).is_err()
        );
    }

    #[test]
    fn datadome_maps_to_existing_session_operation() {
        let (_, command) = parsed_command(&["eoka", "captcha", "datadome"]);
        let request = command_to_request(&command, false);
        assert_eq!(request.cmd(), "captcha_datadome");
        assert_eq!(
            request.args_json(),
            json!({"max_attempts":1,"timeout_ms":60000})
        );
        let (_, command) = parsed_command(&[
            "eoka",
            "captcha",
            "datadome",
            "--max-attempts",
            "2",
            "--timeout-ms",
            "5000",
        ]);
        assert_eq!(
            command_to_request(&command, false).args_json(),
            json!({"max_attempts":2,"timeout_ms":5000})
        );
        for extra in [
            ["--max-attempts", "0"],
            ["--max-attempts", "5"],
            ["--timeout-ms", "4999"],
            ["--timeout-ms", "120001"],
            ["--api-key", "unused"],
        ] {
            assert!(
                Cli::try_parse_from(["eoka", "captcha", "datadome", extra[0], extra[1]]).is_err()
            );
        }
    }

    #[test]
    fn captcha_inject_maps_to_daemon_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "captcha",
            "inject",
            "token-123",
            "--captcha-type",
            "recaptcha",
            "--callback",
            "window.onCaptcha",
            "--click-after",
            "text:Continue Booking",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "captcha_inject");
        assert_eq!(request.args_json()["token"], "token-123");
        assert_eq!(request.args_json()["captcha_type"], "recaptcha");
        assert_eq!(request.args_json()["callback"], "window.onCaptcha");
        assert_eq!(request.args_json()["click_after"], "text:Continue Booking");
    }

    #[test]
    fn network_intercept_subcommand_accepts_trailing_json_flag() {
        let (cli, command) =
            parsed_command(&["eoka", "network", "intercept", "add", "*api*", "--json"]);
        let request = command_to_request(&command, false);

        assert!(cli.json);
        assert_eq!(request.cmd(), "intercept_add");
        assert_eq!(request.args_json()["url_pattern"], "*api*");
    }

    #[test]
    fn network_record_start_clear_maps_to_typed_request() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "record",
            "start",
            "--pattern",
            "*/api/*",
            "--clear",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "network_record_start");
        assert_eq!(request.args_json()["patterns"], json!(["*/api/*"]));
        assert_eq!(request.args_json()["clear"], true);
    }

    #[test]
    fn network_wait_defaults_to_new_entries() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "wait",
            "--pattern",
            "*/api/*",
            "--status",
            "200",
            "--timeout",
            "5000",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "network_wait");
        assert_eq!(request.args_json()["pattern"], "*/api/*");
        assert_eq!(request.args_json()["status"], 200);
        assert_eq!(request.args_json()["timeout"], 5000);
        assert_eq!(request.args_json()["include_existing"], false);
    }

    #[test]
    fn network_export_json_resolves_path_and_format() {
        let (_cli, command) = parsed_command(&[
            "eoka",
            "network",
            "export",
            "capture.json",
            "--format",
            "json",
        ]);
        let request = command_to_request(&command, false);

        assert_eq!(request.cmd(), "network_export");
        assert_eq!(request.args_json()["format"], "json");
        assert!(request.args_json()["path"]
            .as_str()
            .unwrap()
            .ends_with("capture.json"));
    }

    #[test]
    fn top_level_intercept_is_rejected() {
        let err = match Cli::try_parse_from(["eoka", "intercept", "add", "*api*", "--json"]) {
            Ok(_) => panic!("top-level intercept should be rejected"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("unrecognized subcommand"));
    }

    #[test]
    fn daemon_backed_cli_commands_map_to_cataloged_protocol_operations() {
        let commands: &[&[&str]] = &[
            &["eoka", "open", "https://example.com"],
            &["eoka", "back"],
            &["eoka", "forward"],
            &["eoka", "reload"],
            &["eoka", "snapshot"],
            &["eoka", "observe"],
            &["eoka", "screenshot"],
            &["eoka", "emulate"],
            &["eoka", "info"],
            &["eoka", "frames"],
            &["eoka", "frame-eval", "id:fixture-frame", "1+1"],
            &["eoka", "text"],
            &["eoka", "find", "Submit"],
            &["eoka", "content"],
            &[
                "eoka",
                "screenshot",
                "--format",
                "jpeg",
                "--quality",
                "80",
                "--clip",
                "0,0,10,20",
            ],
            &["eoka", "recording", "start", "out.mp4", "--fps", "30"],
            &[
                "eoka",
                "recording",
                "status",
                "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb",
            ],
            &[
                "eoka",
                "recording",
                "stop",
                "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb",
            ],
            &[
                "eoka",
                "recording",
                "cancel",
                "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb",
            ],
            &["eoka", "get-text", "h1"],
            &["eoka", "get-attribute", "a", "href"],
            &["eoka", "exists", "#save"],
            &["eoka", "click-shadow", "button", "--frame", "F1"],
            &["eoka", "wait", "--selector", "#ready", "--state", "hidden"],
            &["eoka", "click", "Submit"],
            &["eoka", "dblclick", "Submit"],
            &["eoka", "fill", "Email", "ada@example.com"],
            &["eoka", "select", "Country", "CA"],
            &["eoka", "hover", "Menu"],
            &["eoka", "key", "Enter"],
            &["eoka", "mouse-down", "10", "20"],
            &["eoka", "mouse-move", "10", "20"],
            &["eoka", "mouse-up", "10", "20", "--button", "right"],
            &["eoka", "key-down", "Shift"],
            &["eoka", "key-up", "Shift"],
            &["eoka", "release-all-inputs"],
            &["eoka", "scroll", "down"],
            &["eoka", "eval", "1 + 1"],
            &["eoka", "exec", "window.clicked = true"],
            &["eoka", "fetch", "https://example.com/api"],
            &["eoka", "cookies"],
            &["eoka", "set-cookie", "sid", "123"],
            &["eoka", "delete-cookie", "sid"],
            &["eoka", "clear-cookies"],
            &["eoka", "storage"],
            &["eoka", "set-storage", "token", "abc"],
            &["eoka", "dump-storage"],
            &["eoka", "save-state", "state.json"],
            &["eoka", "load-state", "state.json"],
            &["eoka", "headers", r#"{"x-test":"1"}"#],
            &["eoka", "console"],
            &["eoka", "errors"],
            &["eoka", "tab", "list"],
            &["eoka", "tab", "new", "https://example.com"],
            &["eoka", "tab", "switch", "tab-1"],
            &["eoka", "tab", "close", "tab-1"],
            &["eoka", "tab", "attach", "tab-1"],
            &["eoka", "wait", "100"],
            &["eoka", "fake-camera", "camera.mp4"],
            &["eoka", "wasm", "info"],
            &["eoka", "wasm", "read", "0x10", "4"],
            &["eoka", "wasm", "write", "0x10", "deadbeef"],
            &["eoka", "wasm", "find", "deadbeef"],
            &["eoka", "js", "mode", "block-all"],
            &["eoka", "js", "allow", "example.com"],
            &["eoka", "js", "block", "ads.example"],
            &["eoka", "js", "remove", "example.com"],
            &["eoka", "js", "list"],
            &["eoka", "spa-info"],
            &["eoka", "spa-navigate", "/account"],
            &["eoka", "captcha", "inject", "token"],
            &["eoka", "captcha", "turnstile"],
            &["eoka", "network", "record", "start"],
            &["eoka", "network", "record", "stop"],
            &["eoka", "network", "record", "status"],
            &["eoka", "network", "log"],
            &["eoka", "network", "show", "1"],
            &["eoka", "network", "wait"],
            &["eoka", "network", "har", "capture.har"],
            &["eoka", "network", "export", "capture.har"],
            &["eoka", "network", "clear"],
            &["eoka", "network", "intercept", "add", "*/api/*"],
            &["eoka", "network", "intercept", "list"],
            &["eoka", "network", "intercept", "remove", "1"],
            &["eoka", "network", "intercept", "log"],
            &["eoka", "clone-from", "9222"],
            &["eoka", "close"],
            &["eoka", "view"],
            &[
                "eoka",
                "step",
                r#"{"action":{"kind":"press_key","key":"Tab"}}"#,
            ],
            &[
                "eoka",
                "action",
                r#"{"operation_id":"a116d158-2cc5-4473-b73c-616b4b73cb0e","action":{"kind":"reload"}}"#,
            ],
            &["eoka", "operations", "get", "operation"],
            &["eoka", "operations", "list"],
            &["eoka", "operations", "wait", "operation"],
            &["eoka", "operations", "cancel", "operation"],
            &[
                "eoka",
                "download",
                "--url",
                "https://example.com/file",
                "--out",
                "/tmp/file",
            ],
            &["eoka", "download", "list"],
            &["eoka", "download", "status", "job"],
            &["eoka", "download", "wait", "job"],
            &["eoka", "download", "cancel", "job"],
            &["eoka", "captcha", "datadome"],
        ];

        let reached: std::collections::BTreeSet<&str> = commands
            .iter()
            .map(|args| assert_command_maps_to_cataloged_operation(args))
            .collect();

        // The catalog is the single list of operations: anything added to it
        // must be reachable from the CLI. These are the only exceptions, each
        // reached another way.
        let not_cli_commands = [
            "session_hello",  // sent by every client connection, not a command
            "action_command", // envelope `main` wraps around back/forward/reload
            "shutdown",       // `eoka kill` signals the daemon instead
        ];
        let unreachable: Vec<&str> = eoka_protocol::all_operations()
            .iter()
            .map(|operation| operation.cmd)
            .filter(|cmd| !reached.contains(cmd) && !not_cli_commands.contains(cmd))
            .collect();
        assert!(
            unreachable.is_empty(),
            "catalog operations with no CLI command (add one in cli.rs and list it here): {unreachable:?}"
        );
    }
}
