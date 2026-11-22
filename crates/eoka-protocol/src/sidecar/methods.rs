use super::*;

pub fn validate_safe_numbers(value: &Value) -> Result<(), ServerError> {
    match value {
        Value::Number(number) => {
            let unsafe_integer = number.as_u64().is_some_and(|n| n > MAX_SAFE_INTEGER)
                || number
                    .as_i64()
                    .is_some_and(|n| n < -(MAX_SAFE_INTEGER as i64))
                || number
                    .as_f64()
                    .is_some_and(|n| n.fract() == 0.0 && n.abs() > MAX_SAFE_INTEGER as f64);
            if unsafe_integer {
                return Err(ServerError::invalid_params(
                    "integer exceeds JavaScript's exact safe integer range",
                ));
            }
        }
        Value::Array(values) => {
            for value in values {
                validate_safe_numbers(value)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_safe_numbers(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn constrain_integers(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let integer = object.get("type").is_some_and(|ty| {
                ty == "integer"
                    || ty
                        .as_array()
                        .is_some_and(|types| types.iter().any(|ty| ty == "integer"))
            });
            if integer {
                let (rust_minimum, rust_maximum) =
                    match object.get("format").and_then(Value::as_str) {
                        Some("int8") => (i8::MIN as f64, i8::MAX as f64),
                        Some("int16") => (i16::MIN as f64, i16::MAX as f64),
                        Some("int32") => (i32::MIN as f64, i32::MAX as f64),
                        Some("uint8") => (0.0, u8::MAX as f64),
                        Some("uint16") => (0.0, u16::MAX as f64),
                        Some("uint32") => (0.0, u32::MAX as f64),
                        Some("uint") => (0.0, (usize::MAX as f64).min(MAX_SAFE_INTEGER as f64)),
                        Some("uint64" | "uint128") => (0.0, MAX_SAFE_INTEGER as f64),
                        _ => (-(MAX_SAFE_INTEGER as f64), MAX_SAFE_INTEGER as f64),
                    };
                let maximum = object
                    .get("maximum")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::INFINITY)
                    .min(rust_maximum);
                let minimum = object
                    .get("minimum")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::NEG_INFINITY)
                    .max(rust_minimum);
                object.insert("maximum".into(), json!(maximum));
                object.insert("minimum".into(), json!(minimum));
            }
            for child in object.values_mut() {
                constrain_integers(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                constrain_integers(child);
            }
        }
        _ => {}
    }
}

pub(super) fn schema<T: JsonSchema>() -> Value {
    let mut value =
        serde_json::to_value(schemars::schema_for!(T)).expect("JSON schemas are serializable");
    constrain_integers(&mut value);
    value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodRole {
    Lifecycle,
    Shared,
    Distinct(&'static str),
}

macro_rules! define_sidecar_methods {
    (@feature) => { None };
    (@feature $feature:literal) => { Some($feature) };
    ($($variant:ident $name:literal: $params:ty $(, $check:expr)? => $result:ty, $role:ident $(($reason:literal))? $(, feature $feature:literal)?;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Method { $($variant),* }

        impl Method {
            pub const ALL: &'static [Method] = &[$(Method::$variant),*];

            pub fn parse(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),* }
            }

            pub fn role(self) -> MethodRole {
                match self { $(Self::$variant => MethodRole::$role $(($reason))?),* }
            }

            pub fn feature(self) -> Option<&'static str> {
                match self { $(Self::$variant => define_sidecar_methods!(@feature $($feature)?)),* }
            }

            pub fn params_schema(self) -> Value {
                match self { $(Self::$variant => schema::<$params>()),* }
            }

            pub fn result_schema(self) -> Value {
                match self { $(Self::$variant => schema::<$result>()),* }
            }

            pub fn validate(self, params: &Value) -> Result<(), ServerError> {
                match self {
                    $(Self::$variant => {
                        let _params = <$params>::deserialize(params)
                            .map_err(|error| ServerError::invalid_params(error.to_string()))?;
                        $(($check)(&_params)?;)?
                        Ok(())
                    }),*
                }
            }
        }
    };
}

define_sidecar_methods! {
    ServerHello "server.hello": HelloParams => HelloResult, Lifecycle;
    ServerShutdown "server.shutdown": Empty => Empty, Lifecycle;
    BrowserLaunch "browser.launch": LaunchParams, LaunchParams::validate => BrowserSessionResult, Lifecycle;
    BrowserConnect "browser.connect": ConnectParams, ConnectParams::validate => BrowserSessionResult, Lifecycle;
    BrowserNewPage "browser.new_page": NewPageParams => NewPageResult, Lifecycle;
    BrowserTabs "browser.tabs": Empty => TabsResult, Lifecycle;
    BrowserCloseTab "browser.close_tab": PageIdParams => CloseTabResult, Lifecycle;
    BrowserClose "browser.close": Empty => Empty, Lifecycle;
    ActionsPerform "actions.perform": ActionPerformParams => crate::OperationRecord, Shared;
    OperationsGet "operations.get": OperationIdArgs => crate::OperationRecord, Shared;
    OperationsList "operations.list": OperationListArgs => Vec<crate::OperationRecord>, Shared;
    OperationsWait "operations.wait": OperationWaitArgs => crate::OperationRecord, Shared;
    OperationsCancel "operations.cancel": OperationIdArgs => crate::OperationRecord, Shared;
    AgentObserve "agent.observe": AgentObserveParams => crate::AgentObservation, Shared;
    AgentStep "agent.step": AgentStepParams => crate::AgentStepResult, Shared;
    PageCaptureState "page.capture_state": PageIdParams => CaptureStateResult,
        Distinct("in-memory state; catalog save_state and load_state use a file");
    PageRestoreState "page.restore_state": RestoreStateParams => Empty,
        Distinct("in-memory state; catalog save_state and load_state use a file");
    RecordingStart "recording.start": RecordingStartParams, RecordingStartParams::validate => RecordingRecord, Shared;
    RecordingGet "recording.get": RecordingIdParams, RecordingIdParams::validate => RecordingRecord, Shared;
    RecordingStop "recording.stop": RecordingIdParams, RecordingIdParams::validate => RecordingRecord, Shared;
    RecordingCancel "recording.cancel": RecordingIdParams, RecordingIdParams::validate => RecordingRecord, Shared;
    PageSolveCaptcha "page.solve_captcha": SolveCaptchaParams => SolveCaptchaResult,
        Distinct("provider solving runs client-side in the CLI"), feature "anti-captcha";
}

pub fn catalog_operations() -> impl Iterator<Item = &'static crate::OperationSpec> {
    page_operations().filter(|operation| Method::parse(operation.path).is_none())
}

pub fn page_operations() -> impl Iterator<Item = &'static crate::OperationSpec> {
    crate::all_operations().iter().filter(|operation| {
        operation.exposure != crate::OperationExposure::Lifecycle
            && !operation.path.starts_with("tab.")
            && !matches!(operation.path, "close" | "shutdown")
    })
}

pub fn catalog_operation(method: &str) -> Option<&'static crate::OperationSpec> {
    catalog_operations().find(|operation| operation.path == method)
}

pub fn catalog_request(
    operation: &crate::OperationSpec,
    params: &Value,
) -> Result<(String, crate::Request), ServerError> {
    let mut input = params.clone();
    let page_id = input
        .as_object_mut()
        .and_then(|object| object.remove("pageId"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| ServerError::invalid_params("missing string field `pageId`"))?;
    let schema = crate::input_schema_for_operation(operation);
    if let (Some(known), Some(given)) = (schema["properties"].as_object(), input.as_object()) {
        if let Some(unknown) = given.keys().find(|key| !known.contains_key(*key)) {
            return Err(ServerError::invalid_params(format!(
                "unknown field `{unknown}` for {}",
                operation.path
            )));
        }
    }
    let request =
        crate::request_from_cmd(operation.cmd, input).map_err(ServerError::invalid_params)?;
    Ok((page_id, request))
}

fn catalog_params_schema(operation: &crate::OperationSpec) -> Value {
    let page_id = json!({
        "type": "object",
        "properties": {"pageId": {"type": "string"}},
        "required": ["pageId"],
    });
    let mut input = crate::input_schema_for_operation(operation);
    constrain_integers(&mut input);
    let plain_object = input["type"] == "object"
        && ["oneOf", "anyOf", "allOf", "$ref"]
            .iter()
            .all(|key| input.get(key).is_none());
    if !plain_object {
        return json!({"allOf": [input, page_id]});
    }
    let object = input.as_object_mut().expect("checked above");
    let properties = object.entry("properties").or_insert_with(|| json!({}));
    properties["pageId"] = page_id["properties"]["pageId"].clone();
    match object.entry("required").or_insert_with(|| json!([])) {
        Value::Array(required) => required.push(json!("pageId")),
        _ => unreachable!("JSON schema `required` is an array"),
    }
    input
}

fn catalog_result_schema(operation: &crate::OperationSpec) -> Value {
    let mut result = crate::result_schema_for_cmd(operation.cmd)
        .expect("every catalog operation declares a result");
    constrain_integers(&mut result);
    result
}

pub fn method_feature(method: &str) -> Option<&'static str> {
    match Method::parse(method) {
        Some(method) => method.feature(),
        None => catalog_operation(method).and_then(|operation| operation.feature),
    }
}

pub fn method_names() -> Vec<&'static str> {
    Method::ALL
        .iter()
        .map(|method| method.name())
        .chain(catalog_operations().map(|operation| operation.path))
        .collect()
}

pub fn validate_params(method: &str, params: &Value) -> Result<(), ServerError> {
    validate_safe_numbers(params)?;
    if let Some(operation) = catalog_operation(method) {
        return catalog_request(operation, params).map(|_| ());
    }
    Method::parse(method)
        .ok_or_else(|| ServerError::unknown_method(method))?
        .validate(params)
}

pub fn schema_bundle() -> Value {
    let methods: BTreeMap<_, _> = Method::ALL
        .iter()
        .map(|method| {
            (
                method.name(),
                json!({"params": method.params_schema(), "result": method.result_schema()}),
            )
        })
        .chain(catalog_operations().map(|operation| {
            (
                operation.path,
                json!({"params": catalog_params_schema(operation), "result": catalog_result_schema(operation)}),
            )
        }))
        .collect();
    let mut bundle = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "methods": methods,
        "types": {
            "Request": schema::<Request>(), "Response": schema::<Response>(),
            "ServerError": schema::<ServerError>(), "ErrorData": schema::<ErrorData>(),
            "ErrorDetail": schema::<crate::ErrorDetail>(), "HelloResult": schema::<HelloResult>(),
            "ProtocolVersion": schema::<ProtocolVersion>(), "HelloParams": schema::<HelloParams>(),
            "TransportLimits": schema::<TransportLimits>(), "ErrorCode": schema::<ErrorCode>(),
            "CdpErrorDetail": schema::<CdpErrorDetail>(),
            "Action": schema::<crate::Action>(), "ActionOptions": schema::<crate::ActionOptions>(),
            "ActionReceipt": schema::<crate::ActionReceipt>(), "LocatorSpec": schema::<crate::LocatorSpec>(),
            "ActionOutcome": schema::<crate::ActionOutcome>(), "DispatchState": schema::<crate::DispatchState>(),
            "LocatorQuery": schema::<crate::LocatorQuery>(),
            "Expectation": schema::<crate::Expectation>(), "OperationRecord": schema::<crate::OperationRecord>(),
            "OperationState": schema::<crate::OperationState>(), "AgentObservation": schema::<crate::AgentObservation>(),
            "AgentStepResult": schema::<crate::AgentStepResult>(), "BrowserState": schema::<BrowserState>(),
            "AgentAction": schema::<crate::AgentAction>(), "AgentMode": schema::<crate::AgentMode>(),
            "ScreenshotFormat": schema::<ScreenshotFormat>(), "ScreenshotClip": schema::<ScreenshotClip>(),
            "Artifact": schema::<Artifact>(), "ArtifactContainer": schema::<ArtifactContainer>(),
            "RecordingStartParams": schema::<RecordingStartParams>(), "RecordingIdParams": schema::<RecordingIdParams>(),
            "RecordingRecord": schema::<RecordingRecord>(), "RecordingState": schema::<RecordingState>(),
            "RecordingFailure": schema::<RecordingFailure>(), "RecordingFailureStage": schema::<RecordingFailureStage>()
        }
    });
    bundle["catalogMethods"] = catalog_operations()
        .map(|operation| operation.path)
        .collect();
    bundle["types"]["InputMode"] = schema::<crate::InputMode>();
    bundle["types"]["Destination"] = schema::<crate::Destination>();
    bundle["types"]["CloudflareAutoSolve"] = schema::<CloudflareAutoSolve>();
    bundle["types"]["PageBlocker"] = schema::<PageBlocker>();
    bundle
}
