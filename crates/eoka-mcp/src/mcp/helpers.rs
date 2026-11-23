use rmcp::model::{CallToolResult, ContentBlock, ErrorData};

use super::error::invalid;

pub(crate) fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(s)]))
}

pub(crate) fn ensure_action_urls_allowed(arguments: &serde_json::Value) -> Result<(), ErrorData> {
    eoka_server::url_policy::ensure_args_allowed(arguments).map_err(invalid)
}

pub(crate) fn parse_proxy_env() -> eoka_server::eoka::Result<Option<String>> {
    if let Ok(path) = std::env::var(eoka_protocol::env::PROXY_FILE) {
        let contents = std::fs::read_to_string(path)?;
        let lines: Vec<&str> = contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        if lines.is_empty() {
            return Err(eoka_server::eoka::Error::Launch(
                "proxy file does not contain a proxy".to_owned(),
            ));
        }
        use std::time::SystemTime;
        let index = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as usize % lines.len())
            .unwrap_or(0);
        Ok(Some(lines[index].to_owned()))
    } else {
        Ok(std::env::var(eoka_protocol::env::PROXY).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_ok_returns_success() {
        let result = text_ok("hello").unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(result.content.len(), 1);
    }
}
