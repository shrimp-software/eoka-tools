use super::{diagnose, failure};
use crate::{observe, target, InteractiveElement, Target};
use eoka::Page;
use eoka_protocol::{ActionStatus, ErrorDetail};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

fn resolution_error(error: impl std::fmt::Display) -> ErrorDetail {
    failure("download_target_resolution_failed", error.to_string())
}

struct RemoteObject {
    page: Page,
    id: String,
}

impl Drop for RemoteObject {
    fn drop(&mut self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let page = self.page.clone();
        let id = std::mem::take(&mut self.id);
        runtime.spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                page.session()
                    .send::<_, Value>("Runtime.releaseObject", &json!({"objectId":id})),
            )
            .await;
        });
    }
}

async fn reference(
    page: &Page,
    refs: &HashMap<String, i64>,
    label: &str,
) -> Result<String, ErrorDetail> {
    let stale = || {
        failure(
            "download_target_stale",
            format!("Ref {label} is stale or missing; take a new snapshot"),
        )
    };
    let backend_id = refs.get(label).ok_or_else(stale)?;
    let resolved: Value = page
        .session()
        .send("DOM.resolveNode", &json!({"backendNodeId":backend_id}))
        .await
        .map_err(|error| match error {
            eoka::Error::Cdp {
                code: Some(-32000), ..
            } => stale(),
            error => resolution_error(error),
        })?;
    let object = RemoteObject {
        page: page.clone(),
        id: resolved["object"]["objectId"]
            .as_str()
            .ok_or_else(stale)?
            .to_owned(),
    };
    let value: Value = page
        .session()
        .send(
            "Runtime.callFunctionOn",
            &json!({
                "objectId":object.id,
                "functionDeclaration":include_str!("../js/download_selector.js"),
                "arguments":[{"objectId":object.id}], "returnByValue":true
            }),
        )
        .await
        .map_err(resolution_error)?;
    if value.get("exceptionDetails").is_some() {
        return Err(resolution_error("Snapshot selector evaluation failed"));
    }
    value["result"]["value"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(stale)
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Availability {
    Ready,
    Missing,
    Hidden,
    Disabled,
}

pub async fn resolve_click(
    page: &Page,
    elements: &mut Vec<InteractiveElement>,
    refs: &HashMap<String, i64>,
    viewport: bool,
    target: &str,
) -> Result<String, ErrorDetail> {
    match resolve_selector(page, elements, refs, viewport, target).await {
        Ok(selector) => Ok(selector),
        Err(error) => Err(diagnose(error, page, ActionStatus::NotAttempted, None).await),
    }
}

async fn resolve_selector(
    page: &Page,
    elements: &mut Vec<InteractiveElement>,
    refs: &HashMap<String, i64>,
    viewport: bool,
    target: &str,
) -> Result<String, ErrorDetail> {
    let parsed = Target::parse(target);
    let cached = matches!(parsed, Target::Index(_) | Target::Ref(_));
    let selector = match parsed {
        Target::Index(index) => {
            if elements.is_empty() {
                *elements = observe::observe(page, viewport)
                    .await
                    .map_err(resolution_error)?;
            }
            elements
                .get(index)
                .ok_or_else(|| {
                    failure(
                        "download_target_stale",
                        format!("Index {index} is unavailable; observe again"),
                    )
                })?
                .selector
                .clone()
        }
        Target::Ref(label) => reference(page, refs, &label).await?,
        Target::Live(pattern) => {
            let resolved = target::resolve(page, &pattern)
                .await
                .map_err(resolution_error)?;
            if !resolved.found {
                return Err(failure(
                    "download_target_not_found",
                    format!("Target {target} not found"),
                ));
            }
            resolved.selector
        }
    };
    let script = format!(
        r#"(() => {{
        const el = document.querySelector({});
        if (!el || !el.isConnected) return 'missing';
        if (el.matches(':disabled') || el.closest('[aria-disabled="true"]')) return 'disabled';
        const rect = el.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return 'hidden';
        for (let node = el; node; node = node.parentElement) {{
            const style = getComputedStyle(node);
            if (node.hidden || style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return 'hidden';
        }}
        return 'ready';
    }})()"#,
        serde_json::to_string(&selector).map_err(resolution_error)?
    );
    match page
        .evaluate_sync::<Availability>(&script)
        .await
        .map_err(resolution_error)?
    {
        Availability::Ready => Ok(selector),
        Availability::Missing => Err(failure(
            if cached {
                "download_target_stale"
            } else {
                "download_target_not_found"
            },
            format!("Target {target} is no longer present"),
        )),
        Availability::Hidden => Err(failure(
            "download_target_hidden",
            format!("Target {target} is hidden"),
        )),
        Availability::Disabled => Err(failure(
            "download_target_disabled",
            format!("Target {target} is disabled"),
        )),
    }
}
