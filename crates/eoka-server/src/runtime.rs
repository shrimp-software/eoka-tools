mod lanes;

use std::{future::Future, sync::Arc, time::Duration};

use eoka_protocol::sidecar::{
    validate_params, validate_safe_numbers, ErrorCode, ErrorData, Method, RecordingStartParams,
    Request, Response, ServerError, MAX_IN_FLIGHT,
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
    operations::sidecar::{self, OperationControl},
    recordings::RecordingControl,
    state::AppState,
    transport::{self, Frame, FrameReader},
};
use lanes::{Lane, Lanes};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(35);

trait Backend: Clone + Send + Sync + 'static {
    fn dispatch(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, ServerError>> + Send;
    fn overtakes(&self, request: &Request) -> bool;
    fn should_shutdown(&self) -> bool;
    fn shutdown(&self) -> impl Future<Output = Result<(), ServerError>> + Send;
}

impl Backend for AppState {
    fn dispatch(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, ServerError>> + Send {
        dispatch::dispatch_shared(self, method, params)
    }

    fn overtakes(&self, request: &Request) -> bool {
        dispatch::clears_a_blocked_page(self, &request.method, &request.params)
    }

    fn should_shutdown(&self) -> bool {
        AppState::should_shutdown(self)
    }

    fn shutdown(&self) -> impl Future<Output = Result<(), ServerError>> + Send {
        AppState::shutdown(self)
    }
}

struct Job {
    request: Request,
    operation_id: Option<String>,
    permit: OwnedSemaphorePermit,
}

struct Outgoing {
    id: Value,
    operation_id: Option<String>,
    response: Response,
    permit: Option<OwnedSemaphorePermit>,
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
            permit,
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

enum Waiter {
    Operation(OperationControl),
    Recording(RecordingControl),
}

fn waiter(method: Option<Method>) -> Option<Waiter> {
    let method = method?;
    OperationControl::of(method)
        .map(Waiter::Operation)
        .or_else(|| RecordingControl::of(method).map(Waiter::Recording))
}

fn lane(request: &Request) -> Lane {
    if matches!(
        Method::parse(&request.method),
        Some(Method::BrowserLaunch | Method::BrowserConnect | Method::BrowserClose)
    ) {
        return Lane::Barrier;
    }
    request
        .params
        .get("pageId")
        .and_then(Value::as_str)
        .map_or(Lane::Independent, |page| Lane::Page(page.to_owned()))
}

fn admit<B: Backend>(backend: &B, lanes: &mut Lanes<Job>, job: Job) {
    let operation = job.operation_id.clone();
    if Method::parse(&job.request.method)
        .and_then(OperationControl::of)
        .is_some()
    {
        lanes.admit_after_operation(operation, job);
    } else if backend.overtakes(&job.request) {
        lanes.admit_overtaking(lane(&job.request), operation, job);
    } else {
        lanes.admit(lane(&job.request), operation, job);
    }
}

fn protected(request: &Request) -> bool {
    matches!(
        Method::parse(&request.method),
        Some(
            Method::BrowserLaunch
                | Method::BrowserConnect
                | Method::ActionsPerform
                | Method::AgentStep
        )
    ) || request.params.get("operation_id").is_some()
}

fn undispatched(job: Job) -> Outgoing {
    Outgoing::result(
        job,
        Err(ServerError::new(
            ErrorCode::ShuttingDown,
            "request was not dispatched because the server is shutting down",
        )),
    )
}

async fn execute<B: Backend>(
    backend: B,
    ticket: u64,
    mut job: Job,
    mut shutdown: watch::Receiver<bool>,
) -> (u64, Outgoing) {
    let protected = protected(&job.request);
    let result = {
        let params = std::mem::take(&mut job.request.params);
        let dispatch = backend.dispatch(&job.request.method, params);
        tokio::pin!(dispatch);
        tokio::select! {
            biased;
            _ = cancelled(&mut shutdown) => {
                if protected {
                    match tokio::time::timeout(SHUTDOWN_GRACE, &mut dispatch).await {
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
    (ticket, Outgoing::result(job, result))
}

fn settled(finished: Result<(u64, Outgoing), tokio::task::JoinError>) -> (u64, Outgoing) {
    match finished {
        Ok(settled) => settled,
        Err(error) => match error.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(error) => panic!("request task ended without a response: {error}"),
        },
    }
}

async fn worker<B: Backend>(
    backend: B,
    mut jobs: mpsc::Receiver<Job>,
    output: mpsc::Sender<Outgoing>,
    mut shutdown: watch::Receiver<bool>,
    stop: watch::Sender<bool>,
) -> Result<(), ServerError> {
    let mut lanes = Lanes::default();
    let mut running = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = cancelled(&mut shutdown) => break,
            Some(finished) = running.join_next(), if !running.is_empty() => {
                let (ticket, response) = settled(finished);
                lanes.finish(ticket);
                if emit(&output, response).is_err() || backend.should_shutdown() {
                    break;
                }
            },
            job = jobs.recv() => match job {
                Some(job) => admit(&backend, &mut lanes, job),
                None => break,
            },
        }
        for (ticket, job) in lanes.ready() {
            running.spawn(execute(backend.clone(), ticket, job, shutdown.clone()));
        }
    }
    stop.send_replace(true);
    jobs.close();
    for job in lanes.drain() {
        let _ = emit(&output, undispatched(job));
    }
    while let Some(job) = jobs.recv().await {
        let _ = emit(&output, undispatched(job));
    }
    while let Some(finished) = running.join_next().await {
        let _ = emit(&output, settled(finished).1);
    }
    backend.shutdown().await
}

async fn writer<W: AsyncWrite + Unpin>(
    mut output: W,
    mut responses: mpsc::Receiver<Outgoing>,
) -> anyhow::Result<()> {
    while let Some(outgoing) = responses.recv().await {
        let Outgoing {
            id,
            operation_id,
            response,
            permit,
        } = outgoing;
        drop(permit);
        let bytes = transport::encode(&response, &id, operation_id.as_deref())?;
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
    crate::engine::blockers::set_surface(crate::engine::blockers::Surface::Sidecar);
    state.operations.halt_on_dialog();
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
                let method = Method::parse(&request.method);
                if method == Some(Method::ServerShutdown) {
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
                if method == Some(Method::RecordingStart) {
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
                if method == Some(Method::ServerHello) {
                    let result = dispatch::hello(std::mem::take(&mut job.request.params));
                    if let Err(error) = emit(&responses, Outgoing::result(job, result)) {
                        intake_error = Some(error);
                        break;
                    }
                } else if let Some(waiter) = waiter(method) {
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
                                match waiter {
                                    Waiter::Operation(control @ (OperationControl::Get | OperationControl::Cancel)) => {
                                        sidecar::existing_control(&operations, control, &job.request.params).await
                                    },
                                    Waiter::Operation(control) => {
                                        let params = std::mem::take(&mut job.request.params);
                                        Some(sidecar::control(&operations, control, params).await)
                                    },
                                    Waiter::Recording(control) => {
                                        let params = std::mem::take(&mut job.request.params);
                                        Some(crate::recordings::control(&recordings, control, params).await)
                                    },
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

#[cfg(test)]
mod tests;
