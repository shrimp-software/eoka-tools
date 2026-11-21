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
