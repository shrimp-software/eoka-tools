use eoka_server::eoka::{Browser, Page, StealthConfig};
use eoka_server::{annotate, observe, spa, InteractiveElement};

async fn open(html: &str) -> (Browser, Page) {
    let browser = Browser::launch_with_config(StealthConfig {
        headless: true,
        geo_align: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let page = browser.new_blank_page().await.unwrap();
    page.goto(&format!("data:text/html,{html}")).await.unwrap();
    (browser, page)
}

fn listing(elements: &[InteractiveElement]) -> String {
    elements.iter().map(|el| format!("{el}\n")).collect()
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn observe_finds_nothing_on_an_empty_page() {
    let (browser, page) = open("").await;

    assert!(observe::observe(&page, true).await.unwrap().is_empty());

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn observe_lists_interactive_elements() {
    let (browser, page) = open(
        r#"<style>body { margin: 0; padding: 20px; }</style>
        <button id="btn1">Click Me</button>
        <input type="text" placeholder="Enter name">
        <a href="https://example.com">Link</a>
        <select><option>Option 1</option></select>"#,
    )
    .await;

    let elements = observe::observe(&page, true).await.unwrap();
    let list = listing(&elements);

    assert!(elements.len() >= 4, "list: {list}");
    assert!(list.contains("<button>"), "list: {list}");
    assert!(list.contains("<input>"), "list: {list}");
    assert!(list.contains("<a>"), "list: {list}");
    assert!(list.contains("<select "), "list: {list}");
    assert!(list.contains("Click Me"), "list: {list}");
    assert!(list.contains("placeholder=\"Enter name\""), "list: {list}");

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn observe_viewport_only_skips_elements_below_the_fold() {
    let (browser, page) = open(
        r#"<style>body { margin: 0; }</style>
        <button style="position:absolute;top:100px">Visible</button>
        <button style="position:absolute;top:5000px">Below Fold</button>"#,
    )
    .await;

    let in_view = observe::observe(&page, true).await.unwrap();
    assert_eq!(in_view.len(), 1);
    assert!(listing(&in_view).contains("Visible"));
    assert_eq!(observe::observe(&page, false).await.unwrap().len(), 2);

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn observe_skips_hidden_elements() {
    let (browser, page) = open(
        r#"<button>Visible</button>
        <button style="display:none">Hidden</button>
        <button style="visibility:hidden">Invisible</button>"#,
    )
    .await;

    let elements = observe::observe(&page, true).await.unwrap();
    assert_eq!(elements.len(), 1);
    assert!(listing(&elements).contains("Visible"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn detect_router_falls_back_to_the_history_api() {
    let (browser, page) = open("<button>Hello</button>").await;

    assert!(spa::detect_router(&page).await.unwrap().can_navigate);

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Chrome"]
async fn annotated_screenshot_is_a_png() {
    let (browser, page) = open("<button>Click</button><input placeholder=Name>").await;

    let elements = observe::observe(&page, true).await.unwrap();
    let png = annotate::annotated_screenshot(&page, &elements)
        .await
        .unwrap();
    assert!(png.starts_with(b"\x89PNG"), "not a PNG");

    browser.close().await.unwrap();
}
