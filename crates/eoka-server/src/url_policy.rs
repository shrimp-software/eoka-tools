use serde_json::Value;

pub const FILE_URL_BLOCKED: &str = "file: URLs are blocked by default because page content can steer an agent into reading local files. Set EOKA_ALLOW_FILE_URLS=1 to allow them.";

fn is_local_file_url(url: &str) -> bool {
    let mut candidate: String = url
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .to_ascii_lowercase();
    loop {
        let inner = candidate
            .strip_prefix("view-source:")
            .or_else(|| candidate.strip_prefix("filesystem:"));
        match inner {
            Some(inner) => candidate = inner.trim_start_matches(|c: char| c <= ' ').to_owned(),
            None => return candidate.starts_with("file:"),
        }
    }
}

fn file_urls_allowed() -> bool {
    std::env::var("EOKA_ALLOW_FILE_URLS").is_ok_and(|value| value == "1" || value == "true")
}

fn contains_local_file_url(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "url" && value.as_str().is_some_and(is_local_file_url))
                || contains_local_file_url(value)
        }),
        Value::Array(items) => items.iter().any(contains_local_file_url),
        _ => false,
    }
}

pub fn ensure_url_allowed(url: &str) -> Result<(), &'static str> {
    if is_local_file_url(url) && !file_urls_allowed() {
        return Err(FILE_URL_BLOCKED);
    }
    Ok(())
}

pub fn ensure_args_allowed(arguments: &Value) -> Result<(), &'static str> {
    if contains_local_file_url(arguments) && !file_urls_allowed() {
        return Err(FILE_URL_BLOCKED);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_file_urls_are_recognised_through_browser_url_cleanup() {
        for url in [
            "file:///etc/passwd",
            "FILE:///etc/passwd",
            "  file:/etc/passwd",
            "\u{1}file:///etc/passwd",
            "fi\tle:///etc/passwd",
            "fi\r\nle:///etc/passwd",
            "view-source:file:///etc/passwd",
            "VIEW-SOURCE: view-source:file:///etc/passwd",
            "view-source:fi\nle:///etc/passwd",
            "filesystem:file:///etc/passwd",
        ] {
            assert!(is_local_file_url(url), "{url}");
        }
        for url in [
            "https://example.com/file:///x",
            "about:blank",
            "data:text/html,file:",
        ] {
            assert!(!is_local_file_url(url), "{url}");
        }
    }

    #[test]
    fn nested_urls_are_scanned() {
        let nested = serde_json::json!({"action":{"kind":"navigate","url":"file:///etc/passwd"}});
        assert!(contains_local_file_url(&nested));
        let listed = serde_json::json!({"steps":[{"url":"https://a.test"},{"url":"FILE:///x"}]});
        assert!(contains_local_file_url(&listed));
        let safe = serde_json::json!({"url":"https://a.test","text":"file:///not-a-url"});
        assert!(!contains_local_file_url(&safe));
    }
}
