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
fn new_command_flags_reach_their_requests() {
    let args = |argv: &[&str]| {
        let (_cli, command) = parsed_command(argv);
        let request = command_to_request(&command, false);
        (request.cmd(), request.args_json())
    };
    let (cmd, shot) = args(&[
        "eoka",
        "screenshot",
        "--format",
        "webp",
        "--quality",
        "80",
        "--clip",
        "1,2,30,40",
    ]);
    assert_eq!(cmd, "screenshot");
    assert_eq!(shot["format"], "webp");
    assert_eq!(shot["quality"], 80);
    assert_eq!(
        shot["clip"],
        serde_json::json!({"x":1.0,"y":2.0,"width":30.0,"height":40.0})
    );
    assert_eq!(
        args(&["eoka", "screenshot", "--full-page"]).1["full_page"],
        true
    );

    let (cmd, wait) = args(&[
        "eoka",
        "wait",
        "--selector",
        "#x",
        "--state",
        "hidden",
        "-t",
        "9",
    ]);
    assert_eq!(cmd, "wait");
    assert_eq!(wait["selector"], "#x");
    assert_eq!(wait["state"], "hidden");
    assert_eq!(wait["timeout"], 9);

    let (cmd, start) = args(&[
        "eoka",
        "recording",
        "start",
        "/tmp/a.mp4",
        "--fps",
        "24",
        "--max-width",
        "640",
    ]);
    assert_eq!(cmd, "recording_start");
    assert_eq!(start["path"], "/tmp/a.mp4");
    assert_eq!(start["fps"], 24);
    assert_eq!(start["max_width"], 640);

    let (cmd, shadow) = args(&[
        "eoka",
        "click-shadow",
        "b",
        "--frame",
        "F",
        "--timeout-ms",
        "5",
    ]);
    assert_eq!(cmd, "click_shadow");
    assert_eq!(shadow["frame_id"], "F");
    assert_eq!(shadow["timeout_ms"], 5);
    assert_eq!(
        args(&["eoka", "dialog", "accept", "--prompt-text", "hi"]).1["prompt_text"],
        "hi"
    );

    for malformed in ["1,2,3", "a,b,c,d", "0,0,0,10", "-1,0,5,5", "0,0,inf,5"] {
        assert!(
            Cli::try_parse_from(["eoka", "screenshot", "--clip", malformed]).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn derived_cli_args_default_like_protocol_input() {
    let cases: &[(&[&str], serde_json::Value)] = &[
        (&["eoka", "view"], json!({})),
        (&["eoka", "snapshot"], json!({})),
        (&["eoka", "emulate"], json!({})),
        (&["eoka", "click-shadow", "b"], json!({"selector": "b"})),
        (&["eoka", "storage"], json!({})),
        (&["eoka", "console"], json!({})),
        (&["eoka", "errors"], json!({})),
        (&["eoka", "tab", "new"], json!({})),
        (&["eoka", "dialog", "accept"], json!({})),
        (&["eoka", "wasm", "find", "ff"], json!({"pattern": "ff"})),
        (&["eoka", "network", "record", "start"], json!({})),
        (&["eoka", "network", "log"], json!({})),
        (&["eoka", "network", "show", "1"], json!({"id": 1})),
        (&["eoka", "network", "wait"], json!({})),
        (&["eoka", "operations", "wait", "op"], json!({"id": "op"})),
        (&["eoka", "captcha", "datadome"], json!({})),
        (&["eoka", "captcha", "turnstile"], json!({})),
        (&["eoka", "hover", "Menu"], json!({"target": "Menu"})),
    ];
    for (argv, input) in cases {
        let (_cli, command) = parsed_command(argv);
        let request = command_to_request(&command, false);
        let protocol = eoka_protocol::request_from_cmd(request.cmd(), input.clone())
            .unwrap_or_else(|error| panic!("{argv:?}: {error}"));
        assert_eq!(request.args_json(), protocol.args_json(), "{argv:?}");
    }
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
    let (_, command) = parsed_command(&["eoka", "captcha", "turnstile", "--timeout-ms", "5000"]);
    let request = command_to_request(&command, false);
    assert_eq!(request.cmd(), "captcha_turnstile");
    assert_eq!(request.args_json(), json!({"timeout_ms":5000}));
    assert!(Cli::try_parse_from(["eoka", "captcha", "turnstile", "--timeout-ms", "0"]).is_err());
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
        assert!(Cli::try_parse_from(["eoka", "captcha", "datadome", extra[0], extra[1]]).is_err());
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

fn subcommands(args: &[&str]) -> String {
    use clap::CommandFactory;
    let mut command = Cli::command();
    let mut words = Vec::new();
    for word in &args[1..] {
        let Some(next) = command.find_subcommand(word).cloned() else {
            break;
        };
        words.push(next.get_name().to_owned());
        command = next;
    }
    words.join(" ")
}

#[test]
fn hints_spell_every_command_the_way_the_cli_does() {
    let mut misspelled = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for args in DAEMON_COMMANDS {
        let cmd = assert_command_maps_to_cataloged_operation(args);
        if !seen.insert(cmd) {
            continue;
        }
        let named = operation_by_cmd(cmd).unwrap().cli_command();
        let real = subcommands(args);
        if named != real {
            misspelled.push(format!(
                "{cmd}: hints say `eoka {named}`, the CLI is `eoka {real}`"
            ));
        }
    }
    assert!(misspelled.is_empty(), "{}", misspelled.join("\n"));
}

const DAEMON_COMMANDS: &[&[&str]] = &[
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
    &["eoka", "dialog", "accept", "--prompt-text", "yes"],
    &["eoka", "dialog", "dismiss"],
    &["eoka", "captcha", "detect"],
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

#[test]
fn daemon_backed_cli_commands_map_to_cataloged_protocol_operations() {
    let commands = DAEMON_COMMANDS;
    let reached: std::collections::BTreeSet<&str> = commands
        .iter()
        .map(|args| assert_command_maps_to_cataloged_operation(args))
        .collect();

    let not_cli_commands = [
        "session_hello",
        "action_command",
        "action_batch",
        "history_go",
        "shutdown",
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
