use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eoka::cdp::Transport;
use eoka::{Page, ScreenshotOptions};
use serde_json::{json, Value};
use tokio::sync::watch;

const FRAME_DEMAND_DEADLINE: Duration = Duration::from_secs(2);
const VISIBILITY_POLL: Duration = Duration::from_millis(100);
const FRAME_GAP: Duration = Duration::from_millis(30);
const FRAME_REQUEST_LIFETIME: Duration = Duration::from_secs(10);
const SETTLE_DEADLINE: Duration = Duration::from_millis(250);
const LINGER_DEADLINE: Duration = Duration::from_secs(300);
const FAILED_ATTEMPTS: u8 = 3;
const QUIET_DEADLINE: Duration = Duration::from_millis(1000);
const HIDDEN: &str = "document.visibilityState === 'hidden'";

type Capture = Pin<Box<dyn Future<Output = eoka::Result<Value>> + Send>>;

static DRIVING: OnceLock<watch::Sender<usize>> = OnceLock::new();

fn driving() -> &'static watch::Sender<usize> {
    DRIVING.get_or_init(|| watch::channel(0).0)
}

struct Driving;

impl Driving {
    fn begin() -> Self {
        driving().send_modify(|count| *count += 1);
        Self
    }
}

impl Drop for Driving {
    fn drop(&mut self) {
        driving().send_modify(|count| *count = count.saturating_sub(1));
    }
}

pub(crate) async fn quiet() {
    let mut count = driving().subscribe();
    let _ = tokio::time::timeout(QUIET_DEADLINE, count.wait_for(|count| *count == 0)).await;
}

pub async fn screenshot(page: &Page, options: &ScreenshotOptions) -> eoka::Result<Vec<u8>> {
    let demand = FrameDemand::open(page).await;
    let captured = page.screenshot_with_options(options).await;
    demand.close().await;
    captured
}

pub async fn painting<T>(page: &Page, work: impl Future<Output = T>) -> T {
    let _frames = Frames::hold(page);
    work.await
}

pub(crate) struct Frames {
    stop: watch::Sender<bool>,
}

impl Frames {
    pub(crate) fn hold(page: &Page) -> Self {
        let (stop, stopped) = watch::channel(false);
        tokio::spawn(drive(page.clone(), stopped));
        Self { stop }
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn released(stopped: &mut watch::Receiver<bool>) {
    let _ = stopped.wait_for(|stop| *stop).await;
}

async fn drive(page: Page, mut stopped: watch::Receiver<bool>) {
    let _driving = Driving::begin();
    let mut demand: Option<FrameDemand> = None;
    let mut failed = 0;
    while failed < FAILED_ATTEMPTS {
        let hidden = tokio::select! {
            biased;
            _ = released(&mut stopped) => break,
            hidden = page.evaluate_sync::<bool>(HIDDEN) => hidden.unwrap_or(false),
        };
        if hidden {
            let held = match demand.take() {
                Some(held) => held,
                None => FrameDemand::attach(&page).await,
            };
            if !held.attached() {
                failed += 1;
                continue;
            }
            let mut capture = held.capture();
            let captured = tokio::select! {
                biased;
                _ = released(&mut stopped) => {
                    held.settle(Some(capture)).await;
                    return;
                }
                captured = tokio::time::timeout(FRAME_REQUEST_LIFETIME, &mut capture) => captured,
            };
            match captured {
                Ok(Ok(_)) => {
                    failed = 0;
                    demand = Some(held);
                }
                Ok(Err(error)) => {
                    tracing::debug!(target_id = page.target_id(), %error, "A hidden tab's frame request failed");
                    failed += 1;
                    held.settle(None).await;
                }
                Err(_) => held.settle(Some(capture)).await,
            }
        }
        tokio::select! {
            biased;
            _ = released(&mut stopped) => break,
            _ = tokio::time::sleep(if hidden { FRAME_GAP } else { VISIBILITY_POLL }) => {}
        }
    }
    if let Some(demand) = demand {
        demand.close().await;
    }
}

struct FrameDemand {
    transport: Arc<Transport>,
    target_id: String,
    session: Option<String>,
}

impl FrameDemand {
    async fn open(page: &Page) -> Self {
        let demand = Self::attach(page).await;
        demand.present().await;
        demand
    }

    async fn attach(page: &Page) -> Self {
        let transport = Arc::clone(page.session().transport());
        let target_id = page.target_id().to_owned();
        let attached = tokio::time::timeout(
            FRAME_DEMAND_DEADLINE,
            transport.send::<_, Value>(
                "Target.attachToTarget",
                &json!({ "targetId": target_id, "flatten": true }),
            ),
        )
        .await;
        let session = match attached {
            Ok(Ok(attached)) => attached["sessionId"].as_str().map(str::to_owned),
            Ok(Err(error)) => {
                tracing::debug!(target_id, %error, "Could not attach to a tab to ask it for frames");
                None
            }
            Err(_) => {
                tracing::debug!(
                    target_id,
                    "Attaching to a tab to ask it for frames exceeded its deadline"
                );
                None
            }
        };
        Self {
            transport,
            target_id,
            session,
        }
    }

    fn attached(&self) -> bool {
        self.session.is_some()
    }

    async fn present(&self) {
        let Some(session) = self.session.as_deref() else {
            return;
        };
        let started = tokio::time::timeout(
            FRAME_DEMAND_DEADLINE,
            self.transport.send_to_session::<_, Value>(
                session,
                "Page.startScreencast",
                &json!({
                    "format": "jpeg",
                    "quality": 1,
                    "maxWidth": 8,
                    "maxHeight": 8,
                    "everyNthFrame": 1000,
                }),
            ),
        )
        .await;
        match started {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                tracing::debug!(target_id = self.target_id, %error, "Could not ask a hidden tab to present frames");
            }
            Err(_) => {
                tracing::debug!(
                    target_id = self.target_id,
                    "Asking a hidden tab to present frames exceeded its deadline"
                );
            }
        }
    }

    fn capture(&self) -> Capture {
        let transport = Arc::clone(&self.transport);
        let session = self.session.clone().unwrap_or_default();
        Box::pin(async move {
            transport
                .send_to_session::<_, Value>(
                    &session,
                    "Page.captureScreenshot",
                    &json!({ "format": "jpeg", "quality": 1, "optimizeForSpeed": true }),
                )
                .await
        })
    }

    async fn settle(self, pending: Option<Capture>) {
        if !self.attached() {
            return;
        }
        self.present().await;
        let mut pending = pending.unwrap_or_else(|| self.capture());
        if tokio::time::timeout(SETTLE_DEADLINE, &mut pending)
            .await
            .is_ok()
        {
            self.close().await;
            return;
        }
        drop(pending);
        tokio::spawn(async move {
            if self.answers().await {
                let _ = tokio::time::timeout(FRAME_DEMAND_DEADLINE, self.capture()).await;
            }
            self.close().await;
        });
    }

    async fn answers(&self) -> bool {
        let Some(session) = self.session.as_deref() else {
            return false;
        };
        let ends = tokio::time::Instant::now() + LINGER_DEADLINE;
        while tokio::time::Instant::now() < ends {
            let answered = tokio::time::timeout(
                FRAME_REQUEST_LIFETIME,
                self.transport.send_to_session::<_, Value>(
                    session,
                    "Runtime.evaluate",
                    &json!({ "expression": "0", "returnByValue": true }),
                ),
            )
            .await;
            match answered {
                Ok(Ok(_)) => return true,
                Ok(Err(_)) => return false,
                Err(_) => {}
            }
        }
        false
    }

    async fn close(mut self) {
        if let Some(session) = self.session.take() {
            release(&self.transport, &session).await;
        }
    }
}

impl Drop for FrameDemand {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let transport = Arc::clone(&self.transport);
            runtime.spawn(async move { release(&transport, &session).await });
        }
    }
}

async fn release(transport: &Transport, session: &str) {
    let detached = tokio::time::timeout(
        FRAME_DEMAND_DEADLINE,
        transport.send::<_, Value>("Target.detachFromTarget", &json!({ "sessionId": session })),
    )
    .await;
    if !matches!(detached, Ok(Ok(_))) {
        tracing::debug!(session, "Could not release the frame demand session");
    }
}
