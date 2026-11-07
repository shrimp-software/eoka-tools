use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use eoka::Page;

pub fn write_private_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "output needs a filename",
        ));
    }
    let temp = parent.join(format!(".eoka-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            std::fs::rename(&temp, path)
        });
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

fn is_saved_state(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|value| {
        ["url", "cookies", "local_storage", "session_storage"]
            .iter()
            .all(|key| value.get(key).is_some())
    })
}

fn refuse(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("Refusing to write state to {}: {reason}", path.display()),
    )
}

fn ensure_state_target(path: &Path) -> io::Result<()> {
    let is_json = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !is_json {
        return Err(refuse(path, "state files must end in .json"));
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Err(refuse(path, "it is not a regular file"));
    }
    if metadata.len() > 0 && !is_saved_state(&std::fs::read(path)?) {
        return Err(refuse(path, "it exists and is not an eoka state file"));
    }
    Ok(())
}

pub fn write_state_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    ensure_state_target(path)?;
    write_private_atomic(path, bytes)
}

fn web_origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.origin().ascii_serialization())
}

fn storage_restore_script(
    storage: &str,
    origin: &str,
    data: &HashMap<String, String>,
) -> serde_json::Result<String> {
    let origin = serde_json::to_string(origin)?;
    let payload = serde_json::to_string(&serde_json::to_string(data)?)?;
    Ok(format!(
        "(() => {{ if (location.origin !== {origin}) return 'origin_mismatch'; const entries = Object.entries(JSON.parse({payload})); {storage}.clear(); for (const [key, value] of entries) {storage}.setItem(key, value); return 'restored'; }})()"
    ))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StorageRestoreReport {
    pub local_restored: usize,
    pub session_restored: usize,
    pub skipped: usize,
}

impl StorageRestoreReport {
    pub fn notice(&self, saved_url: &str) -> String {
        if self.skipped == 0 {
            return String::new();
        }
        format!(
            " WARNING: skipped {} storage keys because the page is not on the saved origin of {saved_url}; navigate there and load again.",
            self.skipped
        )
    }
}

async fn restore_one(
    page: &Page,
    storage: &str,
    origin: Option<&str>,
    data: &HashMap<String, String>,
) -> eoka::Result<bool> {
    let Some(origin) = origin else {
        return Ok(false);
    };
    let script = storage_restore_script(storage, origin, data)?;
    let outcome: String = page.evaluate_sync(&script).await?;
    Ok(outcome == "restored")
}

#[derive(Debug, serde::Deserialize)]
struct JarCookies {
    cookies: Vec<JarCookie>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct JarCookie {
    name: String,
    domain: String,
    path: String,
}

/// Delete the cookies a saved state replaces: those on a domain it holds cookies
/// for, and those applying to its URL. A state only holds the cookies of the page
/// it was captured from, so other sites' cookies are left alone.
pub async fn clear_covered_cookies<'a>(
    page: &eoka::Page,
    url: &str,
    saved_domains: impl IntoIterator<Item = &'a str>,
) -> eoka::Result<()> {
    let session = page.session();
    let jar: JarCookies = session
        .send("Storage.getCookies", &serde_json::json!({}))
        .await?;
    // Chrome's own matching finds cookies that apply to the saved URL but whose
    // domain isn't in the state, such as a login made after it was saved.
    let at_url = if url.starts_with("http://") || url.starts_with("https://") {
        session
            .send::<_, JarCookies>("Network.getCookies", &serde_json::json!({"urls": [url]}))
            .await?
            .cookies
    } else {
        Vec::new()
    };
    for cookie in cookies_to_clear(jar.cookies, at_url, saved_domains) {
        session
            .send::<_, serde_json::Value>(
                "Network.deleteCookies",
                &serde_json::json!({
                    "name": cookie.name,
                    "domain": cookie.domain,
                    "path": cookie.path,
                }),
            )
            .await?;
    }
    Ok(())
}

fn cookies_to_clear<'a>(
    jar: Vec<JarCookie>,
    at_url: Vec<JarCookie>,
    saved_domains: impl IntoIterator<Item = &'a str>,
) -> Vec<JarCookie> {
    fn site(domain: &str) -> String {
        domain.trim_start_matches('.').to_ascii_lowercase()
    }
    let covered: std::collections::HashSet<String> = saved_domains.into_iter().map(site).collect();
    let mut seen = std::collections::HashSet::new();
    jar.into_iter()
        .filter(|c| covered.contains(&site(&c.domain)))
        .chain(at_url)
        .filter(|c| seen.insert((c.name.clone(), c.domain.clone(), c.path.clone())))
        .collect()
}

pub async fn restore_storage(
    page: &Page,
    saved_url: &str,
    local: &HashMap<String, String>,
    session: &HashMap<String, String>,
) -> eoka::Result<StorageRestoreReport> {
    let origin = web_origin(saved_url);
    let mut report = StorageRestoreReport::default();
    if !local.is_empty() {
        if restore_one(page, "localStorage", origin.as_deref(), local).await? {
            report.local_restored = local.len();
        } else {
            report.skipped += local.len();
        }
    }
    if !session.is_empty() {
        if restore_one(page, "sessionStorage", origin.as_deref(), session).await? {
            report.session_restored = session.len();
        } else {
            report.skipped += session.len();
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar(name: &str, domain: &str) -> JarCookie {
        JarCookie {
            name: name.into(),
            domain: domain.into(),
            path: "/".into(),
        }
    }

    fn cleared(jar: Vec<JarCookie>, at_url: Vec<JarCookie>, domains: &[&str]) -> Vec<String> {
        let mut names: Vec<_> = cookies_to_clear(jar, at_url, domains.iter().copied())
            .into_iter()
            .map(|c| c.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn only_covered_sites_are_cleared() {
        let names = cleared(
            vec![
                jar("stale", "app.test"),
                jar("dotted", ".APP.test"),
                jar("sso", "sso.test"),
                jar("lookalike", "notapp.test"),
                jar("sub", "sub.app.test"),
            ],
            vec![],
            &["app.test"],
        );
        assert_eq!(names, ["dotted", "stale"]);
    }

    #[test]
    fn cookies_applying_to_the_url_are_cleared_once() {
        let names = cleared(
            vec![jar("stale", "app.test"), jar("parent", ".parent.test")],
            vec![jar("stale", "app.test"), jar("parent", ".parent.test")],
            &["app.test"],
        );
        assert_eq!(names, ["parent", "stale"]);
    }

    #[test]
    fn a_state_with_no_cookies_clears_nothing_elsewhere() {
        assert!(cleared(vec![jar("sso", "sso.test")], vec![], &[]).is_empty());
    }

    #[test]
    fn private_write_is_owner_only_and_replaces_lax_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        std::fs::write(&path, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        write_private_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn private_write_replaces_a_symlink_instead_of_following_it() {
        let root = tempfile::tempdir().unwrap();
        let victim = root.path().join("victim");
        std::fs::write(&victim, b"IMPORTANT").unwrap();
        let link = root.path().join("state.json");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        write_private_atomic(&link, b"new").unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"IMPORTANT");
        assert_eq!(std::fs::read(&link).unwrap(), b"new");
    }

    const STATE: &[u8] = br#"{"url":"","cookies":[],"local_storage":{},"session_storage":{}}"#;

    #[test]
    fn state_files_must_use_a_json_name() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "authorized_keys",
            ".bashrc",
            "evil.desktop",
            "state.json.sh",
        ] {
            let path = root.path().join(name);
            let error = write_state_file(&path, STATE).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{name}");
            assert!(!path.exists(), "{name}");
        }
        write_state_file(&root.path().join("NEW.JSON"), STATE).unwrap();
    }

    #[test]
    fn state_writes_only_replace_state_files_or_empty_files() {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("settings.json");
        std::fs::write(&settings, b"{\"theme\":\"dark\"}").unwrap();
        assert!(write_state_file(&settings, STATE).is_err());
        assert_eq!(std::fs::read(&settings).unwrap(), b"{\"theme\":\"dark\"}");

        let previous = root.path().join("previous.json");
        std::fs::write(&previous, STATE).unwrap();
        write_state_file(&previous, b"{}").unwrap();
        assert_eq!(std::fs::read(&previous).unwrap(), b"{}");

        let empty = root.path().join("empty.json");
        std::fs::write(&empty, b"").unwrap();
        write_state_file(&empty, STATE).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn state_writes_refuse_symlinks_and_directories() {
        let root = tempfile::tempdir().unwrap();
        let victim = root.path().join("victim");
        std::fs::write(&victim, STATE).unwrap();
        let link = root.path().join("link.json");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        assert!(write_state_file(&link, b"{}").is_err());
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        let directory = root.path().join("directory.json");
        std::fs::create_dir(&directory).unwrap();
        assert!(write_state_file(&directory, b"{}").is_err());
    }

    #[test]
    fn only_web_urls_have_a_restorable_origin() {
        assert_eq!(
            web_origin("HTTPS://Example.com:443/a?b").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            web_origin("http://127.0.0.1:8080/x").as_deref(),
            Some("http://127.0.0.1:8080")
        );
        for url in ["about:blank", "data:text/html,x", "file:///etc/hosts", ""] {
            assert_eq!(web_origin(url), None, "{url}");
        }
    }

    #[test]
    fn restore_script_checks_origin_before_touching_storage() {
        let data = HashMap::from([("__proto__".to_owned(), "x\"');".to_owned())]);
        let script = storage_restore_script("localStorage", "https://a.test", &data).unwrap();
        let guard = script
            .find("location.origin !== \"https://a.test\"")
            .unwrap();
        assert!(guard < script.find("localStorage.clear").unwrap());
        assert!(script.contains("JSON.parse("));
    }

    #[test]
    fn notice_only_appears_when_keys_were_skipped() {
        assert_eq!(StorageRestoreReport::default().notice("https://a.test"), "");
        let skipped = StorageRestoreReport {
            skipped: 2,
            ..Default::default()
        };
        assert!(skipped.notice("https://a.test").contains("skipped 2"));
    }
}
