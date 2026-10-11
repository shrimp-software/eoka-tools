use super::{
    failure,
    files::{self, Destination, Publication},
    redact_url, DownloadJob,
};
use eoka::{DownloadEvent, DownloadWatch, NativeDownloadState, Page};
use eoka_protocol::{DownloadState, ErrorDetail};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, OwnedSemaphorePermit};

pub(super) struct AuxiliaryPage(Option<Page>);

impl AuxiliaryPage {
    pub fn new(page: Page) -> Self {
        Self(Some(page))
    }

    pub async fn close(&mut self) -> Result<(), ErrorDetail> {
        let result = match self.0.as_ref() {
            Some(page) => close_auxiliary(page).await,
            None => Ok(()),
        };
        self.0.take();
        result
    }
}

async fn close_auxiliary(page: &Page) -> Result<(), ErrorDetail> {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        page.session()
            .transport()
            .send::<_, Value>("Target.closeTarget", &json!({"targetId": page.target_id()})),
    )
    .await;
    match result {
        Ok(Ok(_)) => Ok(()),
        _ => Err(failure(
            "download_cleanup_failed",
            "Could not confirm temporary download tab closure",
        )),
    }
}

impl Drop for AuxiliaryPage {
    fn drop(&mut self) {
        if let Some(page) = self.0.take() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move { close_auxiliary(&page).await });
            }
        }
    }
}

pub(super) enum Trigger {
    Click {
        selector: String,
    },
    Navigate {
        url: String,
        referrer: Option<String>,
    },
}

impl Trigger {
    async fn run(&self, page: &Page) -> eoka::Result<()> {
        match self {
            Self::Click { selector } => page.click(selector).await,
            Self::Navigate { url, referrer } => {
                page.navigate_for_download(url, referrer.as_deref()).await
            }
        }
    }
}

pub(super) struct Transfer {
    pub job: DownloadJob,
    pub native: DownloadWatch,
    pub destination: Arc<Destination>,
    pub page: Page,
    pub auxiliary: Option<AuxiliaryPage>,
    pub trigger: Trigger,
    pub expected_bytes: Option<u64>,
    pub start_timeout: Duration,
    pub permit: OwnedSemaphorePermit,
    pub dispatched: Option<oneshot::Sender<()>>,
}

impl Transfer {
    pub async fn run(mut self) {
        let mut guid = None;
        let mut verification = None;
        let result = self.transfer(&mut guid, &mut verification).await;
        if result.is_err() {
            self.job.cancel();
        }
        if let Some(task) = verification {
            let _ = task.await;
        }
        if result.is_err() {
            if let Some(guid) = &guid {
                let _ = self.native.cancel(guid).await;
            }
        }
        if let Some(dispatched) = self.dispatched.take() {
            let _ = dispatched.send(());
        }
        let restore = self.native.stop().await;
        let auxiliary_cleanup = if let Some(mut auxiliary) = self.auxiliary.take() {
            auxiliary.close().await
        } else {
            Ok(())
        };
        let destination = self.destination.clone();
        let retained = tokio::task::spawn_blocking(move || destination.retained_path())
            .await
            .map_err(|_| failure("download_cleanup_failed", "Staging cleanup worker failed"));
        let result = result
            .and(restore.map_err(|_| failure(
                "download_cleanup_failed",
                "Could not restore browser download settings; restart the browser before another download",
            )))
            .and(auxiliary_cleanup);
        let published = *self
            .job
            .publication
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            == Publication::Published;
        self.job.update(|record| {
            let cleanup = retained.map(|path| record.staging_path = path);
            if published {
                record.published_path = Some(record.output.clone());
            }
            match result.and(cleanup) {
                Ok(()) => record.state = DownloadState::Completed,
                Err(error) => {
                    record.state = if error.code == "download_canceled" {
                        DownloadState::Canceled
                    } else {
                        DownloadState::Failed
                    };
                    record.error = Some(error);
                }
            }
        });
        drop(self.permit);
    }

    async fn transfer(
        &mut self,
        guid: &mut Option<String>,
        verification: &mut Option<tokio::task::JoinHandle<Result<(), ErrorDetail>>>,
    ) -> Result<(), ErrorDetail> {
        let frames: HashSet<String> = self
            .page
            .frames()
            .await
            .map_err(|_| {
                failure(
                    "download_setup_failed",
                    "Could not identify the initiating page frames",
                )
            })?
            .into_iter()
            .map(|frame| frame.id)
            .collect();
        let trigger = self.trigger.run(&self.page);
        tokio::pin!(trigger);
        let deadline = tokio::time::Instant::now() + self.start_timeout;
        let mut trigger_result = None;
        let mut cancellation = self.job.cancel.subscribe();
        let mut disk_check = tokio::time::interval(Duration::from_secs(1));
        loop {
            if *cancellation.borrow() {
                return Err(failure("download_canceled", "Download canceled"));
            }
            let event = tokio::select! {
                _ = cancellation.changed() => continue,
                result = &mut trigger, if trigger_result.is_none() => {
                    trigger_result = Some(result);
                    if let Some(dispatched) = self.dispatched.take() { let _ = dispatched.send(()); }
                    continue;
                },
                _ = tokio::time::sleep_until(deadline), if guid.is_none() => {
                    return Err(failure(if matches!(trigger_result, Some(Err(_))) { "download_trigger_failed" } else { "download_not_started" },
                        "No native download started before the deadline; the action was not replayed"));
                },
                _ = disk_check.tick(), if verification.is_none() => {
                    let record = self.job.snapshot();
                    let remaining = record.total_bytes.or(self.expected_bytes).unwrap_or(0).saturating_sub(record.received_bytes);
                    let path = self.destination.staging.clone();
                    tokio::task::spawn_blocking(move || files::ensure_space(&path, remaining))
                        .await.map_err(|_| failure("download_io_error", "Disk space check failed"))??;
                    continue;
                },
                result = async {
                    match verification.as_mut() {
                        Some(task) => task.await,
                        None => std::future::pending().await,
                    }
                } => {
                    verification.take();
                    result.map_err(|_| failure("download_io_error", "File verification worker failed"))??;
                    return Ok(());
                },
                event = self.native.next_event() => event.map_err(|_| failure("download_event_loss", "Native download event stream disconnected or lost events"))?,
            };
            match event {
                DownloadEvent::Beginning(begin) => {
                    if !files::safe_guid(&begin.guid) {
                        return Err(failure(
                            "download_invalid_event",
                            "Invalid native download identifier",
                        ));
                    }
                    let matches = frames.contains(&begin.frame_id)
                        || self
                            .native
                            .has_opener_ancestor(&begin.frame_id, self.page.target_id())
                        || belongs_to_page(&self.page, &begin.frame_id).await;
                    if !matches {
                        self.native.cancel(&begin.guid).await.map_err(|_| {
                            failure(
                                "download_cleanup_failed",
                                "Could not cancel unrelated download",
                            )
                        })?;
                        continue;
                    }
                    if guid.as_deref().is_some_and(|id| id != begin.guid) {
                        let _ = self.native.cancel(&begin.guid).await;
                        self.job.cancel();
                        return Err(failure("download_multiple_files", "The trigger started multiple downloads; only single-file actions are supported"));
                    }
                    *guid = Some(begin.guid);
                    self.job.update(|record| {
                        record.state = DownloadState::Downloading;
                        record.source_url = Some(redact_url(&begin.url));
                        record.suggested_filename =
                            Some(begin.suggested_filename.chars().take(1024).collect());
                    });
                }
                DownloadEvent::Progress(progress) if guid.as_deref() == Some(&progress.guid) => {
                    let received = byte_count(progress.received_bytes)?;
                    let total = byte_count(progress.total_bytes)?;
                    let total = (total > 0).then_some(total);
                    self.job.update(|record| {
                        record.received_bytes = received;
                        record.total_bytes = total;
                    });
                    match progress.state {
                        NativeDownloadState::Canceled => {
                            return Err(failure(
                                "download_interrupted",
                                "Chrome interrupted or canceled the download",
                            ))
                        }
                        NativeDownloadState::Completed if verification.is_none() => {
                            self.job
                                .update(|record| record.state = DownloadState::Verifying);
                            let destination = self.destination.clone();
                            let expected = self.expected_bytes;
                            let gate = self.job.publication.clone();
                            *verification = Some(tokio::task::spawn_blocking(move || {
                                destination.publish(
                                    &progress.guid,
                                    received,
                                    total,
                                    expected,
                                    &gate,
                                )
                            }));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn byte_count(value: f64) -> Result<u64, ErrorDetail> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(failure(
            "download_invalid_event",
            "Invalid native byte count",
        ));
    }
    Ok(value as u64)
}

async fn belongs_to_page(page: &Page, frame_id: &str) -> bool {
    if page
        .frames()
        .await
        .is_ok_and(|frames| frames.iter().any(|frame| frame.id == frame_id))
    {
        return true;
    }
    let Ok(targets) = page
        .session()
        .transport()
        .send::<_, Value>("Target.getTargets", &json!({}))
        .await
    else {
        return false;
    };
    let Some(targets) = targets["targetInfos"].as_array() else {
        return false;
    };
    let mut current = frame_id;
    for _ in 0..16 {
        let Some(target) = targets
            .iter()
            .find(|target| target["targetId"].as_str() == Some(current))
        else {
            return false;
        };
        let Some(opener) = target["openerId"].as_str() else {
            return false;
        };
        if opener == page.target_id() {
            return true;
        }
        current = opener;
    }
    false
}
