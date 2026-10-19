//! Cloudflare challenge handling shared by the sidecar, CLI daemon and MCP.
//!
//! `solve_turnstile` clicks the Turnstile checkbox through its closed shadow
//! root until the page leaves the challenge. `CloudflareWatcher` runs it
//! automatically for every tab whose title is a full-page challenge.

use std::time::{Duration, Instant};

use eoka::Page;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

const CHALLENGE_FRAME: &str = "challenges.cloudflare.com";
const CHECKBOX: &str = r#"input[type="checkbox"]"#;
const CLICK_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(500);

pub const DEFAULT_SOLVE_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_WATCH_INTERVAL: Duration = Duration::from_millis(500);

/// Full-page Cloudflare challenge titles. A Turnstile widget embedded in a
/// normal form uses the same frame URL, so the title is the discriminator.
pub fn is_challenge_title(title: &str) -> bool {
    let title = title.to_lowercase();
    [
        "just a moment",
        "attention required",
        "checking your browser",
        "verify you are human",
    ]
    .iter()
    .any(|phrase| title.contains(phrase))
        || title
            .find("please wait")
            .is_some_and(|start| title[start..].contains("cloudflare"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnstileReceipt {
    pub clicks: u32,
    pub elapsed: Duration,
}

/// Click the Turnstile checkbox until the page leaves the challenge.
///
/// Returns immediately with zero clicks when the page is not a challenge.
/// Click failures are retried until `timeout`, then reported in the timeout.
pub async fn solve_turnstile(page: &Page, timeout: Duration) -> eoka::Result<TurnstileReceipt> {
    let started = Instant::now();
    let deadline = started + timeout;
    let mut clicks = 0;
    let mut last_error = None;
    loop {
        let frame = page
            .frames()
            .await?
            .into_iter()
            .find(|frame| frame.url.contains(CHALLENGE_FRAME));
        match frame {
            Some(frame) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match page
                    .click_shadow_with_timeout(
                        Some(&frame.id),
                        CHECKBOX,
                        remaining.min(CLICK_TIMEOUT).max(Duration::from_millis(1)),
                    )
                    .await
                {
                    Ok(_) => clicks += 1,
                    Err(error) => last_error = Some(error),
                }
            }
            None if !is_challenge_title(&page.title().await?) => {
                return Ok(TurnstileReceipt {
                    clicks,
                    elapsed: started.elapsed(),
                });
            }
            None => {}
        }
        if Instant::now() + RETRY_INTERVAL >= deadline {
            let cause = last_error
                .map(|error| format!("; last click failed: {error}"))
                .unwrap_or_default();
            return Err(eoka::Error::Timeout(format!(
                "Turnstile challenge did not resolve within {}ms after {clicks} clicks{cause}",
                timeout.as_millis()
            )));
        }
        tokio::time::sleep(RETRY_INTERVAL).await;
    }
}

/// Polls the published pages and solves full-page challenges one at a time.
///
/// It only uses `Page` clones handed to `set_pages`, never its own CDP
/// sessions or the browser handle, so stopping it is enough for browser
/// teardown to proceed.
pub struct CloudflareWatcher {
    pages: watch::Sender<Vec<(String, Page)>>,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl CloudflareWatcher {
    pub fn start(interval: Duration, timeout: Duration) -> Self {
        let (pages, receiver) = watch::channel(Vec::new());
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(watch_pages(receiver, stopped, interval, timeout));
        Self { pages, stop, task }
    }

    pub fn set_pages<'a>(&self, pages: impl IntoIterator<Item = (&'a String, &'a Page)>) {
        self.pages.send_replace(
            pages
                .into_iter()
                .map(|(id, page)| (id.clone(), page.clone()))
                .collect(),
        );
    }

    /// Stop polling and wait for an in-flight solve to observe the signal.
    pub async fn stop(mut self) {
        self.stop.send_replace(true);
        if tokio::time::timeout(Duration::from_secs(2), &mut self.task)
            .await
            .is_err()
        {
            tracing::warn!("Cloudflare watcher did not stop within 2s");
        }
    }
}

impl Drop for CloudflareWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn watch_pages(
    pages: watch::Receiver<Vec<(String, Page)>>,
    mut stopped: watch::Receiver<bool>,
    interval: Duration,
    timeout: Duration,
) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = stopped.changed() => return,
            _ = ticks.tick() => {}
        }
        let current = pages.borrow().clone();
        for (id, page) in current {
            let title = match page.title().await {
                Ok(title) => title,
                Err(error) => {
                    tracing::debug!(page_id = %id, %error, "Cloudflare watcher could not read title");
                    continue;
                }
            };
            if !is_challenge_title(&title) {
                continue;
            }
            tracing::info!(page_id = %id, %title, "Cloudflare challenge detected");
            tokio::select! {
                _ = stopped.changed() => return,
                result = solve_turnstile(&page, timeout) => match result {
                    Ok(receipt) => tracing::info!(page_id = %id, clicks = receipt.clicks, elapsed_ms = receipt.elapsed.as_millis() as u64, "Cloudflare challenge solved"),
                    Err(error) => tracing::warn!(page_id = %id, %error, "Cloudflare challenge was not solved"),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_challenge_title;

    #[test]
    fn challenge_titles_are_detected_case_insensitively() {
        for title in [
            "Just a moment...",
            "Attention Required! | Cloudflare",
            "Checking your browser before accessing example.com",
            "Verify you are human",
            "Please wait while Cloudflare checks your connection",
        ] {
            assert!(is_challenge_title(title), "{title}");
        }
    }

    #[test]
    fn normal_pages_with_turnstile_widgets_are_ignored() {
        for title in [
            "Sign in",
            "Torrent Search Engine | 1337x.to",
            "Cloudflare, please wait",
            "",
        ] {
            assert!(!is_challenge_title(title), "{title}");
        }
    }
}
