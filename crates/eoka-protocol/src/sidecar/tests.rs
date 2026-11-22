use super::*;

const RECORDING_ID: &str = "b9b87b7a-cc6d-4a25-b098-73271cc9c4cb";

#[test]
fn capture_validation_preserves_defaults_and_rejects_conflicts() {
    let args = |value: Value| -> crate::ScreenshotArgs { serde_json::from_value(value).unwrap() };
    assert!(validate_params("screenshot", &json!({"pageId":"p","inline":true})).is_ok());
    assert!(args(json!({})).validate().is_ok());
    assert!(args(json!({"inline":true,"annotate":true}))
        .validate()
        .is_ok());
    for format in ["jpeg", "webp"] {
        assert!(args(
            json!({"format":format,"quality":0,"clip":{"x":0,"y":1,"width":1.5,"height":2}})
        )
        .validate()
        .is_ok());
    }
    for value in [
        json!({"quality":10}),
        json!({"format":"jpeg","quality":101}),
        json!({"clip":{"x":-1,"y":0,"width":1,"height":1}}),
        json!({"clip":{"x":0,"y":0,"width":0,"height":1}}),
        json!({"clip":{"x":0,"y":0,"width":1,"height":1},"full_page":true}),
        json!({"inline":true,"output":"/tmp/capture.png"}),
        json!({"annotate":true,"full_page":true}),
    ] {
        assert_eq!(
            args(value.clone()).validate().unwrap_err().code,
            ErrorCode::InvalidParams,
            "{value}"
        );
    }
    assert!(validate_params("screenshot", &json!({"pageId":"p","format":"gif"})).is_err());
    assert!(validate_output_path("/tmp/capture.png").is_ok());
    assert!(validate_output_path("capture.png").is_err());
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let clip = ScreenshotClip {
            x: 0.0,
            y: 0.0,
            width: invalid,
            height: 1.0,
        };
        assert!(validate_screenshot(None, None, Some(&clip), None).is_err());
    }
}

#[test]
fn shared_methods_are_catalog_paths() {
    for method in Method::ALL {
        if method.role() == MethodRole::Shared {
            assert!(
                crate::operation_by_path(method.name()).is_some(),
                "{} is Shared but is not a catalog path",
                method.name()
            );
        }
    }
}

#[test]
fn catalog_operations_are_sidecar_methods_addressed_by_page() {
    let names = method_names();
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "method names must be unique");

    for operation in crate::all_operations() {
        let projected = catalog_operation(operation.path).is_some();
        let own_method = Method::parse(operation.path).is_some();
        let sidecar_lifecycle = operation.exposure == crate::OperationExposure::Lifecycle
            || operation.path.starts_with("tab.")
            || matches!(operation.path, "close" | "shutdown");
        assert_eq!(
            projected,
            !own_method && !sidecar_lifecycle,
            "{}",
            operation.path
        );
        assert!(
            names.contains(&operation.path) || sidecar_lifecycle,
            "{} is not reachable through the sidecar",
            operation.path
        );
    }

    let bundle = schema_bundle();
    for operation in catalog_operations() {
        let params = &bundle["methods"][operation.path]["params"];
        assert_eq!(params["properties"]["pageId"]["type"], "string");
        assert!(params["required"]
            .as_array()
            .unwrap()
            .contains(&json!("pageId")));
        assert_eq!(
            validate_params(operation.path, &json!({}))
                .unwrap_err()
                .code,
            ErrorCode::InvalidParams,
            "{} must require pageId",
            operation.path
        );
    }
    assert_eq!(
        bundle["catalogMethods"].as_array().unwrap().len(),
        catalog_operations().count()
    );

    let console = catalog_operation("console").unwrap();
    let (page, request) = catalog_request(console, &json!({"pageId":"p","clear":true})).unwrap();
    assert_eq!(page, "p");
    assert_eq!(request.cmd(), "console");
    assert_eq!(request.args_json()["clear"], true);
    assert!(request.args_json().get("pageId").is_none());
    assert!(validate_params("cookies", &json!({"pageId":"p"})).is_ok());
    assert!(validate_params("cookies", &json!({"pageId":7})).is_err());
    assert!(validate_params("find", &json!({"pageId":"p"})).is_err());
    let unknown = validate_params(
        "click_shadow",
        &json!({"pageId":"p","selector":"b","timeoutMs":500}),
    )
    .unwrap_err();
    assert!(unknown.message.contains("timeoutMs"), "{}", unknown.message);
    assert!(validate_params(
        "click_shadow",
        &json!({"pageId":"p","selector":"b","timeout_ms":500})
    )
    .is_ok());
    for absent in [
        "close",
        "shutdown",
        "tab.list",
        "session.hello",
        "actions.command",
    ] {
        assert!(catalog_operation(absent).is_none(), "{absent}");
    }
}

#[test]
fn recording_ids_paths_and_native_bounds_are_validated_before_dispatch() {
    let valid = json!({"pageId":"p","recordingId":RECORDING_ID,"path":"/tmp/capture.mp4","fps":120,"maxWidth":2147483647_u32,"maxHeight":1});
    assert!(validate_params("recording.start", &valid).is_ok());
    for (field, value) in [
        ("recordingId", json!("unknown")),
        ("path", json!("capture.mp4")),
        ("fps", json!(0)),
        ("fps", json!(120.5)),
        ("fps", json!(1.5)),
        ("maxWidth", json!(0)),
        ("maxHeight", json!(2147483648_u32)),
    ] {
        let mut params = valid.clone();
        params[field] = value;
        assert_eq!(
            validate_params("recording.start", &params)
                .unwrap_err()
                .code,
            ErrorCode::InvalidParams
        );
    }
    for method in ["recording.get", "recording.stop", "recording.cancel"] {
        assert!(validate_params(method, &json!({"recordingId":RECORDING_ID})).is_ok());
        assert!(validate_params(method, &json!({"recordingId":"not-a-uuid"})).is_err());
    }
}

#[test]
fn recording_terminal_evidence_never_labels_partial_files_complete() {
    let mut record = RecordingRecord {
        recording_id: RECORDING_ID.into(),
        page_id: "p".into(),
        state: RecordingState::Running,
        path: "/tmp/capture.mp4".into(),
        artifact: None,
        failure: None,
        frames: None,
        duration_ms: None,
    };
    assert!(record.validate().is_ok());
    record.frames = Some(3);
    assert!(record.validate().is_err());
    record.frames = None;
    record.state = RecordingState::Completed;
    assert!(record.validate().is_err());
    record.artifact = Some(Artifact {
        path: record.path.clone(),
        bytes: MAX_SAFE_INTEGER,
        mime_type: "video/mp4".into(),
        container: Some(ArtifactContainer::Mp4),
    });
    assert!(record.validate().is_ok());
    assert!(serde_json::to_value(&record).unwrap()["artifact"]
        .get("codec")
        .is_none());
    record.artifact.as_mut().unwrap().bytes += 1;
    assert!(record.validate().is_err());
    record.artifact = None;
    record.state = RecordingState::Failed;
    assert!(record.validate().is_err());
    record.failure = Some(RecordingFailure {
        stage: RecordingFailureStage::Write,
        code: ErrorCode::RecordingFailed,
        message: "disk full".into(),
        partial_path: Some("/tmp/.capture.partial".into()),
        cleanup_uncertain: Some(true),
    });
    assert!(record.validate().is_ok());
    let error = ServerError::new(ErrorCode::RecordingFailed, "disk full").with_data(ErrorData {
        recording_id: Some(RECORDING_ID.into()),
        recording: Some(record.clone()),
        ..Default::default()
    });
    let wire = serde_json::to_value(error).unwrap();
    assert_eq!(wire["data"]["recordingId"], RECORDING_ID);
    assert_eq!(
        wire["data"]["recording"]["failure"]["partialPath"],
        "/tmp/.capture.partial"
    );
    assert_eq!(
        wire["data"]["recording"]["failure"]["cleanupUncertain"],
        true
    );
    record.failure.as_mut().unwrap().partial_path = Some(record.path.clone());
    assert!(record.validate().is_err());
    assert!(RecordingState::Cancelled.is_terminal());
    assert!(!RecordingState::Finalizing.is_terminal());
}

#[test]
fn fetch_limits_reject_malformed_bounds_before_browser_work() {
    for max_body in [0, 16_777_216] {
        validate_params(
            "fetch",
            &json!({"pageId":"page","url":"/text","max_body":max_body}),
        )
        .unwrap();
    }
    for params in [
        json!({"pageId":"page","url":"/text","max_body":-1}),
        json!({"pageId":"page","url":"/text","max_body":1.5}),
        json!({"pageId":"page","url":"/text","maxBytes":8192}),
    ] {
        assert_eq!(
            validate_params("fetch", &params).unwrap_err().code,
            ErrorCode::InvalidParams
        );
    }
}

#[test]
fn blocked_page_errors_carry_their_own_codes_and_what_blocks_the_page() {
    for (code, wire, detail) in [
        (ErrorCode::DialogOpen, "DialogOpen", "dialog_open"),
        (
            ErrorCode::NavigationPending,
            "NavigationPending",
            "navigation_pending",
        ),
        (
            ErrorCode::PageUnresponsive,
            "PageUnresponsive",
            "page_unresponsive",
        ),
    ] {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(wire));
        assert_eq!(code.blocker_detail_code(), Some(detail));
        assert_eq!(ErrorCode::for_blocker_detail(detail), Some(code));
    }
    assert_eq!(ErrorCode::Timeout.blocker_detail_code(), None);
    assert_eq!(ErrorCode::for_blocker_detail("timeout"), None);
    assert!(schema::<ErrorCode>().to_string().contains("\"DialogOpen\""));

    let dialog = ServerError::blocked(
        crate::ErrorDetail::new("dialog_open", "A JavaScript alert dialog is open"),
        Some(PageBlocker::Dialog {
            dialog_type: "alert".into(),
            message: "are you sure?".into(),
            url: "https://example.test/".into(),
            elapsed_ms: 1200,
        }),
    );
    assert_eq!(
        serde_json::to_value(&dialog).unwrap(),
        json!({
            "code": "DialogOpen",
            "message": "A JavaScript alert dialog is open",
            "data": {
                "detail": {"code": "dialog_open", "message": "A JavaScript alert dialog is open"},
                "blocker": {
                    "kind": "dialog", "dialogType": "alert", "message": "are you sure?",
                    "url": "https://example.test/", "elapsedMs": 1200
                }
            }
        })
    );
    let navigation = serde_json::to_value(PageBlocker::Navigation {
        url: Some("https://slow.test/".into()),
        elapsed_ms: 6500,
    })
    .unwrap();
    assert_eq!(
        navigation,
        json!({"kind": "navigation", "url": "https://slow.test/", "elapsedMs": 6500})
    );
    assert_eq!(
        serde_json::to_value(PageBlocker::Navigation {
            url: None,
            elapsed_ms: 1
        })
        .unwrap(),
        json!({"kind": "navigation", "elapsedMs": 1})
    );
    let round_trip: ServerError =
        serde_json::from_value(serde_json::to_value(&dialog).unwrap()).unwrap();
    assert_eq!(round_trip.code, ErrorCode::DialogOpen);
    assert_eq!(
        round_trip.data.unwrap().blocker,
        dialog.data.unwrap().blocker
    );
    let other = ServerError::blocked(crate::ErrorDetail::new("stale_ref", "gone"), None);
    assert_eq!(other.code, ErrorCode::Operation);
}

#[test]
fn hello_names_one_protocol_version() {
    assert_eq!(PROTOCOL_VERSION, ProtocolVersion { major: 4, minor: 0 });
    let hello: HelloParams =
        serde_json::from_value(json!({"protocol": {"major": 4, "minor": 0}})).unwrap();
    assert_eq!(hello.protocol, PROTOCOL_VERSION);
    assert!(validate_params("server.hello", &json!({"protocol": PROTOCOL_VERSION})).is_ok());
}

#[test]
fn request_ids_are_positive_exact_integers() {
    for id in [json!(1), json!(MAX_SAFE_INTEGER)] {
        let request: Request =
            serde_json::from_value(json!({"id":id,"method":"browser.tabs"})).unwrap();
        assert_eq!(request.id, id);
        assert_eq!(request.params, json!({}));
    }
    for id in [
        json!(0),
        json!(-1),
        json!(MAX_SAFE_INTEGER + 1),
        json!(1.5),
        json!(1.0),
        json!("1"),
        Value::Null,
    ] {
        assert!(
            serde_json::from_value::<Request>(json!({"id":id,"method":"browser.tabs"})).is_err()
        );
    }
}

#[test]
fn response_requires_exactly_one_outcome_and_preserves_null() {
    let success = serde_json::to_value(Response::ok(1_u64, Value::Null)).unwrap();
    assert_eq!(success, json!({"id":1,"result":null}));
    let decoded: Response = serde_json::from_value(success).unwrap();
    assert!(matches!(
        decoded,
        Response::Success {
            result: Value::Null,
            ..
        }
    ));
    for invalid in [
        json!({"id":1}),
        json!({"id":1,"result":null,"error":{"code":"Internal","message":"failure"}}),
    ] {
        assert!(serde_json::from_value::<Response>(invalid).is_err());
    }
    let failure = serde_json::to_value(Response::err(
        Value::Null,
        ServerError::invalid_params("bad input"),
    ))
    .unwrap();
    assert!(failure.get("result").is_none());
    assert_eq!(failure["error"]["code"], "InvalidParams");
}

#[test]
fn cdp_and_action_errors_preserve_evidence_through_wire_round_trip() {
    let receipt = crate::ActionReceipt {
        started_at_ms: 1,
        finished_at_ms: 2,
        download_guid: None,
        destination_target_id: None,
        action: "click".into(),
        target_id: "target".into(),
        outcome: crate::ActionOutcome::OutcomeUnknown,
        dispatch: crate::DispatchState::MayHaveBeenSent,
        preparation_may_have_executed: true,
        verification: None,
        elapsed_ms: 1,
        already_satisfied: false,
        cleanup_pending: true,
    };
    let error = ServerError::from(eoka::Error::Action {
        code: "outcome_unknown".into(),
        receipt: Box::new(receipt),
        source: Box::new(eoka::Error::Cdp {
            method: Some("Input.dispatchMouseEvent".into()),
            code: Some(-32000),
            message: "connection interrupted".into(),
        }),
    });
    let wire = serde_json::to_value(Response::err(1_u64, error)).unwrap();
    let decoded: Response = serde_json::from_value(wire).unwrap();
    let Response::Failure { error, .. } = decoded else {
        panic!("expected error")
    };
    assert_eq!(error.code, ErrorCode::Action);
    let data = error.data.unwrap();
    assert_eq!(data.detail.unwrap().code, "outcome_unknown");
    let receipt = data.receipt.unwrap();
    assert_eq!(receipt.outcome, crate::ActionOutcome::OutcomeUnknown);
    assert_eq!(receipt.dispatch, crate::DispatchState::MayHaveBeenSent);
    assert!(receipt.cleanup_pending);
    let cdp = data.source.unwrap().data.unwrap().cdp.unwrap();
    assert_eq!(cdp.method.as_deref(), Some("Input.dispatchMouseEvent"));
    assert_eq!(cdp.code, Some(-32000));
}

#[test]
fn click_shadow_needs_a_selector() {
    assert!(validate_params(
        "click_shadow",
        &json!({"pageId":"p","selector":"input[type=\"checkbox\"]"})
    )
    .is_ok());
    assert!(validate_params(
        "click_shadow",
        &json!({"pageId":"p","selector":"button","frame_id":"frame","timeout_ms":1000})
    )
    .is_ok());
    assert!(validate_params("click_shadow", &json!({"pageId":"p"})).is_err());
    assert!(validate_params("click_shadow", &json!({"pageId":"p","selector":" "})).is_err());
    for timeout in [0, 60_001] {
        assert!(validate_params(
            "click_shadow",
            &json!({"pageId":"p","selector":"b","timeout_ms":timeout})
        )
        .is_err());
    }
    assert!(validate_params(
        "click_shadow",
        &json!({"pageId":"p","selector":"b","frameId":"f"})
    )
    .is_err());
}

#[test]
fn canonical_params_reject_missing_actions_and_unsafe_nested_integers() {
    let click = json!({"kind":"click","target":{"query":{"kind":"css","selector":"button"}}});
    assert!(validate_params("actions.perform", &json!({"pageId":"p","action":click})).is_err());
    let params = json!({"pageId":"p","action":click,"operation_id":"operation","options":{"timeout_ms":30000}});
    assert!(validate_params("actions.perform", &params).is_ok());
    for method in [
        "page.click",
        "page.goto",
        "page.evaluate",
        "page.screenshot",
        "page.close",
    ] {
        assert_eq!(
            validate_params(method, &params).unwrap_err().code,
            ErrorCode::UnknownMethod,
            "{method}"
        );
    }
    assert!(validate_params("actions.perform", &json!({"pageId":"p","operation_id":"operation","action":{"kind":"navigate","url":"https://example.test","referrer":null},"options":{"timeout_ms":MAX_SAFE_INTEGER + 1}})).is_err());
    assert!(
        validate_safe_numbers(&json!({"nested":[{"value":-(MAX_SAFE_INTEGER as i64)-1}]})).is_err()
    );
    assert!(validate_safe_numbers(&json!({"value":9007199254740992.0})).is_err());
    assert!(validate_safe_numbers(
        &json!({"values":[MAX_SAFE_INTEGER, -(MAX_SAFE_INTEGER as i64), 1.25]})
    )
    .is_ok());
    assert!(validate_params(
        "operations.wait",
        &json!({"id":"operation","timeout_ms":MAX_SAFE_INTEGER + 1})
    )
    .is_err());
    assert_eq!(
        validate_params("unknown", &json!({})).unwrap_err().code,
        ErrorCode::UnknownMethod
    );
    for method in ["unknown", "browser.launch"] {
        let error = validate_params(
            method,
            &json!({"sandbox":"false","viewportWidth":MAX_SAFE_INTEGER + 1}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidParams);
        assert_eq!(
            error.message,
            "integer exceeds JavaScript's exact safe integer range"
        );
    }
}

#[test]
fn managed_params_validate_paths_dimensions_and_scale_before_browser_work() {
    for params in [
        json!({"executablePath":"relative/chrome"}),
        json!({"runtimeDir":"relative/runtime"}),
        json!({"viewportWidth":640}),
        json!({"viewportWidth":0,"viewportHeight":360}),
        json!({"viewportWidth":2147483648_u64,"viewportHeight":360}),
        json!({"deviceScaleFactor":0}),
        json!({"deviceScaleFactor":-1}),
        json!({"sandbox":"false"}),
    ] {
        assert_eq!(
            validate_params("browser.launch", &params).unwrap_err().code,
            ErrorCode::InvalidParams
        );
    }
    let params = json!({"viewportWidth":0,"sandbox":"false"});
    assert_eq!(
        validate_params("browser.launch", &params)
            .unwrap_err()
            .message,
        LaunchParams::deserialize(&params).unwrap_err().to_string()
    );
    assert!(validate_params(
        "browser.launch",
        &json!({
            "viewportWidth":640,"viewportHeight":360,"deviceScaleFactor":2.0,"sandbox":true
        })
    )
    .is_ok());
    assert!(validate_params("browser.launch", &json!({"deviceScaleFactor":1.5})).is_ok());
    for (width, height, scale) in [(0, 360, 1.0), (640, u32::MAX, 1.0), (640, 360, -1.0)] {
        assert_eq!(
            validate_viewport(width, height, scale).unwrap_err().code,
            ErrorCode::InvalidParams
        );
    }
    for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            validate_viewport(640, 360, scale).unwrap_err().code,
            ErrorCode::InvalidParams
        );
    }
    assert!(validate_viewport(640, 360, 1.0).is_ok());
    assert!(validate_params(
        "browser.connect",
        &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","sandbox":true})
    )
    .is_err());
}

#[test]
fn canonical_state_conversion_preserves_cookie_and_storage_shapes() {
    let state: BrowserState = serde_json::from_value(json!({
        "cookies":[{"name":"session","value":"private","domain":"example.test","path":"/","secure":true,"http_only":true,"same_site":"Lax","expires":null}],
        "localStorage":{"local":"value"},"sessionStorage":{"tab":"value"},
        "userAgent":"agent","url":"https://example.test/"
    })).unwrap();
    let before = serde_json::to_value(&state).unwrap();
    let core: eoka::BrowserState = state.into();
    assert_eq!(serde_json::to_value(&core).unwrap(), before);
    assert_eq!(
        serde_json::to_value(BrowserState::from(core)).unwrap(),
        before
    );
}

#[test]
fn actual_result_schemas_preserve_array_dynamic_json_and_numeric_bounds() {
    let bundle = schema_bundle();
    let list = &bundle["methods"]["operations.list"]["result"];
    assert_eq!(list["type"], "array");
    assert!(list["items"]["$ref"]
        .as_str()
        .unwrap()
        .ends_with("/OperationRecord"));
    let dynamic = &bundle["methods"]["eval"]["result"];
    assert!(dynamic.is_boolean() || dynamic.is_object(), "{dynamic}");
    let launch = &bundle["methods"]["browser.launch"]["params"];
    assert_eq!(
        launch["properties"]["viewportWidth"]["maximum"].as_f64(),
        Some(i32::MAX as f64)
    );
    let wait = &bundle["methods"]["operations.wait"]["params"];
    assert_eq!(
        wait["properties"]["timeout_ms"]["maximum"].as_f64(),
        Some(86_400_000.0)
    );
    let methods = method_names();
    let unique: std::collections::BTreeSet<_> = methods.iter().collect();
    assert_eq!(unique.len(), methods.len());
    assert!(validate_params("browser.connect", &json!({})).is_err());
    assert!(validate_params("operation_get", &json!({"id":"operation"})).is_err());
}

#[test]
fn turnstile_and_cloudflare_auto_solve_validate_deadlines() {
    assert!(validate_params("captcha.turnstile", &json!({"pageId":"p"})).is_ok());
    for timeout in [0, 120_001] {
        assert!(validate_params(
            "captcha.turnstile",
            &json!({"pageId":"p","timeout_ms":timeout})
        )
        .is_err());
    }
    let launch: LaunchParams = serde_json::from_value(json!({"autoSolveCloudflare":{}})).unwrap();
    assert_eq!(
        launch
            .auto_solve_cloudflare
            .map(|o| (o.interval_ms, o.timeout_ms)),
        Some((500, 30_000))
    );
    assert!(validate_params(
        "browser.launch",
        &json!({"autoSolveCloudflare":{"intervalMs":99}})
    )
    .is_err());
    assert!(validate_params(
        "browser.connect",
        &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","autoSolveCloudflare":{"timeoutMs":5000}})
    )
    .is_ok());
    assert!(validate_params(
        "browser.connect",
        &json!({"wsEndpoint":"ws://localhost/devtools/browser/id","autoSolveCloudflare":{"intervalMs":60001}})
    )
    .is_err());
}
