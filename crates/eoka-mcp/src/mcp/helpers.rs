use std::fmt::Write;

use eoka_server::eoka::Page;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};

use eoka_server::InteractiveElement;

use super::error::{internal, invalid};
use super::types::JsRequest;

pub(crate) fn resolve_js(req: &JsRequest) -> Result<String, ErrorData> {
    let source = resolve_js_source(req)?;
    if req.raw {
        Ok(source)
    } else {
        Page::function_expression(&source, &req.arguments).map_err(internal)
    }
}

pub(crate) fn resolve_js_source(req: &JsRequest) -> Result<String, ErrorData> {
    if req.no_await && !req.raw {
        return Err(invalid(
            "Function execution awaits completion; no_await requires raw=true",
        ));
    }
    let source = if let Some(path) = &req.file {
        std::fs::read_to_string(path)
            .map_err(|e| invalid(format!("Failed to read JS file '{path}': {e}")))?
    } else if let Some(code) = &req.code {
        code.clone()
    } else {
        return Err(invalid("Either code or file must be provided"));
    };
    Ok(source)
}

pub(crate) fn text_ok(s: impl Into<String>) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![ContentBlock::text(s)]))
}

pub(crate) fn element_list(elements: &[InteractiveElement]) -> String {
    let mut out = String::with_capacity(elements.len() * 40);
    for el in elements {
        let _ = writeln!(out, "{}", el);
    }
    out
}

pub(crate) fn ensure_url_allowed(url: &str) -> Result<(), ErrorData> {
    eoka_server::url_policy::ensure_url_allowed(url).map_err(invalid)
}

pub(crate) fn ensure_action_urls_allowed(arguments: &serde_json::Value) -> Result<(), ErrorData> {
    eoka_server::url_policy::ensure_args_allowed(arguments).map_err(invalid)
}

pub(crate) fn parse_proxy_env() -> eoka_server::eoka::Result<Option<String>> {
    if let Ok(path) = std::env::var("EOKA_PROXY_FILE") {
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
        Ok(std::env::var("EOKA_PROXY").ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eoka_server::InteractiveElement;

    fn make_test_element(index: usize, tag: &str, text: &str) -> InteractiveElement {
        InteractiveElement {
            index,
            tag: tag.to_string(),
            role: None,
            text: text.to_string(),
            placeholder: None,
            input_type: None,
            selector: format!("[data-idx=\"{}\"]", index),
            checked: false,
            disabled: false,
            visible: true,
            value: None,
            bbox: eoka_server::eoka::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 30.0,
            },
            fingerprint: 0,
        }
    }

    #[test]
    fn resolve_js_inline_ok() {
        let req = JsRequest {
            raw: true,
            code: Some("console.log('hi')".into()),
            file: None,
            ..Default::default()
        };
        assert_eq!(resolve_js(&req).unwrap(), "console.log('hi')");
    }

    #[test]
    fn resolve_js_neither_field_errors() {
        let req = JsRequest {
            raw: true,
            code: None,
            file: None,
            ..Default::default()
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn resolve_js_nonexistent_file_errors() {
        let req = JsRequest {
            raw: true,
            code: None,
            file: Some("/nonexistent/path/to/script.js".into()),
            ..Default::default()
        };
        assert!(resolve_js(&req).is_err());
    }

    #[test]
    fn element_list_empty() {
        assert_eq!(element_list(&[]), "");
    }

    #[test]
    fn element_list_with_elements() {
        let els = vec![
            make_test_element(0, "button", "Submit"),
            make_test_element(1, "a", "Link"),
        ];
        let result = element_list(&els);
        assert!(result.contains("[0]"));
        assert!(result.contains("[1]"));
        assert!(result.contains("Submit"));
        assert!(result.contains("Link"));
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn text_ok_returns_success() {
        let result = text_ok("hello").unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(result.content.len(), 1);
    }
}
