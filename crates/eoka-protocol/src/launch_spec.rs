use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
pub enum LaunchSpec {
    Reuse,
    Launch {
        headless: bool,
        from_profile: Option<PathBuf>,
        clone_state_from: Option<String>,
        no_stealth: bool,
        proxy: Option<String>,
        no_js: bool,
        js_allow: Vec<String>,
        js_block: Vec<String>,
        persist: bool,
        geo_align: bool,
        auto_solve_cloudflare: bool,
        sandbox: bool,
    },
    Connect {
        ws_url: String,
    },
}

pub fn resolve_proxy_spec(
    proxy: Option<&str>,
    proxy_file: Option<&Path>,
) -> Result<Option<String>, String> {
    if let Some(proxy) = proxy {
        return Ok(Some(proxy.to_owned()));
    }
    let Some(path) = proxy_file else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read --proxy-file '{}': {}", path.display(), e))?;
    let lines: Vec<&str> = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err(format!(
            "--proxy-file '{}' does not contain a proxy",
            path.display()
        ));
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let idx = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize % lines.len())
        .unwrap_or(0);
    Ok(Some(lines[idx].to_owned()))
}

impl LaunchSpec {
    pub fn configuration_fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(serde_json::to_vec(self).unwrap_or_default())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn has_creation_settings(&self) -> bool {
        match self {
            Self::Reuse => false,
            Self::Connect { .. } => true,
            Self::Launch {
                from_profile,
                clone_state_from,
                no_stealth,
                proxy,
                no_js,
                js_allow,
                js_block,
                persist,
                geo_align,
                auto_solve_cloudflare,
                sandbox,
                ..
            } => {
                from_profile.is_some()
                    || clone_state_from.is_some()
                    || *no_stealth
                    || proxy.is_some()
                    || *no_js
                    || !js_allow.is_empty()
                    || !js_block.is_empty()
                    || *persist
                    || !geo_align
                    || *auto_solve_cloudflare
                    || !sandbox
            }
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, LaunchSpec::Connect { .. })
    }
}

pub fn resolve_cdp_spec(spec: &str) -> Result<String, String> {
    let trimmed = spec.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return auto_connect().map(|(_, url)| url);
    }
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return Ok(trimmed.to_string());
    }
    let port: u16 = trimmed.parse().map_err(|_| {
        format!(
            "Invalid --cdp spec '{}': expected port, ws:// URL, or 'auto'",
            spec
        )
    })?;
    eoka::cdp::discover::discover_browser_ws("127.0.0.1", port)
        .map_err(|e| format!("Failed to discover Chrome on 127.0.0.1:{}: {}", port, e))
}

pub fn auto_connect() -> Result<(u16, String), String> {
    eoka::cdp::discover::auto_connect(9222, 9229)
        .ok_or_else(|| "No running Chrome found on ports 9222-9229".to_string())
}

/// Durable Chrome profile directory for a persisted session.
pub fn profile_dir(session: &str) -> PathBuf {
    if let Ok(base) = std::env::var("EOKA_PROFILE_DIR") {
        return PathBuf::from(base).join(session);
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            if home.is_empty() {
                std::env::temp_dir()
            } else {
                PathBuf::from(home).join(".local/state")
            }
        });
    base.join("eoka/profiles").join(session)
}
