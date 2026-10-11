use crate::eoka::{Page, Result};
use serde::Deserialize;

use crate::InteractiveElement;

#[derive(Deserialize)]
struct RawElement {
    tag: String,
    role: Option<String>,
    text: String,
    placeholder: Option<String>,
    input_type: Option<String>,
    selector: String,
    checked: bool,
    #[serde(default)]
    disabled: bool,
    value: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

const OBSERVE_JS: &str = include_str!("js/observe.js");

pub fn parse_raw_elements(json_str: &str) -> std::result::Result<Vec<InteractiveElement>, String> {
    let raw: Vec<RawElement> =
        serde_json::from_str(json_str).map_err(|e| format!("observe parse error: {}", e))?;

    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let fingerprint = InteractiveElement::compute_fingerprint(
                &r.tag,
                &r.text,
                r.role.as_deref(),
                r.input_type.as_deref(),
                r.placeholder.as_deref(),
                &r.selector,
            );
            InteractiveElement {
                index: i,
                tag: r.tag,
                role: r.role,
                text: r.text,
                placeholder: r.placeholder,
                input_type: r.input_type,
                selector: r.selector,
                checked: r.checked,
                disabled: r.disabled,
                visible: true,
                value: if r.value.is_empty() {
                    None
                } else {
                    Some(r.value)
                },
                bbox: crate::eoka::BoundingBox {
                    x: r.x,
                    y: r.y,
                    width: r.width,
                    height: r.height,
                },
                fingerprint,
            }
        })
        .collect())
}

pub async fn observe(page: &Page, viewport_only: bool) -> Result<Vec<InteractiveElement>> {
    let js = format!(
        "var __eoka_viewport_only = {}; {}",
        viewport_only, OBSERVE_JS
    );
    let json_str: String = page.evaluate(&js).await?;

    parse_raw_elements(&json_str).map_err(crate::eoka::Error::cdp_msg)
}

pub fn validate_filter(filter: Option<&str>) -> std::result::Result<(), String> {
    if let Some(filter) = filter {
        if !matches!(filter, "inputs" | "buttons" | "all") {
            return Err(format!(
                "Unknown filter '{filter}'. Valid: inputs, buttons, all"
            ));
        }
    }
    Ok(())
}

pub async fn inspect(
    page: &Page,
    viewport_only: bool,
    args: &eoka_protocol::ObserveArgs,
) -> std::result::Result<(Vec<InteractiveElement>, eoka_protocol::Response), String> {
    use serde_json::json;
    use std::fmt::Write;
    validate_filter(args.filter.as_deref())?;
    let elements = observe(page, viewport_only)
        .await
        .map_err(|error| error.to_string())?;
    let context = crate::page_context::capture(page).await;
    let mut text = context
        .as_ref()
        .map(crate::page_context::format)
        .unwrap_or_else(|| "Page context unavailable.\n".into());
    let selected: Vec<_> = elements
        .iter()
        .filter(|element| match args.filter.as_deref() {
            Some("inputs") => matches!(
                element.tag.as_str(),
                "input" | "select" | "textarea" | "contenteditable"
            ),
            Some("buttons") => {
                matches!(element.tag.as_str(), "button" | "a")
                    || element.role.as_deref() == Some("button")
            }
            _ => true,
        })
        .take(args.max.unwrap_or(usize::MAX))
        .collect();
    if selected.is_empty() {
        text.push_str("No interactive elements found.\n");
    }
    for element in &selected {
        let _ = writeln!(text, "{element}");
    }
    let response = if args.structured {
        let objects: Vec<_> = selected.into_iter().map(|el| json!({
            "index": el.index, "selector": el.selector, "text": el.text,
            "role": el.role, "tag": el.tag, "visible": el.visible,
            "disabled": el.disabled, "checked": el.checked,
            "placeholder": el.placeholder, "input_type": el.input_type,
            "value": el.value,
            "bbox": {"x":el.bbox.x,"y":el.bbox.y,"width":el.bbox.width,"height":el.bbox.height}
        })).collect();
        eoka_protocol::Response::ok(json!({"text":text,"elements":objects,"page":context}))
    } else {
        eoka_protocol::Response::ok_text(text)
    };
    Ok((elements, response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_button() {
        let json = r#"[{
            "tag": "button",
            "role": null,
            "text": "Submit",
            "placeholder": null,
            "input_type": null,
            "selector": "button.primary",
            "checked": false,
            "value": "",
            "x": 100,
            "y": 200,
            "width": 80,
            "height": 30
        }]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].index, 0);
        assert_eq!(elements[0].tag, "button");
        assert_eq!(elements[0].text, "Submit");
        assert_eq!(elements[0].selector, "button.primary");
        assert!(elements[0].value.is_none());
    }

    #[test]
    fn parse_input_with_value() {
        let json = r#"[{
            "tag": "input",
            "role": null,
            "text": "Email",
            "placeholder": "Enter email",
            "input_type": "email",
            "selector": "input[name=\"email\"]",
            "checked": false,
            "value": "user@test.com",
            "x": 10,
            "y": 20,
            "width": 200,
            "height": 40
        }]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements[0].input_type.as_deref(), Some("email"));
        assert_eq!(elements[0].placeholder.as_deref(), Some("Enter email"));
        assert_eq!(elements[0].value.as_deref(), Some("user@test.com"));
    }

    #[test]
    fn sequential_index_assignment() {
        let json = r#"[
            {"tag":"a","role":null,"text":"Link 1","placeholder":null,"input_type":null,"selector":"a:nth-of-type(1)","checked":false,"value":"","x":0,"y":0,"width":50,"height":20},
            {"tag":"a","role":null,"text":"Link 2","placeholder":null,"input_type":null,"selector":"a:nth-of-type(2)","checked":false,"value":"","x":0,"y":30,"width":50,"height":20},
            {"tag":"button","role":null,"text":"Click","placeholder":null,"input_type":null,"selector":"button","checked":false,"value":"","x":0,"y":60,"width":50,"height":20}
        ]"#;
        let elements = parse_raw_elements(json).unwrap();
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].index, 0);
        assert_eq!(elements[1].index, 1);
        assert_eq!(elements[2].index, 2);
    }

    #[test]
    fn empty_array() {
        let elements = parse_raw_elements("[]").unwrap();
        assert!(elements.is_empty());
    }

    #[test]
    fn invalid_json() {
        assert!(parse_raw_elements("not json").is_err());
        assert!(parse_raw_elements("{\"bad\": true}").is_err());
    }

    #[test]
    fn fingerprint_consistency() {
        let json = r#"[{
            "tag": "button",
            "role": null,
            "text": "Submit",
            "placeholder": null,
            "input_type": null,
            "selector": "button.primary",
            "checked": false,
            "value": "",
            "x": 100,
            "y": 200,
            "width": 80,
            "height": 30
        }]"#;
        let e1 = parse_raw_elements(json).unwrap();
        let e2 = parse_raw_elements(json).unwrap();
        assert_eq!(e1[0].fingerprint, e2[0].fingerprint);
        assert_ne!(e1[0].fingerprint, 0);
    }
}
