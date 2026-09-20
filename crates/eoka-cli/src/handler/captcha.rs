use std::time::{Duration, Instant};

use eoka_datadome::{DataDomeSolver, SolveError, SolveOutcome};
use eoka_protocol::{
    CaptchaDatadomeArgs, CaptchaDatadomeResult, DataDomeOutcome, ErrorDetail, Response,
};
use serde_json::Value;

use super::Handler;

fn failure(code: &str, message: &str) -> Response {
    Response::err_detail(ErrorDetail::new(code, message).retryable(false))
}

fn outcome_response(outcome: SolveOutcome, tab_id: String, elapsed_ms: u64) -> Response {
    let (outcome, mut response) = match outcome {
        SolveOutcome::ChallengeCleared => {
            (DataDomeOutcome::ChallengeCleared, Response::ok(Value::Null))
        }
        SolveOutcome::NotPresent => (DataDomeOutcome::NotPresent, Response::ok(Value::Null)),
        SolveOutcome::Blocked => (
            DataDomeOutcome::Blocked,
            failure(
                "datadome_blocked",
                "DataDome reports a block; no fallback or application retry was performed.",
            ),
        ),
        SolveOutcome::IpBanned => (
            DataDomeOutcome::IpBanned,
            failure(
                "datadome_ip_banned",
                "DataDome reports an IP ban; no proxy change was performed.",
            ),
        ),
        SolveOutcome::Failed => (
            DataDomeOutcome::Failed,
            failure(
                "datadome_failed",
                "DataDome clearance was not verified; inspect the page before retrying.",
            ),
        ),
    };
    let Ok(data) = serde_json::to_value(CaptchaDatadomeResult {
        outcome,
        tab_id,
        elapsed_ms,
    }) else {
        return failure(
            "datadome_failed",
            "DataDome result serialization failed; inspect the page before retrying.",
        );
    };
    response.data = Some(data);
    response
}

impl Handler {
    pub(super) async fn cmd_captcha_datadome(&mut self, args: &Value) -> Response {
        let started = Instant::now();
        let prior_failures = self
            .fetch_dropped
            .load(std::sync::atomic::Ordering::Relaxed);
        let Ok(args) = serde_json::from_value::<CaptchaDatadomeArgs>(args.clone()) else {
            return failure(
                "invalid_input",
                "Invalid DataDome arguments; use max_attempts 1..4 and timeout_ms 5000..120000.",
            );
        };
        let Ok(tab) = self.require_tab() else {
            return failure(
                "no_active_tab",
                "No active browser tab; open the intended page first. No browser was launched.",
            );
        };
        let page = tab.page.clone();
        if !self.intercept.is_empty()
            && (!self.intercept.enabled
                || self.fetch_sender.is_none()
                || (self.fetch_events.is_none() && self.idle_fetch_drain.is_none())
                || self.fetch_sessions.is_empty())
        {
            return failure(
                "datadome_interception_unavailable",
                "Request interception is not healthy; repair its configuration before solving.",
            );
        }
        let tab_id = page.target_id().to_string();
        let solver = DataDomeSolver {
            max_attempts: args.max_attempts,
            ..Default::default()
        };
        let normal = page.human();
        let horizontal = page.human().with_speed(eoka::HumanSpeed::Slow);
        let drain = self
            .idle_fetch_drain
            .take()
            .or_else(|| self.start_fetch_drain());
        let remaining = Duration::from_millis(args.timeout_ms).saturating_sub(started.elapsed());
        let result = tokio::time::timeout(remaining, async {
            let _: Value = page
                .session()
                .send("Page.bringToFront", &serde_json::json!({}))
                .await
                .map_err(SolveError::Browser)?;
            solver
                .solve_with_drag(&page, |drag| {
                    let (horizontal, normal) = (&horizontal, &normal);
                    async move {
                        if drag.horizontal_only {
                            horizontal.drag_horizontal_by(drag.x, drag.y, drag.dx).await
                        } else {
                            normal.drag_by(drag.x, drag.y, drag.dx).await
                        }
                    }
                })
                .await
        })
        .await;
        let (drained, normal_cleanup, horizontal_cleanup) = tokio::join!(
            self.stop_fetch_drain(drain),
            normal.finish_drag_cleanup(),
            horizontal.finish_drag_cleanup(),
        );
        if !matches!(result, Ok(Ok(SolveOutcome::NotPresent))) {
            if let Ok(tab) = self.require_tab_mut() {
                tab.invalidate();
            }
        }
        let interception_intact = self
            .fetch_dropped
            .load(std::sync::atomic::Ordering::Relaxed)
            == prior_failures;
        if !drained
            || normal_cleanup.is_err()
            || horizontal_cleanup.is_err()
            || !interception_intact
        {
            eprintln!("[eoka] DataDome cleanup unconfirmed: drain={drained}, normal_release={}, horizontal_release={}, interception={interception_intact}", normal_cleanup.is_ok(), horizontal_cleanup.is_ok());
            return failure("datadome_cleanup_failed", "DataDome cleanup could not be confirmed; inspect the session before further interaction.");
        }
        match result {
            Ok(Ok(outcome)) => outcome_response(outcome, tab_id, started.elapsed().as_millis() as u64),
            Ok(Err(SolveError::Browser(_))) => failure("datadome_browser_error", "DataDome browser operation failed; no replacement browser, reload or replay was attempted."),
            Ok(Err(SolveError::Puzzle(_))) => failure("datadome_puzzle_error", "DataDome puzzle could not be safely matched; no fallback was attempted."),
            Err(_) => failure("datadome_timeout", "DataDome solve budget expired; input cleanup completed. Inspect the page before retrying."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_arguments_and_absent_browser_never_launch() {
        let mut handler = Handler::new(
            "datadome-no-browser",
            crate::launch_spec::LaunchSpec::Connect {
                ws_url: "ws://127.0.0.1:1".into(),
            },
        );
        let invalid = handler
            .cmd_captcha_datadome(&serde_json::json!({"max_attempts":0}))
            .await;
        assert_eq!(invalid.error_detail.unwrap().code, "invalid_input");
        let missing = handler.cmd_captcha_datadome(&serde_json::json!({})).await;
        assert_eq!(missing.error_detail.unwrap().code, "no_active_tab");
        assert!(handler.state.is_none());
    }

    #[test]
    fn outcomes_preserve_machine_data_without_false_success() {
        for (outcome, expected, ok) in [
            (SolveOutcome::ChallengeCleared, "challenge_cleared", true),
            (SolveOutcome::NotPresent, "not_present", true),
            (SolveOutcome::Blocked, "blocked", false),
            (SolveOutcome::IpBanned, "ip_banned", false),
            (SolveOutcome::Failed, "failed", false),
        ] {
            let response = outcome_response(outcome, "fixture-tab".into(), 20);
            assert_eq!(response.ok, ok);
            let data = response.data.unwrap();
            assert_eq!(data["outcome"], expected);
            assert_eq!(data["tab_id"], "fixture-tab");
            assert_eq!(data["elapsed_ms"], 20);
            assert_eq!(data.as_object().unwrap().len(), 3);
            if !ok {
                assert_eq!(response.error_detail.unwrap().retryable, Some(false));
            }
        }
    }
}
