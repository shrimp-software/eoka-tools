use base64::Engine;
use captcha::{build_captcha_inject_js, AntiCaptcha, CaptchaInjectionKind};
use eoka_protocol::sidecar::{
    AttributeResult, BoundedFetchParams, BoundedFetchResult, CaptureStateResult, ClickShadowParams,
    ClickShadowResult, ContentResult, ErrorCode, EvaluateResult, ExistsResult, FetchParams,
    FetchResult, FrameInfo, FrameJsParams, FramesParams, GetAttributeParams, HeldKeyParams,
    JsParams, MouseButtonParams, MouseMoveParams, RestoreStateParams, ScreenshotParams,
    ScreenshotToFileParams, SelectorParams, ServerError, SetViewportParams, SolveCaptchaParams,
    SolveCaptchaResult, TextResult, TitleResult, UrlResult, WaitForParams, WaitForTextParams,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::browser::close_tab_impl;
use super::{parse_params, PageIdParams};
use crate::state::AppState;

fn core_mouse_button(button: eoka_protocol::MouseButton) -> eoka::MouseButton {
    match button {
        eoka_protocol::MouseButton::Left => eoka::MouseButton::Left,
        eoka_protocol::MouseButton::Middle => eoka::MouseButton::Middle,
        eoka_protocol::MouseButton::Right => eoka::MouseButton::Right,
        eoka_protocol::MouseButton::Back => eoka::MouseButton::Back,
        eoka_protocol::MouseButton::Forward => eoka::MouseButton::Forward,
    }
}

fn encode(value: impl serde::Serialize) -> Result<Value, ServerError> {
    serde_json::to_value(value).map_err(|error| ServerError::internal(error.to_string()))
}

pub async fn set_viewport(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SetViewportParams = parse_params(params)?;
    eoka_protocol::sidecar::validate_viewport(
        params.width,
        params.height,
        params.device_scale_factor,
    )?;
    state
        .page(&params.page_id)?
        .set_viewport(params.width, params.height, params.device_scale_factor)
        .await?;
    Ok(json!({}))
}

pub async fn reset_viewport(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    state.page(&params.page_id)?.reset_viewport().await?;
    Ok(json!({}))
}

pub async fn fetch_bounded(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: BoundedFetchParams = parse_params(params)?;
    params.validate()?;
    let request = params.request;
    let response = state
        .page(&request.page_id)?
        .fetch_bounded(&eoka::BoundedFetchRequest {
            url: request.url,
            method: request.method,
            headers: request.headers.into_iter().collect(),
            body: request.body,
            redirect: request.redirect,
            max_bytes: params.max_bytes,
        })
        .await?;
    encode(BoundedFetchResult {
        url: response.url,
        status: response.status,
        status_text: response.status_text,
        r#type: response.r#type,
        redirected: response.redirected,
        headers: response.headers,
        body: response.body,
        truncated: response.truncated,
        bytes_read: response.bytes_read,
        body_omitted: response.body_omitted,
    })
}

pub async fn text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let text = state.page(&params.page_id)?.text().await?;
    encode(TextResult { text })
}

pub async fn content(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let html = state.page(&params.page_id)?.content().await?;
    encode(ContentResult { html })
}

pub async fn title(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let title = state.page(&params.page_id)?.title().await?;
    encode(TitleResult { title })
}

pub async fn url(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let url = state.page(&params.page_id)?.url().await?;
    encode(UrlResult { url })
}

pub async fn get_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    let page = state.page(&params.page_id)?;
    let element = page.find(&params.selector).await?;
    let text = element.text().await?;
    encode(TextResult { text })
}

pub async fn get_attribute(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: GetAttributeParams = parse_params(params)?;
    let page = state.page(&params.page_id)?;
    let element = page.find(&params.selector).await?;
    let value = element.get_attribute(&params.name).await?;
    encode(AttributeResult { value })
}

pub async fn exists(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SelectorParams = parse_params(params)?;
    let exists = state.page(&params.page_id)?.exists(&params.selector).await;
    encode(ExistsResult { exists })
}

pub async fn wait_for(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for(&params.selector, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn wait_for_visible(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for_visible(&params.selector, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn wait_for_text(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: WaitForTextParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .wait_for_text(&params.text, params.timeout_ms)
        .await?;
    Ok(json!({}))
}

pub async fn evaluate(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: JsParams = parse_params(params)?;
    let script = build_json_evaluation_js(&params.js)?;
    let result: BrowserEvaluation = state.page(&params.page_id)?.evaluate(&script).await?;
    encode(EvaluateResult {
        result: result.into_value()?,
    })
}

pub async fn execute(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: JsParams = parse_params(params)?;
    state.page(&params.page_id)?.execute(&params.js).await?;
    Ok(json!({}))
}

/// List every frame in the page, including cross-origin OOPIFs.
pub async fn frames(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FramesParams = parse_params(params)?;
    let frames: Vec<FrameInfo> = state
        .page(&params.page_id)?
        .frames()
        .await?
        .into_iter()
        .map(|frame| FrameInfo {
            id: frame.id,
            url: frame.url,
            name: frame.name,
        })
        .collect();
    encode(frames)
}

/// Evaluate JavaScript in a frame's isolated world. Runtime.enable is not
/// used, and results get the same JSON normalization and size bound as
/// `page.evaluate`.
pub async fn evaluate_in_frame(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FrameJsParams = parse_params(params)?;
    let script = build_json_evaluation_js(&params.js)?;
    let result: BrowserEvaluation = state
        .page(&params.page_id)?
        .evaluate_in_frame_id(&params.frame_id, &script)
        .await?;
    encode(EvaluateResult {
        result: result.into_value()?,
    })
}

/// Click an element inside a closed shadow root in the top frame or a child
/// frame. Coordinates are mapped and the click uses human-like movement.
pub async fn click_shadow(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: ClickShadowParams = parse_params(params)?;
    let receipt = state
        .page(&params.page_id)?
        .click_shadow_with_timeout(
            params.frame_id.as_deref(),
            &params.selector,
            std::time::Duration::from_millis(params.timeout_ms),
        )
        .await?;
    encode(ClickShadowResult {
        frame_id: receipt.frame_id,
        selector: receipt.selector,
        x: receipt.x,
        y: receipt.y,
    })
}

const MAX_EVALUATION_BYTES: usize = 16 * 1024 * 1024 - 4096;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserEvaluation {
    Value { value: Value },
    TooLarge,
}

impl BrowserEvaluation {
    fn into_value(self) -> Result<Value, ServerError> {
        match self {
            Self::Value { value } => Ok(value),
            Self::TooLarge => Err(ServerError::new(
                ErrorCode::MessageTooLarge,
                format!(
                    "evaluation result exceeds the {MAX_EVALUATION_BYTES} byte CDP payload limit"
                ),
            )),
        }
    }
}

fn build_json_evaluation_js(expression: &str) -> Result<String, ServerError> {
    let expression = serde_json::to_string(expression)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    Ok(format!(
        r#"(async () => {{
            const value = await (0, eval)({expression});
            const ancestors = new WeakSet();
            const normalize = (value) => {{
                if (value === null || typeof value === "string" || typeof value === "boolean") return value;
                if (typeof value === "number") {{
                    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) {{
                        throw new TypeError("Evaluation result contains a non-finite or unsafe number");
                    }}
                    return value;
                }}
                if (typeof value !== "object") throw new TypeError("Evaluation result is not JSON-compatible");
                const prototype = Object.getPrototypeOf(value);
                if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) {{
                    throw new TypeError("Evaluation result contains a non-JSON object");
                }}
                if (Object.getOwnPropertySymbols(value).length !== 0) {{
                    throw new TypeError("Evaluation result contains symbol keys");
                }}
                if (ancestors.has(value)) throw new TypeError("Evaluation result contains a cycle");
                ancestors.add(value);
                const normalized = Array.isArray(value) ? [] : Object.create(null);
                if (Array.isArray(value)) {{
                    for (let index = 0; index < value.length; index++) {{
                        if (!Object.hasOwn(value, index)) throw new TypeError("Evaluation result contains an array hole");
                        normalized.push(normalize(value[index]));
                    }}
                }} else {{
                    for (const key of Object.keys(value)) normalized[key] = normalize(value[key]);
                }}
                ancestors.delete(value);
                return normalized;
            }};
            const normalized = normalize(value);
            const serialized = JSON.stringify(normalized);
            if (serialized.length > {MAX_EVALUATION_BYTES} ||
                new TextEncoder().encode(serialized).byteLength > {MAX_EVALUATION_BYTES}) {{
                return {{kind: "too_large"}};
            }}
            return {{kind: "value", value: normalized}};
        }})()"#
    ))
}

pub async fn fetch(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: FetchParams = parse_params(params)?;
    let script = build_browser_fetch_js(&params)?;
    let body: String = state.page(&params.page_id)?.evaluate(&script).await?;
    let result: FetchResult = serde_json::from_str(&body).map_err(|error| {
        ServerError::internal(format!("invalid browser fetch response: {error}"))
    })?;
    encode(result)
}

fn build_browser_fetch_js(params: &FetchParams) -> Result<String, ServerError> {
    if !matches!(params.redirect.as_str(), "follow" | "error" | "manual") {
        return Err(ServerError::invalid_params(
            "redirect must be follow, error, or manual",
        ));
    }
    let url = serde_json::to_string(&params.url)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let method = serde_json::to_string(&params.method)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let headers = serde_json::to_string(&params.headers)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let body = serde_json::to_string(&params.body)
        .map_err(|error| ServerError::internal(error.to_string()))?;
    let redirect = serde_json::to_string(&params.redirect)
        .map_err(|error| ServerError::internal(error.to_string()))?;

    Ok(format!(
        r#"(async () => {{
            const response = await fetch({url}, {{
                method: {method},
                headers: {headers},
                body: {body},
                credentials: "include",
                redirect: {redirect},
            }});
            const body = await response.text();
            return JSON.stringify({{
                url: response.url,
                status: response.status,
                ok: response.ok,
                headers: Object.fromEntries(response.headers.entries()),
                body,
            }});
        }})()"#
    ))
}

pub async fn capture_state(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let state = state.page(&params.page_id)?.capture_state().await?;
    encode(CaptureStateResult {
        state: state.into(),
    })
}

pub async fn restore_state(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: RestoreStateParams = parse_params(params)?;
    let browser_state = params.state.into();
    state
        .page(&params.page_id)?
        .restore_state(&browser_state)
        .await?;
    Ok(json!({}))
}

fn screenshot_bytes(bytes: &[u8]) -> Result<Value, ServerError> {
    if bytes.len() > eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES {
        return Err(ServerError::new(
            ErrorCode::ScreenshotTooLarge,
            "screenshot exceeds the 16 MiB in-memory limit; use page.screenshot_to_file instead",
        ));
    }
    let mut result = json!({ "dataBase64": null });
    result["dataBase64"] = Value::String(base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(result)
}

pub async fn screenshot(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: ScreenshotParams = parse_params(params)?;
    params.validate()?;
    let bytes = state
        .page(&params.page_id)?
        .screenshot_with_options(&params.options())
        .await?;
    screenshot_bytes(&bytes)
}

pub async fn screenshot_to_file(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: ScreenshotToFileParams = parse_params(params)?;
    params.validate()?;
    let page = state.page(&params.page_id)?;
    let mut staging = crate::artifacts::StagedArtifact::prepare(&params.path)
        .await
        .map_err(|error| error.server_error())?;
    let bytes = match page.screenshot_with_options(&params.options()).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let mut error = ServerError::from(error);
            error.data.get_or_insert_with(Default::default).partial_path =
                Some(staging.partial.to_string_lossy().into_owned());
            if let Err(cleanup) = staging.abandon().await {
                error
                    .message
                    .push_str(&format!("; local file cleanup failed: {}", cleanup.message));
            }
            return Err(error);
        }
    };
    for chunk in bytes.chunks(65_536) {
        staging
            .write(chunk)
            .await
            .map_err(|error| error.server_error())?;
    }
    drop(bytes);
    staging.seal().await.map_err(|error| error.server_error())?;
    let mime_type = match params
        .format
        .unwrap_or(eoka_protocol::sidecar::ScreenshotFormat::Png)
    {
        eoka_protocol::sidecar::ScreenshotFormat::Png => "image/png",
        eoka_protocol::sidecar::ScreenshotFormat::Jpeg => "image/jpeg",
        eoka_protocol::sidecar::ScreenshotFormat::Webp => "image/webp",
    };
    let artifact = tokio::task::spawn_blocking(move || staging.publish(mime_type, None))
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?
        .map_err(|error| error.server_error())?;
    encode(artifact)
}

pub async fn mouse_down(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: MouseButtonParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .mouse_down(
            params.mouse.x,
            params.mouse.y,
            core_mouse_button(params.mouse.button),
        )
        .await?;
    Ok(json!({}))
}

pub async fn mouse_move(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: MouseMoveParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .mouse_move(params.mouse.x, params.mouse.y)
        .await?;
    Ok(json!({}))
}

pub async fn mouse_up(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: MouseButtonParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .mouse_up(
            params.mouse.x,
            params.mouse.y,
            core_mouse_button(params.mouse.button),
        )
        .await?;
    Ok(json!({}))
}

pub async fn key_down(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: HeldKeyParams = parse_params(params)?;
    state
        .page(&params.page_id)?
        .key_down(&params.key.key)
        .await?;
    Ok(json!({}))
}

pub async fn key_up(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: HeldKeyParams = parse_params(params)?;
    state.page(&params.page_id)?.key_up(&params.key.key).await?;
    Ok(json!({}))
}

pub async fn release_all_inputs(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    state.page(&params.page_id)?.release_all_inputs().await?;
    Ok(json!({}))
}

pub async fn solve_captcha(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: SolveCaptchaParams = parse_params(params)?;
    if params.captcha_mode != "anti_captcha_proxyless" {
        return Err(ServerError::invalid_params(
            "captchaMode must explicitly be anti_captcha_proxyless; proxy-backed and manual modes belong to the caller",
        ));
    }
    let solution = match params.captcha_type.as_str() {
        "recaptcha_v2_enterprise" => {
            AntiCaptcha::new(params.api_key)
                .solve_recaptcha_v2_enterprise(
                    &params.website_url,
                    &params.website_key,
                    params.enterprise_payload,
                    params.api_domain.as_deref(),
                )
                .await
        }
        other => {
            return Err(ServerError::invalid_params(format!(
                "unsupported captcha type {other:?}; supported: recaptcha_v2_enterprise"
            )));
        }
    }
    .map_err(|error| ServerError::internal(format!("captcha solve failed: {error}")))?;

    let token = solution
        .token()
        .ok_or_else(|| ServerError::internal("captcha solver returned no token"))?;
    let solver_user_agent = solution.user_agent.clone();
    let script = build_captcha_inject_js(
        token,
        CaptchaInjectionKind::Recaptcha,
        params.callback.as_deref(),
    );
    let page = state.page(&params.page_id)?;
    let mut result = captcha_injection_result(page, &script).await?;
    let callback_count = result
        .get("callbacks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if callback_count <= 1 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let settled = captcha_injection_result(page, &script).await?;
        if settled
            .get("callbacks")
            .and_then(Value::as_array)
            .is_some_and(|callbacks| !callbacks.is_empty())
        {
            result = settled;
        }
    }
    if let Some(user_agent) = solver_user_agent {
        result["solverUserAgent"] = Value::String(user_agent);
    }
    let result: SolveCaptchaResult = serde_json::from_value(json!({ "injection": result }))
        .map_err(|error| ServerError::internal(format!("invalid CAPTCHA result: {error}")))?;
    encode(result)
}

async fn captcha_injection_result(page: &eoka::Page, script: &str) -> Result<Value, ServerError> {
    let result: String = page.evaluate(script).await?;
    serde_json::from_str(&result).map_err(|error| {
        ServerError::internal(format!("invalid CAPTCHA injection result: {error}"))
    })
}

pub async fn close(state: &mut AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    if let Some(error) = close_tab_impl(state, &params.page_id).await? {
        return Err(error);
    }
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    #[test]
    fn screenshot_bound_is_checked_before_base64_expansion() {
        let bytes = vec![0; eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES + 1];
        let error = super::screenshot_bytes(&bytes).unwrap_err();
        assert_eq!(
            error.code,
            eoka_protocol::sidecar::ErrorCode::ScreenshotTooLarge
        );
        assert!(error.message.contains("screenshot_to_file"));
        let value = super::screenshot_bytes(&bytes[..eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES])
            .unwrap();
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(value["dataBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded.len(), eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES);
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
            let script = super::build_json_evaluation_js(expression).unwrap();
            let result: super::BrowserEvaluation = page.evaluate(&script).await.unwrap();
            let super::BrowserEvaluation::Value { value } = result else {
                panic!("small JSON value exceeded the evaluation limit");
            };
            assert_eq!(value, expected);
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
            let script = super::build_json_evaluation_js(expression).unwrap();
            assert!(
                page.evaluate::<super::BrowserEvaluation>(&script)
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
