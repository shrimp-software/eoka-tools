use eoka::cdp::Session as CdpSession;
use eoka_protocol::{FrameEvalArgs, ScriptArgs};
use serde_json::{json, Value};

use super::target::json_str;
use super::Engine;
use super::EngineError;
use eoka_protocol::Response;

pub(super) enum Script {
    Eval(eoka_protocol::ScriptArgs),
    Exec(eoka_protocol::ScriptArgs),
    FrameEval(eoka_protocol::FrameEvalArgs),
}

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
                    Script::Eval(args) => self.cmd_eval(&args).await,
                    Script::Exec(args) => self.cmd_exec(&args).await,
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

    async fn cmd_eval(&mut self, args: &ScriptArgs) -> Result<Response, EngineError> {
        let code = resolve_js(args.file.as_deref(), args.code.as_deref())?;
        let max_size = args.max_size;
        if !args.raw {
            if args.no_await {
                return Err(
                    "Function evaluation awaits completion; no-await also requires raw".into(),
                );
            }
            let page = &self.require_tab()?.page;
            let code = prepare_script(page, &code, &args.arguments).await?;
            let value: Value = page
                .evaluate_function(&code, &args.arguments)
                .await
                .map_err(|e| e.to_string())?;
            let size = serde_json::to_vec(&value).map_err(|e| e.to_string())?.len();
            if size > max_size.unwrap_or(1_048_576).min(1_048_576) {
                return Err("Evaluation result exceeds its JSON byte limit".into());
            }
            return Ok(Response::ok(value));
        }
        let await_promise = !args.no_await;
        let session = self.require_tab()?.page.session().clone();

        if let Some(max) = max_size {
            let result: Value = session
                .send(
                    "Runtime.evaluate",
                    &json!({
                        "expression": build_limited_eval_js(&code, max),
                        "returnByValue": true,
                        "awaitPromise": await_promise,
                    }),
                )
                .await
                .map_err(|e| e.to_string())?;
            return Ok(Response::ok_text(runtime_eval_string_value(&result)?));
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
            .await
            .map_err(|e| e.to_string())?;
        Ok(Response::ok_text(
            format_runtime_eval_result(&session, &result).await?,
        ))
    }

    async fn cmd_exec(&mut self, args: &ScriptArgs) -> Result<Response, EngineError> {
        let code = resolve_js(args.file.as_deref(), args.code.as_deref())?;
        let tab = self.require_tab()?;
        if args.no_await {
            if !args.raw {
                return Err(
                    "Function execution awaits completion; no-await also requires raw".into(),
                );
            }
            tab.page
                .execute_sync(&code)
                .await
                .map_err(|error| error.to_string())?;
            return Ok(Response::err_detail(eoka_protocol::ErrorDetail::new("outcome_unknown", "Raw execution dispatched; asynchronous completion was not awaited. Inspect the page before further input.")));
        } else if args.raw {
            tab.page
                .execute(&code)
                .await
                .map_err(|error| error.to_string())?;
        } else {
            let code = prepare_script(&tab.page, &code, &args.arguments).await?;
            tab.page
                .execute_function(&code, &args.arguments)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(Response::ok_text("Executed successfully"))
    }

    pub(super) async fn cmd_frames(&mut self) -> Result<Response, EngineError> {
        let page = self.require_tab()?.page.clone();
        let frames = page.frames().await.map_err(|error| error.to_string())?;
        Ok(Response::ok(json!(frames
            .into_iter()
            .map(|frame| { json!({"id":frame.id,"url":frame.url,"name":frame.name}) })
            .collect::<Vec<_>>())))
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
) -> Result<String, EngineError> {
    let callable = callable_script(source);
    let callable_error = match syntax_error(page, &callable).await? {
        None => return Ok(callable),
        Some(error) => error,
    };
    if !arguments.is_empty() {
        return Err(format!(
            "arguments need code that is a function, such as (a, b) => a + b: {callable_error}"
        )
        .into());
    }
    let statements = statement_script(source);
    match syntax_error(page, &statements).await? {
        None => Ok(statements),
        Some(error) => Err(format!("JavaScript does not parse: {error}").into()),
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

async fn syntax_error(page: &eoka::Page, script: &str) -> Result<Option<String>, EngineError> {
    let encoded = serde_json::to_string(script).map_err(|error| error.to_string())?;
    Ok(page
        .evaluate_sync(&format!(
            "(() => {{ try {{ new Function({encoded}); return null; }} catch (error) {{ return error instanceof SyntaxError ? String(error) : null; }} }})()"
        ))
        .await
        .map_err(|error| error.to_string())?)
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
}
