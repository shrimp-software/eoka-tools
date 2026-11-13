mod lanes;

use std::{future::Future, sync::Arc, time::Duration};

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
use lanes::{Lane, Lanes};

const SHUTDOWN_GRACE: Duration = Duration::from_secs(35);

trait Backend: Clone + Send + Sync + 'static {
    fn dispatch(
        &self,
        method: &str,
        params: Value,
    ) -> impl Future<Output = Result<Value, ServerError>> + Send;
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

fn lane(request: &Request) -> Lane {
    if matches!(
        request.method.as_str(),
        "browser.launch" | "browser.connect" | "browser.close"
    ) {
        return Lane::Barrier;
    }
    request
        .params
        .get("pageId")
        .and_then(Value::as_str)
        .map_or(Lane::Independent, |page| Lane::Page(page.to_owned()))
}

fn admit(lanes: &mut Lanes<Job>, job: Job) {
    let operation = job.operation_id.clone();
    if sidecar::supports_control(&job.request.method) {
        lanes.admit_after_operation(operation, job);
    } else {
        lanes.admit(lane(&job.request), operation, job);
    }
}

fn protected(request: &Request) -> bool {
    matches!(
        request.method.as_str(),
        "browser.launch" | "browser.connect" | "actions.perform" | "agent.step"
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
                Some(job) => admit(&mut lanes, job),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::{Barrier, Notify};

    const PATIENCE: Duration = Duration::from_secs(10);
    const QUIET: Duration = Duration::from_millis(150);

    #[derive(Clone)]
    struct Fake {
        started: Arc<parking_lot::Mutex<Vec<String>>>,
        finished: Arc<parking_lot::Mutex<Vec<String>>>,
        rendezvous: Arc<Barrier>,
        gate: Arc<Notify>,
        closed: Arc<AtomicBool>,
        shutdowns: Arc<AtomicUsize>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                started: Default::default(),
                finished: Default::default(),
                rendezvous: Arc::new(Barrier::new(2)),
                gate: Arc::new(Notify::new()),
                closed: Default::default(),
                shutdowns: Default::default(),
            }
        }

        fn started(&self) -> Vec<String> {
            self.started.lock().clone()
        }

        fn finished(&self) -> Vec<String> {
            self.finished.lock().clone()
        }
    }

    impl Backend for Fake {
        async fn dispatch(&self, method: &str, params: Value) -> Result<Value, ServerError> {
            let label = params["label"].as_str().unwrap_or(method).to_owned();
            self.started.lock().push(label.clone());
            match params["behaviour"].as_str().unwrap_or("answer") {
                "never" => std::future::pending::<()>().await,
                "rendezvous" => {
                    self.rendezvous.wait().await;
                }
                "gate" => self.gate.notified().await,
                "sleep" => {
                    tokio::time::sleep(Duration::from_millis(params["ms"].as_u64().unwrap())).await
                }
                "close" => self.closed.store(true, Ordering::Release),
                _ => {}
            }
            self.finished.lock().push(label.clone());
            Ok(json!({ "label": label }))
        }

        fn should_shutdown(&self) -> bool {
            self.closed.load(Ordering::Acquire)
        }

        async fn shutdown(&self) -> Result<(), ServerError> {
            self.shutdowns.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }

    struct Harness {
        fake: Fake,
        jobs: mpsc::Sender<Job>,
        responses: mpsc::Receiver<Outgoing>,
        stop: watch::Sender<bool>,
        permits: Arc<Semaphore>,
        worker: tokio::task::JoinHandle<Result<(), ServerError>>,
    }

    impl Harness {
        fn start() -> Self {
            let fake = Fake::new();
            let (jobs, job_receiver) = mpsc::channel(MAX_IN_FLIGHT);
            let (output, responses) = mpsc::channel(MAX_IN_FLIGHT + 1);
            let (stop, shutdown) = watch::channel(false);
            let worker = tokio::spawn(worker(
                fake.clone(),
                job_receiver,
                output,
                shutdown,
                stop.clone(),
            ));
            Self {
                fake,
                jobs,
                responses,
                stop,
                permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
                worker,
            }
        }

        fn send(&self, id: u64, method: &str, params: Value) {
            let request = Request {
                id: json!(id),
                method: method.to_owned(),
                params,
            };
            let job = Job {
                operation_id: operation_id(&request),
                request,
                permit: self.permits.clone().try_acquire_owned().unwrap(),
            };
            assert!(self.jobs.try_send(job).is_ok());
        }

        async fn reply(&mut self) -> Value {
            let outgoing = tokio::time::timeout(PATIENCE, self.responses.recv())
                .await
                .expect("a response must arrive")
                .expect("the response queue must stay open");
            serde_json::to_value(&outgoing.response).unwrap()
        }

        async fn replies(&mut self, count: usize) -> Vec<Value> {
            let mut replies = Vec::new();
            for _ in 0..count {
                replies.push(self.reply().await);
            }
            replies.sort_by_key(|reply| reply["id"].as_u64());
            replies
        }

        async fn quiet(&mut self) {
            assert!(
                tokio::time::timeout(QUIET, self.responses.recv())
                    .await
                    .is_err(),
                "no response may arrive yet"
            );
        }

        async fn finish(self) -> Fake {
            self.stop.send_replace(true);
            tokio::time::timeout(PATIENCE, self.worker)
                .await
                .expect("the worker must stop")
                .unwrap()
                .unwrap();
            self.fake
        }
    }

    fn page(page: &str, label: &str, behaviour: &str) -> Value {
        json!({ "pageId": page, "label": label, "behaviour": behaviour })
    }

    #[tokio::test]
    async fn requests_for_different_pages_are_in_flight_together() {
        let mut harness = Harness::start();
        harness.send(1, "page.evaluate", page("a", "a", "rendezvous"));
        harness.send(2, "page.evaluate", page("b", "b", "rendezvous"));
        let replies = harness.replies(2).await;
        assert_eq!(replies[0]["result"]["label"], "a");
        assert_eq!(replies[1]["result"]["label"], "b");
        harness.finish().await;
    }

    #[tokio::test]
    async fn a_page_that_never_answers_delays_only_its_own_requests() {
        let mut harness = Harness::start();
        harness.send(1, "page.evaluate", page("a", "stuck", "never"));
        harness.send(2, "page.title", page("a", "behind stuck", "answer"));
        harness.send(3, "page.title", page("b", "other page", "answer"));
        harness.send(4, "browser.new_page", json!({ "label": "new page" }));
        harness.send(5, "browser.tabs", json!({ "label": "tabs" }));
        let replies = harness.replies(3).await;
        assert_eq!(
            replies
                .iter()
                .map(|reply| reply["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [3, 4, 5]
        );
        harness.quiet().await;
        assert!(!harness.fake.started().contains(&"behind stuck".to_owned()));
        harness.stop.send_replace(true);
        let replies = harness.replies(2).await;
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[0]["error"]["code"], "Cancelled");
        assert_eq!(replies[1]["id"], 2);
        assert_eq!(replies[1]["error"]["code"], "ShuttingDown");
        let fake = harness.finish().await;
        assert_eq!(fake.shutdowns.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn one_page_runs_its_requests_in_admission_order() {
        let mut harness = Harness::start();
        for (id, ms) in [(1, 60), (2, 30), (3, 1), (4, 15), (5, 0)] {
            harness.send(
                id,
                "page.evaluate",
                json!({ "pageId": "a", "label": id.to_string(), "behaviour": "sleep", "ms": ms }),
            );
            harness.send(
                id + 10,
                "page.evaluate",
                json!({ "pageId": "b", "label": (id + 10).to_string(), "behaviour": "sleep", "ms": 60 - ms }),
            );
        }
        harness.replies(10).await;
        let fake = harness.finish().await;
        for order in [fake.started(), fake.finished()] {
            for lane in [["1", "2", "3", "4", "5"], ["11", "12", "13", "14", "15"]] {
                let seen: Vec<&str> = order
                    .iter()
                    .map(String::as_str)
                    .filter(|label| lane.contains(label))
                    .collect();
                assert_eq!(seen, lane);
            }
        }
    }

    #[tokio::test]
    async fn browser_lifecycle_waits_for_earlier_requests_and_ends_the_session() {
        let mut harness = Harness::start();
        harness.send(1, "page.evaluate", page("a", "earlier", "gate"));
        harness.send(
            2,
            "browser.close",
            json!({ "label": "close", "behaviour": "close" }),
        );
        harness.send(3, "page.title", page("b", "later", "answer"));
        harness.quiet().await;
        assert_eq!(harness.fake.started(), ["earlier"]);
        harness.fake.gate.notify_one();
        let replies = harness.replies(3).await;
        assert_eq!(replies[0]["result"]["label"], "earlier");
        assert_eq!(replies[1]["result"]["label"], "close");
        assert_eq!(replies[2]["error"]["code"], "ShuttingDown");
        assert_eq!(harness.fake.started(), ["earlier", "close"]);
        let worker = tokio::time::timeout(PATIENCE, &mut harness.worker).await;
        assert!(matches!(worker, Ok(Ok(Ok(())))));
        assert!(*harness.stop.borrow());
        assert_eq!(harness.fake.shutdowns.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn a_fresh_operation_control_follows_only_its_own_admission() {
        let mut harness = Harness::start();
        let operation = "6e8b0670-a457-4ce1-8d21-f1e4cf8c8e9b";
        harness.send(
            1,
            "actions.perform",
            json!({ "pageId": "a", "operation_id": operation, "label": "admission", "behaviour": "gate" }),
        );
        harness.send(
            2,
            "operations.get",
            json!({ "id": operation, "label": "get" }),
        );
        harness.send(
            3,
            "operations.get",
            json!({ "id": "00000000-0000-4000-8000-000000000000", "label": "unrelated" }),
        );
        let unrelated = harness.reply().await;
        assert_eq!(unrelated["id"], 3);
        harness.quiet().await;
        assert_eq!(harness.fake.started(), ["admission", "unrelated"]);
        harness.fake.gate.notify_one();
        let replies = harness.replies(2).await;
        assert_eq!(replies[0]["result"]["label"], "admission");
        assert_eq!(replies[1]["result"]["label"], "get");
        assert_eq!(harness.fake.finished(), ["unrelated", "admission", "get"]);
        harness.finish().await;
    }

    #[tokio::test]
    async fn shutdown_lets_protected_requests_finish_and_interrupts_the_rest() {
        let mut harness = Harness::start();
        harness.send(
            1,
            "actions.perform",
            json!({ "pageId": "a", "label": "admission", "behaviour": "gate" }),
        );
        harness.send(2, "page.evaluate", page("b", "ordinary", "never"));
        harness.send(
            3,
            "page.fill",
            json!({ "pageId": "c", "operation_id": "op", "label": "typed admission", "behaviour": "gate" }),
        );
        harness.send(4, "page.title", page("b", "queued", "answer"));
        while harness.fake.started().len() < 3 {
            tokio::task::yield_now().await;
        }
        harness.stop.send_replace(true);
        let replies = harness.replies(2).await;
        assert_eq!(replies[0]["id"], 2);
        assert_eq!(replies[0]["error"]["code"], "Cancelled");
        assert_eq!(replies[1]["id"], 4);
        assert_eq!(replies[1]["error"]["code"], "ShuttingDown");
        harness.quiet().await;
        assert_eq!(harness.fake.shutdowns.load(Ordering::Acquire), 0);
        harness.fake.gate.notify_waiters();
        let replies = harness.replies(2).await;
        assert_eq!(replies[0]["result"]["label"], "admission");
        assert_eq!(replies[1]["result"]["label"], "typed admission");
        let fake = harness.finish().await;
        assert_eq!(fake.shutdowns.load(Ordering::Acquire), 1);
    }

    #[test]
    fn requests_are_routed_by_the_page_they_address() {
        let request = |method: &str, params: Value| Request {
            id: json!(1),
            method: method.to_owned(),
            params,
        };
        for method in ["browser.launch", "browser.connect", "browser.close"] {
            assert_eq!(lane(&request(method, json!({}))), Lane::Barrier);
        }
        for method in [
            "page.title",
            "page.evaluate",
            "page.close",
            "browser.close_tab",
            "actions.perform",
            "agent.step",
            "recording.start",
            "console",
        ] {
            assert_eq!(
                lane(&request(method, json!({ "pageId": "tab" }))),
                Lane::Page("tab".into())
            );
        }
        for (method, params) in [
            ("browser.new_page", json!({})),
            ("browser.tabs", json!({})),
            ("page.title", json!({})),
            ("page.title", json!({ "pageId": 7 })),
        ] {
            assert_eq!(lane(&request(method, params)), Lane::Independent);
        }
    }
}
