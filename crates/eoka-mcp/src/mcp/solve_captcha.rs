use rmcp::handler::server::common::schema_for_type;
use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData, Tool};
use serde::Deserialize;

use super::error::internal;
use super::helpers::text_ok;
use super::EokaServer;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SolveCaptchaRequest {
    #[schemars(description = "Anti-captcha.com API key")]
    pub api_key: String,
    #[schemars(
        description = "Captcha type: 'hcaptcha', 'recaptcha_v2', 'recaptcha_v2_enterprise', 'recaptcha_v3', or 'amazon_waf'"
    )]
    pub captcha_type: String,
    #[schemars(description = "Website/page URL")]
    pub website_url: String,
    #[schemars(description = "Site key for the captcha")]
    pub website_key: String,
    #[schemars(description = "Page action (for reCAPTCHA v3)")]
    pub page_action: Option<String>,
    #[schemars(description = "Minimum score (for reCAPTCHA v3, default 0.3)")]
    pub min_score: Option<f32>,
    #[schemars(
        description = "Optional object passed to grecaptcha.enterprise.render for reCAPTCHA v2 Enterprise"
    )]
    pub enterprise_payload: Option<serde_json::Value>,
    #[schemars(
        description = "Optional reCAPTCHA script domain: www.google.com or www.recaptcha.net"
    )]
    pub api_domain: Option<String>,
    #[schemars(description = "AWS WAF iv value from window.gokuProps (required for amazon_waf)")]
    pub iv: Option<String>,
    #[schemars(
        description = "AWS WAF context value from window.gokuProps (required for amazon_waf)"
    )]
    pub context: Option<String>,
    #[schemars(description = "Optional AWS WAF captcha.js URL")]
    pub captcha_script: Option<String>,
    #[schemars(description = "Optional AWS WAF challenge.js URL")]
    pub challenge_script: Option<String>,
}

pub(super) fn register(router: &mut ToolRouter<EokaServer>) {
    let mut schema = (*schema_for_type::<SolveCaptchaRequest>()).clone();
    schema.remove("title");
    let tool = Tool::new(
        "solve_captcha",
        "Solve hCaptcha, reCAPTCHA, or AWS WAF CAPTCHAs using anti-captcha.com",
        schema,
    );
    router.add_route(ToolRoute::new_dyn(
        tool,
        |context: ToolCallContext<'_, EokaServer>| {
            let arguments = serde_json::Value::Object(context.arguments.unwrap_or_default());
            Box::pin(async move {
                match serde_json::from_value(arguments) {
                    Ok(request) => solve(request).await.map(Into::into),
                    Err(error) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "failed to deserialize parameters: {error}"
                    ))])
                    .into()),
                }
            })
        },
    ));
}

async fn solve(req: SolveCaptchaRequest) -> Result<CallToolResult, ErrorData> {
    let solver = ::eoka_solvers::AntiCaptcha::new(req.api_key);
    let solution = match req.captcha_type.to_lowercase().as_str() {
        "hcaptcha" => solver.solve_hcaptcha(&req.website_url, &req.website_key).await,
        "recaptcha_v2" => {
            solver
                .solve_recaptcha_v2(&req.website_url, &req.website_key)
                .await
        }
        "recaptcha_v2_enterprise" => {
            solver
                .solve_recaptcha_v2_enterprise(
                    &req.website_url,
                    &req.website_key,
                    req.enterprise_payload,
                    req.api_domain.as_deref(),
                )
                .await
        }
        "recaptcha_v3" => {
            let page_action = req.page_action.unwrap_or_else(|| "submit".to_string());
            let min_score = req.min_score.unwrap_or(0.3);
            solver
                .solve_recaptcha_v3(&req.website_url, &req.website_key, &page_action, min_score)
                .await
        }
        "amazon_waf" => {
            let iv = req
                .iv
                .as_deref()
                .ok_or_else(|| internal("amazon_waf requires iv"))?;
            let context = req
                .context
                .as_deref()
                .ok_or_else(|| internal("amazon_waf requires context"))?;
            solver
                .solve_amazon_waf(
                    &req.website_url,
                    &req.website_key,
                    iv,
                    context,
                    req.captcha_script.as_deref(),
                    req.challenge_script.as_deref(),
                )
                .await
        }
        _ => {
            return Err(internal(format!(
                "Unknown captcha type: {}. Use 'hcaptcha', 'recaptcha_v2', 'recaptcha_v2_enterprise', 'recaptcha_v3', or 'amazon_waf'",
                req.captcha_type
            )))
        }
    };
    let solution = solution.map_err(|e| internal(format!("Failed to solve captcha: {e}")))?;
    let token = solution
        .token()
        .ok_or_else(|| internal("Captcha solution contained no token"))?;
    text_ok(
        serde_json::json!({
            "token": token,
            "user_agent": solution.user_agent,
        })
        .to_string(),
    )
}
