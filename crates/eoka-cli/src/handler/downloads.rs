use super::Handler;
use eoka_protocol::{DownloadStartArgs, Response};
use eoka_server::downloads::{failure, rejected, resolve_click, start_response, DownloadManager};
use serde_json::Value;

impl Handler {
    pub(crate) fn download_manager(&self) -> DownloadManager {
        self.downloads.clone()
    }

    pub(super) async fn cmd_download_start(&mut self, value: &Value) -> Response {
        let args: DownloadStartArgs = match serde_json::from_value(value.clone()) {
            Ok(args) => args,
            Err(error) => return rejected(failure("invalid_input", error.to_string())),
        };
        if let Err(error) = args.validate() {
            return rejected(failure("invalid_input", error));
        }
        match self.downloads.existing(&args).await {
            Ok(Some(record)) => return start_response(Ok(record)),
            Ok(None) => {}
            Err(error) => return rejected(error),
        }
        let Some(state) = &self.state else {
            return rejected(failure(
                "session_unavailable",
                "Open the intended page first",
            ));
        };
        if !state.browser.owns_process() {
            return rejected(failure(
                "download_unsupported",
                "Only Eoka-owned Chrome supports downloads",
            ));
        }
        if self.downloads.is_active() {
            return rejected(failure("download_busy", "A download is already active"));
        }
        let selector = if let Some(target) = args.click.as_deref() {
            let (tab, viewport) = match self.tab_with_config() {
                Ok(tab) => tab,
                Err(error) => return rejected(failure("session_unavailable", error)),
            };
            match resolve_click(
                &tab.page,
                &mut tab.elements,
                &tab.snapshot_refs,
                viewport,
                target,
            )
            .await
            {
                Ok(selector) => {
                    tab.invalidate();
                    Some(selector)
                }
                Err(error) => return rejected(error),
            }
        } else {
            None
        };
        let drain = self.start_fetch_drain();
        let result = async {
            let state = self
                .state
                .as_ref()
                .ok_or_else(|| failure("session_unavailable", "No browser open"))?;
            let page = &state
                .current_tab()
                .ok_or_else(|| failure("session_unavailable", "No selected page"))?
                .page;
            self.downloads
                .start(&state.browser, page, args, selector)
                .await
        }
        .await;
        self.stop_fetch_drain(drain).await;
        start_response(result)
    }
}
