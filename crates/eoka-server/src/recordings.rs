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
use tokio::sync::watch;

use crate::artifacts::{FileFailure, StagedArtifact};

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

#[derive(Clone, Default)]
pub struct RecordingManager {
    entries: Arc<Mutex<HashMap<String, Arc<Entry>>>>,
}

impl RecordingManager {
    fn entry(&self, id: &str) -> Result<Arc<Entry>, ServerError> {
        let entry = self.entries.lock().get(id).cloned();
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
        let mut entries = self.entries.lock();
        if let Some(entry) = entries.get(&params.recording_id).cloned() {
            drop(entries);
            if entry.params != params {
                return Err(entry.error(
                    ErrorCode::RecordingConflict,
                    "recording ID already belongs to different start parameters",
                ));
            }
            return Ok(entry);
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
        entries.insert(entry.params.recording_id.clone(), entry.clone());
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
            .entries
            .lock()
            .values()
            .filter(|entry| entry.params.page_id == page_id && !entry.is_terminal())
            .cloned()
            .collect();
        self.cancel_entries(entries).await
    }

    pub fn request_shutdown(&self) {
        for entry in self.entries.lock().values() {
            entry.request(true);
        }
    }

    pub fn shutdown_timeout_error(&self) -> Option<ServerError> {
        let entries: Vec<_> = self.entries.lock().values().cloned().collect();
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
            .entries
            .lock()
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
            stage.write(b"completed").await.unwrap();
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
                assert_eq!(std::fs::read(&output).unwrap(), b"completed");
            }
        }
    }

    struct FakeSource {
        chunks: Mutex<std::collections::VecDeque<Result<eoka::RecordingChunk, RecordingError>>>,
        calls: Mutex<Vec<&'static str>>,
        close_fails: bool,
    }

    impl RecordingSource for FakeSource {
        async fn stop(&self) -> Result<(), RecordingError> {
            self.calls.lock().push("stop");
            Ok(())
        }
        async fn read_chunk(&self) -> Result<eoka::RecordingChunk, RecordingError> {
            self.calls.lock().push("read");
            self.chunks.lock().pop_front().unwrap()
        }
        async fn close(&self) -> Result<(), RecordingError> {
            self.calls.lock().push("close");
            if self.close_fails {
                Err(RecordingError::Core(eoka::Error::cdp_msg(
                    "IO.close failed",
                )))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn drains_final_eof_bytes_and_closes_stream_before_publication() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.mp4");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        let source = FakeSource {
            chunks: Mutex::new(std::collections::VecDeque::from([
                Ok(eoka::RecordingChunk {
                    data: b"first".to_vec(),
                    eof: false,
                }),
                Ok(eoka::RecordingChunk {
                    data: b"last".to_vec(),
                    eof: true,
                }),
            ])),
            calls: Mutex::new(Vec::new()),
            close_fails: false,
        };
        drain(&source, &mut stage).await.unwrap();
        assert_eq!(*source.calls.lock(), ["stop", "read", "read", "close"]);
        assert!(!output.exists());
        let artifact = stage
            .publish("video/mp4", Some(ArtifactContainer::Mp4))
            .unwrap();
        assert_eq!(artifact.bytes, 9);
        assert_eq!(std::fs::read(output).unwrap(), b"firstlast");
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
                calls: Mutex::new(Vec::new()),
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
            calls: Mutex::new(Vec::new()),
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
        let params = params("/tmp/never-started.mp4".to_owned());
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
            stage.write(b"completed").await.unwrap();
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
            assert_eq!(std::fs::read(&output).unwrap(), b"completed");
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
