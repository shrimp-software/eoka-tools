pub(crate) mod durable;
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
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{watch, Semaphore};
use worker::Trigger;

#[derive(Clone)]
pub struct DownloadJob {
    journal: Option<durable::Journal>,
    owned: bool,
    record: watch::Sender<DownloadRecord>,
    cancel: watch::Sender<bool>,
    publication: Arc<Mutex<Publication>>,
}

impl DownloadJob {
    pub fn snapshot(&self) -> DownloadRecord {
        self.record.borrow().clone()
    }

    async fn update(&self, apply: impl FnOnce(&mut DownloadRecord)) -> Result<(), ErrorDetail> {
        let mut record = self.snapshot();
        apply(&mut record);
        record.updated_at_ms = now();
        if let Some(journal) = &self.journal {
            journal.save(record.clone()).await.map_err(|error| {
                failure(
                    "download_store_failed",
                    format!("Could not persist download state: {error}"),
                )
            })?;
        }
        self.record.send_replace(record);
        Ok(())
    }

    pub fn cancel(&self) -> DownloadRecord {
        if !self.owned {
            return self.snapshot();
        }
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

struct SetupGuard {
    journal: durable::Journal,
    pending: Option<DownloadRecord>,
}
impl Drop for SetupGuard {
    fn drop(&mut self) {
        let Some(record) = self.pending.take() else {
            return;
        };
        let journal = self.journal.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                fail_setup(
                    &journal,
                    record,
                    failure(
                        "download_setup_canceled",
                        "Download setup was canceled before trigger dispatch",
                    ),
                )
                .await;
            });
        }
    }
}
async fn fail_setup(journal: &durable::Journal, mut record: DownloadRecord, error: ErrorDetail) {
    record.state = DownloadState::Failed;
    record.updated_at_ms = now();
    record.error = Some(error.clone());
    let _ = journal.save(record).await;
    let _ = journal
        .store
        .update(journal.id.clone(), move |op| {
            op.state = eoka_protocol::OperationState::Finished;
            op.finished_at_ms = Some(now());
            op.error_code = Some(error.code);
            op.outcome = eoka::ActionOutcome::NotDispatched;
        })
        .await;
}

struct Inner {
    operations: crate::operations::OperationManager,
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
                operations: Default::default(),
                jobs: Mutex::new(VecDeque::new()),
                slot: Arc::new(Semaphore::new(1)),
                waiters: Semaphore::new(32),
            }),
        }
    }
}

impl DownloadManager {
    pub async fn restore(&self) -> Result<(), ErrorDetail> {
        let store = self
            .inner
            .operations
            .store()
            .await
            .map_err(|_| {
                failure(
                    "download_store_failed",
                    "Could not open durable download history",
                )
            })?
            .clone();
        store
            .recover()
            .await
            .map_err(|e| failure("download_store_failed", e.to_string()))?;
        let records = store
            .run(|connection| {
                let rows = connection
                    .prepare("SELECT record FROM download_records ORDER BY id")?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows.into_iter()
                    .map(|row| Ok(serde_json::from_str::<DownloadRecord>(&row)?))
                    .collect::<anyhow::Result<Vec<_>>>()
            })
            .await
            .map_err(|_| {
                failure(
                    "download_store_failed",
                    "Could not restore download history",
                )
            })?;
        let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
        for record in records {
            if let Some(job) = jobs.iter().find(|j| j.snapshot().id == record.id) {
                if !job.owned {
                    job.record.send_replace(record);
                }
            } else {
                jobs.push_back(DownloadJob {
                    journal: None,
                    owned: false,
                    record: watch::channel(record).0,
                    cancel: watch::channel(false).0,
                    publication: Arc::new(Mutex::new(Publication::Open)),
                });
            }
        }
        Ok(())
    }

    pub async fn existing(
        &self,
        args: &DownloadStartArgs,
    ) -> Result<Option<DownloadRecord>, ErrorDetail> {
        let Some(id) = &args.operation_id else {
            return Ok(None);
        };
        let store = self
            .inner
            .operations
            .store()
            .await
            .map_err(|_| failure("download_store_failed", "Could not open durable history"))?;
        let existing = store
            .lookup(id.clone(), canonical_request(args)?)
            .await
            .map_err(|e| failure("download_admission_failed", e.to_string()))?;
        if existing.is_none() {
            return Ok(None);
        };
        self.restore().await?;
        self.get(id).map(|job| Some(job.snapshot()))
    }

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
        if let Err(error) = self.restore().await {
            return Response::err_detail(error);
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Err(error) = self.restore().await {
                return Response::err_detail(error);
            }
            let job = match self.get(id) {
                Ok(job) => job,
                Err(error) => return Response::err_detail(error),
            };
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let response = job
                .wait(if job.owned {
                    remaining
                } else {
                    remaining.min(Duration::from_millis(100))
                })
                .await;
            if job.owned
                || job.snapshot().state.is_terminal()
                || tokio::time::Instant::now() >= deadline
            {
                return response;
            }
        }
    }

    pub fn cancel(&self, id: &str) -> Result<DownloadRecord, ErrorDetail> {
        let job = self.get(id)?;
        if !job.owned && !job.snapshot().state.is_terminal() {
            return Err(failure(
                "download_owned_elsewhere",
                "Request cancellation from the owning runtime",
            ));
        }
        Ok(job.cancel())
    }

    pub async fn stop(&self) {
        let jobs: Vec<_> = self
            .inner
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|job| job.owned)
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
        selector: Option<eoka::LocatorSpec>,
    ) -> Result<DownloadRecord, ErrorDetail> {
        if let Some(record) = self.existing(&args).await? {
            return Ok(record);
        }
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
        let id = args
            .operation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        uuid::Uuid::parse_str(&id)
            .map_err(|_| failure("invalid_input", "operation_id must be a UUID"))?;
        let store = self
            .inner
            .operations
            .store()
            .await
            .map_err(|_| failure("download_store_failed", "Could not open durable store"))?
            .clone();
        let operation = eoka_protocol::OperationRecord {
            id: id.clone(),
            runtime_id: store.runtime.clone(),
            target_id: source.target_id().into(),
            action: "download".into(),
            state: eoka_protocol::OperationState::Running,
            outcome: eoka::ActionOutcome::NotDispatched,
            dispatch_intent: false,
            preparation_intent: false,
            started_at_ms: now(),
            finished_at_ms: None,
            receipt: None,
            error_code: None,
        };
        let (_, created) = store
            .admit(operation, canonical_request(&args)?)
            .await
            .map_err(|error| failure("download_admission_failed", error.to_string()))?;
        if !created {
            self.restore().await?;
            return self.get(&id).map(|job| job.snapshot());
        }
        let journal = durable::Journal {
            store,
            id: id.clone(),
        };
        let pending = DownloadRecord {
            native_guid: None,
            native_complete: false,
            sha256: None,
            id: id.clone(),
            tab_id: source.target_id().into(),
            state: DownloadState::Pending,
            output: args.output.clone(),
            published_path: None,
            staging_path: None,
            received_bytes: 0,
            total_bytes: None,
            suggested_filename: None,
            source_url: args.url.as_deref().map(redact_url),
            created_at_ms: now(),
            updated_at_ms: now(),
            error: None,
        };
        let mut setup = SetupGuard {
            journal: journal.clone(),
            pending: Some(pending.clone()),
        };
        journal
            .save(pending.clone())
            .await
            .map_err(|e| failure("download_store_failed", e.to_string()))?;
        let result: Result<DownloadRecord, ErrorDetail> = async {
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
                native_guid: None,
                native_complete: false,
                sha256: None,
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
            journal.save(record.clone()).await.map_err(|_| {
                failure(
                    "download_store_failed",
                    "Could not persist download admission",
                )
            })?;
            let job = DownloadJob {
                journal: Some(journal.clone()),
                owned: true,
                record: watch::channel(record).0,
                cancel: watch::channel(false).0,
                publication: Arc::new(Mutex::new(Publication::Open)),
            };
            {
                let mut jobs = self.inner.jobs.lock().unwrap_or_else(|e| e.into_inner());
                jobs.push_back(job.clone());
            }
            let (dispatched, _dispatched_rx) = tokio::sync::oneshot::channel();
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
            Ok(job.snapshot())
        }
        .await;
        if let Err(error) = &result {
            fail_setup(&journal, pending, error.clone()).await;
        }
        setup.pending.take();
        result
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

fn canonical_request(args: &DownloadStartArgs) -> Result<Vec<u8>, ErrorDetail> {
    let mut args = args.clone();
    args.operation_id = None;
    serde_json::to_vec(&args)
        .map_err(|_| failure("invalid_input", "Could not encode download request"))
}
