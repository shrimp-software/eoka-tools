use super::*;

#[allow(clippy::too_many_arguments)]
fn make_element(
    index: usize,
    tag: &str,
    text: &str,
    role: Option<&str>,
    input_type: Option<&str>,
    placeholder: Option<&str>,
    value: Option<&str>,
    checked: bool,
) -> InteractiveElement {
    let selector = format!("[data-idx=\"{}\"]", index);
    InteractiveElement {
        index,
        tag: tag.to_string(),
        text: text.to_string(),
        role: role.map(|s| s.to_string()),
        input_type: input_type.map(|s| s.to_string()),
        placeholder: placeholder.map(|s| s.to_string()),
        value: value.map(|s| s.to_string()),
        checked,
        selector,
        disabled: false,
        visible: true,
        bbox: BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        },
    }
}

#[test]
fn test_element_display_basic() {
    let el = make_element(0, "button", "Submit", None, None, None, None, false);
    assert_eq!(el.to_string(), "[0] <button> \"Submit\"");
}

#[test]
fn test_element_display_with_input_type() {
    let el = make_element(0, "input", "", None, Some("text"), None, None, false);
    assert_eq!(el.to_string(), "[0] <input>");

    let el = make_element(0, "input", "", None, Some("password"), None, None, false);
    assert_eq!(el.to_string(), "[0] <input type=\"password\">");
}

#[test]
fn test_element_display_with_placeholder() {
    let el = make_element(
        0,
        "input",
        "",
        None,
        Some("text"),
        Some("Enter email"),
        None,
        false,
    );
    assert_eq!(el.to_string(), "[0] <input> placeholder=\"Enter email\"");
}

#[test]
fn test_element_display_with_value() {
    let el = make_element(
        0,
        "input",
        "",
        None,
        Some("text"),
        None,
        Some("hello"),
        false,
    );
    assert_eq!(el.to_string(), "[0] <input> value=\"hello\"");
}

#[test]
fn test_element_display_checked() {
    let el = make_element(0, "input", "", None, Some("checkbox"), None, None, true);
    assert_eq!(el.to_string(), "[0] <input type=\"checkbox\"> [checked]");
}

#[test]
fn test_element_display_redundant_role_suppressed() {
    let el = make_element(
        0,
        "button",
        "Click",
        Some("button"),
        None,
        None,
        None,
        false,
    );
    assert_eq!(el.to_string(), "[0] <button> \"Click\"");

    let el = make_element(0, "a", "Link", Some("link"), None, None, None, false);
    assert_eq!(el.to_string(), "[0] <a> \"Link\"");

    let el = make_element(0, "a", "Menu", Some("menuitem"), None, None, None, false);
    assert_eq!(el.to_string(), "[0] <a> \"Menu\"");
}

#[test]
fn test_element_display_non_redundant_role_shown() {
    let el = make_element(0, "button", "Tab 1", Some("tab"), None, None, None, false);
    assert_eq!(el.to_string(), "[0] <button> \"Tab 1\" role=\"tab\"");

    let el = make_element(0, "div", "Click", Some("button"), None, None, None, false);
    assert_eq!(el.to_string(), "[0] <div> \"Click\" role=\"button\"");
}

#[test]
fn test_observe_config_default() {
    let config = ObserveConfig::default();
    assert!(config.viewport_only);
}
