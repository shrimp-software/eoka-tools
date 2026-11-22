use super::*;

#[tokio::test]
async fn a_request_and_its_reply_round_trip_as_sidecar_frames() {
    let (client, server) = tokio::io::duplex(1 << 16);
    let (server_read, mut server_write) = tokio::io::split(server);
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut server_read = tokio::io::BufReader::new(server_read);
    let mut client_read = tokio::io::BufReader::new(client_read);
    let click = Request::Click(ClickArgs {
        element: TargetArgs {
            operation: Default::default(),
            target: "@e1".into(),
        },
        intercept_nav: None,
    });
    wire::write_frame(&mut client_write, &wire::request(&click))
        .await
        .unwrap();
    let frame = wire::read_frame(&mut server_read).await.unwrap().unwrap();
    assert_eq!(frame["method"], "click");
    let incoming = wire::incoming(frame).ok().unwrap();
    let request = incoming.request().unwrap();
    assert_eq!(request.cmd(), "click");
    assert_eq!(request.args_json()["target"], "@e1");
    let failed = Response::err_detail(
        ErrorDetail::new("daemon_busy", "busy")
            .retryable(true)
            .hint("wait"),
    );
    wire::write_frame(&mut server_write, &wire::reply_frame(&incoming.id, &failed))
        .await
        .unwrap();
    let reply = wire::response(wire::read_frame(&mut client_read).await.unwrap().unwrap());
    assert_eq!(
        serde_json::to_value(&reply).unwrap(),
        serde_json::to_value(&failed).unwrap()
    );
}

#[test]
fn replies_keep_every_field_the_cli_renders() {
    let mut failed = Response::err_detail(
        ErrorDetail::new("dialog_open", "A dialog is open").hint("dialog accept"),
    );
    failed.error = Some("Dialog blocks the tab".into());
    failed.data = Some(serde_json::json!({"operation_id": "op", "status": "not_dispatched"}));
    let frame = wire::reply_frame(&serde_json::json!(2), &failed);
    assert_eq!(frame["error"]["code"], "DialogOpen");
    let back = wire::response(frame);
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&failed).unwrap()
    );
    let ok = wire::response(wire::reply_frame(
        &serde_json::json!(2),
        &Response::ok_text("done"),
    ));
    assert_eq!(
        serde_json::to_value(&ok).unwrap(),
        serde_json::to_value(Response::ok_text("done")).unwrap()
    );
}

#[test]
fn the_session_wire_is_locked() {
    use serde_json::json;
    let failed = {
        let mut response = Response::err_detail(
            ErrorDetail::new("daemon_busy", "Session `work` is busy")
                .retryable(true)
                .hint("Run it again once the running command returns."),
        );
        response.data = Some(json!({"operation_id": "op-1", "status": "not_dispatched"}));
        response
    };
    let frames = [
        ("hello", wire::hello_frame()),
        (
            "open",
            wire::request_frame(
                "open",
                json!({"url": "https://example.com/", "operation_id": "op-1"}),
            ),
        ),
        (
            "key.down",
            wire::request(&Request::KeyDown(HeldKeyArgs {
                key: "Shift".into(),
            })),
        ),
        ("tab.new", wire::request_frame("tab_new", json!({}))),
        (
            "reply ok",
            wire::reply_frame(&json!(2), &Response::ok_text("done")),
        ),
        ("reply error", wire::reply_frame(&json!(2), &failed)),
    ];
    let actual: String = frames
        .iter()
        .map(|(name, frame)| format!("{}\n", json!({"name": name, "frame": frame})))
        .collect();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/locks/session-wire.jsonl");
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
    }
    let locked = std::fs::read_to_string(&path)
        .expect("run with EOKA_UPDATE_LOCKS=1 to create the session wire lock");
    assert_eq!(
        actual, locked,
        "the session wire changed; review it and update with EOKA_UPDATE_LOCKS=1"
    );
}

#[tokio::test]
async fn an_unterminated_or_oversized_frame_is_an_error() {
    let mut partial = tokio::io::BufReader::new(&b"{\"id\":1"[..]);
    assert!(wire::read_frame(&mut partial).await.is_err());
    let big = vec![b'x'; sidecar::MAX_MESSAGE_BYTES + 8];
    let mut oversized = tokio::io::BufReader::new(&big[..]);
    let error = wire::read_frame(&mut oversized).await.unwrap_err();
    assert!(error.to_string().contains("maxMessageBytes"));
    let mut empty = tokio::io::BufReader::new(&b""[..]);
    assert!(wire::read_frame(&mut empty).await.unwrap().is_none());
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
        "Target element. Supports scoped refs from view, text:Submit, placeholder:Email, role:button, css:form button, id:my-btn, or plain text search."
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
            "recording_start" => serde_json::json!({"path":"/tmp/recording.mp4"}),
            "recording_get" | "recording_stop" | "recording_cancel" => {
                serde_json::json!({"recording_id":"b9b87b7a-cc6d-4a25-b098-73271cc9c4cb"})
            }
            "get_text" | "exists" | "click_shadow" => serde_json::json!({"selector":"h1"}),
            "get_attribute" => serde_json::json!({"selector":"a","name":"href"}),
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
            "action_batch" => serde_json::json!({"actions":[]}),
            "history_go" => serde_json::json!({"delta":-1}),
            _ => serde_json::json!({}),
        }
    }
}

fn untyped(schema: &serde_json::Value) -> bool {
    let shape = [
        "type",
        "$ref",
        "anyOf",
        "oneOf",
        "allOf",
        "properties",
        "items",
        "enum",
        "const",
    ];
    schema.as_bool() == Some(true)
        || schema
            .as_object()
            .is_some_and(|object| shape.iter().all(|key| !object.contains_key(*key)))
}

#[test]
fn every_catalog_operation_declares_its_result() {
    let mut arbitrary = Vec::new();
    for operation in all_operations() {
        let schema = result_schema_for_cmd(operation.cmd)
            .unwrap_or_else(|| panic!("{} declares no result", operation.cmd));
        if untyped(&schema) {
            arbitrary.push(operation.cmd);
        }
    }
    assert_eq!(
        arbitrary,
        ["eval"],
        "only user-defined JavaScript values are untyped"
    );
    assert!(result_schema_for_cmd("no_such_command").is_none());
}

#[test]
fn a_result_must_round_trip_through_its_declared_type() {
    use serde_json::json;
    decode_result_for_cmd("exists", &json!({"exists": true})).unwrap();
    decode_result_for_cmd("text", &json!("Heading")).unwrap();
    decode_result_for_cmd("emulate", &json!("Device emulation cleared")).unwrap();
    decode_result_for_cmd(
        "emulate",
        &json!({"width": 640, "height": 360, "dpr": 2.0, "mobile": false}),
    )
    .unwrap();
    decode_result_for_cmd("eval", &json!({"any": ["value", 1]})).unwrap();
    assert!(decode_result_for_cmd("exists", &json!({"exists": "yes"})).is_err());
    assert!(decode_result_for_cmd("exists", &json!({"exists": true, "extra": 1})).is_err());
    assert!(decode_result_for_cmd("info", &json!({"url": "u"})).is_err());
    assert!(decode_result_for_cmd("no_such_command", &json!(null)).is_err());
}

#[test]
fn an_action_result_is_its_record_plus_a_status() {
    use serde_json::json;
    let record = json!({
        "id": "op", "runtime_id": "rt", "target_id": "tab", "action": "reload",
        "state": "finished", "outcome": "not_dispatched", "dispatch_intent": false,
        "preparation_intent": false, "started_at_ms": 1, "finished_at_ms": 2,
        "receipt": null, "error_code": null, "error": null
    });
    let response = Response::action_result(serde_json::from_value(record.clone()).unwrap());
    let data = response.data.unwrap();
    assert_eq!(data["status"], "not_dispatched");
    decode_result_for_cmd("reload", &data).unwrap();
    assert!(decode_result_for_cmd("reload", &record).is_err());
    decode_result_for_cmd("operation_get", &record).unwrap();
}

#[test]
fn network_rows_keep_their_compact_or_full_shape() {
    use serde_json::json;
    let meta = json!({
        "namespace": "session:s", "active": true, "entry_count": 1, "in_flight": 0,
        "last_id": 1, "next_since": 1, "body_pending": 0, "body_failed": 0,
        "warnings": [], "suggested_commands": []
    });
    let filters = json!({"limit": 100, "pattern": null, "method": null, "status": null, "since": null, "compact": true});
    let compact = json!({"id": 1, "url": "u", "method": "GET", "status": 200, "complete": true});
    let full = json!({
        "id": 1, "url": "u", "method": "GET", "status": null, "resource_type": null,
        "duration_ms": 1.5, "request_body": null,
        "response_body": {"bytes": 2, "base64": false, "omitted": null},
        "complete": false, "failed": null
    });
    for row in [compact, full] {
        let log = json!({"meta": meta, "count": 1, "entries": [row], "filters": filters});
        decode_result_for_cmd("network_log", &log).unwrap();
    }
    let body = |text: serde_json::Value| {
        json!({
            "id": 1, "session_id": "s", "target_id": "t", "request_id": "r", "hop": 0,
            "url": "u", "method": "GET", "resource_type": null, "started_at": null,
            "duration_ms": 0.0,
            "request": {"headers": {}, "headers_text": null, "post_data": null},
            "response": {
                "status": 200, "status_text": "OK", "protocol": null, "mime_type": null,
                "headers": {"a": "b"}, "headers_text": null, "remote_ip": null,
                "remote_port": null, "encoded_data_length": null,
                "timing": {"requestTime": 1.25, "proxyStart": -1}, "redirect_url": null,
                "body": text
            },
            "failure": null, "canceled": false, "complete": true
        })
    };
    for entry in [
        body(json!({"bytes": 2, "base64": false, "omitted": null})),
        body(
            json!({"bytes": 2, "base64": false, "text": "ok", "omitted": null, "mime_type": null}),
        ),
        body(
            json!({"bytes": 9, "base64": false, "text": "ok", "omitted": null, "mime_type": null, "truncated": true}),
        ),
    ] {
        decode_result_for_cmd("network_show", &json!({"meta": meta, "entry": entry})).unwrap();
    }
}
