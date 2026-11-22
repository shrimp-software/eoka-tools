use eoka_protocol::sidecar::{CaptureStateResult, RestoreStateParams, ServerError};
#[cfg(feature = "anti-captcha")]
use eoka_solvers::{build_captcha_inject_js, CaptchaInjectionKind};
use serde_json::{json, Value};

use super::{parse_params, PageIdParams};
use crate::state::AppState;

fn encode(value: impl serde::Serialize) -> Result<Value, ServerError> {
    serde_json::to_value(value).map_err(|error| ServerError::internal(error.to_string()))
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

#[cfg(feature = "anti-captcha")]
pub async fn solve_captcha(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: eoka_protocol::sidecar::SolveCaptchaParams = parse_params(params)?;
    if params.captcha_mode != "anti_captcha_proxyless" {
        return Err(ServerError::invalid_params(
            "captchaMode must explicitly be anti_captcha_proxyless; proxy-backed and manual modes belong to the caller",
        ));
    }
    let solution = match params.captcha_type.as_str() {
        "recaptcha_v2_enterprise" => {
            eoka_solvers::AntiCaptcha::new(params.api_key)
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
    let mut result = captcha_injection_result(&page, &script).await?;
    let callback_count = result
        .get("callbacks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if callback_count <= 1 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let settled = captcha_injection_result(&page, &script).await?;
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
    let result: eoka_protocol::sidecar::SolveCaptchaResult =
        serde_json::from_value(json!({ "injection": result }))
            .map_err(|error| ServerError::internal(format!("invalid CAPTCHA result: {error}")))?;
    encode(result)
}

#[cfg(feature = "anti-captcha")]
async fn captcha_injection_result(page: &eoka::Page, script: &str) -> Result<Value, ServerError> {
    let result: String = page.evaluate(script).await?;
    serde_json::from_str(&result).map_err(|error| {
        ServerError::internal(format!("invalid CAPTCHA injection result: {error}"))
    })
}
