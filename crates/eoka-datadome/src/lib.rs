#[cfg(feature = "test-fixtures")]
#[path = "../tests/support/fixture.rs"]
pub mod fixture;

mod detect;
mod extract;
mod slider;

pub mod solve;

use detect::Detection;

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use eoka::{Page, SessionCookie};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveOutcome {
    ChallengeCleared,
    IpBanned,
    Blocked,
    NotPresent,
    Failed,
}

#[derive(Debug)]
pub enum SolveError {
    Browser(eoka::Error),
    Puzzle(String),
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Browser(e) => write!(f, "browser error: {e}"),
            Self::Puzzle(m) => write!(f, "puzzle error: {m}"),
        }
    }
}

impl std::error::Error for SolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Browser(error) => Some(error),
            Self::Puzzle(_) => None,
        }
    }
}

impl From<eoka::Error> for SolveError {
    fn from(e: eoka::Error) -> Self {
        Self::Browser(e)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SliderDrag {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub horizontal_only: bool,
}

pub struct DataDomeSolver {
    pub max_attempts: u32,
    pub cookie_timeout: Duration,
    pub poll_interval: Duration,
    pub widget_settle: Duration,
    pub completion_settle: Duration,
}

impl Default for DataDomeSolver {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            cookie_timeout: Duration::from_secs(10),
            poll_interval: Duration::from_millis(500),
            widget_settle: Duration::from_millis(800),
            completion_settle: Duration::from_secs(2),
        }
    }
}

fn is_block_notice(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim()
            .trim_start_matches(|c: char| !c.is_alphabetic())
            .eq_ignore_ascii_case("you have been blocked")
    })
}

type CookieKey = (String, String, String, String);

fn cookie_key(cookie: &SessionCookie) -> CookieKey {
    (
        cookie.name.clone(),
        cookie.domain.clone(),
        cookie.path.clone(),
        cookie.value.clone(),
    )
}

fn is_new_datadome_cookie(key: &CookieKey, baseline: &HashSet<CookieKey>) -> bool {
    key.0 == "datadome" && !key.3.is_empty() && !baseline.contains(key)
}

impl DataDomeSolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn solve(&self, page: &Page) -> Result<SolveOutcome, SolveError> {
        self.solve_with_drag(page, |drag| async move {
            if drag.horizontal_only {
                page.human()
                    .with_speed(eoka::HumanSpeed::Slow)
                    .drag_horizontal_by(drag.x, drag.y, drag.dx)
                    .await
            } else {
                page.human().drag_by(drag.x, drag.y, drag.dx).await
            }
        })
        .await
    }

    pub async fn solve_with_drag<F, Fut>(
        &self,
        page: &Page,
        mut perform_drag: F,
    ) -> Result<SolveOutcome, SolveError>
    where
        F: FnMut(SliderDrag) -> Fut,
        Fut: std::future::Future<Output = eoka::Result<()>>,
    {
        match detect::detect(page).await? {
            Detection::NotPresent => return Ok(SolveOutcome::NotPresent),
            Detection::IpBanned => return Ok(SolveOutcome::IpBanned),
            Detection::EmbeddedChallenge | Detection::Slider => {}
        }
        let baseline: HashSet<_> = page
            .cookies()
            .await?
            .iter()
            .filter(|cookie| cookie.name == "datadome")
            .map(cookie_key)
            .collect();

        for attempt in 1..=self.max_attempts {
            tracing::debug!(attempt, "datadome: solve attempt");
            sleep(self.widget_settle).await;
            let Some(frame) = page
                .frames()
                .await?
                .into_iter()
                .find(|f| detect::classify_url(&f.url) != Detection::NotPresent)
            else {
                return Ok(SolveOutcome::Failed);
            };
            if detect::classify_url(&frame.url) == Detection::IpBanned {
                return Ok(SolveOutcome::IpBanned);
            }
            let text: String = page
                .evaluate_in_frame_id(&frame.id, "document.body?.innerText || ''")
                .await?;
            if is_block_notice(&text) {
                return Ok(SolveOutcome::Blocked);
            }
            let mut lineage = page.frame_ancestor_ids(&frame.id).await?;
            lineage.push(frame.id.clone());
            if let Some(drag) = slider::simple_geometry(page, &frame.id).await? {
                perform_drag(drag).await?;
                if let Some(outcome) = self.wait_for_completion(page, &baseline, &lineage).await? {
                    return Ok(outcome);
                }
                continue;
            }

            let mut puzzle = None;
            for _ in 0..5 {
                if let Some(p) = extract::extract(page, &frame.id).await? {
                    puzzle = Some(p);
                    break;
                }
                sleep(Duration::from_millis(400)).await;
            }
            let Some(puzzle) = puzzle else {
                tracing::debug!(attempt, "datadome: puzzle images not extractable");
                continue;
            };

            let (puzzle, gap) = solve::match_puzzle(puzzle).await?;
            let gap = gap.ok_or_else(|| SolveError::Puzzle("gap localization failed".into()))?;
            tracing::debug!(
                attempt,
                x = gap.x,
                y = gap.y,
                score = gap.score,
                "datadome: gap localized"
            );
            if gap.score < 0.6 {
                return Err(SolveError::Puzzle(format!(
                    "weak image correlation: {:.3}",
                    gap.score
                )));
            }
            perform_drag(slider::image_geometry(page, &frame.id, &puzzle, &gap).await?).await?;
            if let Some(outcome) = self.wait_for_completion(page, &baseline, &lineage).await? {
                return Ok(outcome);
            }
        }
        Ok(SolveOutcome::Failed)
    }

    async fn wait_for_completion(
        &self,
        page: &Page,
        baseline: &HashSet<CookieKey>,
        lineage: &[String],
    ) -> Result<Option<SolveOutcome>, SolveError> {
        let deadline = tokio::time::Instant::now() + self.cookie_timeout;
        let mut clear_since = None;
        loop {
            let cookies = page.cookies().await?;
            let changed = cookies
                .iter()
                .filter(|cookie| cookie.name == "datadome")
                .any(|c| is_new_datadome_cookie(&cookie_key(c), baseline));
            let frames = page.frames().await?;
            let challenge_present = frames
                .iter()
                .any(|f| detect::classify_url(&f.url) != Detection::NotPresent);
            let top_text: String = page.evaluate_sync("document.body?.innerText || ''").await?;
            if is_block_notice(&top_text) {
                return Ok(Some(SolveOutcome::Blocked));
            }
            for frame in frames.iter().filter(|f| {
                lineage.contains(&f.id) || detect::classify_url(&f.url) != Detection::NotPresent
            }) {
                if detect::classify_url(&frame.url) == Detection::IpBanned {
                    return Ok(Some(SolveOutcome::IpBanned));
                }
                let text: String = page
                    .evaluate_in_frame_id(&frame.id, "document.body?.innerText || ''")
                    .await?;
                if is_block_notice(&text) {
                    return Ok(Some(SolveOutcome::Blocked));
                }
            }
            let now = tokio::time::Instant::now();
            if changed && !challenge_present {
                let since = *clear_since.get_or_insert(now);
                if now.duration_since(since) >= self.completion_settle {
                    return Ok(Some(SolveOutcome::ChallengeCleared));
                }
            } else {
                clear_since = None;
            }
            if now >= deadline {
                return Ok(None);
            }
            sleep(
                self.poll_interval
                    .max(Duration::from_millis(1))
                    .min(deadline - now),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_errors_preserve_their_source_chain() {
        use std::error::Error;

        let error = SolveError::from(eoka::Error::Io(std::io::Error::other("fixture failure")));
        let source = error.source().unwrap();
        assert!(source.is::<eoka::Error>());
        assert_eq!(source.source().unwrap().to_string(), "fixture failure");
        assert!(SolveError::Puzzle("unsupported".into()).source().is_none());
    }

    #[test]
    fn recognizes_observed_block_notice_without_inferring_an_ip_ban() {
        assert!(is_block_notice(
            "⛔ You have been blocked\nWe detected unusual activity"
        ));
        assert!(!is_block_notice("Drag the slider to complete the puzzle"));
    }

    #[test]
    fn existing_cookie_is_not_a_solve() {
        let key = (
            "datadome".into(),
            ".example.com".into(),
            "/".into(),
            "old".into(),
        );
        let baseline = HashSet::from([key.clone()]);
        assert!(!is_new_datadome_cookie(&key, &baseline));
        let mut changed = key;
        changed.3 = "new".into();
        assert!(is_new_datadome_cookie(&changed, &baseline));
        changed.0 = "unrelated-session".into();
        assert!(!is_new_datadome_cookie(&changed, &baseline));
        changed.0 = "datadome".into();
        changed.3.clear();
        assert!(!is_new_datadome_cookie(&changed, &baseline));
    }
}
