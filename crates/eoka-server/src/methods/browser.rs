use serde_json::{json, Value};

use eoka::Browser;
use eoka_proxy::ProxyConfig;

use super::{parse_params, PageIdParams};
use crate::state::AppState;
use eoka_protocol::sidecar::{
    BrowserOwnership, BrowserSessionResult, CloseTabResult, CloudflareAutoSolve, ConnectParams,
    LaunchParams, NewPageParams, NewPageResult, ProxyParams, ServerError, Tab, TabsResult,
};

fn close_tab_response(cleanup_error: Option<ServerError>) -> Value {
    serde_json::to_value(CloseTabResult {
        cleanup_error: cleanup_error.map(|error| error.message),
    })
    .expect("close tab response is serializable")
}

pub async fn launch(state: &AppState, params: Value) -> Result<Value, ServerError> {
    if state.is_launched() {
        return Err(ServerError::internal("browser already connected"));
    }
    let params: LaunchParams = parse_params(params)?;
    params.validate()?;
    let proxy = resolve_proxy(params.proxy)?;
    let auto_solve = params.auto_solve_cloudflare;
    let browser = Browser::launch_with(|config| {
        config.headless = params.headless;
        config.user_agent = params.user_agent;
        config.timezone = params.timezone;
        config.user_data_dir = params.user_data_dir;
        config.chrome_path = params.executable_path;
        config.runtime_dir = params.runtime_dir;
        config.sandbox = params.sandbox;
        config.aggressive_cdp_evasion = params.aggressive_cdp_evasion;
        config.geo_align = params.geo_align;
        if !params.extra_args.is_empty() {
            config.extra_args = params.extra_args;
        }
        config.device_scale_factor = params.device_scale_factor.unwrap_or(1.0);
        if let Some(width) = params.viewport_width {
            config.viewport_width = width;
        }
        if let Some(height) = params.viewport_height {
            config.viewport_height = height;
        }
        if let Some(proxy) = proxy {
            config.proxy = Some(proxy.server);
            config.proxy_username = proxy.username;
            config.proxy_password = proxy.password;
            config.cdp_timeout = 90;
        }
    })
    .await?;
    initialize_browser(state, browser, auto_solve).await
}

fn validate_endpoint(endpoint: &str) -> Result<(), ServerError> {
    let valid = url::Url::parse(endpoint).is_ok_and(|url| {
        url.scheme() == "ws"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url
                .path()
                .strip_prefix("/devtools/browser/")
                .is_some_and(|id| !id.is_empty() && !id.contains('/'))
    });
    if !valid {
        return Err(ServerError::invalid_params(
            "wsEndpoint must be a ws:// browser DevTools endpoint without credentials, query or fragment; wss:// is unsupported",
        ));
    }
    Ok(())
}

pub async fn connect(state: &AppState, params: Value) -> Result<Value, ServerError> {
    if state.is_launched() {
        return Err(ServerError::internal("browser already connected"));
    }
    let params: ConnectParams = parse_params(params)?;
    params.validate()?;
    validate_endpoint(&params.ws_endpoint)?;
    let browser = Browser::connect(&params.ws_endpoint).await?;
    initialize_browser(state, browser, params.auto_solve_cloudflare).await
}

async fn initialize_browser(
    state: &AppState,
    browser: Browser,
    auto_solve: Option<CloudflareAutoSolve>,
) -> Result<Value, ServerError> {
    let ownership = if browser.owns_process() {
        BrowserOwnership::Owned
    } else {
        BrowserOwnership::Borrowed
    };
    state.set_browser(browser);
    let initialization = async {
        let browser_version = state.browser()?.version().await?;
        state.reconcile_pages().await?;
        serde_json::to_value(BrowserSessionResult {
            ownership,
            browser_version,
        })
        .map_err(|error| ServerError::internal(error.to_string()))
    }
    .await;
    match initialization {
        Ok(result) => {
            if let Some(options) = auto_solve {
                state.start_cloudflare_watcher(
                    std::time::Duration::from_millis(options.interval_ms),
                    std::time::Duration::from_millis(options.timeout_ms),
                );
            }
            Ok(result)
        }
        Err(mut error) => {
            let cleanup =
                tokio::time::timeout(std::time::Duration::from_secs(5), state.cleanup_browser())
                    .await;
            let cleanup_error = match cleanup {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => {
                    state.abandon_cleanup();
                    Some(ServerError::internal(
                        "browser cleanup exceeded the 5 second deadline",
                    ))
                }
            };
            if let Some(cleanup_error) = cleanup_error {
                error.message.push_str("; cleanup failed: ");
                error.message.push_str(&cleanup_error.message);
                let data = error.data.get_or_insert_with(Default::default);
                data.detail.get_or_insert_with(|| {
                    eoka_protocol::ErrorDetail::new("cleanup_failed", cleanup_error.message)
                });
            }
            Err(error)
        }
    }
}

fn resolve_proxy(params: Option<ProxyParams>) -> Result<Option<ProxyConfig>, ServerError> {
    params
        .map(|params| {
            eoka_proxy::parse_server(&params.server, params.username, params.password)
                .map_err(|error| ServerError::invalid_params(error.to_string()))
        })
        .transpose()
}

pub async fn new_page(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: NewPageParams = parse_params(params)?;
    let page = state.open_page().await?;
    if let Some(url) = params.url {
        navigate_opened_page(&page, &url).await?;
    }
    serde_json::to_value(NewPageResult {
        page_id: page.target_id().to_string(),
    })
    .map_err(|error| ServerError::internal(error.to_string()))
}

const OPENED_PAGE_SETTLE: std::time::Duration = std::time::Duration::from_millis(100);

async fn navigate_opened_page(page: &eoka::Page, url: &str) -> Result<(), ServerError> {
    let navigation = page
        .session()
        .send::<_, Value>("Page.navigate", &json!({ "url": url }))
        .await;
    match navigation {
        Ok(result) => {
            if let Some(error) = result["errorText"].as_str() {
                return Err(eoka::Error::Navigation(error.to_owned()).into());
            }
            tokio::time::sleep(OPENED_PAGE_SETTLE).await;
            Ok(())
        }
        Err(error) => {
            crate::operations::abandon_navigation(page).await;
            Err(error.into())
        }
    }
}

pub async fn tabs(state: &AppState, _params: Value) -> Result<Value, ServerError> {
    let tabs = state.reconcile_pages().await?;
    let tabs = tabs
        .into_iter()
        .map(|tab| Tab {
            id: tab.id,
            title: tab.title,
            url: tab.url,
        })
        .collect();
    serde_json::to_value(TabsResult { tabs })
        .map_err(|error| ServerError::internal(error.to_string()))
}

const CLOSING_INPUT_RELEASE: std::time::Duration = std::time::Duration::from_secs(3);

pub(crate) async fn close_tab_impl(
    state: &AppState,
    page_id: &str,
) -> Result<Option<ServerError>, ServerError> {
    let page = state.page(page_id)?;
    let recording_error = state.recordings.cancel_page(page_id).await.err();
    let release_error =
        match tokio::time::timeout(CLOSING_INPUT_RELEASE, page.release_all_inputs()).await {
            Ok(released) => released.err().map(ServerError::from),
            Err(_) => Some(ServerError::new(
                eoka_protocol::sidecar::ErrorCode::Timeout,
                "held input was not released within 3 seconds before the tab closed",
            )),
        };
    state.release_engine_tab(page_id).await;
    state.close_page(page_id).await?;
    Ok(match (recording_error, release_error) {
        (Some(mut recording), Some(release)) => {
            recording.message.push_str("; input cleanup failed: ");
            recording.message.push_str(&release.message);
            Some(recording)
        }
        (Some(recording), None) => Some(recording),
        (None, release) => release,
    })
}

pub async fn close_tab(state: &AppState, params: Value) -> Result<Value, ServerError> {
    let params: PageIdParams = parse_params(params)?;
    let cleanup_error = close_tab_impl(state, &params.page_id).await?;
    Ok(close_tab_response(cleanup_error))
}

pub async fn close(state: &AppState, _params: Value) -> Result<Value, ServerError> {
    if !state.is_launched() {
        return Err(ServerError::internal("browser not launched"));
    }
    state.shutdown().await?;
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_requires_an_uncredentialed_browser_websocket_endpoint() {
        assert!(validate_endpoint("ws://127.0.0.1:9222/devtools/browser/abc").is_ok());
        for endpoint in [
            "http://127.0.0.1:9222/json/version",
            "wss://127.0.0.1/devtools/browser/abc",
            "ws://127.0.0.1:9222/devtools/page/abc",
            "ws://127.0.0.1:9222/devtools/browser/",
            "ws://127.0.0.1:9222/devtools/browser/abc/def",
            "ws://user:private@127.0.0.1:9222/devtools/browser/abc",
            "ws://127.0.0.1:9222/devtools/browser/abc?token=private",
            "ws://127.0.0.1:9222/devtools/browser/abc#fragment",
            "not a URL",
        ] {
            let error = validate_endpoint(endpoint).unwrap_err();
            assert_eq!(error.code, eoka_protocol::sidecar::ErrorCode::InvalidParams);
            assert!(!error.message.contains("private"));
        }
    }

    #[test]
    fn resolves_authenticated_socks5_proxy() {
        let proxy = resolve_proxy(Some(ProxyParams {
            server: "socks5://127.0.0.1:1080".to_owned(),
            username: Some("user".to_owned()),
            password: Some("password".to_owned()),
        }))
        .unwrap()
        .unwrap();
        assert_eq!(proxy.server, "socks5://127.0.0.1:1080");
        assert_eq!(proxy.username.as_deref(), Some("user"));
        assert_eq!(proxy.password.as_deref(), Some("password"));
    }

    #[test]
    fn rejects_proxy_credentials_in_server_url() {
        let error = resolve_proxy(Some(ProxyParams {
            server: "socks5://user:supersensitive@127.0.0.1:1080".to_owned(),
            username: None,
            password: None,
        }))
        .unwrap_err();
        assert!(error.message.contains("credentials"));
        assert!(!error.message.contains("supersensitive"));
    }

    #[test]
    fn close_tab_reports_cleanup_error_after_close() {
        let response = close_tab_response(Some(eoka::Error::cdp_msg("release failed").into()));

        assert!(response["cleanupError"]
            .as_str()
            .unwrap()
            .contains("release failed"));
    }
}
