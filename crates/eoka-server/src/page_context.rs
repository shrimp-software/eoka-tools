use eoka::Page;
use eoka_protocol::PageContext;
use std::{fmt::Write, time::Duration};

pub async fn capture(page: &Page) -> Option<PageContext> {
    let mut context: PageContext = tokio::time::timeout(
        Duration::from_secs(2),
        page.evaluate_sync(include_str!("js/page_context.js")),
    )
    .await
    .ok()?
    .ok()?;
    context.tab_id = page.target_id().to_owned();
    Some(context)
}

pub fn format(context: &PageContext) -> String {
    let mut text = format!(
        "URL: {}\nTitle: {}\nDocument: {} (application readiness not inferred)\n",
        context.url, context.title, context.ready_state
    );
    for heading in &context.headings {
        let _ = writeln!(text, "Heading: {heading}");
    }
    for message in &context.messages {
        let _ = writeln!(text, "Page message: {message}");
    }
    if !context.text_excerpt.is_empty() {
        let _ = writeln!(text, "Page text: {}", context.text_excerpt);
    }
    if context.truncated {
        text.push_str("Page context truncated.\n");
    }
    text
}
