use super::*;

impl Engine {
    pub(super) async fn cmd_tab_list(&mut self) -> Result<Response, EngineError> {
        let state = self.state.as_ref().ok_or("No browser open.")?;
        let tabs = state.browser.tabs().await?;
        let current_id = state.current_tab_id.as_deref();

        let mut out = String::new();
        for tab in tabs {
            let marker = if Some(tab.id.as_str()) == current_id {
                " *"
            } else {
                ""
            };
            let _ = writeln!(out, "[{}]{} {}\n  {}", tab.id, marker, tab.title, tab.url);
        }
        Ok(Response::ok_text(if out.is_empty() {
            "No tabs open.".into()
        } else {
            out
        }))
    }

    pub(super) async fn cmd_tab_new(&mut self, args: &TabNewArgs) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let url = args.url.as_deref();
        let active_recorder = self.network.clone();
        let script_enabled = url.and_then(|url| self.script_enabled_for(url));
        let state = self.require_state_mut()?;
        let (tab_id, url, title) = {
            let record_first_navigation = match active_recorder.as_ref() {
                Some(recorder) => recorder.is_active().await,
                None => false,
            };
            let prepared = record_first_navigation || script_enabled.is_some();
            let (tab_id, tab) = if let (Some(url), true) = (url, prepared) {
                let (tab_id, tab) = state.new_tab(None).await?;
                let session = tab.page.session().clone();
                if let Some(recorder) = active_recorder.as_ref().filter(|_| record_first_navigation)
                {
                    recorder.update_session(&session).await?;
                }
                if let Some(enabled) = script_enabled {
                    tab.page.set_javascript_enabled(enabled).await?;
                }
                tab.page.goto(url).await?;
                (tab_id, tab)
            } else {
                state.new_tab(url).await?
            };
            let (url, title) = tab_summary(&tab).await?;
            (tab_id, url, title)
        };
        if let Some(enabled) = script_enabled {
            self.script_policy.note_applied(enabled);
        }
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    pub(super) async fn cmd_tab_switch(
        &mut self,
        args: &TabIdArgs,
    ) -> Result<Response, EngineError> {
        let tab_id = args.tab_id.as_str();
        let state = self.require_state_mut()?;
        state.switch_tab(tab_id).await?;
        let (url, title) = {
            let tab = state.current_tab().ok_or("Tab switch failed")?;
            tab_summary(&tab).await?
        };
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!(
            "Switched to tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    pub(super) async fn cmd_tab_attach(
        &mut self,
        args: &TabIdArgs,
    ) -> Result<Response, EngineError> {
        let tab_id = args.tab_id.clone();
        self.ensure_browser().await?;
        let state = self.require_state_mut()?;
        state.attach_existing_tab(&tab_id).await?;
        let (url, title) = {
            let tab = state.current_tab().ok_or("Attach failed")?;
            tab_summary(&tab).await?
        };
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!(
            "Attached to tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        )))
    }

    pub(super) async fn cmd_clone_from(
        &mut self,
        args: &CloneFromArgs,
    ) -> Result<Response, EngineError> {
        let source = args.source.clone();
        let to = args.to.as_deref().map(std::path::PathBuf::from);

        let saved = persist::clone_state_from_source(&source).await?;

        if let Some(path) = to {
            let json = serde_json::to_string_pretty(&saved)?;
            crate::saved_state::write_state_file(&path, json.as_bytes())?;
            return Ok(Response::ok_text(format!(
                "Saved state from {} to {} ({} cookies, {} localStorage, {} sessionStorage)",
                source,
                path.display(),
                saved.cookies.len(),
                saved.local_storage.len(),
                saved.session_storage.len()
            )));
        }

        self.ensure_browser().await?;
        let state = self.require_state_mut()?;
        let url = saved.url.clone();
        let tab = if !url.is_empty() {
            state.ensure_tab(&url).await?
        } else {
            state.ensure_blank_tab().await?
        };
        let _ = wait_for_stable(&tab.page).await;
        let restored =
            restore_state_and_maybe_reload(&tab, &saved, state_url_can_reload(&url)).await?;
        Ok(Response::ok_text(format!(
            "Hydrated session from {} ({} cookies, {} localStorage, {} sessionStorage).{}",
            source,
            saved.cookies.len(),
            restored.local_restored,
            restored.session_restored,
            restored.notice(&saved.url)
        )))
    }

    pub(super) async fn cmd_tab_close(
        &mut self,
        args: &TabIdArgs,
    ) -> Result<Response, EngineError> {
        let tab_id = args.tab_id.clone();
        let closing_session = self
            .state
            .as_ref()
            .and_then(|s| s.tabs.get(&tab_id))
            .map(|t| t.page.session().clone());
        if let Err(error) = self.recordings.cancel_page(&tab_id).await {
            eprintln!("[eoka] recording cleanup failed: {}", error.message);
        }
        let state = self.require_state_mut()?;
        state.close_tab(&tab_id).await?;
        if let Some(session) = closing_session {
            self.disable_fetch_session(&session).await;
        }
        self.sync_current_target_features().await?;
        Ok(Response::ok_text(format!("Closed tab [{}]", tab_id)))
    }
}
