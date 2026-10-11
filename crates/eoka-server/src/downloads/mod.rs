mod files;
mod target;
mod worker;

pub use target::resolve_click;

use eoka::{Browser, Page};
use eoka_protocol::{
    ActionStatus, DownloadRecord, DownloadStartArgs, DownloadState, ErrorContext, ErrorDetail,
    Response,
};
use files::{Destination, Publication};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{watch, Semaphore};
use worker::Trigger;

#[derive(Clone)]
pub struct DownloadJob {
    record: watch::Sender<DownloadRecord>,
    cancel: watch::Sender<bool>,
    publication: Arc<Mutex<Publication>>,
}

impl DownloadJob {
    pub fn snapshot(&self) -> DownloadRecord {
        self.record.borrow().clone()
    }

    fn update(&self, apply: impl FnOnce(&mut DownloadRecord)) {
        self.record.send_modify(|record| {
            apply(record);
            record.updated_at_ms = now();
        });
    }

    pub fn cancel(&self) -> DownloadRecord {
        let mut gate = self.publication.lock().unwrap_or_else(|e| e.into_inner());
        if *gate == Publication::Open && !self.record.borrow().state.is_terminal() {
            *gate = Publication::Canceled;
            self.cancel.send_replace(true);
        }
        self.snapshot()
    }

    pub async fn wait(&self, timeout: Duration) -> Response {
        let mut changes = self.record.subscribe();
        let result = tokio::time::timeout(timeout, async {
            loop {
                let record = changes.borrow_and_update().clone();
                if record.state.is_terminal() {
                    return record;
                }
                if changes.changed().await.is_err() {
                    return self.snapshot();
                }
            }
        })
        .await;
        match result {
            Ok(record) if record.state == DownloadState::Completed => record_response(record),
            Ok(record) => {
                let error = record
                    .error
                    .clone()
                    .unwrap_or_else(|| failure("download_canceled", "Download canceled"));
                error_with_record(record, error)
            }
            Err(_) => {
                let record = self.snapshot();
                let error = failure(
                    "download_wait_timeout",
                    "Wait timed out; the job continues. Inspect its status or wait again.",
                )
                .hint(format!(
                    "Use download status {} or download wait {}; do not start another transfer.",
                    record.id, record.id
                ));
                error_with_record(record, error)
            }
        }
    }
}

struct Inner {
    jobs: Mutex<VecDeque<DownloadJob>>,
    slot: Arc<Semaphore>,
    waiters: Semaphore,
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<Inner>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(VecDeque::new()),
                slot: Arc::new(Semaphore::new(1)),
                waiters: Semaphore::new(32),
            }),
        }
    }
}

impl DownloadManager {
    pub fn is_active(&self) -> bool {
        self.inner.slot.available_permits() == 0
    }

    pub fn list(&self) -> Vec<DownloadRecord> {
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .map(DownloadJob::snapshot)
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<DownloadJob, ErrorDetail> {
        self.inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|job| job.record.borrow().id == id)
            .cloned()
            .ok_or_else(|| {
                failure(
                    "download_not_found",
                    "Download job not found in this daemon",
                )
            })
    }

    pub async fn wait(&self, id: &str, timeout_ms: u64) -> Response {
        let Ok(_permit) = self.inner.waiters.try_acquire() else {
            return Response::err_detail(failure(
                "download_wait_busy",
                "Too many concurrent download waiters",
            ));
        };
        if timeout_ms == 0 || timeout_ms > 86_400_000 {
            return Response::err_detail(failure(
                "invalid_input",
                "timeout_ms must be between 1 and 86400000",
            ));
        }
        match self.get(id) {
            Ok(job) => job.wait(Duration::from_millis(timeout_ms)).await,
            Err(error) => Response::err_detail(error),
        }
    }

    pub async fn stop(&self) {
        let jobs: Vec<_> = self
            .inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        for job in &jobs {
            job.cancel();
        }
        for job in jobs {
            if !job.snapshot().state.is_terminal() {
                job.wait(Duration::from_secs(12)).await;
            }
        }
    }

    pub async fn start(
        &self,
        browser: &Browser,
        source: &Page,
        args: DownloadStartArgs,
        selector: Option<String>,
    ) -> Result<DownloadRecord, ErrorDetail> {
        args.validate()
            .map_err(|message| failure("invalid_input", message))?;
        if !browser.owns_process() {
            return Err(failure(
                "download_unsupported",
                "Only Eoka-owned Chrome supports downloads",
            ));
        }
        let trigger = if let Some(url) = &args.url {
            let parsed = url::Url::parse(url)
                .map_err(|_| failure("invalid_input", "Invalid download URL"))?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                return Err(failure(
                    "invalid_input",
                    "Attachment URLs must use HTTP or HTTPS",
                ));
            }
            Trigger::Navigate {
                url: url.clone(),
                referrer: source.url().await.ok(),
            }
        } else {
            Trigger::Click {
                selector: selector
                    .ok_or_else(|| failure("invalid_input", "Click target was not resolved"))?,
            }
        };
        let permit = self.inner.slot.clone().try_acquire_owned().map_err(|_| {
            failure(
                "download_busy",
                "One download is already active in this session",
            )
        })?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            now(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let output = args.output.clone();
        let prepare_id = id.clone();
        let expected = args.expect_bytes;
        let destination = Arc::new(
            tokio::task::spawn_blocking(move || {
                Destination::prepare(&output, &prepare_id, expected)
            })
            .await
            .map_err(|_| failure("download_io_error", "Destination preparation failed"))??,
        );
        let auxiliary = args.url.is_some();
        let page = if auxiliary {
            match browser.new_blank_page().await {
                Ok(page) => page,
                Err(error) => {
                    destination.retained_path();
                    return Err(failure("download_setup_failed", error.to_string()));
                }
            }
        } else {
            source.clone()
        };
        let mut auxiliary = auxiliary.then(|| worker::AuxiliaryPage::new(page.clone()));
        let native = match browser.watch_downloads(&destination.staging).await {
            Ok(native) => native,
            Err(error) => {
                if let Some(auxiliary) = auxiliary.as_mut() {
                    let _ = auxiliary.close().await;
                }
                destination.retained_path();
                return Err(failure("download_setup_failed", error.to_string()));
            }
        };
        let record = DownloadRecord {
            id,
            tab_id: source.target_id().into(),
            state: DownloadState::Pending,
            output: destination.output.to_string_lossy().into_owned(),
            published_path: None,
            staging_path: Some(destination.staging.to_string_lossy().into_owned()),
            received_bytes: 0,
            total_bytes: None,
            suggested_filename: None,
            source_url: args.url.as_deref().map(redact_url),
            created_at_ms: now(),
            updated_at_ms: now(),
            error: None,
        };
        let job = DownloadJob {
            record: watch::channel(record).0,
            cancel: watch::channel(false).0,
            publication: Arc::new(Mutex::new(Publication::Open)),
        };
        {
            let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
            while jobs.len() > 256 {
                jobs.pop_front();
            }
            jobs.push_back(job.clone());
        }
        let (dispatched, dispatched_rx) = tokio::sync::oneshot::channel();
        let context = worker::Transfer {
            job: job.clone(),
            native,
            destination,
            page,
            auxiliary,
            trigger,
            expected_bytes: args.expect_bytes,
            start_timeout: Duration::from_millis(args.start_timeout_ms),
            permit,
            dispatched: Some(dispatched),
            action_status: ActionStatus::NotAttempted,
        };
        tokio::spawn(context.run());
        let _ = dispatched_rx.await;
        Ok(job.snapshot())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for job in self.jobs.get_mut().unwrap_or_else(|e| e.into_inner()) {
            job.cancel();
        }
    }
}

pub fn failure(code: &str, message: impl Into<String>) -> ErrorDetail {
    ErrorDetail::new(code, message).retryable(false)
}

pub fn rejected(mut error: ErrorDetail) -> Response {
    error
        .context
        .get_or_insert_with(Default::default)
        .action_status
        .get_or_insert(ActionStatus::NotAttempted);
    error.hint.get_or_insert_with(|| "No download action was attempted. Inspect the page before choosing a target; do not pre-click or restart the browser.".into());
    Response::err_detail(error)
}

async fn diagnose(
    error: ErrorDetail,
    page: &Page,
    action_status: ActionStatus,
    id: Option<String>,
) -> ErrorDetail {
    let mut context = crate::page_context::capture(page).await;
    if let Some(context) = context.as_mut() {
        context.url = redact_url(&context.url);
    }
    error.context(ErrorContext {
        page: context,
        download_id: id,
        action_status: Some(action_status),
    })
}

pub fn record_response(record: DownloadRecord) -> Response {
    match serde_json::to_value(record) {
        Ok(record) => Response::ok(record),
        Err(error) => Response::err(error.to_string()),
    }
}

pub fn start_response(result: Result<DownloadRecord, ErrorDetail>) -> Response {
    match result {
        Ok(record)
            if matches!(
                record.state,
                DownloadState::Failed | DownloadState::Canceled
            ) =>
        {
            let error = record
                .error
                .clone()
                .unwrap_or_else(|| failure("download_failed", "Download failed"));
            error_with_record(record, error)
        }
        Ok(record) => record_response(record),
        Err(error) => rejected(error),
    }
}

pub fn error_with_record(record: DownloadRecord, mut error: ErrorDetail) -> Response {
    let context = error.context.get_or_insert_with(Default::default);
    context.download_id = Some(record.id.clone());
    context
        .action_status
        .get_or_insert(ActionStatus::MayHaveExecuted);
    match serde_json::to_value(record) {
        Ok(record) => {
            let mut response = Response::err_detail(error);
            response.data = Some(record);
            response
        }
        Err(error) => Response::err(error.to_string()),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn redact_url(value: &str) -> String {
    url::Url::parse(value)
        .map(|mut url| {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string().chars().take(4096).collect()
        })
        .unwrap_or_else(|_| "[unavailable]".into())
}
