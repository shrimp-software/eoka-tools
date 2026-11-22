use super::*;

impl Engine {
    pub(super) async fn cmd_cookies(&mut self) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        let cookies = tab.page.cookies().await?;
        let cookies: Vec<eoka_protocol::Cookie> = cookies
            .into_iter()
            .map(|c| eoka_protocol::Cookie {
                name: c.name,
                value: c.value,
                domain: c.domain,
                path: c.path,
                http_only: c.http_only,
                secure: c.secure,
                same_site: c.same_site,
                expires: c.expires,
            })
            .collect();
        Ok(typed(cookies))
    }

    pub(super) async fn cmd_set_cookie(
        &mut self,
        args: &SetCookieArgs,
    ) -> Result<Response, EngineError> {
        let name = args.name.as_str();
        let value = args.value.as_str();
        let path = args.path.as_deref().unwrap_or("/");

        let tab = self.require_tab()?;
        let domain = match args.domain.as_deref() {
            Some(d) => d.to_string(),
            None => tab.page.evaluate_sync("location.hostname").await?,
        };

        let cookie = eoka::SessionCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            path: path.to_string(),
            secure: false,
            http_only: false,
            same_site: None,
            expires: None,
        };
        tab.page.set_cookies_bulk(vec![cookie]).await?;
        Ok(Response::ok_text(format!("Set cookie: {}={}", name, value)))
    }

    pub(super) async fn cmd_delete_cookie(
        &mut self,
        args: &DeleteCookieArgs,
    ) -> Result<Response, EngineError> {
        let name = args.name.as_str();
        let tab = self.require_tab()?;
        let domain = match args.domain.as_deref() {
            Some(d) => d.to_string(),
            None => {
                tab.page
                    .evaluate_sync::<String>("location.hostname")
                    .await?
            }
        };
        tab.page
            .session()
            .send::<_, Value>(
                "Network.deleteCookies",
                &json!({ "name": name, "domain": domain }),
            )
            .await?;
        Ok(Response::ok_text(format!("Deleted cookie: {}", name)))
    }

    pub(super) async fn cmd_clear_cookies(&mut self) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        tab.page.clear_all_cookies().await?;
        Ok(Response::ok_text("Cleared all cookies"))
    }

    pub(super) async fn cmd_storage(
        &mut self,
        args: &StorageArgs,
    ) -> Result<Response, EngineError> {
        let key = args.key.as_deref();
        let session = args.session_storage;
        let s = if session {
            "sessionStorage"
        } else {
            "localStorage"
        };
        let tab = self.require_tab()?;

        if let Some(k) = key {
            let js = format!("{}.getItem({})", s, json_str(k));
            let val: Value = tab.page.evaluate(&js).await?;
            Ok(Response::ok(val))
        } else {
            let js = format!("JSON.stringify(Object.fromEntries(Object.entries({})))", s);
            Ok(Response::ok(eval_json_or_string(&tab.page, &js).await?))
        }
    }

    pub(super) async fn cmd_set_storage(
        &mut self,
        args: &SetStorageArgs,
    ) -> Result<Response, EngineError> {
        let key = args.key.as_str();
        let value = args.value.as_str();
        let session = args.session_storage;
        let s = if session {
            "sessionStorage"
        } else {
            "localStorage"
        };
        let tab = self.require_tab()?;
        let js = format!("{}.setItem({}, {})", s, json_str(key), json_str(value));
        tab.page.execute_sync(&js).await?;
        Ok(Response::ok_text(format!(
            "Set {}.{} = \"{}\"",
            s, key, value
        )))
    }

    pub(super) async fn cmd_dump_storage(&mut self) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        let js = "JSON.stringify({\
                    localStorage: Object.fromEntries(Object.entries(localStorage)),\
                    sessionStorage: Object.fromEntries(Object.entries(sessionStorage))\
                })";
        Ok(Response::ok(eval_json_or_string(&tab.page, js).await?))
    }

    pub(super) async fn cmd_save_state(
        &mut self,
        args: &PathArgs,
    ) -> Result<Response, EngineError> {
        let path = args.path.as_str();
        let tab = self.require_tab()?;
        let state = capture_state(&tab.page).await?;
        let json = serde_json::to_string_pretty(&state)?;
        crate::saved_state::write_state_file(std::path::Path::new(path), json.as_bytes())?;
        Ok(Response::ok_text(format!(
            "Saved state to {} ({} cookies, {} localStorage, {} sessionStorage)",
            path,
            state.cookies.len(),
            state.local_storage.len(),
            state.session_storage.len()
        )))
    }

    pub(super) async fn cmd_load_state(
        &mut self,
        args: &LoadStateArgs,
    ) -> Result<Response, EngineError> {
        let path = args.path.as_str();
        let no_navigate = args.no_navigate;

        let saved = read_saved_state_file(path)?;
        if !no_navigate && !saved.url.is_empty() {
            crate::url_policy::ensure_url_allowed(&saved.url)?;
        }

        self.ensure_browser().await?;
        let state = self.require_state_mut()?;

        if !no_navigate && !saved.url.is_empty() {
            let tab = state.ensure_tab(&saved.url).await?;
            let _ = wait_for_stable(&tab.page).await;
        }

        let tab = state.current_tab().ok_or("No tab open.")?;
        let reload_after_restore = if !no_navigate && !saved.url.is_empty() {
            state_url_can_reload(&saved.url)
        } else {
            page_url_can_reload_state(&tab.page).await
        };
        let restored = restore_state_and_maybe_reload(&tab, &saved, reload_after_restore).await?;

        Ok(Response::ok_text(format!(
            "Loaded state from {} ({} cookies, {} localStorage, {} sessionStorage).{}",
            path,
            saved.cookies.len(),
            restored.local_restored,
            restored.session_restored,
            restored.notice(&saved.url)
        )))
    }
}

pub(super) async fn restore_state_and_maybe_reload(
    tab: &crate::tabs::Tab,
    saved: &SavedState,
    reload_after_restore: bool,
) -> Result<crate::saved_state::StorageRestoreReport, String> {
    let restored = restore_state(&tab.page, saved).await?;
    tab.invalidate();

    if reload_after_restore {
        tab.page.reload().await.map_err(|e| e.to_string())?;
        let _ = wait_for_stable(&tab.page).await;
        tab.invalidate();
    }

    Ok(restored)
}

pub(super) async fn page_url_can_reload_state(page: &eoka::Page) -> bool {
    page.url()
        .await
        .map(|url| state_url_can_reload(&url))
        .unwrap_or(false)
}

pub(super) fn state_url_can_reload(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub(super) fn web_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = url[..scheme_end].to_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if authority_end == 0 {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme,
        rest[..authority_end].to_lowercase()
    ))
}

pub(super) fn same_web_origin(left: &str, right: &str) -> bool {
    match (web_origin(left), web_origin(right)) {
        (Some(left_origin), Some(right_origin)) => left_origin == right_origin,
        _ => false,
    }
}

pub(super) fn read_saved_state_file(path: &str) -> Result<SavedState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let saved: SavedState = serde_json::from_str(&json).map_err(|e| format!("{path}: {e}"))?;
    crate::url_policy::ensure_url_allowed(&saved.url).map_err(|e| format!("{path}: {e}"))?;
    Ok(saved)
}

pub(super) fn resolve_open_url_for_state(
    url: &str,
    saved: Option<&SavedState>,
) -> Result<String, String> {
    if url.starts_with('/') && !url.starts_with("//") {
        let saved = saved.ok_or_else(|| {
            "Relative URLs require --load-state with a saved http(s) URL.".to_string()
        })?;
        let origin = web_origin(&saved.url).ok_or_else(|| {
            "Relative URLs require --load-state with a saved http(s) URL.".to_string()
        })?;
        return Ok(format!("{origin}{url}"));
    }
    Ok(url.to_string())
}

pub(super) fn state_should_prime_open(saved: &SavedState, url: &str) -> bool {
    same_web_origin(&saved.url, url)
        && (!saved.local_storage.is_empty() || !saved.session_storage.is_empty())
}

pub(super) fn build_storage_seed_js(saved: &SavedState) -> Option<String> {
    if saved.local_storage.is_empty() && saved.session_storage.is_empty() {
        return None;
    }

    let origin = web_origin(&saved.url)?;
    let local = serde_json::to_string(&saved.local_storage).ok()?;
    let session = serde_json::to_string(&saved.session_storage).ok()?;
    Some(format!(
        "(() => {{\
            if (location.origin !== {origin}) return;\
            try {{ const d = {local}; for (const [k,v] of Object.entries(d)) localStorage.setItem(k,v); }} catch (_e) {{}}\
            try {{ const d = {session}; for (const [k,v] of Object.entries(d)) sessionStorage.setItem(k,v); }} catch (_e) {{}}\
        }})();",
        origin = json_str(&origin)
    ))
}

#[cfg(test)]
mod tests;
