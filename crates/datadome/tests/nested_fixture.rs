mod support {
    pub mod fixture;
}
use eoka::Browser;
use eoka_datadome::{DataDomeSolver, SolveOutcome};
use std::time::Duration;
use support::fixture::Fixture;

#[tokio::test]
#[ignore = "local Chrome fixture, not a live DataDome acceptance test"]
async fn nested_local_slider_and_block_notice() {
    for (blocked, simple, reblock) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (false, true, true),
    ] {
        let fixture = Fixture::start(blocked, simple, reblock).await;
        let browser = Browser::launch_with(|c| {
            c.extra_args.extend([
                "--host-resolver-rules=MAP * 127.0.0.1".into(),
                "--no-proxy-server".into(),
                "--disable-features=AutomationControlled,EnableAutomation".into(),
                "--site-per-process".into(),
            ])
        })
        .await
        .unwrap();
        let page = browser.new_page(&fixture.url).await.unwrap();
        for _ in 0..40 {
            if page
                .frames()
                .await
                .unwrap()
                .iter()
                .any(|f| f.url.contains("/captcha/"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let solver = DataDomeSolver {
            max_attempts: 1,
            cookie_timeout: Duration::from_secs(3),
            ..DataDomeSolver::new()
        };
        if simple && !reblock {
            page.execute_sync("const unrelated=document.createElement('iframe'); unrelated.srcdoc='<h1>You have been blocked</h1>'; unrelated.style.left='9000px'; document.body.appendChild(unrelated)").await.unwrap();
            let failed = solver
                .solve_with_drag(&page, |_| async {
                    Err(eoka::Error::Io(std::io::Error::other(
                        "fixture input failure",
                    )))
                })
                .await;
            assert!(
                matches!(failed,Err(eoka_datadome::SolveError::Browser(eoka::Error::Io(error))) if error.to_string()=="fixture input failure")
            );
        }
        let outcome = if simple {
            solver
                .solve_with_drag(&page, |drag| {
                    assert!(drag.horizontal_only);
                    assert!((drag.dx - 222.0).abs() < 1.0);
                    let page = &page;
                    async move {
                        page.human()
                            .with_speed(eoka::HumanSpeed::Slow)
                            .drag_horizontal_by(drag.x, drag.y, drag.dx)
                            .await
                    }
                })
                .await
        } else {
            solver.solve(&page).await
        };
        if blocked || reblock {
            assert_eq!(outcome.unwrap(), SolveOutcome::Blocked);
        } else {
            assert_eq!(outcome.unwrap(), SolveOutcome::ChallengeCleared);
            let restored: bool = page
                .evaluate_sync("document.body.dataset.restored === 'true'")
                .await
                .unwrap();
            assert!(restored, "local fixture must confirm accepted displacement");
            tokio::time::timeout(Duration::from_secs(2), fixture.started.notified())
                .await
                .unwrap();
        }
        browser.close().await.unwrap();
    }
}

async fn adversarial_fixture(mode: &str) {
    let fixture = Fixture::scenario(mode).await;
    let browser = Browser::launch_with(|c| {
        c.extra_args.extend([
            "--host-resolver-rules=MAP * 127.0.0.1".into(),
            "--no-proxy-server".into(),
            "--disable-features=AutomationControlled,EnableAutomation".into(),
            "--site-per-process".into(),
        ])
    })
    .await
    .unwrap();
    let page = browser.new_page(&fixture.url).await.unwrap();
    for _ in 0..40 {
        if page
            .frames()
            .await
            .unwrap()
            .iter()
            .any(|f| f.url.contains("/captcha/"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let solver = DataDomeSolver {
        max_attempts: 1,
        ..Default::default()
    };
    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        if ["mirrored", "many-images", "shadow-open", "shadow-closed"].contains(&mode) {
            solver
                .solve_with_drag(&page, |_| async {
                    panic!("unsupported puzzle dispatched a drag")
                })
                .await
        } else {
            solver.solve(&page).await
        }
    })
    .await
    .expect("bounded fixture solve");
    if ["mirrored", "many-images", "shadow-open", "shadow-closed"].contains(&mode) {
        assert!(
            matches!(&outcome, Err(eoka_datadome::SolveError::Puzzle(message)) if message == if mode == "many-images" { "image element limit exceeded" } else { "unsupported image placement" }),
            "{mode}: {outcome:?}"
        );
        assert!(!page
            .evaluate_sync::<bool>("document.body.dataset.pressed==='true'")
            .await
            .unwrap());
    } else {
        assert_eq!(
            outcome.unwrap(),
            if mode == "shadow-positive" {
                SolveOutcome::ChallengeCleared
            } else {
                SolveOutcome::Blocked
            },
            "{mode}"
        );
        assert!(page
            .evaluate_sync::<bool>("document.body.dataset.restored==='true'")
            .await
            .unwrap());
        assert!(page
            .frames()
            .await
            .unwrap()
            .iter()
            .all(|f| !f.url.contains("/captcha/")));
        assert!(page
            .cookies()
            .await
            .unwrap()
            .iter()
            .any(|c| c.name == "datadome" && !c.value.is_empty()));
    }
    assert_eq!(page.evaluate_sync::<i32>("1+1").await.unwrap(), 2);
    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore = "local Chrome adversarial fixture"]
async fn adversarial_top_document_reblock() {
    adversarial_fixture("top-block").await;
}

#[tokio::test]
#[ignore = "local Chrome adversarial fixture"]
async fn adversarial_auth_parent_reblock() {
    adversarial_fixture("auth-block").await;
}

#[tokio::test]
#[ignore = "local Chrome adversarial fixture"]
async fn adversarial_mirrored_images() {
    adversarial_fixture("mirrored").await;
}

#[tokio::test]
#[ignore = "local Chrome adversarial fixture"]
async fn adversarial_many_maximum_images() {
    adversarial_fixture("many-images").await;
}

#[tokio::test]
#[ignore = "local Chrome slotted image geometry regression"]
async fn adversarial_open_shadow_reflection() {
    adversarial_fixture("shadow-open").await;
}

#[tokio::test]
#[ignore = "local Chrome slotted image geometry regression"]
async fn adversarial_closed_shadow_reflection() {
    adversarial_fixture("shadow-closed").await;
}

#[tokio::test]
#[ignore = "local Chrome slotted image geometry control"]
async fn supported_shadow_geometry() {
    adversarial_fixture("shadow-positive").await;
}
