use std::{sync::Arc, time::Duration};

use eoka_protocol::sidecar::{
    validate_params, validate_safe_numbers, ErrorCode, ErrorData, RecordingStartParams, Request,
    Response, ServerError, MAX_IN_FLIGHT,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufRead, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};

use crate::{
    dispatch,
    operations::sidecar,
    state::AppState,
    transport::{self, Frame, FrameReader},
};

struct Job {
    request: Request,
    operation_id: Option<String>,
    permit: OwnedSemaphorePermit,
}

struct Outgoing {
    id: Value,
    operation_id: Option<String>,
    response: Response,
    _permit: Option<OwnedSemaphorePermit>,
}

fn operation_id(request: &Request) -> Option<String> {
    request
        .params
        .get("operation_id")
        .or_else(|| {
            request
                .method
                .starts_with("operations.")
                .then(|| request.params.get("id"))
                .flatten()
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn with_operation(mut error: ServerError, operation_id: Option<&str>) -> ServerError {
    if let Some(id) = operation_id {
        error
            .data
            .get_or_insert_with(Box::<ErrorData>::default)
            .operation_id = Some(id.to_owned());
    }
    error
}

impl Outgoing {
    fn result(job: Job, result: Result<Value, ServerError>) -> Self {
        Self::new(job.request.id, result, job.operation_id, Some(job.permit))
    }

    fn new(
        id: Value,
        result: Result<Value, ServerError>,
        operation_id: Option<String>,
        permit: Option<OwnedSemaphorePermit>,
    ) -> Self {
        let result = result.and_then(|result| {
            validate_safe_numbers(&result).map_err(|error| {
                ServerError::internal(format!("unsafe integer in response: {}", error.message))
            })?;
            Ok(result)
        });
        let response = match result {
            Ok(result) => Response::ok(id.clone(), result),
            Err(error) => Response::err(id.clone(), with_operation(error, operation_id.as_deref())),
        };
        Self {
            id,
            operation_id,
            response,
            _permit: permit,
        }
    }
}

fn emit(output: &mpsc::Sender<Outgoing>, response: Outgoing) -> anyhow::Result<()> {
    output
        .try_send(response)
        .map_err(|_| anyhow::anyhow!("response queue is full or closed"))
}

async fn cancelled(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    let _ = shutdown.changed().await;
}

async fn worker(
    mut state: AppState,
    mut jobs: mpsc::Receiver<Job>,
    output: mpsc::Sender<Outgoing>,
    mut shutdown: watch::Receiver<bool>,
    stop: watch::Sender<bool>,
) -> Result<(), ServerError> {
    loop {
        let mut job = tokio::select! {
            biased;
            _ = cancelled(&mut shutdown) => break,
            job = jobs.recv() => match job { Some(job) => job, None => break },
        };
        let protected = matches!(
            job.request.method.as_str(),
            "browser.launch" | "browser.connect"
        ) || matches!(
            job.request.method.as_str(),
            "actions.perform" | "agent.step"
        ) || job.request.params.get("operation_id").is_some();
        let result = {
            let params = std::mem::take(&mut job.request.params);
            let dispatch = dispatch::dispatch(&mut state, &job.request.method, params);
            tokio::pin!(dispatch);
            tokio::select! {
                biased;
                _ = cancelled(&mut shutdown) => {
                    if protected {
                        match tokio::time::timeout(Duration::from_secs(35), &mut dispatch).await {
                            Ok(result) => result,
                            Err(_) => Err(ServerError::new(ErrorCode::Cancelled, "request exceeded the shutdown launch/admission grace deadline; inspect the operation ID before retrying")),
                        }
                    } else {
                        Err(ServerError::new(ErrorCode::Cancelled, "request interrupted by server shutdown"))
                    }
                },
                result = &mut dispatch => result,
            }
        };
        if emit(&output, Outgoing::result(job, result)).is_err() {
            break;
        }
        if state.should_shutdown() {
            stop.send_replace(true);
            break;
        }
    }
    stop.send_replace(true);
    jobs.close();
    while let Some(job) = jobs.recv().await {
        let _ = emit(
            &output,
            Outgoing::result(
                job,
                Err(ServerError::new(
                    ErrorCode::ShuttingDown,
                    "request was not dispatched because the server is shutting down",
                )),
            ),
        );
    }
    state.shutdown().await
}

async fn writer<W: AsyncWrite + Unpin>(
    mut output: W,
    mut responses: mpsc::Receiver<Outgoing>,
) -> anyhow::Result<()> {
    while let Some(response) = responses.recv().await {
        let bytes = transport::encode(
            &response.response,
            &response.id,
            response.operation_id.as_deref(),
        )?;
        tokio::time::timeout(Duration::from_secs(2), async {
            output.write_all(&bytes).await?;
            output.flush().await
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!("stdout backpressure exceeded the 2 second write deadline")
        })??;
    }
    Ok(())
}

pub async fn serve<R, W>(input: R, output: W, state: AppState) -> anyhow::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut input = FrameReader::new(input);
    let operations = state.operations.clone();
    let recordings = state.recordings.clone();
    let pending = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
    let (jobs, job_receiver) = mpsc::channel(MAX_IN_FLIGHT);
    let (responses, response_receiver) = mpsc::channel(MAX_IN_FLIGHT + 1);
    let (stop, shutdown) = watch::channel(false);
    let mut intake_shutdown = shutdown.clone();
    let mut worker = tokio::spawn(worker(
        state,
        job_receiver,
        responses.clone(),
        shutdown.clone(),
        stop.clone(),
    ));
    let mut writer = tokio::spawn(writer(output, response_receiver));
    let mut waiters = JoinSet::new();
    let mut shutdown_id = None;
    let mut worker_result = None;
    let mut writer_result = None;
    let mut intake_error = None;
    let mut input_closed = false;
    loop {
        tokio::select! {
            biased;
            _ = cancelled(&mut intake_shutdown) => break,
            result = &mut writer => {
                writer_result = Some(result);
                break;
            },
            result = &mut worker => {
                worker_result = Some(result);
                break;
            },
            Some(result) = waiters.join_next(), if !waiters.is_empty() => {
                match result {
                    Err(error) => {
                        intake_error = Some(anyhow::anyhow!("operation waiter failed: {error}"));
                        break;
                    },
                    Ok(Err(error)) => {
                        intake_error = Some(error);
                        break;
                    },
                    Ok(Ok(())) => {},
                }
            },
            frame = input.next() => {
                let frame = match frame {
                    Ok(Some(frame)) => frame,
                    Ok(None) => { input_closed = true; break; },
                    Err(error) => { intake_error = Some(error.into()); break; },
                };
                let request = match frame {
                    Frame::Empty => continue,
                    Frame::Error { id, error } => {
                        if let Err(error) = emit(&responses, Outgoing::new(id, Err(error), None, None)) {
                            intake_error = Some(error);
                            break;
                        }
                        continue;
                    },
                    Frame::Request(request) => request,
                };
                if request.method == "server.shutdown" {
                    match validate_params(&request.method, &request.params) {
                        Ok(()) => { shutdown_id = Some(request.id); break; },
                        Err(error) => {
                            if let Err(error) = emit(&responses, Outgoing::new(request.id, Err(error), None, None)) {
                                intake_error = Some(error);
                                break;
                            }
                            continue;
                        },
                    }
                }
                let permit = match pending.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let id = operation_id(&request);
                        if let Err(error) = emit(&responses, Outgoing::new(request.id, Err(ServerError::new(ErrorCode::ServerBusy, "maxInFlight pending requests exceeded")), id, None)) {
                            intake_error = Some(error);
                            break;
                        }
                        continue;
                    },
                };
                let mut job = Job { operation_id: operation_id(&request), request, permit };
                if job.request.method == "recording.start" {
                    let admission = RecordingStartParams::deserialize(&job.request.params)
                        .map_err(|error| ServerError::invalid_params(error.to_string()))
                        .and_then(|params| recordings.admit(params));
                    if let Err(error) = admission {
                        if let Err(error) = emit(&responses, Outgoing::result(job, Err(error))) {
                            intake_error = Some(error);
                            break;
                        }
                        continue;
                    }
                }
                if job.request.method == "server.hello" {
                    let result = dispatch::hello(std::mem::take(&mut job.request.params));
                    if let Err(error) = emit(&responses, Outgoing::result(job, result)) {
                        intake_error = Some(error);
                        break;
                    }
                } else if crate::recordings::supports_control(&job.request.method)
                    || sidecar::supports_control(&job.request.method)
                {
                    let operations = operations.clone();
                    let recordings = recordings.clone();
                    let output = responses.clone();
                    let jobs = jobs.clone();
                    let mut shutdown = shutdown.clone();
                    waiters.spawn(async move {
                        let result = tokio::select! {
                            biased;
                            _ = cancelled(&mut shutdown) => Some(Err(ServerError::new(ErrorCode::ShuttingDown, "wait ended because the server is shutting down; cancelling a waiter does not cancel its operation or recording"))),
                            result = async {
                                if matches!(job.request.method.as_str(), "operations.get" | "operations.cancel") {
                                    sidecar::existing_control(&operations, &job.request.method, &job.request.params).await
                                } else {
                                    let params = std::mem::take(&mut job.request.params);
                                    Some(if crate::recordings::supports_control(&job.request.method) {
                                        crate::recordings::control(&recordings, &job.request.method, params).await
                                    } else {
                                        sidecar::control(&operations, &job.request.method, params).await
                                    })
                                }
                            } => result,
                        };
                        match result {
                            Some(result) => emit(&output, Outgoing::result(job, result)),
                            None => match jobs.try_send(job) {
                                Ok(()) => Ok(()),
                                Err(error) => emit(
                                    &output,
                                    Outgoing::result(
                                        error.into_inner(),
                                        Err(ServerError::new(ErrorCode::ShuttingDown, "dispatch worker is shutting down")),
                                    ),
                                ),
                            },
                        }
                    });
                } else if let Err(error) = jobs.try_send(job) {
                    let job = error.into_inner();
                    if let Err(error) = emit(&responses, Outgoing::result(job, Err(ServerError::new(ErrorCode::ShuttingDown, "dispatch worker is shutting down")))) {
                        intake_error = Some(error);
                    }
                    break;
                }
            },
        }
    }
    stop.send_replace(true);
    drop(jobs);
    let cleanup = match worker_result {
        Some(result) => result,
        None => worker.await,
    }
    .map_err(|error| anyhow::anyhow!("dispatch worker failed: {error}"))?;
    while let Some(result) = waiters.join_next().await {
        if let Err(error) = result {
            intake_error.get_or_insert_with(|| anyhow::anyhow!("operation waiter failed: {error}"));
        } else if let Ok(Err(error)) = result {
            intake_error.get_or_insert(error);
        }
    }
    if let Some(id) = shutdown_id {
        let result = match &cleanup {
            Ok(()) => Ok(json!({})),
            Err(error) => Err(error.clone()),
        };
        if let Err(error) = emit(&responses, Outgoing::new(id, result, None, None)) {
            intake_error.get_or_insert(error);
        }
    }
    drop(responses);
    let written = match writer_result {
        Some(result) => result.map_err(anyhow::Error::from)?,
        None => match tokio::time::timeout(Duration::from_secs(2), &mut writer).await {
            Ok(result) => result.map_err(anyhow::Error::from)?,
            Err(_) => {
                writer.abort();
                let _ = writer.await;
                Err(anyhow::anyhow!(
                    "response drain exceeded the 2 second deadline"
                ))
            }
        },
    };
    cleanup.map_err(|error| anyhow::anyhow!(error.message))?;
    let parent_lost = written.as_ref().err().is_some_and(|error| {
        error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::BrokenPipe)
    });
    if input_closed || parent_lost {
        return Ok(());
    }
    written?;
    match intake_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
