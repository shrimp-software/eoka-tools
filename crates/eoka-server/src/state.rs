use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;

use eoka::{Browser, Page};

use eoka_protocol::sidecar::ServerError;

#[derive(Default)]
pub struct AppState {
    pub agent: crate::agent::SharedAgentSession,
    pub operations: crate::operations::OperationManager,
    pub recordings: crate::recordings::RecordingManager,
    browser: Option<Arc<Browser>>,
    engine: Option<crate::engine::Engine>,
    pages: HashMap<String, Page>,
    cloudflare: Option<crate::cloudflare::CloudflareWatcher>,
    shutdown: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_browser(&mut self, browser: Browser) {
        self.browser = Some(Arc::new(browser));
    }

    pub fn take_browser(&mut self) -> Option<Arc<Browser>> {
        // The engine shares the browser; release its handle with ours.
        self.engine = None;
        self.browser.take()
    }

    /// The catalog operation engine for this browser, created on first use.
    pub fn engine(&mut self) -> Result<&mut crate::engine::Engine, ServerError> {
        let browser = self.browser()?.clone();
        let operations = self.operations.clone();
        let recordings = self.recordings.clone();
        let agent = self.agent.clone();
        Ok(self.engine.get_or_insert_with(|| {
            crate::engine::Engine::adopt("sidecar", browser, operations, recordings, agent)
        }))
    }

    pub fn is_launched(&self) -> bool {
        self.browser.is_some()
    }

    pub fn browser(&self) -> Result<&Arc<Browser>, ServerError> {
        self.browser.as_ref().ok_or_else(|| {
            ServerError::internal(
                "browser not connected; call browser.launch or browser.connect first",
            )
        })
    }

    pub fn insert_page(&mut self, id: String, page: Page) {
        self.pages.insert(id, page);
        self.publish_pages();
    }

    /// Run one catalog operation on a page through the engine.
    ///
    /// An action that was admitted returns its operation record even when the
    /// outcome is unverified or failed, like `actions.perform`: the record is
    /// the evidence, and the caller must not have to guess the operation ID.
    pub async fn catalog(
        &mut self,
        page_id: &str,
        operation: &eoka_protocol::OperationSpec,
        request: &eoka_protocol::Request,
    ) -> Result<serde_json::Value, ServerError> {
        use eoka_protocol::sidecar::ErrorCode;
        let page = self.page(page_id)?.clone();
        let response = self
            .engine()?
            .handle_on(page_id, &page, operation.cmd, &request.args_json())
            .await;
        if response.ok {
            return Ok(response.data.unwrap_or(serde_json::Value::Null));
        }
        if let Some(record) = response.data.as_ref().filter(|data| {
            data.get("id").is_some_and(|id| id.is_string()) && data.get("state").is_some()
        }) {
            return Ok(record.clone());
        }
        let mut error = match response.error_detail {
            // Core errors keep the kind a registered method would report.
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

    /// Release what the engine attached to a page that is about to close.
    pub async fn release_engine_tab(&mut self, id: &str) {
        if let Some(engine) = &mut self.engine {
            engine.release_tab(id).await;
        }
    }

    pub fn remove_page(&mut self, id: &str) -> Option<Page> {
        if let Some(engine) = &mut self.engine {
            engine.forget_tab(id);
        }
        let page = self.pages.remove(id);
        self.publish_pages();
        page
    }

    /// Solve full-page Cloudflare challenges in every tab until the browser
    /// is cleaned up.
    pub fn start_cloudflare_watcher(&mut self, interval: Duration, timeout: Duration) {
        self.cloudflare = Some(crate::cloudflare::CloudflareWatcher::start(
            interval, timeout,
        ));
        self.publish_pages();
    }

    fn publish_pages(&self) {
        if let Some(watcher) = &self.cloudflare {
            watcher.set_pages(&self.pages);
        }
    }

    pub fn page(&self, id: &str) -> Result<&Page, ServerError> {
        self.pages
            .get(id)
            .ok_or_else(|| ServerError::invalid_page(id))
    }
    pub async fn reconcile_pages(&mut self) -> Result<Vec<eoka::TabInfo>, ServerError> {
        let browser = self.browser()?;
        let mut tabs = browser.tabs().await?;
        let mut attached = Vec::new();
        for tab in &tabs {
            if !self.pages.contains_key(&tab.id) {
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
        let mut release_error = None;
        for (id, page) in &self.pages {
            if !live_tabs.contains(id.as_str()) {
                if let Err(error) = page.release_all_inputs().await {
                    release_error.get_or_insert(error);
                }
            }
        }
        self.pages.retain(|id, _| live_tabs.contains(id.as_str()));
        self.publish_pages();
        for (id, page) in attached {
            if live_tabs.contains(id.as_str()) {
                self.insert_page(id, page);
            }
        }
        tabs.retain(|tab| self.pages.contains_key(&tab.id));
        match release_error {
            Some(error) => Err(error.into()),
            None => Ok(tabs),
        }
    }

    pub async fn cleanup_browser(&mut self) -> Result<(), ServerError> {
        if let Some(watcher) = self.cloudflare.take() {
            watcher.stop().await;
        }
        if let Some(engine) = &mut self.engine {
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
        if let Some(browser) = self.take_browser() {
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

    pub fn pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.values()
    }

    pub fn clear_pages(&mut self) {
        self.pages.clear();
        self.publish_pages();
    }

    pub fn mark_shutdown(&mut self) {
        self.shutdown = true;
    }

    pub fn should_shutdown(&self) -> bool {
        self.shutdown
    }

    pub async fn shutdown(&mut self) -> Result<(), ServerError> {
        self.mark_shutdown();
        self.recordings.request_shutdown();
        let cleanup = async {
            self.operations.stop().await;
            self.cleanup_browser().await
        };
        match tokio::time::timeout(std::time::Duration::from_secs(5), cleanup).await {
            Ok(result) => result,
            Err(_) => {
                self.cloudflare = None;
                self.clear_pages();
                self.take_browser();
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
