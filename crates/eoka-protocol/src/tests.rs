use super::*;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};

fn temp_socket_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "eoka-protocol-test-{}-{}.sock",
        std::process::id(),
        name
    ))
}

#[tokio::test]
async fn round_trip_over_unix_socket() {
    let sock_path = temp_socket_path("roundtrip");
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, mut writer) = stream.into_split();
        let req: Request = read_msg(&mut reader).await.unwrap();
        assert_eq!(req.cmd(), "click");
        assert_eq!(req.args_json(), serde_json::json!({ "target": "@e1" }));
        write_msg(&mut writer, &Response::ok_text("pong"))
            .await
            .unwrap();
    });

    let stream = UnixStream::connect(&sock_path).await.unwrap();
    let (mut reader, mut writer) = stream.into_split();
    write_msg(
        &mut writer,
        &Request::Click(TargetArgs {
            operation: Default::default(),
            target: "@e1".into(),
        }),
    )
    .await
    .unwrap();
    let response: Response = read_msg(&mut reader).await.unwrap();

    server.await.unwrap();
    let _ = std::fs::remove_file(&sock_path);

    assert!(response.ok);
    assert_eq!(
        response.data,
        Some(serde_json::Value::String("pong".into()))
    );
    assert_eq!(response.error, None);
}

#[tokio::test]
async fn read_msg_rejects_oversized_length_prefix() {
    let sock_path = temp_socket_path("oversized");
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path).unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (mut reader, _writer) = stream.into_split();
        let result: std::io::Result<Request> = read_msg(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    });

    let mut stream = UnixStream::connect(&sock_path).await.unwrap();
    let oversized_len: u32 = 65 * 1024 * 1024;
    stream
        .write_all(&oversized_len.to_be_bytes())
        .await
        .unwrap();

    server.await.unwrap();
    let _ = std::fs::remove_file(&sock_path);
}

#[test]
fn typed_network_request_round_trips_with_cmd_and_args() {
    let request = Request::NetworkWait(NetworkWaitArgs {
        pattern: Some("*/api/*".into()),
        method: Some("POST".into()),
        status: Some(201),
        timeout: Some(5000),
        since: Some(10),
        include_existing: false,
    });

    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["cmd"], "network_wait");
    assert_eq!(value["args"]["pattern"], "*/api/*");

    let parsed: Request = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.cmd(), "network_wait");
    assert_eq!(parsed.args_json()["status"], 201);
}

#[test]
fn typed_request_rejects_unknown_command() {
    let result: Result<Request, _> =
        serde_json::from_value(serde_json::json!({ "cmd": "not_real", "args": {} }));

    assert!(result.is_err());
}

#[test]
fn operation_catalog_maps_stable_paths_to_protocol_commands() {
    let tab = operation_by_path("tab.new").unwrap();
    assert_eq!(tab.cmd, "tab_new");
    assert_eq!(tab.exposure, OperationExposure::DefaultAgent);

    let request =
        request_from_operation_path("tab.new", serde_json::json!({"url":"https://example.com"}))
            .unwrap();
    assert_eq!(request.cmd(), "tab_new");
    assert_eq!(request.args_json()["url"], "https://example.com");
}

#[test]
fn default_catalog_excludes_opt_in_and_lifecycle_operations() {
    let paths: Vec<&str> = default_agent_operations()
        .map(|operation| operation.path)
        .collect();

    assert!(paths.contains(&"open"));
    assert!(paths.contains(&"mouse.down"));
    assert!(paths.contains(&"key.down"));
    assert!(paths.contains(&"release_all_inputs"));
    assert!(paths.contains(&"tab.list"));
    assert!(!paths.contains(&"network.log"));
    assert!(!paths.contains(&"close"));
}

#[test]
fn operation_catalog_has_unique_paths_and_commands() {
    let mut paths = std::collections::BTreeSet::new();
    let mut commands = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();

    for operation in all_operations() {
        assert!(ids.insert(operation.id), "duplicate id {:?}", operation.id);
        assert!(
            paths.insert(operation.path),
            "duplicate path {}",
            operation.path
        );
        assert!(
            commands.insert(operation.cmd),
            "duplicate cmd {}",
            operation.cmd
        );
        assert_eq!(operation_by_path(operation.path).unwrap().id, operation.id);
        assert_eq!(operation_by_cmd(operation.cmd).unwrap().id, operation.id);
        assert_eq!(
            request_from_operation_path(operation.path, operation.representative_input_for_test())
                .unwrap()
                .cmd(),
            operation.cmd
        );
    }
}

#[test]
fn manifest_uses_catalog_metadata_and_schemas() {
    let default_manifest = manifest_for_operations("eoka", false);
    let paths: std::collections::BTreeSet<&str> = default_manifest
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();

    assert!(paths.contains("eoka.open"));
    assert!(paths.contains("eoka.mouse.down"));
    assert!(paths.contains("eoka.key.down"));
    assert!(paths.contains("eoka.release_all_inputs"));
    assert!(paths.contains("eoka.tab.list"));
    assert!(!paths.contains("eoka.network.log"));
    assert!(!paths.contains("eoka.close"));

    let open = default_manifest
        .iter()
        .find(|entry| entry.path == "eoka.open")
        .unwrap();
    assert_eq!(open.capability, "navigation");
    assert_eq!(open.exposure, "defaultAgent");
    assert_eq!(open.tags, vec!["eoka", "navigation"]);
    assert!(open.input_schema.get("properties").is_some());

    let all_manifest = manifest_for_operations("eoka", true);
    let all_paths: std::collections::BTreeSet<&str> = all_manifest
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert!(all_paths.contains("eoka.network.log"));
    assert!(!all_paths.contains("eoka.close"));
}

#[test]
fn shared_mcp_inputs_keep_schema_descriptions() {
    let click_schema = input_schema_for_cmd("click");
    assert_eq!(
        click_schema["properties"]["target"]["description"],
        "Target element. Supports index, snapshot refs like @e1, text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search."
    );

    let tab_schema = input_schema_for_cmd("tab_switch");
    assert_eq!(
        tab_schema["properties"]["tab_id"]["description"],
        "Tab ID from list_tabs."
    );
}

impl OperationSpec {
    fn representative_input_for_test(&self) -> serde_json::Value {
        match self.cmd {
            "action_command" => {
                serde_json::json!({"operation_id":"a116d158-2cc5-4473-b73c-616b4b73cb0e","command":"reload","arguments":{}})
            }
            "agent_step" => serde_json::json!({"action":{"kind":"press_key","key":"Tab"}}),
            "action_perform" => {
                serde_json::json!({"operation_id":"a116d158-2cc5-4473-b73c-616b4b73cb0e","action":{"kind":"reload"}})
            }
            "operation_get" | "operation_wait" | "operation_cancel" => {
                serde_json::json!({"id":"operation"})
            }
            "open" => serde_json::json!({"url":"about:blank"}),
            "find" => serde_json::json!({"text":"needle"}),
            "click" | "dblclick" | "hover" | "scroll" => {
                serde_json::json!({"target":"body"})
            }
            "fill" => serde_json::json!({"target":"body","text":"value"}),
            "select" => serde_json::json!({"target":"select","value":"A"}),
            "key" | "key_down" | "key_up" => serde_json::json!({"key":"Enter"}),
            "mouse_down" | "mouse_up" => {
                serde_json::json!({"x":10.0,"y":20.0,"button":"left"})
            }
            "mouse_move" => serde_json::json!({"x":10.0,"y":20.0}),
            "eval" | "exec" => serde_json::json!({"code":"return 1"}),
            "frame_eval" => serde_json::json!({"frame":"iframe","code":"1+1"}),
            "frame_click" => serde_json::json!({"frame":"iframe","target":"button"}),
            "frame_fill" => {
                serde_json::json!({"frame":"iframe","target":"input","text":"value"})
            }
            "emulate" => serde_json::json!({"width":390,"height":844}),
            "download_start" => serde_json::json!({"click":"@e1","output":"/tmp/download"}),
            "download_get" | "download_cancel" | "download_wait" => {
                serde_json::json!({"id":"job"})
            }
            "fetch" => serde_json::json!({"url":"https://example.com"}),
            "set_cookie" => serde_json::json!({"name":"a","value":"b"}),
            "delete_cookie" => serde_json::json!({"name":"a"}),
            "storage" => serde_json::json!({"key":"a"}),
            "set_storage" => serde_json::json!({"key":"a","value":"b"}),
            "save_state" | "load_state" | "network_har" | "network_export" => {
                serde_json::json!({"path":"/tmp/eoka-state.json"})
            }
            "headers" => serde_json::json!({"headers_json":"{}"}),
            "console" | "errors" => serde_json::json!({"clear":false}),
            "tab_new" => serde_json::json!({"url":"about:blank"}),
            "tab_switch" | "tab_close" | "tab_attach" => serde_json::json!({"tab_id":"1"}),
            "wait" => serde_json::json!({"text":"ready"}),
            "spa_navigate" => serde_json::json!({"path":"/"}),
            "fake_camera" => serde_json::json!({"file":"/tmp/video.y4m"}),
            "wasm_read" => serde_json::json!({"addr":"0","len":1}),
            "wasm_write" => serde_json::json!({"addr":"0","hex":"00"}),
            "wasm_find" => serde_json::json!({"pattern":"00"}),
            "network_record_start" => serde_json::json!({"patterns":["*"]}),
            "network_log" => serde_json::json!({"limit":10}),
            "network_show" => serde_json::json!({"id":1}),
            "intercept_remove" => serde_json::json!({"id":"1"}),
            "network_wait" => serde_json::json!({"pattern":"*"}),
            "intercept_add" => serde_json::json!({"url_pattern":"*"}),
            "intercept_log" => serde_json::json!({"clear":false}),
            "js_mode" => serde_json::json!({"mode":"block"}),
            "js_allow" | "js_block" | "js_remove" => {
                serde_json::json!({"domain":"example.com"})
            }
            "clone_from" => serde_json::json!({"source":"9222"}),
            "captcha_solve" => serde_json::json!({
                "api_key":"key",
                "captcha_type":"hcaptcha",
                "website_url":"https://example.com",
                "website_key":"site"
            }),
            "captcha_detect" => serde_json::json!({}),
            "captcha_inject" => serde_json::json!({"token":"token"}),
            _ => serde_json::json!({}),
        }
    }
}
