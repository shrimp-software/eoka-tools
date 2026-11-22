use std::collections::HashSet;
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eoka::{Browser, Page};
use parking_lot::Mutex;

use eoka_protocol::sidecar::ServerError;

pub fn catalog_result(
    response: eoka_protocol::Response,
    page: Option<&Page>,
) -> Result<serde_json::Value, ServerError> {
    use eoka_protocol::sidecar::ErrorCode;
    if response.ok {
        return Ok(response.data.unwrap_or(serde_json::Value::Null));
    }
    if let Some(record) = response.data.as_ref().filter(|data| {
        data.get("id").is_some_and(|id| id.is_string()) && data.get("state").is_some()
    }) {
        return Ok(record.clone());
    }
    let mut error = match response.error_detail {
        Some(detail) if ErrorCode::for_blocker_detail(&detail.code).is_some() => match page {
            Some(page) => crate::methods::blocked::blocked_detail(detail, page),
            None => ServerError::blocked(detail, None),
        },
        Some(detail) => match eoka_protocol::sidecar::core_error_code(&detail.code) {
            Some(code) => ServerError::new(code, detail.message.clone()).with_data(
                eoka_protocol::sidecar::ErrorData {
                    detail: Some(detail),
                    ..Default::default()
                },
            ),
            None => ServerError::from_detail(detail),
        },
        None => ServerError::new(
            ErrorCode::Operation,
            response
                .error
                .unwrap_or_else(|| "Operation failed".to_owned()),
        ),
    };
    let operation_id = response.data.as_ref().and_then(|data| {
        data["operation_id"]
            .as_str()
            .or_else(|| data["id"].as_str())
    });
    if let Some(id) = operation_id {
        error.data.get_or_insert_with(Default::default).operation_id = Some(id.to_owned());
    }
    Err(error)
}

#[derive(Clone, Default)]
pub struct AppState {
    pub agent: crate::agent::SharedAgentSession,
    pub operations: crate::operations::OperationManager,
    pub recordings: crate::recordings::RecordingManager,
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    browser: Mutex<Option<Arc<Browser>>>,
    tabs: crate::tabs::Tabs,
    engine: tokio::sync::Mutex<Option<crate::engine::Engine>>,
    topology: tokio::sync::Mutex<()>,
    shutdown: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_browser(&self, browser: Browser) {
        *self.shared.browser.lock() = Some(Arc::new(browser));
    }

    pub async fn take_browser(&self) -> Option<Arc<Browser>> {
        *self.shared.engine.lock().await = None;
        self.shared.browser.lock().take()
    }

    fn abandon_browser(&self) {
        if let Ok(mut engine) = self.shared.engine.try_lock() {
            *engine = None;
        }
        self.shared.browser.lock().take();
    }

    pub fn is_launched(&self) -> bool {
        self.shared.browser.lock().is_some()
    }

    pub fn browser(&self) -> Result<Arc<Browser>, ServerError> {
        self.shared.browser.lock().clone().ok_or_else(|| {
            ServerError::internal(
                "browser not connected; call browser.launch or browser.connect first",
            )
        })
    }

    pub async fn open_page(&self) -> Result<Page, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        let page = browser.new_blank_page().await?;
        self.shared.tabs.insert(page.target_id(), page.clone());
        Ok(page)
    }

    pub async fn attach_page(&self, id: &str) -> Result<Page, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        if let Ok(page) = self.page(id) {
            return Ok(page);
        }
        let page = browser.attach_page(id).await?;
        self.shared.tabs.insert(id, page.clone());
        Ok(page)
    }

    pub async fn catalog(
        &self,
        page_id: &str,
        request: eoka_protocol::Request,
        spoken: Option<&str>,
    ) -> Result<serde_json::Value, ServerError> {
        let page = self.page(page_id)?;
        if crate::engine::page_ops::runs_on_page(&request) {
            return Box::pin(crate::engine::page_ops::on_page(
                &self.operations,
                &page,
                request,
            ))
            .await;
        }
        let response = self.respond(page_id, request, spoken).await?;
        catalog_result(response, Some(&page))
    }

    pub async fn respond(
        &self,
        page_id: &str,
        request: eoka_protocol::Request,
        spoken: Option<&str>,
    ) -> Result<eoka_protocol::Response, ServerError> {
        self.page(page_id)?;
        let browser = self.browser()?;
        let mut engine = self.shared.engine.lock().await;
        let engine = engine.get_or_insert_with(|| {
            crate::engine::Engine::adopt(
                "sidecar",
                browser,
                self.shared.tabs.clone(),
                self.operations.clone(),
                self.recordings.clone(),
                self.agent.clone(),
            )
        });
        Ok(Box::pin(engine.handle_on(page_id, request, spoken)).await)
    }

    pub async fn close_page(&self, id: &str) -> Result<(), ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        browser.close_tab(id).await?;
        self.shared.tabs.remove(id);
        Ok(())
    }

    pub fn start_cloudflare_watcher(&self, interval: Duration, timeout: Duration) {
        self.shared.tabs.watch_cloudflare(interval, timeout);
    }

    pub fn page(&self, id: &str) -> Result<Page, ServerError> {
        self.shared
            .tabs
            .page(id)
            .ok_or_else(|| ServerError::invalid_page(id))
    }

    pub async fn reconcile_pages(&self) -> Result<Vec<eoka::TabInfo>, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        let mut tabs = browser.tabs().await?;
        let mut attached = Vec::new();
        for tab in &tabs {
            if !self.shared.tabs.contains(&tab.id) {
                match browser.attach_page(&tab.id).await {
                    Ok(page) => {
                        attached.push((tab.id.clone(), page));
                    }
                    Err(error) => {
                        if browser
                            .tabs()
                            .await?
                            .iter()
                            .any(|current| current.id == tab.id)
                        {
                            return Err(error.into());
                        }
                    }
                }
            }
        }
        tabs = browser.tabs().await?;
        let live_tabs: HashSet<&str> = tabs.iter().map(|tab| tab.id.as_str()).collect();
        let closed: Vec<(String, Page)> = self
            .shared
            .tabs
            .entries()
            .into_iter()
            .filter(|(id, _)| !live_tabs.contains(id.as_str()))
            .collect();
        let mut release_error = None;
        for (_, page) in &closed {
            if let Err(error) = page.release_all_inputs().await {
                release_error.get_or_insert(error);
            }
        }
        for (id, _) in &closed {
            self.shared.tabs.remove(id);
        }
        for (id, page) in attached {
            if live_tabs.contains(id.as_str()) {
                self.shared.tabs.insert_missing(id, page);
            }
        }
        tabs.retain(|tab| self.shared.tabs.contains(&tab.id));
        match release_error {
            Some(error) => Err(error.into()),
            None => Ok(tabs),
        }
    }

    pub async fn cleanup_browser(&self) -> Result<(), ServerError> {
        let watcher = self.shared.tabs.take_cloudflare();
        if let Some(watcher) = watcher {
            watcher.stop().await;
        }
        crate::capture::quiet().await;
        if let Some(engine) = self.shared.engine.lock().await.as_mut() {
            engine.release().await;
        }
        let mut cleanup_error = self.recordings.shutdown().await.err();
        for page in self.pages() {
            if let Err(error) = page.release_all_inputs().await {
                let error = ServerError::from(error);
                if let Some(existing) = &mut cleanup_error {
                    existing.message.push_str("; input cleanup failed: ");
                    existing.message.push_str(&error.message);
                } else {
                    cleanup_error = Some(error);
                }
            }
        }
        self.clear_pages();
        if let Some(browser) = self.take_browser().await {
            let browser = Arc::try_unwrap(browser).map_err(|_| {
                let mut error = cleanup_error.take().unwrap_or_else(|| {
                    ServerError::internal("browser is still in use during shutdown")
                });
                if error.code != eoka_protocol::sidecar::ErrorCode::Internal {
                    error
                        .message
                        .push_str("; browser is still in use during shutdown");
                }
                error
            })?;
            let result = if browser.owns_process() {
                browser.close().await
            } else {
                browser.disconnect().await
            };
            if let Err(error) = result {
                if let Some(existing) = &mut cleanup_error {
                    let _ = write!(existing.message, "; browser cleanup failed: {error}");
                } else {
                    cleanup_error = Some(error.into());
                }
            }
        }
        match cleanup_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    pub fn pages(&self) -> Vec<Page> {
        self.shared.tabs.pages()
    }

    pub fn clear_pages(&self) {
        self.shared.tabs.clear();
    }

    pub fn abandon_cleanup(&self) {
        self.shared.tabs.take_cloudflare();
        self.clear_pages();
        self.abandon_browser();
    }

    pub fn mark_shutdown(&self) {
        self.shared.shutdown.store(true, Ordering::Release);
    }

    pub fn should_shutdown(&self) -> bool {
        self.shared.shutdown.load(Ordering::Acquire)
    }

    pub async fn shutdown(&self) -> Result<(), ServerError> {
        self.mark_shutdown();
        self.recordings.request_shutdown();
        let cleanup = async {
            self.operations.stop().await;
            self.cleanup_browser().await
        };
        match tokio::time::timeout(Duration::from_secs(5), cleanup).await {
            Ok(result) => result,
            Err(_) => {
                self.abandon_cleanup();
                let mut error = self.recordings.shutdown_timeout_error().unwrap_or_else(|| {
                    ServerError::internal("browser cleanup exceeded the 5 second deadline")
                });
                if error.code != eoka_protocol::sidecar::ErrorCode::Internal {
                    error
                        .message
                        .push_str("; browser cleanup exceeded the 5 second deadline");
                }
                Err(error)
            }
        }
    }
}
