use super::state::TabState;
use eoka::Page;

pub async fn click_once(
    tab: &mut TabState,
    target: &str,
    _viewport_only: bool,
) -> Result<String, String> {
    let target =
        eoka_server::operations::commands::live_locator(target).map_err(|e| e.to_string())?;
    tab.page
        .act(
            eoka::Action::Click {
                target,
                human: false,
                double: false,
            },
            Default::default(),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok("Click dispatched".into())
}

pub async fn wait_for_stable(page: &Page) -> Result<(), String> {
    page.wait_for_expectation(&eoka::Expectation::DocumentReady, 10_000)
        .await
        .map_err(|e| e.to_string())
}

pub async fn title_nonblocking(page: &Page) -> String {
    page.evaluate_sync("document.title || ''")
        .await
        .unwrap_or_default()
}

pub fn json_str(s: &str) -> String {
    serde_json::to_string(s).expect("string serialization is infallible")
}
