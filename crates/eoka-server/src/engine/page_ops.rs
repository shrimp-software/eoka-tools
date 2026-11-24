use std::future::Future;
use std::time::Duration;

use base64::Engine as _;
use eoka::{MouseButton, Page};
use eoka_protocol::sidecar::{ErrorCode, ServerError};
use eoka_protocol::{
    AttributeValue, CaptchaTurnstileResult, EmulateArgs, Existence, FrameSummary, InlineScreenshot,
    PageInfo, Request, Response, ScreenshotArgs, ScreenshotFile, ScreenshotResult, ShadowClick,
    Viewport, WaitArgs, WaitSelectorState,
};
use serde_json::{json, Value};

use crate::artifacts::StagedArtifact;

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
    match request {
        page_request!() | Request::Eval(_) | Request::Exec(_) | Request::Fetch(_) => true,
        Request::Screenshot(args) => !args.annotate,
        Request::Emulate(args) => args.desktop || args.reset,
        _ => false,
    }
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
        Request::Eval(args) => {
            super::eval::evaluate(page, &args, super::eval::SIDECAR_EVALUATION_BYTES).await
        }
        Request::Exec(args) => super::eval::execute(page, &args).await,
        Request::Fetch(args) => match crate::url_policy::ensure_url_allowed(&args.url) {
            Ok(()) => Ok(crate::browser_fetch::fetch(page, args).await),
            Err(error) => Ok(Response::err(error)),
        },
        Request::Screenshot(args) => {
            if let Some(output) = args.output.as_deref() {
                eoka_protocol::sidecar::validate_output_path(output)?;
            }
            ok(screenshot(page, &args).await?)
        }
        Request::Emulate(args) => viewport(page, &args).await,
        other => Err(ServerError::internal(format!(
            "{} does not run on a page alone",
            other.cmd()
        ))),
    }
}

pub(super) async fn screenshot(
    page: &Page,
    args: &ScreenshotArgs,
) -> Result<ScreenshotResult, ServerError> {
    args.validate()?;
    let options = args.options();
    publish_screenshot(args, async {
        Ok(crate::capture::screenshot(page, &options).await?)
    })
    .await
}

pub(super) async fn publish_screenshot(
    args: &ScreenshotArgs,
    capture: impl Future<Output = Result<Vec<u8>, ServerError>>,
) -> Result<ScreenshotResult, ServerError> {
    if args.inline {
        return inline_screenshot(&capture.await?);
    }
    let output = match args.output.as_deref() {
        Some(path) => std::path::absolute(path)
            .map_err(|error| ServerError::invalid_params(error.to_string()))?,
        None => std::env::temp_dir().join(format!(
            "eoka-screenshot-{}.{}",
            uuid::Uuid::new_v4(),
            args.extension()
        )),
    };
    let mut staging = StagedArtifact::prepare(&output.to_string_lossy())
        .await
        .map_err(|error| error.server_error())?;
    let bytes = match capture.await {
        Ok(bytes) => bytes,
        Err(mut error) => {
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
    let size = bytes.len();
    drop(bytes);
    staging.seal().await.map_err(|error| error.server_error())?;
    let mime_type = args.mime_type();
    let artifact = tokio::task::spawn_blocking(move || staging.publish(mime_type, None))
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?
        .map_err(|error| error.server_error())?;
    Ok(ScreenshotResult::File(ScreenshotFile {
        path: artifact.path,
        size,
        mime_type: artifact.mime_type,
        bytes: artifact.bytes,
        elements: None,
    }))
}

fn inline_screenshot(bytes: &[u8]) -> Result<ScreenshotResult, ServerError> {
    if bytes.len() > eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES {
        return Err(ServerError::new(
            ErrorCode::ScreenshotTooLarge,
            "screenshot exceeds the 16 MiB inline limit; write it to a file with output instead",
        ));
    }
    Ok(ScreenshotResult::Inline(InlineScreenshot {
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        elements: None,
    }))
}

pub(super) async fn viewport(page: &Page, args: &EmulateArgs) -> Result<Response, ServerError> {
    if args.reset {
        page.reset_viewport().await?;
        disable_touch(page).await;
        return Ok(Response::ok_text("Device emulation cleared"));
    }
    eoka_protocol::sidecar::validate_viewport(args.width, args.height, args.dpr)?;
    page.set_viewport(args.width, args.height, args.dpr).await?;
    disable_touch(page).await;
    ok(Viewport {
        width: args.width,
        height: args.height,
        dpr: args.dpr,
        mobile: false,
    })
}

async fn disable_touch(page: &Page) {
    let _ = page
        .session()
        .send::<_, Value>(
            "Emulation.setTouchEmulationEnabled",
            &json!({ "enabled": false }),
        )
        .await;
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
        across_navigations(
            timeout,
            || format!("Element with text '{text}' not found within {timeout}ms"),
            || request_new_document(page),
            |left| page.wait_for_text(text, left),
        )
        .await?;
        return Ok(Response::ok_text(format!("Text \"{text}\" appeared")));
    }
    if let Some(url) = args.url.as_deref() {
        page.wait_for_url_contains(url, timeout).await?;
        return Ok(Response::ok_text(format!("URL matched \"{url}\"")));
    }
    if let Some(selector) = args.selector.as_deref() {
        let outcome = match args.state.unwrap_or_default() {
            WaitSelectorState::Attached => {
                across_navigations(
                    timeout,
                    || format!("Element '{selector}' not found within {timeout}ms"),
                    || request_new_document(page),
                    |left| page.wait_for(selector, left),
                )
                .await?;
                "attached"
            }
            WaitSelectorState::Visible => {
                across_navigations(
                    timeout,
                    || format!("Element '{selector}' not visible within {timeout}ms"),
                    || request_new_document(page),
                    |left| page.wait_for_visible(selector, left),
                )
                .await?;
                "visible"
            }
            WaitSelectorState::Hidden => {
                across_navigations(
                    timeout,
                    || format!("Element '{selector}' still visible after {timeout}ms"),
                    || request_new_document(page),
                    |left| page.wait_for_hidden(selector, left),
                )
                .await?;
                "hidden"
            }
        };
        return Ok(Response::ok_text(format!("\"{selector}\" is {outcome}")));
    }
    if args.network_idle.is_some() || args.load.is_some() {
        page.wait_for_network_idle(args.network_idle.unwrap_or(500), timeout)
            .await?;
        return Ok(Response::ok_text("Network idle"));
    }
    Err(ServerError::invalid_params(
        "Provide ms, --text, --url, --selector, or --load",
    ))
}

const NAVIGATION_RETRY: Duration = Duration::from_millis(100);

async fn across_navigations<T, Fut, Recovered>(
    timeout: u64,
    missed: impl FnOnce() -> String,
    recover: impl Fn() -> Recovered,
    wait: impl Fn(u64) -> Fut,
) -> eoka::Result<T>
where
    Fut: Future<Output = eoka::Result<T>>,
    Recovered: Future<Output = ()>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(eoka::Error::Timeout(missed()));
        }
        match wait(left.as_millis() as u64).await {
            Err(error) if navigated(&error) => {
                tokio::time::sleep(NAVIGATION_RETRY).await;
                recover().await;
            }
            Err(eoka::Error::Timeout(_)) => return Err(eoka::Error::Timeout(missed())),
            result => return result,
        }
    }
}

async fn request_new_document(page: &Page) {
    let _ = page
        .session()
        .send::<_, Value>("DOM.getDocument", &json!({ "depth": 0 }))
        .await;
}

fn navigated(error: &eoka::Error) -> bool {
    matches!(error, eoka::Error::Cdp { message, .. } if [
        "Cannot find context with specified id",
        "Execution context was destroyed",
        "Inspected target navigated or closed",
    ]
    .iter()
    .any(|text| message.contains(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inline_screenshot_is_bounded_before_base64_expansion() {
        let bytes = vec![0; eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES + 1];
        let error = inline_screenshot(&bytes).unwrap_err();
        assert_eq!(error.code, ErrorCode::ScreenshotTooLarge);
        assert!(error.message.contains("output"));
        let ScreenshotResult::Inline(value) =
            inline_screenshot(&bytes[..eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES]).unwrap()
        else {
            panic!("an inline screenshot is inline");
        };
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(value.data_base64)
            .unwrap();
        assert_eq!(decoded.len(), eoka_protocol::sidecar::MAX_SCREENSHOT_BYTES);
    }

    #[tokio::test]
    async fn a_dom_wait_retries_a_poll_the_navigation_raced_until_its_deadline() {
        let raced = || eoka::Error::Cdp {
            method: Some("DOM.requestNode".into()),
            code: Some(-32000),
            message: "Cannot find context with specified id".into(),
        };
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let recovered = std::sync::atomic::AtomicUsize::new(0);
        let found = across_navigations(
            5000,
            || "missed".into(),
            || async {
                recovered.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
            |_| {
                let call = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    match call {
                        0 => Err(raced()),
                        _ => Ok(call),
                    }
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(found, 1);
        assert_eq!(recovered.load(std::sync::atomic::Ordering::SeqCst), 1);

        let started = std::time::Instant::now();
        let error = across_navigations(
            300,
            || "Element 'h1' not found within 300ms".into(),
            || async {},
            |_| async { Err::<(), _>(raced()) },
        )
        .await
        .unwrap_err();
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(
            matches!(&error, eoka::Error::Timeout(message) if message == "Element 'h1' not found within 300ms"),
            "{error}"
        );

        let error = across_navigations(
            5000,
            || "missed".into(),
            || async {},
            |_| async { Err::<(), _>(eoka::Error::cdp_msg("SyntaxError: bad selector")) },
        )
        .await
        .unwrap_err();
        assert!(matches!(error, eoka::Error::Cdp { .. }), "{error}");
        assert!(!navigated(&eoka::Error::ElementNotFound("h1".into())));
    }

    #[test]
    fn folded_operations_run_on_their_page() {
        let request = |cmd: &str, args: Value| eoka_protocol::request_from_cmd(cmd, args).unwrap();
        for (cmd, args) in [
            ("eval", json!({"code": "1"})),
            ("exec", json!({"code": "1"})),
            ("fetch", json!({"url": "https://a.test/"})),
            ("screenshot", json!({})),
            ("screenshot", json!({"inline": true})),
            ("emulate", json!({"desktop": true})),
            ("emulate", json!({"reset": true})),
            ("text", json!({})),
        ] {
            assert!(runs_on_page(&request(cmd, args.clone())), "{cmd} {args}");
        }
        for (cmd, args) in [
            ("screenshot", json!({"annotate": true})),
            ("emulate", json!({})),
            ("frame_eval", json!({"frame": "iframe", "code": "1"})),
            ("open", json!({"url": "https://a.test/"})),
        ] {
            assert!(!runs_on_page(&request(cmd, args.clone())), "{cmd} {args}");
        }
    }
}
