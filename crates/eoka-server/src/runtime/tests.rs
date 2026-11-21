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

    fn overtakes(&self, request: &Request) -> bool {
        request.params["overtakes"] == true
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
    harness.send(2, "info", page("a", "behind stuck", "answer"));
    harness.send(3, "info", page("b", "other page", "answer"));
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
async fn a_request_that_clears_a_blocked_page_runs_while_that_page_is_stuck() {
    let mut harness = Harness::start();
    harness.send(1, "captcha.turnstile", page("a", "stuck", "never"));
    harness.send(2, "info", page("a", "queued", "answer"));
    harness.send(
        3,
        "dialog.accept",
        json!({ "pageId": "a", "label": "accept", "overtakes": true }),
    );
    let accepted = harness.reply().await;
    assert_eq!(accepted["id"], 3);
    assert_eq!(accepted["result"]["label"], "accept");
    harness.quiet().await;
    assert_eq!(harness.fake.started(), ["stuck", "accept"]);
    harness.finish().await;
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
    harness.send(3, "info", page("b", "later", "answer"));
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
    harness.send(4, "info", page("b", "queued", "answer"));
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

struct SlotProbe {
    permits: Arc<Semaphore>,
    free_when_written: Arc<parking_lot::Mutex<Vec<usize>>>,
}

impl AsyncWrite for SlotProbe {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.free_when_written
            .lock()
            .push(self.permits.available_permits());
        std::task::Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn an_in_flight_slot_is_free_before_its_response_reaches_the_client() {
    let permits = Arc::new(Semaphore::new(2));
    let free_when_written = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let (responses, receiver) = mpsc::channel(4);
    for id in [1, 2] {
        let permit = permits.clone().try_acquire_owned().unwrap();
        let queued = Outgoing::new(json!(id), Ok(json!({})), None, Some(permit));
        assert!(responses.try_send(queued).is_ok());
    }
    assert_eq!(permits.available_permits(), 0);
    drop(responses);
    writer(
        SlotProbe {
            permits: permits.clone(),
            free_when_written: free_when_written.clone(),
        },
        receiver,
    )
    .await
    .unwrap();
    assert_eq!(*free_when_written.lock(), [1, 2]);
}

#[derive(Clone, Default)]
struct Written(Arc<parking_lot::Mutex<Vec<u8>>>);

impl AsyncWrite for Written {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.0.lock().extend_from_slice(bytes);
        std::task::Poll::Ready(Ok(bytes.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

async fn written(error: ServerError) -> Value {
    let (responses, receiver) = mpsc::channel(1);
    assert!(responses
        .try_send(Outgoing::new(json!(7), Err(error), None, None))
        .is_ok());
    drop(responses);
    let written = Written::default();
    writer(written.clone(), receiver).await.unwrap();
    let line = written.0.lock().clone();
    serde_json::from_slice(&line).unwrap()
}

#[tokio::test]
async fn a_blocked_page_error_is_written_with_its_blocker() {
    let blocked = ServerError::blocked(
        eoka_protocol::ErrorDetail::new("dialog_open", "A JavaScript alert dialog is open"),
        Some(eoka_protocol::sidecar::PageBlocker::Dialog {
            dialog_type: "alert".into(),
            message: "are you sure?".into(),
            url: "https://example.test/".into(),
            elapsed_ms: 10,
        }),
    );
    let reply = written(blocked).await;
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["error"]["code"], "DialogOpen");
    assert_eq!(reply["error"]["data"]["detail"]["code"], "dialog_open");
    assert_eq!(reply["error"]["data"]["blocker"]["kind"], "dialog");
    assert_eq!(
        reply["error"]["data"]["blocker"]["message"],
        "are you sure?"
    );
    let ordinary = written(ServerError::invalid_page("gone")).await;
    assert_eq!(ordinary["error"]["code"], "InvalidPage");
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
        "info",
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
        ("info", json!({})),
        ("info", json!({ "pageId": 7 })),
    ] {
        assert_eq!(lane(&request(method, params)), Lane::Independent);
    }
}
