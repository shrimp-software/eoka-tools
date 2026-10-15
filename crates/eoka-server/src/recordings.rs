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
                },
                admitted: false,
                started: false,
                cancelled: false,
                publishing: false,
                partial_path: None,
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
        Some(Ok(())) => {}
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

async fn repair_stage(stage: &mut StagedArtifact) -> Result<(), RecordingFailure> {
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
) -> Result<(), RecordingFailure> {
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
    repair_stage(stage).await?;
    stage.seal().await.map_err(file_failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(path: String) -> RecordingStartParams {
        RecordingStartParams {
            page_id: "target".to_owned(),
            recording_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            path,
            max_width: None,
            max_height: None,
            fps: None,
        }
    }

    fn indexed_params(index: usize) -> RecordingStartParams {
        let path = std::env::temp_dir().join(format!("recording-{index}.mp4"));
        let mut params = params(path.to_string_lossy().into_owned());
        params.recording_id = format!("{index:08x}-1111-4111-8111-111111111111");
        params
    }

    fn parameter_bytes(params: &RecordingStartParams) -> usize {
        params.recording_id.len() + params.page_id.len() + params.path.len()
    }

    fn sized_params(index: usize, bytes: usize) -> RecordingStartParams {
        let mut params = indexed_params(index);
        let fixed_bytes = params.recording_id.len() + params.path.len();
        params.page_id = "é".to_owned();
        params
            .page_id
            .push_str(&"x".repeat(bytes - fixed_bytes - params.page_id.len()));
        assert_eq!(parameter_bytes(&params), bytes);
        params
    }

    #[test]
    fn distinct_id_limit_preserves_recovery_and_conflict_precedence() {
        let manager = RecordingManager::default();
        for index in 0..MAX_SESSION_RECORDINGS {
            manager.admit(indexed_params(index)).unwrap();
        }
        let existing = indexed_params(MAX_SESSION_RECORDINGS - 1);
        let entry = manager.entry(&existing.recording_id).unwrap();
        manager.admit(existing.clone()).unwrap();
        assert!(Arc::ptr_eq(
            &entry,
            &manager.reserve(existing.clone()).unwrap()
        ));
        let mut different = existing.clone();
        different.fps = Some(30);
        assert_eq!(
            manager.admit(different).unwrap_err().code,
            ErrorCode::RecordingConflict
        );
        let rejected = indexed_params(MAX_SESSION_RECORDINGS);
        let error = manager.admit(rejected.clone()).unwrap_err();
        assert_eq!(error.code, ErrorCode::ServerBusy);
        assert!(error.message.contains("1024 distinct IDs"));
        assert!(error.message.contains("completed, cancelled, and failed"));
        assert!(error.message.contains("sidecar lifetime"));
        assert!(error.message.contains("new Browser/session"));
        let data = error.data.unwrap();
        assert_eq!(
            data.recording_id.as_deref(),
            Some(rejected.recording_id.as_str())
        );
        assert_eq!(data.path.as_deref(), Some(rejected.path.as_str()));
        assert!(data.recording.is_none());
        assert_eq!(manager.history.lock().entries.len(), MAX_SESSION_RECORDINGS);
        assert!(manager.entry(&rejected.recording_id).is_err());
        RecordingManager::default().admit(rejected).unwrap();
    }

    #[test]
    fn retained_parameter_budget_has_exact_utf8_boundary_without_consuming_rejections() {
        let manager = RecordingManager::default();
        let last = indexed_params(1);
        let last_bytes = parameter_bytes(&last);
        let first = sized_params(0, MAX_SESSION_RECORDING_PARAMETER_BYTES - last_bytes);
        manager.admit(first).unwrap();
        let before = manager.history.lock().retained_parameter_bytes;
        let oversized = sized_params(2, MAX_SESSION_RECORDING_PARAMETER_BYTES + 1);
        assert_eq!(
            manager.admit(oversized).unwrap_err().code,
            ErrorCode::ServerBusy
        );
        let over_budget = sized_params(3, last_bytes + 1);
        let error = manager.admit(over_budget).unwrap_err();
        assert_eq!(error.code, ErrorCode::ServerBusy);
        assert!(error
            .message
            .contains("16777216 retained start-parameter bytes"));
        assert_eq!(manager.history.lock().retained_parameter_bytes, before);
        assert_eq!(manager.history.lock().entries.len(), 1);
        assert!(manager.entry(&indexed_params(2).recording_id).is_err());
        assert!(manager.entry(&indexed_params(3).recording_id).is_err());
        manager.admit(last.clone()).unwrap();
        assert_eq!(
            manager.history.lock().retained_parameter_bytes,
            MAX_SESSION_RECORDING_PARAMETER_BYTES
        );
        manager.admit(last.clone()).unwrap();
        let mut different = last.clone();
        different.page_id.push('x');
        assert_eq!(
            manager.admit(different).unwrap_err().code,
            ErrorCode::RecordingConflict
        );
        assert_eq!(
            manager.admit(indexed_params(4)).unwrap_err().code,
            ErrorCode::ServerBusy
        );
        assert_eq!(manager.history.lock().entries.len(), 2);
        assert_eq!(
            manager.history.lock().retained_parameter_bytes,
            MAX_SESSION_RECORDING_PARAMETER_BYTES
        );
    }

    #[test]
    fn invalid_parameters_do_not_consume_history_capacity() {
        let manager = RecordingManager::default();
        for index in 0..MAX_SESSION_RECORDINGS - 1 {
            manager.admit(indexed_params(index)).unwrap();
        }
        let bytes = manager.history.lock().retained_parameter_bytes;
        let mut invalid = indexed_params(MAX_SESSION_RECORDINGS);
        invalid.fps = Some(0);
        assert_eq!(
            manager.admit(invalid).unwrap_err().code,
            ErrorCode::InvalidParams
        );
        assert_eq!(manager.history.lock().retained_parameter_bytes, bytes);
        assert_eq!(
            manager.history.lock().entries.len(),
            MAX_SESSION_RECORDINGS - 1
        );
        manager
            .admit(indexed_params(MAX_SESSION_RECORDINGS))
            .unwrap();
        assert_eq!(
            manager
                .admit(indexed_params(MAX_SESSION_RECORDINGS + 1))
                .unwrap_err()
                .code,
            ErrorCode::ServerBusy
        );
    }

    #[tokio::test]
    async fn terminal_outcomes_retain_slots_and_remain_recoverable_at_capacity() {
        let manager = RecordingManager::default();
        let root = tempfile::tempdir().unwrap();
        let mut completed = indexed_params(0);
        completed.path = root
            .path()
            .join("completed.mp4")
            .to_string_lossy()
            .into_owned();
        let entry = manager.reserve(completed.clone()).unwrap();
        entry.update(|state| {
            state.started = true;
            state.admitted = true;
        });
        let mut stage = StagedArtifact::prepare(&completed.path).await.unwrap();
        stage.write(mp4::VALID_MP4).await.unwrap();
        repair_stage(&mut stage).await.unwrap();
        stage.seal().await.unwrap();
        assert!(publish(&entry, &stage));
        let cancelled = indexed_params(1);
        manager.admit(cancelled.clone()).unwrap();
        manager.cancel(&cancelled.recording_id).await.unwrap();
        for index in 3..MAX_SESSION_RECORDINGS {
            manager.admit(indexed_params(index)).unwrap();
        }
        let failed = indexed_params(2);
        assert_eq!(
            manager
                .start(failed.clone(), Err(ServerError::invalid_page("missing")))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidPage
        );
        assert_eq!(
            manager
                .admit(indexed_params(MAX_SESSION_RECORDINGS))
                .unwrap_err()
                .code,
            ErrorCode::ServerBusy
        );
        let bytes = manager.history.lock().retained_parameter_bytes;
        manager.shutdown().await.unwrap();
        assert_eq!(manager.history.lock().entries.len(), MAX_SESSION_RECORDINGS);
        assert_eq!(manager.history.lock().retained_parameter_bytes, bytes);
        for (params, state) in [
            (completed, RecordingState::Completed),
            (cancelled, RecordingState::Cancelled),
            (failed, RecordingState::Failed),
        ] {
            manager.admit(params.clone()).unwrap();
            assert_eq!(
                manager.get(&params.recording_id).await.unwrap().state,
                state
            );
            assert_eq!(
                manager
                    .start(
                        params.clone(),
                        Err(ServerError::internal("must not restart"))
                    )
                    .await
                    .unwrap()
                    .state,
                state
            );
            let mut different = params;
            different.fps = Some(30);
            assert_eq!(
                manager.admit(different).unwrap_err().code,
                ErrorCode::RecordingConflict
            );
        }
    }

    #[tokio::test]
    async fn capacity_rejection_does_not_reserve_launch_or_stage() {
        let manager = RecordingManager::default();
        for index in 0..MAX_SESSION_RECORDINGS {
            manager.admit(indexed_params(index)).unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        let mut rejected = indexed_params(MAX_SESSION_RECORDINGS);
        rejected.path = root
            .path()
            .join("rejected.mp4")
            .to_string_lossy()
            .into_owned();
        let bytes = manager.history.lock().retained_parameter_bytes;
        assert_eq!(
            manager
                .start(
                    rejected.clone(),
                    Err(ServerError::internal("worker must not run"))
                )
                .await
                .unwrap_err()
                .code,
            ErrorCode::ServerBusy
        );
        tokio::task::yield_now().await;
        assert_eq!(
            manager.get(&rejected.recording_id).await.unwrap_err().code,
            ErrorCode::RecordingNotFound
        );
        assert_eq!(manager.history.lock().entries.len(), MAX_SESSION_RECORDINGS);
        assert_eq!(manager.history.lock().retained_parameter_bytes, bytes);
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        assert_eq!(
            manager
                .entry(&indexed_params(0).recording_id)
                .unwrap()
                .snapshot()
                .state,
            RecordingState::Running
        );
    }

    #[test]
    fn concurrent_new_ids_cannot_exceed_count_or_parameter_byte_limits() {
        for byte_limited in [false, true] {
            let manager = RecordingManager::default();
            let contender_bytes = 16 * 1024;
            if byte_limited {
                manager
                    .admit(sized_params(
                        0,
                        MAX_SESSION_RECORDING_PARAMETER_BYTES - 8 * contender_bytes,
                    ))
                    .unwrap();
            } else {
                for index in 0..MAX_SESSION_RECORDINGS - 8 {
                    manager.admit(indexed_params(index)).unwrap();
                }
            }
            let barrier = Arc::new(std::sync::Barrier::new(16));
            let results = std::thread::scope(|scope| {
                let mut jobs = Vec::new();
                for index in MAX_SESSION_RECORDINGS..MAX_SESSION_RECORDINGS + 16 {
                    let manager = manager.clone();
                    let barrier = barrier.clone();
                    jobs.push(scope.spawn(move || {
                        let params = if byte_limited {
                            sized_params(index, contender_bytes)
                        } else {
                            indexed_params(index)
                        };
                        barrier.wait();
                        manager.admit(params)
                    }));
                }
                jobs.into_iter()
                    .map(|job| job.join().unwrap())
                    .collect::<Vec<_>>()
            });
            assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 8);
            for error in results.into_iter().filter_map(Result::err) {
                assert_eq!(error.code, ErrorCode::ServerBusy);
            }
            let history = manager.history.lock();
            if byte_limited {
                assert_eq!(history.entries.len(), 9);
                assert_eq!(
                    history.retained_parameter_bytes,
                    MAX_SESSION_RECORDING_PARAMETER_BYTES
                );
            } else {
                assert_eq!(history.entries.len(), MAX_SESSION_RECORDINGS);
                assert!(history.retained_parameter_bytes < MAX_SESSION_RECORDING_PARAMETER_BYTES);
            }
        }
    }

    #[tokio::test]
    async fn failed_admission_is_recoverable_and_same_id_never_restarts() {
        let manager = RecordingManager::default();
        let root = tempfile::tempdir().unwrap();
        let params = params(
            root.path()
                .join("capture.mp4")
                .to_string_lossy()
                .into_owned(),
        );
        let error = manager
            .start(params.clone(), Err(ServerError::invalid_page("missing")))
            .await
            .unwrap_err();
        assert_eq!(
            error.data.unwrap().recording_id.as_deref(),
            Some(params.recording_id.as_str())
        );
        let first = manager.get(&params.recording_id).await.unwrap();
        assert_eq!(first.state, RecordingState::Failed);
        assert_eq!(
            manager
                .start(params.clone(), Err(ServerError::internal("different")))
                .await
                .unwrap()
                .state,
            RecordingState::Failed
        );
        let recovered = manager.get(&params.recording_id).await.unwrap();
        assert_eq!(
            recovered.failure.unwrap().message,
            first.failure.unwrap().message
        );
        let mut different = params.clone();
        different.fps = Some(30);
        assert_eq!(
            manager
                .start(different, Err(ServerError::internal("unused")))
                .await
                .unwrap_err()
                .code,
            ErrorCode::RecordingConflict
        );
        assert_eq!(
            manager.stop(&params.recording_id).await.unwrap().state,
            RecordingState::Failed
        );
        assert_eq!(
            manager.cancel(&params.recording_id).await.unwrap().state,
            RecordingState::Failed
        );
    }

    #[tokio::test]
    async fn shared_waiters_observe_one_terminal_result_and_abort_does_not_cancel() {
        let manager = RecordingManager::default();
        let entry = manager
            .reserve(params("/tmp/video.mp4".to_owned()))
            .unwrap();
        entry.update(|state| state.admitted = true);
        let id = entry.params.recording_id.clone();
        let first_manager = manager.clone();
        let first_id = id.clone();
        let first = tokio::spawn(async move { first_manager.stop(&first_id).await.unwrap() });
        tokio::task::yield_now().await;
        first.abort();
        assert!(!entry.state.lock().cancelled);
        assert_eq!(*entry.control.borrow(), Control::Stop);
        let other_manager = manager.clone();
        let second = tokio::spawn(async move { other_manager.stop(&id).await.unwrap() });
        entry.fail(RecordingFailure {
            stage: RecordingFailureStage::Read,
            code: ErrorCode::RecordingFailed,
            message: "stream disconnected".to_owned(),
            partial_path: Some("/tmp/.partial".to_owned()),
            cleanup_uncertain: Some(true),
        });
        let result = second.await.unwrap();
        assert_eq!(result.state, RecordingState::Failed);
        assert_eq!(
            result.failure.unwrap().partial_path.as_deref(),
            Some("/tmp/.partial")
        );
    }

    #[tokio::test]
    async fn cancellation_and_publication_are_linearized_without_success_named_partials() {
        for cancel_first in [false, true] {
            let manager = RecordingManager::default();
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("video.mp4");
            let entry = manager
                .reserve(params(output.to_string_lossy().into_owned()))
                .unwrap();
            entry.update(|state| {
                state.started = true;
                state.admitted = true;
            });
            let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .unwrap();
            stage.write(mp4::VALID_MP4).await.unwrap();
            repair_stage(&mut stage).await.unwrap();
            stage.seal().await.unwrap();
            if cancel_first {
                entry.request(true);
                entry.request(false);
                assert_eq!(*entry.control.borrow(), Control::Cancel);
                assert!(!publish(&entry, &stage));
                finish_cancel(&entry, None, &mut stage, None).await;
                assert_eq!(entry.snapshot().state, RecordingState::Cancelled);
                assert!(!output.exists());
                assert!(stage.partial.exists());
            } else {
                assert!(publish(&entry, &stage));
                assert_eq!(
                    manager
                        .cancel(&entry.params.recording_id)
                        .await
                        .unwrap()
                        .state,
                    RecordingState::Completed
                );
                assert_eq!(std::fs::read(&output).unwrap(), mp4::VALID_MP4);
            }
        }
    }

    struct FakeSource {
        chunks: Mutex<std::collections::VecDeque<Result<eoka::RecordingChunk, RecordingError>>>,
        close_fails: bool,
    }

    impl RecordingSource for FakeSource {
        async fn stop(&self) -> Result<(), RecordingError> {
            Ok(())
        }
        async fn read_chunk(&self) -> Result<eoka::RecordingChunk, RecordingError> {
            self.chunks.lock().pop_front().unwrap()
        }
        async fn close(&self) -> Result<(), RecordingError> {
            if self.close_fails {
                Err(RecordingError::Core(eoka::Error::cdp_msg(
                    "IO.close failed",
                )))
            } else {
                Ok(())
            }
        }
    }

    struct ClosingSource {
        source: FakeSource,
        entered: tokio::sync::Notify,
    }

    impl RecordingSource for ClosingSource {
        async fn stop(&self) -> Result<(), RecordingError> {
            self.source.stop().await
        }

        async fn read_chunk(&self) -> Result<eoka::RecordingChunk, RecordingError> {
            self.source.read_chunk().await
        }

        async fn close(&self) -> Result<(), RecordingError> {
            self.entered.notify_one();
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn cancellation_during_finalization_retains_unpublished_partial() {
        let manager = RecordingManager::default();
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("cancelled.mp4");
        let entry = manager
            .reserve(params(output.to_string_lossy().into_owned()))
            .unwrap();
        entry.update(|state| {
            state.started = true;
            state.admitted = true;
        });
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        entry.retain_partial_path(stage.partial.to_string_lossy().into_owned());
        let mut control = entry.control.subscribe();
        let source = ClosingSource {
            source: FakeSource {
                chunks: Mutex::new(std::collections::VecDeque::from([Ok(
                    eoka::RecordingChunk {
                        data: mp4::REPAIRABLE_MP4.to_vec(),
                        eof: true,
                    },
                )])),
                close_fails: false,
            },
            entered: tokio::sync::Notify::new(),
        };
        let draining = async {
            tokio::select! {
                biased;
                _ = cancellation(&mut control) => None,
                result = drain(&source, &mut stage) => Some(result),
            }
        };
        let cancelling = async {
            source.entered.notified().await;
            entry.request(true);
        };
        let (result, ()) = tokio::join!(draining, cancelling);
        assert!(result.is_none());
        finish_cancel(&entry, None, &mut stage, None).await;
        assert!(!publish(&entry, &stage));
        let result = entry.snapshot();
        assert_eq!(result.state, RecordingState::Cancelled);
        assert!(result.artifact.is_none());
        let failure = result.failure.unwrap();
        assert_eq!(
            failure.partial_path.as_deref(),
            Some(stage.partial.to_str().unwrap())
        );
        assert_eq!(failure.cleanup_uncertain, None);
        assert!(!output.exists());
        assert_eq!(std::fs::read(&stage.partial).unwrap(), mp4::REPAIRABLE_MP4);
    }

    #[tokio::test]
    async fn valid_and_repairable_eof_payloads_finalize_before_publication() {
        for data in [mp4::VALID_MP4, mp4::REPAIRABLE_MP4] {
            let manager = RecordingManager::default();
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("capture.mp4");
            let entry = manager
                .reserve(params(output.to_string_lossy().into_owned()))
                .unwrap();
            entry.update(|state| {
                state.started = true;
                state.admitted = true;
            });
            let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .unwrap();
            let source = FakeSource {
                chunks: Mutex::new(std::collections::VecDeque::from([
                    Ok(eoka::RecordingChunk {
                        data: data[..data.len() - 16].to_vec(),
                        eof: false,
                    }),
                    Ok(eoka::RecordingChunk {
                        data: data[data.len() - 16..].to_vec(),
                        eof: true,
                    }),
                ])),
                close_fails: false,
            };
            drain(&source, &mut stage).await.unwrap();
            assert!(!output.exists());
            assert!(publish(&entry, &stage));
            let result = entry.snapshot();
            assert_eq!(result.state, RecordingState::Completed);
            assert_eq!(result.artifact.unwrap().bytes, data.len() as u64);
            assert!(!stage.partial.exists());
            assert_eq!(std::fs::read(output).unwrap(), mp4::VALID_MP4);
        }
    }

    #[tokio::test]
    async fn invalid_native_containers_fail_without_sealing_or_publication() {
        for data in [
            &[][..],
            &b"not an MP4 container"[..],
            &mp4::VALID_MP4[..mp4::VALID_MP4.len() - 1],
        ] {
            let manager = RecordingManager::default();
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("invalid.mp4");
            let entry = manager
                .reserve(params(output.to_string_lossy().into_owned()))
                .unwrap();
            entry.update(|state| {
                state.started = true;
                state.admitted = true;
            });
            let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .unwrap();
            let source = FakeSource {
                chunks: Mutex::new(std::collections::VecDeque::from([Ok(
                    eoka::RecordingChunk {
                        data: data.to_vec(),
                        eof: true,
                    },
                )])),
                close_fails: false,
            };
            let failure = drain(&source, &mut stage).await.unwrap_err();
            assert_eq!(failure.stage, RecordingFailureStage::Close);
            assert_eq!(failure.code, ErrorCode::RecordingFailed);
            assert!(stage
                .publish("video/mp4", Some(ArtifactContainer::Mp4))
                .is_err());
            finish_cancel(&entry, None, &mut stage, Some(failure)).await;
            let result = entry.snapshot();
            assert_eq!(result.state, RecordingState::Failed);
            assert!(result.artifact.is_none());
            let failure = result.failure.unwrap();
            assert_eq!(
                failure.partial_path.as_deref(),
                Some(stage.partial.to_str().unwrap())
            );
            assert_eq!(failure.cleanup_uncertain, None);
            assert!(!output.exists());
            assert_eq!(std::fs::read(&stage.partial).unwrap(), data);
            assert!(!publish(&entry, &stage));
        }
    }

    #[tokio::test]
    async fn stream_and_close_faults_preserve_stage_without_success_named_output() {
        for close_fails in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("capture.mp4");
            let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .unwrap();
            let mut chunks = std::collections::VecDeque::new();
            chunks.push_back(Ok(eoka::RecordingChunk {
                data: b"partial".to_vec(),
                eof: close_fails,
            }));
            if !close_fails {
                chunks.push_back(Err(RecordingError::Core(eoka::Error::cdp_msg(
                    "IO.read disconnected",
                ))));
            }
            let source = FakeSource {
                chunks: Mutex::new(chunks),
                close_fails,
            };
            let failure = drain(&source, &mut stage).await.unwrap_err();
            assert_eq!(
                failure.stage,
                if close_fails {
                    RecordingFailureStage::Close
                } else {
                    RecordingFailureStage::Read
                }
            );
            assert_eq!(
                failure.partial_path.as_deref(),
                Some(stage.partial.to_str().unwrap())
            );
            stage.abandon().await.unwrap();
            assert!(!output.exists());
            assert_eq!(std::fs::read(&stage.partial).unwrap(), b"partial");
        }
    }

    #[tokio::test]
    async fn oversized_native_chunks_are_rejected_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.mp4");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        let source = FakeSource {
            chunks: Mutex::new(std::collections::VecDeque::from([Ok(
                eoka::RecordingChunk {
                    data: vec![0; 65_537],
                    eof: true,
                },
            )])),
            close_fails: false,
        };
        assert_eq!(
            drain(&source, &mut stage).await.unwrap_err().stage,
            RecordingFailureStage::Read
        );
        stage.abandon().await.unwrap();
        assert_eq!(std::fs::metadata(&stage.partial).unwrap().len(), 0);
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn reserved_admission_can_be_cancelled_without_starting_a_recorder() {
        let manager = RecordingManager::default();
        let params = indexed_params(0);
        manager.admit(params.clone()).unwrap();
        assert_eq!(
            manager.cancel(&params.recording_id).await.unwrap().state,
            RecordingState::Cancelled
        );
        assert_eq!(
            manager
                .start(params, Err(ServerError::internal("must not dispatch")))
                .await
                .unwrap()
                .state,
            RecordingState::Cancelled
        );
    }

    #[tokio::test]
    async fn status_and_cancel_remain_responsive_while_stop_waits() {
        let manager = RecordingManager::default();
        let entry = manager
            .reserve(params("/tmp/controls.mp4".to_owned()))
            .unwrap();
        entry.update(|state| {
            state.started = true;
            state.admitted = true;
        });
        let mut command = entry.control.subscribe();
        let stop_manager = manager.clone();
        let id = entry.params.recording_id.clone();
        let stopping = tokio::spawn(async move { stop_manager.stop(&id).await.unwrap() });
        command.changed().await.unwrap();
        let status = tokio::time::timeout(
            Duration::from_millis(100),
            manager.get(&entry.params.recording_id),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(status.state, RecordingState::Finalizing);
        let cancel_manager = manager.clone();
        let id = entry.params.recording_id.clone();
        let cancelling = tokio::spawn(async move { cancel_manager.cancel(&id).await.unwrap() });
        command.changed().await.unwrap();
        assert_eq!(*command.borrow(), Control::Cancel);
        assert!(!stopping.is_finished());
        entry.update(|state| {
            state.record.state = RecordingState::Cancelled;
        });
        assert_eq!(stopping.await.unwrap().state, RecordingState::Cancelled);
        let result = cancelling.await.unwrap();
        assert_eq!(result.state, RecordingState::Cancelled);
        assert!(result.artifact.is_none());
    }

    #[tokio::test]
    async fn shutdown_expiry_keeps_partial_evidence_and_cannot_later_turn_into_success() {
        let manager = RecordingManager::default();
        let entry = manager
            .reserve(params("/tmp/timed-out.mp4".to_owned()))
            .unwrap();
        entry.update(|state| {
            state.started = true;
            state.admitted = true;
            state.partial_path = Some("/tmp/.eoka-capture.partial".to_owned());
        });
        let error = manager.shutdown_timeout_error().unwrap();
        let failure = error.data.unwrap().recording.unwrap().failure.unwrap();
        assert_eq!(failure.stage, RecordingFailureStage::Cancel);
        assert_eq!(failure.cleanup_uncertain, Some(true));
        assert_eq!(
            failure.partial_path.as_deref(),
            Some("/tmp/.eoka-capture.partial")
        );
        let root = tempfile::tempdir().unwrap();
        let mut stage = StagedArtifact::prepare(root.path().join("late.mp4").to_str().unwrap())
            .await
            .unwrap();
        finish_cancel(&entry, None, &mut stage, None).await;
        assert_eq!(entry.snapshot().state, RecordingState::Failed);
        assert!(entry.snapshot().artifact.is_none());
    }

    #[tokio::test]
    async fn blocked_commit_does_not_block_status_cancel_or_shutdown_arbitration() {
        for expire in [false, true] {
            let manager = RecordingManager::default();
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("commit.mp4");
            let entry = manager
                .reserve(params(output.to_string_lossy().into_owned()))
                .unwrap();
            entry.update(|state| {
                state.started = true;
                state.admitted = true;
            });
            entry.request(false);
            let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .unwrap();
            stage.write(mp4::VALID_MP4).await.unwrap();
            repair_stage(&mut stage).await.unwrap();
            stage.seal().await.unwrap();
            entry.retain_partial_path(stage.partial.to_string_lossy().into_owned());
            let (ready, waiting) = std::sync::mpsc::channel();
            let (release, blocked) = std::sync::mpsc::channel();
            let publisher = entry.clone();
            let publishing = tokio::task::spawn_blocking(move || {
                publish_with(&publisher, || {
                    ready.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(2)).unwrap();
                    stage.publish("video/mp4", Some(ArtifactContainer::Mp4))
                })
            });
            waiting.recv_timeout(Duration::from_secs(1)).unwrap();
            let started = std::time::Instant::now();
            assert_eq!(
                manager.get(&entry.params.recording_id).await.unwrap().state,
                RecordingState::Finalizing
            );
            entry.request(true);
            manager.request_shutdown();
            assert!(started.elapsed() < Duration::from_millis(100));
            assert!(!terminal(entry.snapshot().state));
            if expire {
                let started = std::time::Instant::now();
                let error = manager.shutdown_timeout_error().unwrap();
                assert!(started.elapsed() < Duration::from_millis(100));
                let failure = error.data.unwrap().recording.unwrap().failure.unwrap();
                assert_eq!(failure.stage, RecordingFailureStage::Publish);
                assert_eq!(failure.cleanup_uncertain, Some(true));
                assert!(failure.message.contains("may still complete"));
            }
            release.send(()).unwrap();
            assert!(publishing.await.unwrap());
            let result = manager.cancel(&entry.params.recording_id).await.unwrap();
            assert_eq!(
                result.state,
                if expire {
                    RecordingState::Failed
                } else {
                    RecordingState::Completed
                }
            );
            assert_eq!(std::fs::read(&output).unwrap(), mp4::VALID_MP4);
            if expire {
                let failure = result.failure.unwrap();
                assert!(failure.partial_path.is_none());
                assert!(failure.message.contains("publication later completed"));
                assert!(result.artifact.is_none());
            }
        }
    }

    #[tokio::test]
    async fn late_staging_after_terminal_shutdown_keeps_path_without_changing_outcome() {
        let manager = RecordingManager::default();
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("late.mp4");
        let entry = manager
            .reserve(params(output.to_string_lossy().into_owned()))
            .unwrap();
        entry.update(|state| {
            state.started = true;
            state.admitted = true;
        });
        manager.shutdown_timeout_error().unwrap();
        assert!(entry.snapshot().failure.unwrap().partial_path.is_none());
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        entry.retain_partial_path(stage.partial.to_string_lossy().into_owned());
        finish_cancel(&entry, None, &mut stage, None).await;
        let record = manager.get(&entry.params.recording_id).await.unwrap();
        assert_eq!(record.state, RecordingState::Failed);
        assert_eq!(
            record.failure.unwrap().partial_path.as_deref(),
            Some(stage.partial.to_str().unwrap())
        );
        assert!(stage.partial.exists());
        assert!(!output.exists());
    }
}
