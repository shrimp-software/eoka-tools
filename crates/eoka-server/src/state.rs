use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eoka::{Browser, Page};
use parking_lot::Mutex;

use eoka_protocol::sidecar::ServerError;

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
    pages: Mutex<HashMap<String, Page>>,
    engine: tokio::sync::Mutex<Option<crate::engine::Engine>>,
    engine_pages: Mutex<HashSet<String>>,
    cloudflare: Mutex<Option<crate::cloudflare::CloudflareWatcher>>,
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
        self.shared.engine_pages.lock().clear();
        self.shared.browser.lock().take()
    }

    fn abandon_browser(&self) {
        if let Ok(mut engine) = self.shared.engine.try_lock() {
            *engine = None;
        }
        self.shared.engine_pages.lock().clear();
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

    pub fn insert_page(&self, id: String, page: Page) {
        self.shared.pages.lock().insert(id, page);
        self.publish_pages();
    }

    pub async fn open_page(&self) -> Result<Page, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        let page = browser.new_blank_page().await?;
        self.insert_page(page.target_id().to_owned(), page.clone());
        Ok(page)
    }

    pub async fn attach_page(&self, id: &str) -> Result<Page, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        if let Ok(page) = self.page(id) {
            return Ok(page);
        }
        let page = browser.attach_page(id).await?;
        self.insert_page(id.to_owned(), page.clone());
        Ok(page)
    }

    pub async fn catalog(
        &self,
        page_id: &str,
        operation: &eoka_protocol::OperationSpec,
        request: &eoka_protocol::Request,
    ) -> Result<serde_json::Value, ServerError> {
        use eoka_protocol::sidecar::ErrorCode;
        let page = self.page(page_id)?;
        let browser = self.browser()?;
        let response = {
            let mut engine = self.shared.engine.lock().await;
            self.shared.engine_pages.lock().insert(page_id.to_owned());
            engine
                .get_or_insert_with(|| {
                    crate::engine::Engine::adopt(
                        "sidecar",
                        browser,
                        self.operations.clone(),
                        self.recordings.clone(),
                        self.agent.clone(),
                    )
                })
                .handle_on(page_id, &page, operation.cmd, &request.args_json())
                .await
        };
        if response.ok {
            return Ok(response.data.unwrap_or(serde_json::Value::Null));
        }
        if let Some(record) = response.data.as_ref().filter(|data| {
            data.get("id").is_some_and(|id| id.is_string()) && data.get("state").is_some()
        }) {
            return Ok(record.clone());
        }
        let mut error = match response.error_detail {
            Some(detail) => match eoka_protocol::sidecar::core_error_code(&detail.code) {
                Some(code) => ServerError::new(code, detail.message),
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

    pub async fn release_engine_tab(&self, id: &str) {
        if !self.shared.engine_pages.lock().remove(id) {
            return;
        }
        if let Some(engine) = self.shared.engine.lock().await.as_mut() {
            engine.release_tab(id).await;
        }
    }

    pub fn remove_page(&self, id: &str) -> Option<Page> {
        let page = self.shared.pages.lock().remove(id);
        self.publish_pages();
        page
    }

    pub async fn close_page(&self, id: &str) -> Result<(), ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        browser.close_tab(id).await?;
        self.remove_page(id);
        Ok(())
    }

    pub fn start_cloudflare_watcher(&self, interval: Duration, timeout: Duration) {
        *self.shared.cloudflare.lock() = Some(crate::cloudflare::CloudflareWatcher::start(
            interval, timeout,
        ));
        self.publish_pages();
    }

    fn publish_pages(&self) {
        let pages = self.shared.pages.lock();
        if let Some(watcher) = self.shared.cloudflare.lock().as_ref() {
            watcher.set_pages(pages.iter());
        }
    }

    pub fn page(&self, id: &str) -> Result<Page, ServerError> {
        self.shared
            .pages
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| ServerError::invalid_page(id))
    }

    fn has_page(&self, id: &str) -> bool {
        self.shared.pages.lock().contains_key(id)
    }

    pub async fn reconcile_pages(&self) -> Result<Vec<eoka::TabInfo>, ServerError> {
        let browser = self.browser()?;
        let _topology = self.shared.topology.lock().await;
        let mut tabs = browser.tabs().await?;
        let mut attached = Vec::new();
        for tab in &tabs {
            if !self.has_page(&tab.id) {
                match browser.attach_page(&tab.id).await {
                    Ok(page) => attached.push((tab.id.clone(), page)),
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
            .pages
            .lock()
            .iter()
            .filter(|(id, _)| !live_tabs.contains(id.as_str()))
            .map(|(id, page)| (id.clone(), page.clone()))
            .collect();
        let mut release_error = None;
        for (_, page) in &closed {
            if let Err(error) = page.release_all_inputs().await {
                release_error.get_or_insert(error);
            }
        }
        {
            let mut pages = self.shared.pages.lock();
            for (id, _) in &closed {
                pages.remove(id);
            }
            for (id, page) in attached {
                if live_tabs.contains(id.as_str()) {
                    pages.entry(id).or_insert(page);
                }
            }
        }
        self.publish_pages();
        tabs.retain(|tab| self.has_page(&tab.id));
        match release_error {
            Some(error) => Err(error.into()),
            None => Ok(tabs),
        }
    }

    pub async fn cleanup_browser(&self) -> Result<(), ServerError> {
        let watcher = self.shared.cloudflare.lock().take();
        if let Some(watcher) = watcher {
            watcher.stop().await;
        }
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
        self.shared.pages.lock().values().cloned().collect()
    }

    pub fn clear_pages(&self) {
        self.shared.pages.lock().clear();
        self.publish_pages();
    }

    pub fn abandon_cleanup(&self) {
        self.shared.cloudflare.lock().take();
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
