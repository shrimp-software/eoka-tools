use super::EokaServer;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{RoleClient, RunningService};
use rmcp::ServiceExt;
use serde_json::{json, Value};

pub(super) type Client = RunningService<RoleClient, ()>;

pub(super) async fn connect(server: EokaServer) -> Client {
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        if let Ok(service) = server.serve(server_io).await {
            let _ = service.waiting().await;
        }
    });
    ().serve(client_io).await.expect("the client connects")
}

pub(super) async fn call(
    client: &Client,
    tool: &str,
    arguments: Value,
) -> Result<CallToolResult, String> {
    let mut params = CallToolRequestParams::new(tool.to_owned());
    if let Value::Object(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    client
        .call_tool(params)
        .await
        .map_err(|error| error.to_string())
}

pub(super) fn text(result: &CallToolResult) -> String {
    serde_json::to_value(&result.content)
        .ok()
        .and_then(|content| content[0]["text"].as_str().map(str::to_owned))
        .unwrap_or_else(|| serde_json::to_string(&result.content).unwrap_or_default())
}

fn changed_lines(expected: &str, actual: &str) -> String {
    let short = |line: &str| line.chars().take(240).collect::<String>();
    let removed = expected
        .lines()
        .filter(|line| !actual.lines().any(|other| other == *line))
        .map(|line| format!("- {}", short(line)));
    let added = actual
        .lines()
        .filter(|line| !expected.lines().any(|other| other == *line))
        .map(|line| format!("+ {}", short(line)));
    removed.chain(added).collect::<Vec<_>>().join("\n")
}

pub(super) fn assert_locked(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/locks")
        .join(name);
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        expected == actual,
        "{name} no longer matches what MCP clients see. If the change is intended, rerun with EOKA_UPDATE_LOCKS=1 and list it in the pull request.\n{}",
        changed_lines(&expected, actual)
    );
}

fn tool_lines(tools: &[Tool]) -> Vec<String> {
    let mut lines: Vec<String> = tools
        .iter()
        .map(|tool| serde_json::to_value(tool).unwrap().to_string())
        .collect();
    lines.sort();
    lines
}

async fn listed(include_opt_in: bool) -> Vec<Tool> {
    let client = connect(EokaServer::listing(include_opt_in)).await;
    let tools = client.list_all_tools().await.unwrap();
    let _ = client.cancel().await;
    tools
}

#[tokio::test]
async fn the_tool_list_a_client_sees_is_locked() {
    let every = tool_lines(&listed(true).await);
    assert_locked("tools.jsonl", &(every.join("\n") + "\n"));
    let default = listed(false).await;
    for line in tool_lines(&default) {
        assert!(
            every.contains(&line),
            "a default tool is defined differently when every tool is listed: {line}"
        );
    }
    let mut names: Vec<&str> = default.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    assert_locked("tools.default.txt", &(names.join("\n") + "\n"));
}

const PAGE: &str = "<!doctype html><title>Fixture</title><h1 id=\"h\">Heading</h1><p>Body text</p><a href=\"/next\">Next</a><button id=\"b\">Press</button><input id=\"i\" placeholder=\"Name\"><script>localStorage.setItem('k','v');sessionStorage.setItem('s','t');document.cookie='c=1; path=/';console.log('hello');</script>";

async fn fixture() -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let served = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => request.extend_from_slice(&chunk[..read]),
                    }
                }
                let request = String::from_utf8_lossy(&request).into_owned();
                let path = request.split_whitespace().nth(1).unwrap_or("/");
                let (kind, body) = if path.starts_with("/data") {
                    ("application/json", "{\"ok\":true}")
                } else {
                    ("text/html; charset=utf-8", PAGE)
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    (origin, served)
}

fn is_uuid(candidate: &str) -> bool {
    candidate.len() == 36
        && candidate.char_indices().all(|(index, character)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

fn without_identifiers(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        let window: String = characters[index..(index + 36).min(characters.len())]
            .iter()
            .collect();
        if is_uuid(&window) {
            out.push_str("<uuid>");
            index += 36;
            continue;
        }
        let hex: String = characters[index..]
            .iter()
            .take_while(|character| character.is_ascii_hexdigit())
            .collect();
        if hex.len() == 32 {
            out.push_str("<id>");
            index += 32;
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

fn volatile(key: &str) -> bool {
    key.ends_with("_ms")
        || key.ends_with("_at")
        || key.ends_with("Ms")
        || matches!(
            key,
            "elapsed"
                | "duration"
                | "timestamp"
                | "time"
                | "expires"
                | "size"
                | "age"
                | "x"
                | "y"
                | "width"
                | "height"
        )
}

fn without_timings(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                if volatile(key) && entry.is_number() {
                    *entry = json!(0);
                } else {
                    without_timings(entry);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(without_timings),
        _ => {}
    }
}

fn located(text: &str, origin: &str, scratch: &str) -> String {
    let host = origin.trim_start_matches("http://");
    without_identifiers(
        &text
            .replace(origin, "http://fixture")
            .replace(host, "fixture")
            .replace(scratch, "<scratch>"),
    )
}

fn stable(reply: &str, origin: &str, scratch: &str) -> Value {
    let reply = located(reply, origin, scratch);
    match serde_json::from_str::<Value>(&reply) {
        Ok(mut parsed) if parsed.is_object() || parsed.is_array() => {
            without_timings(&mut parsed);
            if let Value::Array(items) = &mut parsed {
                items.sort_by_key(|item| item["name"].as_str().map(str::to_owned));
            }
            parsed
        }
        _ => Value::String(reply),
    }
}

#[tokio::test]
#[ignore = "requires Chrome; records what every engine-backed MCP tool replies"]
async fn every_mcp_tool_answers() {
    let (origin, served) = fixture().await;
    let scratch = std::env::temp_dir().join(format!("eoka-mcp-surface-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let scratch_text = scratch.to_string_lossy().into_owned();
    let state = scratch.join("state.json").to_string_lossy().into_owned();
    let client = connect(EokaServer::listing(false)).await;
    let steps: Vec<(&str, Value)> = vec![
        ("page_info", json!({})),
        ("navigate", json!({"url": format!("{origin}/")})),
        ("page_info", json!({})),
        ("page_text", json!({})),
        ("find_text", json!({"text": "Heading"})),
        ("find_text", json!({"text": "Next"})),
        ("find_text", json!({"text": "Absent"})),
        ("snapshot", json!({})),
        ("observe", json!({})),
        ("observe", json!({"structured": true, "max": 2})),
        ("spa_info", json!({})),
        ("cookies", json!({})),
        ("set_cookie", json!({"name": "added", "value": "yes"})),
        (
            "set_cookie",
            json!({"name": "scoped", "value": "1", "domain": "127.0.0.1", "path": "/"}),
        ),
        ("set_cookie", json!({"value": "missing name"})),
        ("cookies", json!({})),
        ("delete_cookie", json!({"name": "added"})),
        ("delete_cookie", json!({"name": "never-set"})),
        ("clear_cookies", json!({})),
        ("cookies", json!({})),
        ("local_storage_get", json!({"key": "k"})),
        ("local_storage_get", json!({"key": "absent"})),
        ("local_storage_get", json!({})),
        ("local_storage_set", json!({"key": "k2", "value": "v2"})),
        ("local_storage_get", json!({"key": "k2"})),
        ("session_storage_get", json!({"key": "s"})),
        ("session_storage_get", json!({"key": "absent"})),
        ("session_storage_set", json!({"key": "s2", "value": "t2"})),
        ("dump_storage", json!({})),
        ("console", json!({})),
        ("console", json!({"level": "error", "clear": true})),
        ("errors", json!({})),
        ("errors", json!({"clear": true})),
        ("detect_captcha", json!({})),
        ("wait_ms", json!({"ms": 20})),
        (
            "wait_for_text",
            json!({"text": "heading", "timeout_ms": 3000}),
        ),
        (
            "wait_for_text",
            json!({"text": "never shown", "timeout_ms": 300}),
        ),
        ("scroll", json!({"target": "css:#b"})),
        ("scroll", json!({"target": "down"})),
        ("mouse_move", json!({"x": 5, "y": 5})),
        ("mouse_down", json!({"x": 5, "y": 5, "button": "left"})),
        ("mouse_up", json!({"x": 5, "y": 5, "button": "left"})),
        ("mouse_down", json!({"x": 5, "y": 5})),
        ("key_down", json!({"key": "Shift"})),
        ("key_up", json!({"key": "Shift"})),
        ("release_all_inputs", json!({})),
        ("fetch", json!({"url": format!("{origin}/data")})),
        (
            "fetch",
            json!({"url": format!("{origin}/data"), "body_only": true}),
        ),
        ("save_state", json!({"path": state})),
        ("spa_navigate", json!({"path": "/next"})),
        ("page_info", json!({})),
        ("content", json!({})),
        ("get_text", json!({"selector": "#h"})),
        ("get_text", json!({"selector": "#absent"})),
        ("exists", json!({"selector": "#h"})),
        ("close", json!({})),
        ("page_text", json!({})),
    ];
    let mut transcript = String::new();
    for (tool, arguments) in steps {
        let outcome = call(&client, tool, arguments.clone()).await;
        let (answered, reply) = match &outcome {
            Ok(result) if result.is_error == Some(true) => ("tool_error", text(result)),
            Ok(result) => ("ok", text(result)),
            Err(error) => ("protocol_error", error.clone()),
        };
        let arguments: Value =
            serde_json::from_str(&located(&arguments.to_string(), &origin, &scratch_text)).unwrap();
        let line = json!({
            "tool": tool,
            "arguments": arguments,
            "answered": answered,
            "reply": stable(&reply, &origin, &scratch_text),
        });
        transcript.push_str(&line.to_string());
        transcript.push('\n');
    }
    let _ = client.cancel().await;
    served.abort();
    let _ = std::fs::remove_dir_all(&scratch);
    assert_locked("replies.jsonl", &transcript);
}

#[tokio::test]
#[ignore = "requires Chrome; an operation the engine admitted is cancelled from an MCP tool"]
async fn an_operation_started_by_the_engine_can_be_cancelled_from_mcp() {
    use eoka_server::eoka::{Action, ActionOptions, Expectation, LocatorSpec};
    let server = EokaServer::listing(false);
    let client = connect(server.clone()).await;
    call(
        &client,
        "navigate",
        json!({"url": "data:text/html,<title>one</title><button id='b'>Press</button>"}),
    )
    .await
    .unwrap();
    let (page, engine_operations) = {
        let guard = server.state.lock().await;
        let state = guard.as_ref().unwrap();
        (
            state.current_tab().unwrap().page.clone(),
            state.server.operations.clone(),
        )
    };
    let id = uuid::Uuid::new_v4().to_string();
    let mut options: ActionOptions = serde_json::from_value(json!({"timeout_ms": 20000})).unwrap();
    options.expect = Some(Expectation::Text {
        target: LocatorSpec::css("#never"),
        value: "never".into(),
        contains: false,
    });
    engine_operations
        .submit(
            page,
            id.clone(),
            Action::Click {
                target: LocatorSpec::css("#b"),
                human: false,
                double: false,
            },
            options,
        )
        .await
        .unwrap();
    let cancelled = call(&client, "operation_cancel", json!({"id": id}))
        .await
        .unwrap();
    assert_ne!(cancelled.is_error, Some(true), "{}", text(&cancelled));
    let waited = call(
        &client,
        "operation_wait",
        json!({"id": id, "timeout_ms": 5000}),
    )
    .await
    .unwrap();
    assert!(
        !text(&waited).contains("\"state\":\"running\""),
        "{}",
        text(&waited)
    );
    call(&client, "close", json!({})).await.unwrap();
    let _ = client.cancel().await;
}
