use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::Write;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use eoka::{NativeRecording, NativeRecordingOptions, Page, RecordingError};
use eoka_protocol::sidecar::{
    ArtifactContainer, ErrorCode, RecordingFailure, RecordingFailureStage, RecordingIdParams,
    RecordingRecord, RecordingStartParams, RecordingState, ServerError,
};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::artifacts::{FileFailure, StagedArtifact};

mod mp4;

const MAX_SESSION_RECORDINGS: usize = 1024;
const MAX_SESSION_RECORDING_PARAMETER_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Run,
    Stop,
    Cancel,
}

struct EntryState {
    record: RecordingRecord,
    admitted: bool,
    started: bool,
    cancelled: bool,
    publishing: bool,
    partial_path: Option<String>,
    media: Option<mp4::MediaSummary>,
}

struct Entry {
    params: RecordingStartParams,
    state: Mutex<EntryState>,
    changed: watch::Sender<u64>,
    control: watch::Sender<Control>,
}

fn terminal(state: RecordingState) -> bool {
    matches!(
        state,
        RecordingState::Completed | RecordingState::Cancelled | RecordingState::Failed
    )
}

impl Entry {
    fn update(&self, apply: impl FnOnce(&mut EntryState)) {
        let mut state = self.state.lock();
        apply(&mut state);
        drop(state);
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    fn snapshot(&self) -> RecordingRecord {
        self.state.lock().record.clone()
    }
    fn is_terminal(&self) -> bool {
        terminal(self.state.lock().record.state)
    }
    fn partial_path(&self) -> Option<String> {
        self.state.lock().partial_path.clone()
    }

    fn retain_partial_path(&self, path: String) {
        self.update(|state| {
            if let Some(failure) = &mut state.record.failure {
                failure.partial_path = Some(path.clone());
            }
            state.partial_path = Some(path);
        });
    }

    fn deadline_failure(&self, message: &str) -> RecordingFailure {
        let state = self.state.lock();
        RecordingFailure {
            stage: if state.publishing {
                RecordingFailureStage::Publish
            } else {
                RecordingFailureStage::Cancel
            },
            code: ErrorCode::RecordingFailed,
            message: if state.publishing {
                format!(
                    "{message}; publication is in flight and may still complete at {}",
                    state.record.path
                )
            } else {
                message.to_owned()
            },
            partial_path: state.partial_path.clone(),
            cleanup_uncertain: Some(true),
        }
    }

    fn begin_publication(&self) -> bool {
        let mut state = self.state.lock();
        if state.publishing || state.cancelled || terminal(state.record.state) {
            return false;
        }
        state.publishing = true;
        true
    }

    fn finish_publication(&self, result: Result<eoka_protocol::sidecar::Artifact, FileFailure>) {
        self.update(|state| {
            state.publishing = false;
            if terminal(state.record.state) {
                if let Some(failure) = &mut state.record.failure {
                    match result {
                        Ok(artifact) => {
                            let _ = write!(failure.message, "; publication later completed at {} ({} bytes); no completed outcome was acknowledged", artifact.path, artifact.bytes);
                            failure.partial_path = None;
                            state.partial_path = None;
                        },
                        Err(error) => {
                            failure.message.push_str("; publication later failed: ");
                            failure.message.push_str(&error.message);
                            failure.partial_path = error.partial_path;
                        },
                    }
                }
                return;
            }
            match result {
                Ok(artifact) => {
                    state.record.state = RecordingState::Completed;
                    state.record.artifact = Some(artifact);
                    if let Some(media) = state.media {
                        state.record.frames = Some(media.frames);
                        state.record.duration_ms = Some(media.duration_ms);
                    }
                    state.record.failure = None;
                    state.partial_path = None;
                },
                Err(error) => {
                    state.record.state = RecordingState::Failed;
                    state.record.failure = Some(file_failure(error));
                },
            }
        });
    }

    async fn wait(&self, admission: bool) -> RecordingRecord {
        let mut changed = self.changed.subscribe();
        loop {
            {
                let state = self.state.lock();
                if (admission && state.admitted) || terminal(state.record.state) {
                    return state.record.clone();
                }
            }
            if changed.changed().await.is_err() {
                return self.snapshot();
            }
        }
    }

    fn request(&self, cancel: bool) {
        self.update(|state| {
            if terminal(state.record.state) {
                return;
            }
            state.record.state = RecordingState::Finalizing;
            state.admitted = true;
            if cancel {
                state.cancelled = true;
                self.control.send_replace(Control::Cancel);
                if !state.started {
                    state.record.state = RecordingState::Cancelled;
                    state.record.failure = Some(RecordingFailure {
                        stage: RecordingFailureStage::Cancel,
                        code: ErrorCode::Cancelled,
                        message: "recording cancelled before native admission".to_owned(),
                        partial_path: None,
                        cleanup_uncertain: None,
                    });
                }
            } else if !state.cancelled {
                self.control.send_replace(Control::Stop);
            }
        });
    }

    fn fail(&self, failure: RecordingFailure) {
        self.update(|state| {
            if terminal(state.record.state) {
                if let (Some(existing), Some(path)) =
                    (&mut state.record.failure, failure.partial_path)
                {
                    existing.partial_path = Some(path.clone());
                    state.partial_path = Some(path);
                }
                return;
            }
            state.admitted = true;
            state.record.state = RecordingState::Failed;
            state.record.failure = Some(failure);
        });
    }

    fn error(&self, code: ErrorCode, message: impl Into<String>) -> ServerError {
        let mut error = ServerError::new(code, message);
        let data = error.data.get_or_insert_with(Default::default);
        data.recording_id = Some(self.params.recording_id.clone());
        data.path = Some(self.params.path.clone());
        data.recording = Some(self.snapshot());
        error
    }
}

#[derive(Default)]
struct RecordingHistory {
    entries: HashMap<String, Arc<Entry>>,
    retained_parameter_bytes: usize,
}

#[derive(Clone, Default)]
pub struct RecordingManager {
    history: Arc<Mutex<RecordingHistory>>,
}

impl RecordingManager {
    fn entry(&self, id: &str) -> Result<Arc<Entry>, ServerError> {
        let entry = self.history.lock().entries.get(id).cloned();
        entry.ok_or_else(|| {
            let mut error = ServerError::new(
                ErrorCode::RecordingNotFound,
                format!("unknown recording ID: {id}"),
            );
            error.data.get_or_insert_with(Default::default).recording_id = Some(id.to_owned());
            error
        })
    }

    fn reserve(&self, params: RecordingStartParams) -> Result<Arc<Entry>, ServerError> {
        let mut history = self.history.lock();
        if let Some(entry) = history.entries.get(&params.recording_id).cloned() {
            drop(history);
            if entry.params != params {
                return Err(entry.error(
                    ErrorCode::RecordingConflict,
                    "recording ID already belongs to different start parameters",
                ));
            }
            return Ok(entry);
        }
        let retained_parameter_bytes = params
            .recording_id
            .len()
            .checked_add(params.page_id.len())
            .and_then(|bytes| bytes.checked_add(params.path.len()))
            .and_then(|bytes| bytes.checked_add(history.retained_parameter_bytes))
            .filter(|bytes| *bytes <= MAX_SESSION_RECORDING_PARAMETER_BYTES);
        let capacity_limit = if history.entries.len() >= MAX_SESSION_RECORDINGS {
            Some(format!(
                "session recording history limit of {MAX_SESSION_RECORDINGS} distinct IDs reached"
            ))
        } else if retained_parameter_bytes.is_none() {
            Some(format!(
                "session recording history limit of {MAX_SESSION_RECORDING_PARAMETER_BYTES} retained start-parameter bytes exceeded"
            ))
        } else {
            None
        };
        if let Some(limit) = capacity_limit {
            let mut error = ServerError::new(
                ErrorCode::ServerBusy,
                format!(
                    "{limit}; completed, cancelled, and failed recordings retain their slots and parameter bytes for the sidecar lifetime; start a new Browser/session to reset recording history"
                ),
            );
            let data = error.data.get_or_insert_with(Default::default);
            data.recording_id = Some(params.recording_id);
            data.path = Some(params.path);
            return Err(error);
        }
        let (changed, _) = watch::channel(0);
        let (control, _) = watch::channel(Control::Run);
        let entry = Arc::new(Entry {
            state: Mutex::new(EntryState {
                record: RecordingRecord {
                    recording_id: params.recording_id.clone(),
                    page_id: params.page_id.clone(),
                    state: RecordingState::Running,
                    path: params.path.clone(),
                    artifact: None,
                    failure: None,
                    frames: None,
                    duration_ms: None,
                },
                admitted: false,
                started: false,
                cancelled: false,
                publishing: false,
                partial_path: None,
                media: None,
            }),
            params,
            changed,
            control,
        });
        history.retained_parameter_bytes =
            retained_parameter_bytes.expect("recording parameter bytes passed admission");
        history
            .entries
            .insert(entry.params.recording_id.clone(), entry.clone());
        Ok(entry)
    }

    pub fn admit(&self, params: RecordingStartParams) -> Result<(), ServerError> {
        params.validate()?;
        self.reserve(params)?;
        Ok(())
    }

    pub async fn start(
        &self,
        params: RecordingStartParams,
        page: Result<Page, ServerError>,
    ) -> Result<RecordingRecord, ServerError> {
        params.validate()?;
        let entry = self.reserve(params)?;
        let fresh = {
            let mut state = entry.state.lock();
            if state.started || terminal(state.record.state) {
                false
            } else {
                state.started = true;
                true
            }
        };
        if !fresh {
            return Ok(entry.wait(true).await);
        }
        let worker_entry = entry.clone();
        tokio::spawn(async move {
            run(worker_entry, page).await;
        });
        let record = entry.wait(true).await;
        if record.state == RecordingState::Failed {
            let failure = record
                .failure
                .as_ref()
                .expect("failed recording has failure evidence");
            return Err(entry.error(failure.code, failure.message.clone()));
        }
        Ok(record)
    }

    pub async fn get(&self, id: &str) -> Result<RecordingRecord, ServerError> {
        Ok(self.entry(id)?.wait(true).await)
    }

    pub async fn stop(&self, id: &str) -> Result<RecordingRecord, ServerError> {
        let entry = self.entry(id)?;
        entry.request(false);
        Ok(entry.wait(false).await)
    }

    pub async fn cancel(&self, id: &str) -> Result<RecordingRecord, ServerError> {
        let entry = self.entry(id)?;
        entry.request(true);
        Ok(entry.wait(false).await)
    }

    pub fn is_active(&self) -> bool {
        self.history
            .lock()
            .entries
            .values()
            .any(|entry| !entry.is_terminal())
    }

    pub async fn cancel_page(&self, page_id: &str) -> Result<(), ServerError> {
        let entries: Vec<_> = self
            .history
            .lock()
            .entries
            .values()
            .filter(|entry| entry.params.page_id == page_id && !entry.is_terminal())
            .cloned()
            .collect();
        self.cancel_entries(entries).await
    }

    pub fn request_shutdown(&self) {
        for entry in self.history.lock().entries.values() {
            entry.request(true);
        }
    }

    pub fn shutdown_timeout_error(&self) -> Option<ServerError> {
        let entries: Vec<_> = self.history.lock().entries.values().cloned().collect();
        let mut failure = None;
        for entry in entries {
            entry.request(true);
            if !entry.is_terminal() {
                entry.fail(entry.deadline_failure(
                    "recording cleanup was not acknowledged before browser shutdown",
                ));
            }
            if failure.is_none() {
                let detail = {
                    let state = entry.state.lock();
                    state
                        .record
                        .failure
                        .as_ref()
                        .filter(|detail| detail.cleanup_uncertain == Some(true))
                        .map(|detail| (detail.code, detail.message.clone()))
                };
                if let Some((code, message)) = detail {
                    failure = Some(entry.error(code, message));
                }
            }
        }
        failure
    }

    pub async fn shutdown(&self) -> Result<(), ServerError> {
        let entries: Vec<_> = self
            .history
            .lock()
            .entries
            .values()
            .filter(|entry| !entry.is_terminal())
            .cloned()
            .collect();
        self.cancel_entries(entries).await
    }

    async fn cancel_entries(&self, entries: Vec<Arc<Entry>>) -> Result<(), ServerError> {
        for entry in &entries {
            entry.request(true);
        }
        let mut failure = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        for entry in entries {
            match tokio::time::timeout_at(deadline, entry.wait(false)).await {
                Ok(record) if record.state == RecordingState::Failed => {
                    if let Some(detail) = record.failure {
                        failure.get_or_insert_with(|| entry.error(detail.code, detail.message));
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    entry.fail(
                        entry.deadline_failure("recording cleanup exceeded the shutdown deadline"),
                    );
                    failure.get_or_insert_with(|| {
                        entry.error(
                            ErrorCode::RecordingFailed,
                            "recording cleanup exceeded the shutdown deadline",
                        )
                    });
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

pub fn supports_control(method: &str) -> bool {
    matches!(
        method,
        "recording.get" | "recording.stop" | "recording.cancel"
    )
}

pub async fn control(
    manager: &RecordingManager,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    let params: RecordingIdParams = crate::methods::parse_params(params)?;
    params.validate()?;
    let record = match method {
        "recording.get" => manager.get(&params.recording_id).await?,
        "recording.stop" => manager.stop(&params.recording_id).await?,
        "recording.cancel" => manager.cancel(&params.recording_id).await?,
        _ => return Err(ServerError::unknown_method(method)),
    };
    serde_json::to_value(record).map_err(|error| ServerError::internal(error.to_string()))
}

fn native_failure(
    stage: RecordingFailureStage,
    error: RecordingError,
    partial: Option<String>,
) -> RecordingFailure {
    let code = match &error {
        RecordingError::Unsupported(_) => ErrorCode::RecordingUnsupported,
        RecordingError::AlreadyActive => ErrorCode::RecordingAlreadyActive,
        RecordingError::InvalidOptions(_) => ErrorCode::InvalidParams,
        RecordingError::InvalidState(_) | RecordingError::Precondition(_) => {
            ErrorCode::RecordingInvalidState
        }
        _ => ErrorCode::RecordingFailed,
    };
    let cleanup_uncertain = matches!(error, RecordingError::CleanupUncertain(_)).then_some(true);
    RecordingFailure {
        stage,
        code,
        message: error.to_string(),
        partial_path: partial,
        cleanup_uncertain,
    }
}

fn file_failure(error: FileFailure) -> RecordingFailure {
    RecordingFailure {
        stage: error.stage,
        code: ErrorCode::FileOutput,
        message: error.message,
        partial_path: error.partial_path,
        cleanup_uncertain: None,
    }
}

async fn cancellation(control: &mut watch::Receiver<Control>) {
    loop {
        if *control.borrow_and_update() == Control::Cancel {
            return;
        }
        if control.changed().await.is_err() {
            return;
        }
    }
}

async fn cleanup(
    native: Option<&NativeRecording>,
    stage: &mut StagedArtifact,
    mut failure: Option<RecordingFailure>,
) -> Option<RecordingFailure> {
    if let Some(native) = native {
        let result = tokio::time::timeout(Duration::from_secs(3), native.cancel()).await;
        let message = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(_) => Some("native recorder cleanup exceeded its deadline".to_owned()),
        };
        if let Some(message) = message {
            let detail = failure.get_or_insert_with(|| RecordingFailure {
                stage: RecordingFailureStage::Cancel,
                code: ErrorCode::RecordingFailed,
                message: String::new(),
                partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                cleanup_uncertain: None,
            });
            if !detail.message.is_empty() {
                detail.message.push_str("; ");
            }
            detail.message.push_str("cleanup failed: ");
            detail.message.push_str(&message);
            detail.cleanup_uncertain = Some(true);
        }
    }
    match tokio::time::timeout(Duration::from_secs(2), stage.abandon()).await {
        Ok(Ok(())) => {}
        result => {
            let message = match result {
                Ok(Err(error)) => error.message,
                _ => "local file cleanup exceeded its deadline".to_owned(),
            };
            let detail = failure.get_or_insert_with(|| RecordingFailure {
                stage: RecordingFailureStage::Close,
                code: ErrorCode::FileOutput,
                message: String::new(),
                partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                cleanup_uncertain: None,
            });
            if !detail.message.is_empty() {
                detail.message.push_str("; ");
            }
            detail.message.push_str(&message);
            detail.cleanup_uncertain = Some(true);
        }
    }
    failure
}

async fn finish_cancel(
    entry: &Entry,
    native: Option<&NativeRecording>,
    stage: &mut StagedArtifact,
    failure: Option<RecordingFailure>,
) {
    match cleanup(native, stage, failure).await {
        Some(failure) => entry.fail(failure),
        None => entry.update(|state| {
            if terminal(state.record.state) {
                return;
            }
            state.admitted = true;
            state.record.state = RecordingState::Cancelled;
            state.record.failure = Some(RecordingFailure {
                stage: RecordingFailureStage::Cancel,
                code: ErrorCode::Cancelled,
                message: "recording cancelled; partial file was not published".to_owned(),
                partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                cleanup_uncertain: None,
            });
        }),
    }
}

async fn run(entry: Arc<Entry>, page: Result<Page, ServerError>) {
    let page = match page {
        Ok(page) => page,
        Err(error) => {
            entry.fail(RecordingFailure {
                stage: RecordingFailureStage::Start,
                code: error.code,
                message: error.message,
                partial_path: None,
                cleanup_uncertain: None,
            });
            return;
        }
    };
    let mut stage = match StagedArtifact::prepare(&entry.params.path).await {
        Ok(stage) => stage,
        Err(error) => {
            entry.fail(file_failure(error));
            return;
        }
    };
    entry.retain_partial_path(stage.partial.to_string_lossy().into_owned());
    let mut control = entry.control.subscribe();
    if *control.borrow() == Control::Cancel {
        finish_cancel(&entry, None, &mut stage, None).await;
        return;
    }
    let options = NativeRecordingOptions {
        max_width: entry.params.max_width,
        max_height: entry.params.max_height,
        fps: entry.params.fps.map(f64::from),
    };
    let started =
        tokio::time::timeout(Duration::from_secs(20), page.start_recording(options)).await;
    let native = match started {
        Ok(Ok(native)) => native,
        Ok(Err(error)) => {
            let failure = native_failure(
                RecordingFailureStage::Start,
                error,
                Some(stage.partial.to_string_lossy().into_owned()),
            );
            finish_cancel(&entry, None, &mut stage, Some(failure)).await;
            return;
        }
        Err(_) => {
            let failure = RecordingFailure {
                stage: RecordingFailureStage::Start,
                code: ErrorCode::RecordingFailed,
                message: "native recording start exceeded its deadline; cleanup is uncertain"
                    .to_owned(),
                partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                cleanup_uncertain: Some(true),
            };
            finish_cancel(&entry, None, &mut stage, Some(failure)).await;
            return;
        }
    };
    entry.update(|state| {
        state.admitted = true;
    });
    loop {
        let command = *control.borrow_and_update();
        match command {
            Control::Cancel => {
                finish_cancel(&entry, Some(&native), &mut stage, None).await;
                return;
            }
            Control::Stop => break,
            Control::Run => {}
        }
        tokio::select! {
            biased;
            _ = control.changed() => {},
            error = native.wait_for_failure() => {
                let failure = native_failure(RecordingFailureStage::Disconnect, error, Some(stage.partial.to_string_lossy().into_owned()));
                finish_cancel(&entry, Some(&native), &mut stage, Some(failure)).await;
                return;
            },
        }
    }
    let drained = tokio::select! {
        biased;
        _ = cancellation(&mut control) => None,
        result = drain(&native, &mut stage) => Some(result),
    };
    match drained {
        None => {
            finish_cancel(&entry, Some(&native), &mut stage, None).await;
            return;
        }
        Some(Err(failure)) => {
            finish_cancel(&entry, Some(&native), &mut stage, Some(failure)).await;
            return;
        }
        Some(Ok(media)) => entry.update(|state| state.media = Some(media)),
    }
    let publisher = entry.clone();
    let published = tokio::task::spawn_blocking(move || {
        let attempted = publish(&publisher, &stage);
        (stage, attempted)
    })
    .await;
    match published {
        Ok((mut stage, false)) => finish_cancel(&entry, Some(&native), &mut stage, None).await,
        Err(error) => entry.fail(RecordingFailure {
            stage: RecordingFailureStage::Publish,
            code: ErrorCode::FileOutput,
            message: error.to_string(),
            partial_path: entry.partial_path(),
            cleanup_uncertain: None,
        }),
        _ => {}
    }
}

fn publish(entry: &Entry, stage: &StagedArtifact) -> bool {
    publish_with(entry, || {
        stage.publish("video/mp4", Some(ArtifactContainer::Mp4))
    })
}

fn publish_with(
    entry: &Entry,
    commit: impl FnOnce() -> Result<eoka_protocol::sidecar::Artifact, FileFailure>,
) -> bool {
    if !entry.begin_publication() {
        return false;
    }
    let result = commit();
    entry.finish_publication(result);
    true
}

trait RecordingSource {
    fn stop(&self) -> impl Future<Output = Result<(), RecordingError>> + Send;
    fn read_chunk(
        &self,
    ) -> impl Future<Output = Result<eoka::RecordingChunk, RecordingError>> + Send;
    fn close(&self) -> impl Future<Output = Result<(), RecordingError>> + Send;
}

impl RecordingSource for NativeRecording {
    fn stop(&self) -> impl Future<Output = Result<(), RecordingError>> + Send {
        NativeRecording::stop(self)
    }
    fn read_chunk(
        &self,
    ) -> impl Future<Output = Result<eoka::RecordingChunk, RecordingError>> + Send {
        NativeRecording::read_chunk(self)
    }
    fn close(&self) -> impl Future<Output = Result<(), RecordingError>> + Send {
        NativeRecording::close(self)
    }
}

async fn repair_stage(stage: &mut StagedArtifact) -> Result<mp4::MediaSummary, RecordingFailure> {
    let partial_path = Some(stage.partial.to_string_lossy().into_owned());
    let file = stage.recording_file().map_err(file_failure)?;
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        file.flush().await.map_err(|error| RecordingFailure {
            stage: RecordingFailureStage::Flush,
            code: ErrorCode::FileOutput,
            message: error.to_string(),
            partial_path: partial_path.clone(),
            cleanup_uncertain: None,
        })?;
        mp4::repair(file).await.map_err(|error| RecordingFailure {
            stage: RecordingFailureStage::Close,
            code: ErrorCode::RecordingFailed,
            message: format!("MP4 timing validation and repair failed: {error}"),
            partial_path: partial_path.clone(),
            cleanup_uncertain: None,
        })
    })
    .await;
    match result {
        Ok(result) => result,
        Err(_) => Err(RecordingFailure {
            stage: RecordingFailureStage::Close,
            code: ErrorCode::RecordingFailed,
            message: "MP4 timing validation and repair exceeded its deadline; the retained partial may contain an incomplete timing correction".to_owned(),
            partial_path,
            cleanup_uncertain: None,
        }),
    }
}

async fn drain(
    native: &impl RecordingSource,
    stage: &mut StagedArtifact,
) -> Result<mp4::MediaSummary, RecordingFailure> {
    let partial = || Some(stage.partial.to_string_lossy().into_owned());
    match tokio::time::timeout(Duration::from_secs(15), native.stop()).await {
        Ok(result) => {
            result.map_err(|error| native_failure(RecordingFailureStage::Stop, error, partial()))?
        }
        Err(_) => {
            return Err(RecordingFailure {
                stage: RecordingFailureStage::Stop,
                code: ErrorCode::RecordingFailed,
                message: "native stop exceeded its deadline".to_owned(),
                partial_path: partial(),
                cleanup_uncertain: Some(true),
            })
        }
    }
    loop {
        let chunk = match tokio::time::timeout(Duration::from_secs(15), native.read_chunk()).await {
            Ok(result) => result.map_err(|error| {
                native_failure(
                    RecordingFailureStage::Read,
                    error,
                    Some(stage.partial.to_string_lossy().into_owned()),
                )
            })?,
            Err(_) => {
                return Err(RecordingFailure {
                    stage: RecordingFailureStage::Read,
                    code: ErrorCode::RecordingFailed,
                    message: "native stream read exceeded its deadline".to_owned(),
                    partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                    cleanup_uncertain: None,
                })
            }
        };
        if chunk.data.len() > 65_536 {
            return Err(RecordingFailure {
                stage: RecordingFailureStage::Read,
                code: ErrorCode::RecordingFailed,
                message: "native stream exceeded its bounded chunk contract".to_owned(),
                partial_path: Some(stage.partial.to_string_lossy().into_owned()),
                cleanup_uncertain: None,
            });
        }
        stage.write(&chunk.data).await.map_err(file_failure)?;
        if chunk.eof {
            break;
        }
    }
    native.close().await.map_err(|error| {
        native_failure(
            RecordingFailureStage::Close,
            error,
            Some(stage.partial.to_string_lossy().into_owned()),
        )
    })?;
    let media = repair_stage(stage).await?;
    stage.seal().await.map_err(file_failure)?;
    Ok(media)
}

#[cfg(test)]
mod tests;
