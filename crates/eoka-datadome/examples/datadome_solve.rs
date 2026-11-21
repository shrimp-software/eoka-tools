#[path = "support/click.rs"]
mod click;
#[path = "support/login.rs"]
mod login;
#[path = "support/options.rs"]
mod options;
#[path = "support/profile.rs"]
mod profile;

use std::time::Duration;

use click::click_visible;
use eoka::{Browser, Page};
use eoka_datadome::{DataDomeSolver, SolveOutcome};
use options::Options;

type RunResult = Result<(), Box<dyn std::error::Error>>;

async fn observe_pass(page: &Page) -> RunResult {
    let auth = profile::auth_frame(page)
        .await?
        .ok_or("missing auth frame")?
        .id;
    page.evaluate_in_frame_id::<bool>(&auth, r#"(() => {
        const frame=[...document.querySelectorAll('iframe')].find(f => {
            try { const u=new URL(f.src); return u.origin==='https://geo.captcha-delivery.com' && u.pathname==='/captcha/'; } catch { return false; }
        });
        if (!frame?.contentWindow) return false;
        const source=frame.contentWindow;
        document.documentElement.dataset.eokaDatadomePassed='false';
        const listener=e=>{
            if (e.origin!=='https://geo.captcha-delivery.com' || e.source!==source) return;
            try {
                const data=typeof e.data==='string' ? JSON.parse(e.data) : e.data;
                if (data?.eventType==='passed' && data.responseType==='captcha' && typeof data.cookie==='string' && data.cookie.length>0) {
                    document.documentElement.dataset.eokaDatadomePassed='true';
                    window.removeEventListener('message',listener);
                }
            } catch {}
        };
        window.addEventListener('message',listener);
        return true;
    })()"#).await?;
    Ok(())
}

async fn verify_password_stage(page: &Page, options: &Options, identifier: &str) -> RunResult {
    let mut passed = false;
    let mut advanced = false;
    let mut retried = false;
    for tick in 0..40 {
        if let Some(frame) = profile::auth_frame(page).await? {
            let auth = frame.id;
            passed |= page
                .evaluate_in_frame_id::<bool>(
                    &auth,
                    "document.documentElement.dataset.eokaDatadomePassed === 'true'",
                )
                .await?;
            advanced = profile::at_password_stage(page, &auth, identifier).await?;
            if advanced {
                break;
            }
            if !retried && tick >= 8 && passed {
                let initial: bool = page.evaluate_in_frame_id(&auth,"!!document.querySelector('input[name=email]')?.checkVisibility({checkOpacity:true,checkVisibilityCSS:true})").await?;
                if initial {
                    page.goto(&options.url).await?;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    click_visible(page, None, "button.loginButton", None).await?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    profile::enter(page, identifier).await?;
                    retried = true;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if options.require_solved && !passed {
        return Err("proof failed: missing origin/source-checked DataDome passed event".into());
    }
    if !advanced {
        return Err("proof failed: matching visible password form not reached".into());
    }
    println!("PASSWORD STAGE VERIFIED: no password submitted yet");
    Ok(())
}

async fn verify_navigation(page: &Page, options: &Options) -> RunResult {
    page.goto(&options.url).await?;
    page.wait_for_visible(
        options
            .success_selector
            .as_deref()
            .expect("validated proof selector"),
        10_000,
    )
    .await?;
    let expected = url::Url::parse(&options.url)?;
    let actual = url::Url::parse(&page.url().await?)?;
    if actual.origin() != expected.origin() || actual.path() != expected.path() {
        return Err("proof failed: navigation did not restore the requested page".into());
    }
    if page.frames().await?.iter().any(|f| {
        url::Url::parse(&f.url).is_ok_and(|u| {
            u.host_str().is_some_and(|h| {
                h == "captcha-delivery.com" || h.ends_with(".captcha-delivery.com")
            })
        })
    }) {
        return Err("proof failed: DataDome frame remains after navigation".into());
    }
    println!("CHALLENGE FLOW VERIFIED: expected content visible after navigation; not an authentication claim");
    Ok(())
}

async fn run(browser: &Browser, options: &Options) -> RunResult {
    println!("browser version: {}", browser.version().await?);
    let page = browser.new_page(&options.url).await?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    if let Some(selector) = &options.click_selector {
        click_visible(&page, None, selector, None).await?;
        tokio::time::sleep(Duration::from_secs(8)).await;
    }
    if let Some(identifier) = &options.profile {
        profile::enter(&page, identifier).await?;
        tokio::time::sleep(Duration::from_secs(10)).await;
        observe_pass(&page).await?;
    }
    let outcome = DataDomeSolver {
        max_attempts: 1,
        ..Default::default()
    }
    .solve(&page)
    .await?;
    println!("solver outcome: {outcome:?}");
    require_outcome(outcome, options.require_solved)?;
    if let Some(identifier) = &options.profile {
        verify_password_stage(&page, options, identifier).await?;
        if login::enabled() {
            if let Err(stage) = login::attempt(&page, identifier).await {
                eprintln!("credential verification stopped: {stage}");
                return Err(stage.into());
            }
            println!("AUTHENTICATED IDENTITY VERIFIED: matching account returned by /me after one submit action");
            return Ok(());
        }
    }
    if options.require_solved {
        verify_navigation(&page, options).await?;
    }
    Ok(())
}

fn require_outcome(outcome: SolveOutcome, require_solved: bool) -> Result<(), &'static str> {
    match outcome {
        SolveOutcome::ChallengeCleared => Ok(()),
        SolveOutcome::NotPresent if !require_solved => Ok(()),
        SolveOutcome::NotPresent => Err("no challenge was solved"),
        SolveOutcome::IpBanned => Err("challenge reported an IP ban"),
        SolveOutcome::Blocked => Err("DataDome displayed a block notice"),
        SolveOutcome::Failed => Err("solver attempts failed"),
    }
}

#[tokio::main]
async fn main() -> RunResult {
    let options = Options::read()?;
    login::validate_configuration()?;
    if !login::enabled() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "eoka_datadome=info".into()),
            )
            .init();
    }
    let browser = options.launch().await?;
    let result = tokio::time::timeout(Duration::from_secs(90), run(&browser, &options)).await;
    let close = browser.close().await;
    if login::enabled() {
        return match result {
            Ok(Ok(())) if close.is_ok() => Ok(()),
            _ => Err("credential-mode verification did not complete; sensitive failure details suppressed; no retry".into()),
        };
    }
    result??;
    close?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_challenge_is_not_a_new_solve_and_failures_always_fail() {
        assert!(require_outcome(SolveOutcome::NotPresent, false).is_ok());
        assert!(require_outcome(SolveOutcome::NotPresent, true).is_err());
        for strict in [false, true] {
            assert!(require_outcome(SolveOutcome::ChallengeCleared, strict).is_ok());
            for failed in [
                SolveOutcome::Failed,
                SolveOutcome::Blocked,
                SolveOutcome::IpBanned,
            ] {
                assert!(require_outcome(failed, strict).is_err());
            }
        }
    }
}
