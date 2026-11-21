use std::time::Duration;

use eoka::{MouseButton, Page};
use eoka_protocol::sidecar::ServerError;
use eoka_protocol::{
    AttributeValue, CaptchaTurnstileResult, Existence, FrameSummary, PageInfo, Request, Response,
    ShadowClick, WaitArgs, WaitSelectorState,
};
use serde_json::Value;

use crate::operations::OperationManager;

macro_rules! page_request {
    () => {
        Request::Text
            | Request::Content
            | Request::Info
            | Request::GetText(_)
            | Request::GetAttribute(_)
            | Request::Exists(_)
            | Request::Frames
            | Request::ClickShadow(_)
            | Request::MouseDown(_)
            | Request::MouseMove(_)
            | Request::MouseUp(_)
            | Request::KeyDown(_)
            | Request::KeyUp(_)
            | Request::ReleaseAllInputs
            | Request::Wait(_)
            | Request::CaptchaTurnstile(_)
    };
}
pub(crate) use page_request;

pub fn runs_on_page(request: &Request) -> bool {
    matches!(request, page_request!())
}

pub async fn on_page(
    operations: &OperationManager,
    page: &Page,
    request: Request,
) -> Result<Value, ServerError> {
    let call = crate::methods::blocked::Call::catalog(&request);
    let work = async {
        let response = execute(page, request).await?;
        crate::state::catalog_result(response, Some(page))
    };
    crate::methods::blocked::guarded(operations, page, call, work).await
}

pub(super) async fn execute(page: &Page, request: Request) -> Result<Response, ServerError> {
    match request {
        Request::Text => Ok(Response::ok_text(page.text().await?)),
        Request::Content => Ok(Response::ok_text(page.content().await?)),
        Request::Info => ok(PageInfo {
            url: page.url().await?,
            title: page.title().await?,
        }),
        Request::GetText(args) => Ok(Response::ok_text(
            page.find(&args.selector).await?.text().await?,
        )),
        Request::GetAttribute(args) => {
            let value = page
                .find(&args.selector)
                .await?
                .get_attribute(&args.name)
                .await?;
            ok(AttributeValue { value })
        }
        Request::Exists(args) => ok(Existence {
            exists: page.exists(&args.selector).await,
        }),
        Request::Frames => {
            let frames: Vec<FrameSummary> = page
                .frames()
                .await?
                .into_iter()
                .map(|frame| FrameSummary {
                    id: frame.id,
                    url: frame.url,
                    name: frame.name,
                })
                .collect();
            ok(frames)
        }
        Request::ClickShadow(args) => {
            let receipt = crate::capture::painting(
                page,
                page.click_shadow_with_timeout(
                    args.frame_id.as_deref(),
                    &args.selector,
                    Duration::from_millis(args.timeout_ms),
                ),
            )
            .await?;
            ok(ShadowClick {
                frame_id: receipt.frame_id,
                selector: receipt.selector,
                x: receipt.x,
                y: receipt.y,
            })
        }
        Request::MouseDown(args) => {
            page.mouse_down(args.x, args.y, button(args.button)).await?;
            Ok(Response::ok_text("Mouse button held"))
        }
        Request::MouseMove(args) => {
            crate::capture::painting(page, page.mouse_move(args.x, args.y)).await?;
            Ok(Response::ok_text("Mouse moved"))
        }
        Request::MouseUp(args) => {
            page.mouse_up(args.x, args.y, button(args.button)).await?;
            Ok(Response::ok_text("Mouse button released"))
        }
        Request::KeyDown(args) => {
            page.key_down(&args.key).await?;
            Ok(Response::ok_text(format!("Held {}", args.key)))
        }
        Request::KeyUp(args) => {
            page.key_up(&args.key).await?;
            Ok(Response::ok_text(format!("Released {}", args.key)))
        }
        Request::ReleaseAllInputs => {
            page.release_all_inputs().await?;
            Ok(Response::ok_text("Released all held inputs"))
        }
        Request::Wait(args) => wait(page, &args).await,
        Request::CaptchaTurnstile(args) => {
            let receipt =
                crate::cloudflare::solve_turnstile(page, Duration::from_millis(args.timeout_ms))
                    .await
                    .map_err(|error| ServerError::internal(error.to_string()))?;
            ok(CaptchaTurnstileResult {
                tab_id: page.target_id().to_string(),
                clicks: receipt.clicks,
                elapsed_ms: receipt.elapsed.as_millis() as u64,
            })
        }
        other => Err(ServerError::internal(format!(
            "{} does not run on a page alone",
            other.cmd()
        ))),
    }
}

fn ok(value: impl serde::Serialize) -> Result<Response, ServerError> {
    serde_json::to_value(value)
        .map(Response::ok)
        .map_err(|error| ServerError::internal(error.to_string()))
}

fn button(button: eoka_protocol::MouseButton) -> MouseButton {
    match button {
        eoka_protocol::MouseButton::Left => MouseButton::Left,
        eoka_protocol::MouseButton::Middle => MouseButton::Middle,
        eoka_protocol::MouseButton::Right => MouseButton::Right,
        eoka_protocol::MouseButton::Back => MouseButton::Back,
        eoka_protocol::MouseButton::Forward => MouseButton::Forward,
    }
}

async fn wait(page: &Page, args: &WaitArgs) -> Result<Response, ServerError> {
    if let Some(ms) = args.ms {
        let ms = ms.min(30_000);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        return Ok(Response::ok_text(format!("Waited {ms}ms")));
    }
    let timeout = args.timeout.unwrap_or(10_000);
    if let Some(expect) = args.expect.as_ref() {
        page.wait_for_expectation(expect, timeout).await?;
        return Ok(Response::ok_text("Readiness condition satisfied"));
    }
    if let Some(text) = args.text.as_deref() {
        page.wait_for_text(text, timeout).await?;
        return Ok(Response::ok_text(format!("Text \"{text}\" appeared")));
    }
    if let Some(url) = args.url.as_deref() {
        page.wait_for_url_contains(url, timeout).await?;
        return Ok(Response::ok_text(format!("URL matched \"{url}\"")));
    }
    if let Some(selector) = args.selector.as_deref() {
        let outcome = match args.state.unwrap_or_default() {
            WaitSelectorState::Attached => {
                page.wait_for(selector, timeout).await?;
                "attached"
            }
            WaitSelectorState::Visible => {
                page.wait_for_visible(selector, timeout).await?;
                "visible"
            }
            WaitSelectorState::Hidden => {
                page.wait_for_hidden(selector, timeout).await?;
                "hidden"
            }
        };
        return Ok(Response::ok_text(format!("\"{selector}\" is {outcome}")));
    }
    if args.load.as_deref().is_some() {
        page.wait_for_network_idle(500, timeout).await?;
        return Ok(Response::ok_text("Network idle"));
    }
    Err(ServerError::invalid_params(
        "Provide ms, --text, --url, --selector, or --load",
    ))
}
