use super::{
    failure,
    files::{self, Destination, Publication},
    redact_url, DownloadJob,
};
use eoka::{DownloadEvent, DownloadWatch, NativeDownloadState, Page};
use eoka_protocol::{ActionStatus, DownloadState, ErrorDetail};
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
        selector: eoka::LocatorSpec,
    },
    Navigate {
        url: String,
        referrer: Option<String>,
    },
}

impl Trigger {
    async fn run(&self, page: &Page, job: &DownloadJob, timeout: Duration) -> eoka::Result<()> {
        let action = match self {
            Self::Click { selector } => eoka::Action::Click {
                target: selector.clone(),
                human: false,
                double: false,
            },
            Self::Navigate { url, referrer } => eoka::Action::Navigate {
                url: url.clone(),
                referrer: referrer.clone(),
            },
        };
        let mut options = eoka::ActionOptions::expecting(eoka::Expectation::DownloadStarted);
        options.timeout_ms = timeout.as_millis().min(u128::from(u64::MAX)) as u64;
        if let Some(journal) = &job.journal {
            options.observer = Some(Arc::new(crate::operations::Intent {
                store: journal.store.clone(),
                id: journal.id.clone(),
            }));
        }
        page.act(action, options).await.map(|_| ())
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
    pub action_status: ActionStatus,
}

impl Transfer {
    pub async fn run(mut self) {
        let mut guid = None;
        let mut verification = None;
        let mut result = self.transfer(&mut guid, &mut verification).await;
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
        if let Some(journal) = &self.job.journal {
            if let Ok(operation) = journal.store.get(journal.id.clone()).await {
                self.action_status = if operation.dispatch_intent {
                    ActionStatus::MayHaveExecuted
                } else {
                    ActionStatus::NotAttempted
                };
            }
        }
        if let Err(error) = result {
            result = Err(super::diagnose(
                error,
                &self.page,
                self.action_status,
                Some(self.job.snapshot().id),
            )
            .await);
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
        drop(self.permit);
        let final_record = self
            .job
            .update(|record| {
                let cleanup = retained.map(|path| record.staging_path = path);
                if published {
                    record.published_path = Some(record.output.clone());
                }
                match result.and(cleanup) {
                    Ok(()) => record.state = DownloadState::Completed,
                    Err(error) => {
                        record.state = if published {
                            DownloadState::Completed
                        } else if error.code == "download_canceled" {
                            DownloadState::Canceled
                        } else {
                            DownloadState::Failed
                        };
                        record.error = Some(error);
                    }
                }
            })
            .await;
        if final_record.is_err() {
            tracing::error!("Could not persist final download state");
        }
        if let Some(journal) = &self.job.journal {
            let record = self.job.snapshot();
            let _ = journal
                .store
                .update(journal.id.clone(), move |operation| {
                    operation.state = eoka_protocol::OperationState::Finished;
                    operation.finished_at_ms = Some(super::now());
                    operation.outcome = if record.state == DownloadState::Completed {
                        eoka::ActionOutcome::VerifiedComplete
                    } else if operation.dispatch_intent {
                        eoka::ActionOutcome::OutcomeUnknown
                    } else {
                        eoka::ActionOutcome::NotDispatched
                    };
                    operation.error_code = record.error.map(|e| e.code);
                })
                .await;
        }
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
        self.action_status = ActionStatus::MayHaveExecuted;
        let trigger = self.trigger.run(&self.page, &self.job, self.start_timeout);
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
                    let error = match trigger_result.as_ref() {
                        Some(Err(eoka::Error::Action{code,..})) if code=="action_timeout" => failure("download_not_started","No native download started before the deadline; the action was not replayed"),
                        Some(Err(error)) => failure("download_trigger_failed", format!("No native download started; trigger failed: {error}")),
                        _ => failure("download_not_started", "No native download started before the deadline; the action was not replayed"),
                    };
                    return Err(error.hint("Inspect this job and its page evidence before another action. No click or navigation was replayed."));
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
                    if let Some(journal)=&self.job.journal {
                        let mut record=journal.snapshot().await.map_err(|e|failure("download_store_failed",e.to_string()))?;
                        record.state=DownloadState::Verifying;
                        self.job.record.send_replace(record);
                    }
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
                    *guid = Some(begin.guid.clone());
                    self.job
                        .update(|record| {
                            record.native_guid = Some(begin.guid.clone());
                            record.state = DownloadState::Downloading;
                            record.source_url = Some(redact_url(&begin.url));
                            record.suggested_filename =
                                Some(begin.suggested_filename.chars().take(1024).collect());
                        })
                        .await?;
                }
                DownloadEvent::Progress(progress) if guid.as_deref() == Some(&progress.guid) => {
                    if verification.is_some() {
                        continue;
                    }
                    let received = byte_count(progress.received_bytes)?;
                    let total = byte_count(progress.total_bytes)?;
                    let total = (total > 0).then_some(total);
                    self.job
                        .update(|record| {
                            record.received_bytes = received;
                            record.total_bytes = total;
                        })
                        .await?;
                    match progress.state {
                        NativeDownloadState::Canceled => {
                            return Err(failure(
                                "download_interrupted",
                                "Chrome interrupted or canceled the download",
                            ))
                        }
                        NativeDownloadState::Completed if verification.is_none() => {
                            self.job
                                .update(|record| record.state = DownloadState::Verifying)
                                .await?;
                            if let Some(journal) = &self.job.journal {
                                journal
                                    .completed_event(super::durable::Evidence {
                                        guid: Some(progress.guid.clone()),
                                        native_complete: true,
                                        received,
                                        total,
                                        expected: self.expected_bytes,
                                        ..Default::default()
                                    })
                                    .await
                                    .map_err(|_| {
                                        failure(
                                            "download_store_failed",
                                            "Could not persist native completion",
                                        )
                                    })?;
                            }
                            let journal = self.job.journal.clone();
                            let destination = self.destination.clone();
                            let expected = self.expected_bytes;
                            let gate = self.job.publication.clone();
                            *verification = Some(tokio::task::spawn_blocking(move || {
                                destination.publish_durable(
                                    &progress.guid,
                                    received,
                                    total,
                                    expected,
                                    &gate,
                                    journal.as_ref(),
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
