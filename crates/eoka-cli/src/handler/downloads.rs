use super::{
    target::{auto_observe_if_needed, resolve_target},
    Handler,
};
use eoka_protocol::{DownloadStartArgs, Response};
use eoka_server::downloads::{failure, start_response, DownloadManager};
use serde_json::Value;

impl Handler {
    pub(crate) fn download_manager(&self) -> DownloadManager {
        self.downloads.clone()
    }

    pub(super) async fn cmd_download_start(&mut self, value: &Value) -> Response {
        let args: DownloadStartArgs = match serde_json::from_value(value.clone()) {
            Ok(args) => args,
            Err(error) => return Response::err_detail(failure("invalid_input", error.to_string())),
        };
        if let Err(error) = args.validate() {
            return Response::err_detail(failure("invalid_input", error));
        }
        let Some(state) = &self.state else {
            return Response::err_detail(failure(
                "session_unavailable",
                "Open the intended page first",
            ));
        };
        if !state.browser.owns_process() {
            return Response::err_detail(failure(
                "download_unsupported",
                "Only Eoka-owned Chrome supports downloads",
            ));
        }
        if self.downloads.is_active() {
            return Response::err_detail(failure("download_busy", "A download is already active"));
        }
        let selector = if let Some(target) = args.click.as_deref() {
            let result = async {
                let (tab, viewport) = self.tab_with_config()?;
                auto_observe_if_needed(tab, target, viewport).await?;
                let resolved = resolve_target(tab, target).await?;
                tab.invalidate();
                Ok::<_, String>(resolved.selector)
            }
            .await;
            match result {
                Ok(selector) => Some(selector),
                Err(error) => return Response::err(error),
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
