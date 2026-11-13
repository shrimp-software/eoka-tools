use std::sync::Arc;
use std::time::Duration;

use eoka::cdp::Transport;
use eoka::{Page, ScreenshotOptions};
use serde_json::{json, Value};

const FRAME_DEMAND_DEADLINE: Duration = Duration::from_secs(2);

pub async fn screenshot(page: &Page, options: &ScreenshotOptions) -> eoka::Result<Vec<u8>> {
    let demand = FrameDemand::open(page).await;
    let captured = page.screenshot_with_options(options).await;
    demand.close().await;
    captured
}

struct FrameDemand {
    transport: Arc<Transport>,
    session: Option<String>,
}

impl FrameDemand {
    async fn open(page: &Page) -> Self {
        let mut demand = Self {
            transport: Arc::clone(page.session().transport()),
            session: None,
        };
        let started =
            tokio::time::timeout(FRAME_DEMAND_DEADLINE, demand.start(page.target_id())).await;
        match started {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::debug!(target_id = page.target_id(), %error, "Could not ask a hidden tab for frames before capture");
            }
            Err(_) => {
                tracing::debug!(
                    target_id = page.target_id(),
                    "Asking a hidden tab for frames before capture exceeded its deadline"
                );
            }
        }
        demand
    }

    async fn start(&mut self, target_id: &str) -> eoka::Result<()> {
        let attached: Value = self
            .transport
            .send(
                "Target.attachToTarget",
                &json!({ "targetId": target_id, "flatten": true }),
            )
            .await?;
        self.session = attached["sessionId"].as_str().map(str::to_owned);
        let session = self.session.as_deref().ok_or_else(|| {
            eoka::Error::cdp_msg("Target.attachToTarget returned no session for frame demand")
        })?;
        self.transport
            .send_to_session::<_, Value>(
                session,
                "Page.startScreencast",
                &json!({
                    "format": "jpeg",
                    "quality": 1,
                    "maxWidth": 8,
                    "maxHeight": 8,
                    "everyNthFrame": 1000,
                }),
            )
            .await?;
        Ok(())
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
