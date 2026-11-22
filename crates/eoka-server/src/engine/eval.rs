use eoka::cdp::Session as CdpSession;
use eoka_protocol::{FrameEvalArgs, ScriptArgs};
use serde_json::{json, Value};

use super::target::json_str;
use super::Engine;
use super::EngineError;
use eoka_protocol::sidecar::ServerError;
use eoka_protocol::Response;

pub(super) enum Script {
    Eval(eoka_protocol::ScriptArgs),
    Exec(eoka_protocol::ScriptArgs),
    FrameEval(eoka_protocol::FrameEvalArgs),
}

pub(crate) const SIDECAR_EVALUATION_BYTES: usize = 16 * 1024 * 1024 - 4096;
const FUNCTION_RESULT_BYTES: usize = 1_048_576;
const ENGINE_EVALUATION_BYTES: usize = FUNCTION_RESULT_BYTES;

impl Engine {
    pub(super) async fn script(
        &mut self,
        command: &str,
        script: Script,
    ) -> Result<Response, EngineError> {
        let drain = self.start_fetch_drain();
        let page = self.current_page();
        let session = self.session_name.clone();
        let remedies = super::blockers::Remedies::new(Some(&session));
        let name = self.named(&remedies, command, &Value::Null);
        let result = super::blockers::guard(
            page.as_ref(),
            remedies,
            &name,
            super::blockers::Policy::Script,
            None,
            async {
                match script {
                    Script::Eval(args) => {
                        let page = self.require_tab()?.page.clone();
                        evaluate(&page, &args, ENGINE_EVALUATION_BYTES)
                            .await
                            .map_err(|error| error.message.into())
                    }
                    Script::Exec(args) => {
                        let page = self.require_tab()?.page.clone();
                        execute(&page, &args)
                            .await
                            .map_err(|error| error.message.into())
                    }
                    Script::FrameEval(args) => self.cmd_frame_eval(&args).await,
                }
            },
        )
        .await;
        if !self.stop_fetch_drain(drain).await {
            return Err("Request interception cleanup failed".into());
        }
        result
    }

    async fn cmd_frame_eval(&mut self, args: &FrameEvalArgs) -> Result<Response, EngineError> {
        let code = resolve_js(args.file.as_deref(), args.code.as_deref())?;
        let frame = args.frame.as_str();
        if frame.is_empty()
            || frame.bytes().all(|byte| byte.is_ascii_digit())
            || frame.starts_with("http://")
            || frame.starts_with("https://")
        {
            return Err("Use a CSS frame selector or id:<frame-id> from frames; bare URLs and indices are not supported".into());
        }
        let page = self.require_tab()?.page.clone();
        let value: Value = if let Some(id) = frame.strip_prefix("id:") {
            if id.is_empty() {
                return Err("Missing frame ID after id:".into());
            }
            page.evaluate_in_frame_id(id, &code).await
        } else {
            page.evaluate_in_frame(frame, &code).await
        }
        .map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&value).map(Response::ok_text)?)
    }
}

pub(crate) async fn evaluate(
    page: &eoka::Page,
    args: &ScriptArgs,
    limit: usize,
) -> Result<Response, ServerError> {
    let code = resolve_js(args.file.as_deref(), args.code.as_deref())
        .map_err(ServerError::invalid_params)?;
    check_modes(args)?;
    if !args.raw {
        let limit = args
            .max_size
            .unwrap_or(FUNCTION_RESULT_BYTES)
            .min(FUNCTION_RESULT_BYTES);
        if args.no_await {
            return Err(ServerError::invalid_params(
                "Function evaluation awaits completion; no-await also requires raw",
            ));
        }
        let code = if args.function {
            code
        } else {
            prepare_script(page, &code, &args.arguments).await?
        };
        let value: Value = page
            .evaluate_json(
                &eoka::Page::function_expression(&code, &args.arguments)?,
                limit,
            )
            .await?;
        return Ok(Response::ok(value));
    }
    if args.json {
        if args.no_await {
            return Err(ServerError::invalid_params(
                "Strict JSON evaluation awaits completion; no-await conflicts with json",
            ));
        }
        let limit = args.max_size.unwrap_or(limit).min(limit);
        let value: Value = page.evaluate_json(&code, limit).await?;
        return Ok(Response::ok(value));
    }
    let await_promise = !args.no_await;
    let session = page.session().clone();
    if let Some(max) = args.max_size {
        let result: Value = session
            .send(
                "Runtime.evaluate",
                &json!({
                    "expression": build_limited_eval_js(&code, max),
                    "returnByValue": true,
                    "awaitPromise": await_promise,
                }),
            )
            .await?;
        return Ok(Response::ok_text(
            runtime_eval_string_value(&result).map_err(ServerError::internal)?,
        ));
    }
    let result: Value = session
        .send(
            "Runtime.evaluate",
            &json!({
                "expression": code,
                "returnByValue": false,
                "awaitPromise": await_promise,
            }),
        )
        .await?;
    Ok(Response::ok_text(
        format_runtime_eval_result(&session, &result)
            .await
            .map_err(ServerError::internal)?,
    ))
}

pub(crate) async fn execute(page: &eoka::Page, args: &ScriptArgs) -> Result<Response, ServerError> {
    let code = resolve_js(args.file.as_deref(), args.code.as_deref())
        .map_err(ServerError::invalid_params)?;
    check_modes(args)?;
    if args.json {
        return Err(ServerError::invalid_params("json applies only to eval"));
    }
    if args.no_await {
        if !args.raw {
            return Err(ServerError::invalid_params(
                "Function execution awaits completion; no-await also requires raw",
            ));
        }
        page.execute_sync(&code).await?;
        return Ok(Response::err_detail(eoka_protocol::ErrorDetail::new("outcome_unknown", "Raw execution dispatched; asynchronous completion was not awaited. Inspect the page before further input.")));
    }
    if args.raw {
        page.execute(&code).await?;
    } else {
        let code = if args.function {
            code
        } else {
            prepare_script(page, &code, &args.arguments).await?
        };
        page.execute_function(&code, &args.arguments).await?;
    }
    Ok(Response::ok_text("Executed successfully"))
}

fn check_modes(args: &ScriptArgs) -> Result<(), ServerError> {
    if args.function && args.raw {
        return Err(ServerError::invalid_params("function conflicts with raw"));
    }
    if args.json && !args.raw {
        return Err(ServerError::invalid_params("json requires raw"));
    }
    Ok(())
}

async fn format_runtime_eval_result(
    session: &CdpSession,
    result: &Value,
) -> Result<String, String> {
    if let Some(exception) = result.get("exceptionDetails").filter(|v| !v.is_null()) {
        return Err(format_runtime_exception(exception));
    }

    let remote = result
        .get("result")
        .ok_or_else(|| "Runtime.evaluate response missing result".to_string())?;
    format_runtime_remote_object(session, remote).await
}

fn runtime_eval_string_value(result: &Value) -> Result<String, String> {
    if let Some(exception) = result.get("exceptionDetails").filter(|v| !v.is_null()) {
        return Err(format_runtime_exception(exception));
    }

    let remote = result
        .get("result")
        .ok_or_else(|| "Runtime.evaluate response missing result".to_string())?;
    if remote.get("type").and_then(Value::as_str) == Some("undefined") {
        return Ok("undefined".into());
    }
    remote
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Runtime.evaluate did not return a string".to_string())
}

fn format_runtime_exception(exception: &Value) -> String {
    eoka::Error::javascript_exception(exception).to_string()
}

async fn format_runtime_remote_object(
    session: &CdpSession,
    remote: &Value,
) -> Result<String, String> {
    if let Some(text) = runtime_primitive_object_text(remote) {
        return Ok(text);
    }

    if let Some(object_id) = remote.get("objectId").and_then(Value::as_str) {
        let text = stringify_remote_object(session, object_id)
            .await?
            .unwrap_or_else(|| runtime_remote_description(remote));
        let _ = session
            .send::<_, Value>("Runtime.releaseObject", &json!({ "objectId": object_id }))
            .await;
        return Ok(text);
    }

    Ok(runtime_remote_description(remote))
}

fn runtime_primitive_object_text(remote: &Value) -> Option<String> {
    let remote_type = remote.get("type").and_then(Value::as_str).unwrap_or("");
    if remote_type == "undefined" || remote_type == "function" || remote_type == "symbol" {
        return Some("undefined".into());
    }

    if let Some(value) = remote.get("value") {
        return Some(serde_json::to_string(value).unwrap_or_else(|_| value.to_string()));
    }

    if let Some(value) = remote.get("unserializableValue").and_then(Value::as_str) {
        return Some(unserializable_runtime_value_text(value));
    }

    None
}

fn runtime_remote_description(remote: &Value) -> String {
    let remote_type = remote.get("type").and_then(Value::as_str).unwrap_or("");
    remote
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or(remote_type)
        .to_string()
}

async fn stringify_remote_object(
    session: &CdpSession,
    object_id: &str,
) -> Result<Option<String>, String> {
    let result: Value = session
        .send(
            "Runtime.callFunctionOn",
            &json!({
                "objectId": object_id,
                "functionDeclaration": RUNTIME_OBJECT_TO_TEXT_JS,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )
        .await
        .map_err(|e| e.to_string())?;

    if result.get("exceptionDetails").is_some_and(|v| !v.is_null()) {
        return Ok(None);
    }
    Ok(runtime_eval_string_value(&result).ok())
}

const RUNTIME_OBJECT_TO_TEXT_JS: &str = r#"function() {
  let text;
  try {
    text = JSON.stringify(this);
  } catch (error) {
    text = String(this);
  }
  if (text === undefined) {
    text = String(this);
  }
  if (text === undefined) {
    text = "undefined";
  }
  return text;
}"#;

fn unserializable_runtime_value_text(value: &str) -> String {
    match value {
        "NaN" | "Infinity" | "-Infinity" => "null".into(),
        "-0" => "0".into(),
        bigint if bigint.ends_with('n') => bigint.trim_end_matches('n').into(),
        other => other.into(),
    }
}

fn build_limited_eval_js(code: &str, max_size: usize) -> String {
    format!(
        r#"(async () => {{
  const source = {source};
  const value = await Function("return eval(arguments[0])")(source);
  let text;
  try {{
    text = JSON.stringify(value);
  }} catch (error) {{
    text = String(value);
  }}
  if (text === undefined) {{
    text = "undefined";
  }}
  if (text.length <= {max}) {{
    return text;
  }}
  return text.slice(0, {max}) + "...(truncated " + text.length + " chars)";
}})()"#,
        source = json_str(code),
        max = max_size,
    )
}

async fn prepare_script(
    page: &eoka::Page,
    source: &str,
    arguments: &[Value],
) -> Result<String, ServerError> {
    let callable = callable_script(source);
    let callable_error = match syntax_error(page, &callable).await? {
        None => return Ok(callable),
        Some(error) => error,
    };
    if !arguments.is_empty() {
        return Err(ServerError::invalid_params(format!(
            "arguments need code that is a function, such as (a, b) => a + b: {callable_error}"
        )));
    }
    let statements = statement_script(source);
    match syntax_error(page, &statements).await? {
        None => Ok(statements),
        Some(error) => Err(ServerError::invalid_params(format!(
            "JavaScript does not parse: {error}"
        ))),
    }
}

fn callable_script(source: &str) -> String {
    let expression = source.trim_end().trim_end_matches(';');
    format!(
        "(async (...eokaArguments) => {{ const eokaValue = ({expression}\n); if (typeof eokaValue === 'function') return await eokaValue(...eokaArguments); if (eokaArguments.length) throw new TypeError('arguments were given but code is not a function'); return await eokaValue; }})"
    )
}

fn statement_script(source: &str) -> String {
    format!("(async () => {{\n{source}\n}})")
}

async fn syntax_error(page: &eoka::Page, script: &str) -> Result<Option<String>, ServerError> {
    let encoded =
        serde_json::to_string(script).map_err(|error| ServerError::internal(error.to_string()))?;
    Ok(page
        .evaluate_sync(&format!(
            "(() => {{ try {{ new Function({encoded}); return null; }} catch (error) {{ return error instanceof SyntaxError ? String(error) : null; }} }})()"
        ))
        .await?)
}

fn resolve_js(file: Option<&str>, code: Option<&str>) -> Result<String, String> {
    if let Some(path) = file {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read JS file '{}': {}", path, e))
    } else if let Some(code) = code {
        Ok(code.to_string())
    } else {
        Err("Either 'code' or '--file' must be provided".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_eval_formats_undefined() {
        let text = runtime_primitive_object_text(&json!({ "type": "undefined" })).unwrap();

        assert_eq!(text, "undefined");
    }

    #[test]
    fn runtime_eval_formats_string_as_json_string() {
        let text = runtime_primitive_object_text(&json!({ "type": "string", "value": "patched" }))
            .unwrap();

        assert_eq!(text, "\"patched\"");
    }

    #[test]
    fn runtime_eval_formats_remote_object_descriptions() {
        let text = runtime_remote_description(&json!({
            "type": "object",
            "description": "Object"
        }));

        assert_eq!(text, "Object");
    }

    #[test]
    fn runtime_eval_formats_exception_details() {
        let err = format_runtime_exception(&json!({
            "text": "Uncaught",
            "lineNumber": 1,
            "columnNumber": 2
        }));

        assert_eq!(err, "CDP error: JavaScript error: Uncaught at 2:3");
    }

    #[test]
    fn runtime_eval_result_detects_exception_details() {
        let result = json!({
            "exceptionDetails": {
                "text": "Uncaught",
                "lineNumber": 1,
                "columnNumber": 2
            },
            "result": { "type": "object" }
        });
        let err = result
            .get("exceptionDetails")
            .filter(|v| !v.is_null())
            .map(format_runtime_exception)
            .unwrap();

        assert_eq!(err, "CDP error: JavaScript error: Uncaught at 2:3");
    }

    #[test]
    fn runtime_eval_formats_unserializable_values_like_json_stringify() {
        assert_eq!(
            runtime_primitive_object_text(
                &json!({ "type": "number", "unserializableValue": "NaN" })
            ),
            Some("null".to_string())
        );
        assert_eq!(
            runtime_primitive_object_text(
                &json!({ "type": "bigint", "unserializableValue": "123n" })
            ),
            Some("123".to_string())
        );
    }

    #[test]
    fn limited_eval_js_truncates_in_browser() {
        let js = build_limited_eval_js("document.body.innerText", 128);

        assert!(js.contains("Function(\"return eval(arguments[0])\")"));
        assert!(js.contains("text.slice(0, 128)"));
        assert!(js.contains("...(truncated "));
    }

    #[test]
    fn runtime_eval_string_value_extracts_limited_helper_result() {
        let text = runtime_eval_string_value(
            &json!({ "result": { "type": "string", "value": "abc...(truncated 20 chars)" } }),
        )
        .unwrap();

        assert_eq!(text, "abc...(truncated 20 chars)");
    }

    #[tokio::test]
    #[ignore = "requires Chrome"]
    async fn evaluation_preserves_json_and_rejects_non_json_values() {
        let browser = eoka::Browser::launch().await.unwrap();
        let page = browser.new_blank_page().await.unwrap();
        for (expression, expected) in [
            ("null", serde_json::Value::Null),
            ("false", serde_json::json!(false)),
            ("0", serde_json::json!(0)),
            ("''", serde_json::json!("")),
            (
                "Promise.resolve({items:[1,null,true],nested:{value:'ok'}})",
                serde_json::json!({"items":[1,null,true],"nested":{"value":"ok"}}),
            ),
            (
                "(() => { const child = {n:1}; return [child,child]; })()",
                serde_json::json!([{"n":1},{"n":1}]),
            ),
            (
                "({['__proto__']:{safe:true}})",
                serde_json::json!({"__proto__":{"safe":true}}),
            ),
        ] {
            let result: Value = page
                .evaluate_json(expression, SIDECAR_EVALUATION_BYTES)
                .await
                .unwrap();
            assert_eq!(result, expected);
        }
        for expression in [
            "undefined",
            "NaN",
            "Infinity",
            "9007199254740992",
            "1n",
            "() => 1",
            "Symbol('x')",
            "({nested:undefined})",
            "[1,,2]",
            "new Date()",
            "new Map()",
            "({[Symbol('key')]:1})",
            "(() => { const value = {}; value.self = value; return value; })()",
        ] {
            assert!(
                page.evaluate_json::<Value>(expression, SIDECAR_EVALUATION_BYTES)
                    .await
                    .is_err(),
                "{expression}"
            );
        }
        page.execute("undefined").await.unwrap();
        drop(page);
        browser.close().await.unwrap();
    }
}
