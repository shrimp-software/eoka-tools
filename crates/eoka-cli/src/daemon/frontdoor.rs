use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eoka_server::engine::blockers::{self, Blocker, Remedies, Surface};
use eoka_server::operations::OperationManager;
use serde_json::{json, Value};
use tokio::{
    io::AsyncReadExt,
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixListener,
    },
    sync::{mpsc, OwnedMutexGuard},
    task::JoinSet,
};

use crate::protocol::{read_msg, write_msg, Request, Response};

pub(super) struct Admitted {
    pub reader: OwnedReadHalf,
    pub writer: OwnedWriteHalf,
    pub request: Request,
    pub raw: Value,
    pub turn: OwnedMutexGuard<()>,
}

struct Running {
    command: String,
    since: Instant,
}

pub(super) struct Board {
    session: String,
    hello: Value,
    queue_wait: Duration,
    turn: Arc<tokio::sync::Mutex<()>>,
    running: Mutex<Option<Running>>,
    page: Mutex<Option<eoka::Page>>,
    last_request: Mutex<Instant>,
    queued: AtomicUsize,
    waiting: AtomicUsize,
}

struct Counted<'a>(&'a AtomicUsize);

impl<'a> Counted<'a> {
    fn new(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn queue_wait() -> Duration {
    Duration::from_millis(
        std::env::var("EOKA_QUEUE_WAIT_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5_000),
    )
}

impl Board {
    pub fn new(session: &str, hello: Value) -> Arc<Self> {
        Arc::new(Self {
            session: session.to_owned(),
            hello,
            queue_wait: queue_wait(),
            turn: Default::default(),
            running: Mutex::new(None),
            page: Mutex::new(None),
            last_request: Mutex::new(Instant::now()),
            queued: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
        })
    }

    pub fn started(&self, command: &str, arguments: &Value) {
        *locked(&self.running) = Some(Running {
            command: self.remedies().name(command, arguments),
            since: Instant::now(),
        });
    }

    pub fn finished(&self, page: Option<eoka::Page>) {
        *locked(&self.running) = None;
        *locked(&self.page) = page;
    }

    pub fn last_request(&self) -> Instant {
        *locked(&self.last_request)
    }

    pub fn has_waiters(&self) -> bool {
        self.waiting.load(Ordering::SeqCst) + self.queued.load(Ordering::SeqCst) > 0
    }

    fn remedies(&self) -> Remedies<'_> {
        Remedies::for_surface(Surface::Cli, Some(&self.session))
    }

    fn page(&self) -> Option<eoka::Page> {
        locked(&self.page).clone()
    }

    fn blocker(&self) -> Option<Blocker> {
        self.page().as_ref().and_then(blockers::current)
    }

    fn busy(&self) -> Option<(String, Duration)> {
        locked(&self.running)
            .as_ref()
            .map(|running| (running.command.clone(), running.since.elapsed()))
    }

    fn activity(&self) -> Value {
        json!({
            "busy": self.busy().map(|(command, elapsed)| json!({
                "cmd": command,
                "for_ms": elapsed.as_millis() as u64,
            })),
            "queued": self.queued.load(Ordering::SeqCst),
            "blocker": self.blocker().map(|blocker| {
                let mut summary = blocker.summary();
                summary["description"] = json!(blocker.describe());
                summary["hint"] = json!(blocker.remedy(&self.remedies()));
                summary
            }),
        })
    }

    fn hello(&self) -> Value {
        let mut hello = self.hello.clone();
        if hello.is_object() {
            hello["activity"] = self.activity();
        }
        hello
    }

    fn busy_response(&self, command: &str) -> Response {
        let remedies = self.remedies();
        let waiting = self.queued.load(Ordering::SeqCst).saturating_sub(1);
        let others = match waiting {
            0 => String::new(),
            1 => "; 1 more command is queued".into(),
            many => format!("; {many} more commands are queued"),
        };
        let occupant = match self.busy() {
            Some((running, elapsed)) => format!(
                "`{running}` has been running for {:.1} s{others}",
                elapsed.as_secs_f64()
            ),
            None => format!("another command holds the browser{others}"),
        };
        let hint = match self.blocker() {
            Some(blocker) => format!("{}. {}", blocker.describe(), blocker.remedy(&remedies)),
            None => format!(
                "Run it again once the running command returns; {} shows what the session is doing.",
                remedies.status()
            ),
        };
        Response::err_detail(
            eoka_protocol::ErrorDetail::new(
                "daemon_busy",
                format!(
                    "Session `{}` is busy: {occupant}. `{command}` was not started.",
                    self.session
                ),
            )
            .retryable(true)
            .hint(hint)
            .context(eoka_protocol::ErrorContext {
                action_status: Some(eoka_protocol::ActionStatus::NotAttempted),
                ..Default::default()
            }),
        )
    }
}

pub(super) async fn serve(
    listener: UnixListener,
    board: Arc<Board>,
    operations: OperationManager,
    accepted: mpsc::Sender<Admitted>,
) {
    let mut connections = JoinSet::new();
    loop {
        let connection = tokio::select! {
            connection = listener.accept() => connection,
            _ = connections.join_next(), if !connections.is_empty() => continue,
        };
        let Ok((stream, _)) = connection else { break };
        *locked(&board.last_request) = Instant::now();
        if connections.len() >= 256 {
            continue;
        }
        connections.spawn(connection_task(
            stream,
            board.clone(),
            operations.clone(),
            accepted.clone(),
        ));
    }
}

async fn connection_task(
    stream: tokio::net::UnixStream,
    board: Arc<Board>,
    operations: OperationManager,
    accepted: mpsc::Sender<Admitted>,
) {
    let (mut reader, mut writer) = stream.into_split();
    let raw: Value = match tokio::time::timeout(Duration::from_secs(5), read_msg(&mut reader)).await
    {
        Ok(Ok(raw)) => raw,
        _ => return,
    };
    let request: Request = match serde_json::from_value(raw.clone()) {
        Ok(request) => request,
        Err(error) => {
            reply(
                &mut writer,
                Response::err(format!("Invalid command request: {error}")),
            )
            .await;
            return;
        }
    };
    if matches!(request, Request::SessionHello) {
        reply(&mut writer, Response::ok(board.hello())).await;
        return;
    }
    if raw["protocol_major"] != eoka_protocol::sidecar::PROTOCOL_VERSION.major {
        reply(&mut writer, Response::err_detail(eoka_protocol::ErrorDetail::new("protocol_mismatch", "Client and daemon protocols differ; upgrade explicitly. No browser was restarted and no command was dispatched."))).await;
        return;
    }
    let arguments = raw.get("args").cloned().unwrap_or(Value::Null);
    if matches!(request, Request::OperationWait(_)) {
        if board.waiting.load(Ordering::SeqCst) >= 64 {
            reply(
                &mut writer,
                Response::err_detail(eoka_protocol::ErrorDetail::new(
                    "daemon_busy",
                    "Too many operation waiters; no action was admitted",
                )),
            )
            .await;
            return;
        }
        let _waiting = Counted::new(&board.waiting);
        tokio::select! {
            response = operations.query(request.cmd(), &arguments) => {
                if let Some(response) = response {
                    reply(&mut writer, response).await;
                }
            }
            _ = hung_up(&mut reader) => {}
        }
        return;
    }
    if let Some(response) = operations.query(request.cmd(), &arguments).await {
        reply(&mut writer, response).await;
        return;
    }
    let turn = match board.turn.clone().try_lock_owned() {
        Ok(turn) => turn,
        Err(_) => {
            if let Some(response) = handle_dialog(&board, &request, &arguments).await {
                reply(&mut writer, response).await;
                return;
            }
            let name = board.remedies().name(request.cmd(), &arguments);
            let mut budget = board.queue_wait;
            if matches!(request, Request::AgentObserve(_) | Request::AgentStep(_)) {
                if let Some(own) = arguments["timeout_ms"].as_u64() {
                    budget = budget.min(Duration::from_millis(own));
                }
            }
            let _queued = Counted::new(&board.queued);
            tokio::select! {
                turn = board.turn.clone().lock_owned() => turn,
                _ = tokio::time::sleep(budget) => {
                    reply(&mut writer, board.busy_response(&name)).await;
                    return;
                }
                _ = hung_up(&mut reader) => return,
            }
        }
    };
    let _ = accepted
        .send(Admitted {
            reader,
            writer,
            request,
            raw,
            turn,
        })
        .await;
}

async fn handle_dialog(board: &Board, request: &Request, arguments: &Value) -> Option<Response> {
    let accept = match request.cmd() {
        "dialog_accept" => true,
        "dialog_dismiss" => false,
        _ => return None,
    };
    let page = board.page()?;
    let handled = tokio::time::timeout(Duration::from_secs(5), async {
        if accept {
            page.accept_dialog(arguments["prompt_text"].as_str()).await
        } else {
            page.dismiss_dialog().await
        }
    })
    .await;
    Some(match handled {
        Ok(Ok(())) if accept => Response::ok_text("Dialog accepted"),
        Ok(Ok(())) => Response::ok_text("Dialog dismissed"),
        Ok(Err(error)) => Response::err(error.to_string()),
        Err(_) => Response::err("Timeout: the browser did not acknowledge the dialog command"),
    })
}

async fn hung_up(reader: &mut OwnedReadHalf) {
    let mut probe = [0u8; 1];
    let _ = reader.read(&mut probe).await;
}

async fn reply(writer: &mut OwnedWriteHalf, response: Response) {
    match tokio::time::timeout(Duration::from_secs(5), write_msg(writer, &response)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("[eoka] control response disconnected: {error}"),
        Err(_) => eprintln!("[eoka] control response timed out"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Arc<Board> {
        Board::new("work", json!({"protocol_major":2,"mode":"headless"}))
    }

    #[test]
    fn hello_reports_what_the_session_is_doing() {
        let board = board();
        assert!(board.hello()["activity"]["busy"].is_null());
        board.started("action_command", &json!({"command":"reload"}));
        let hello = board.hello();
        assert_eq!(hello["protocol_major"], 2);
        assert_eq!(hello["activity"]["busy"]["cmd"], "reload");
        assert_eq!(hello["activity"]["queued"], 0);
        board.finished(None);
        assert!(board.hello()["activity"]["busy"].is_null());
    }

    #[test]
    fn a_busy_refusal_names_the_running_command_and_the_next_step() {
        let board = board();
        board.started("agent_observe", &Value::Null);
        let _first = Counted::new(&board.queued);
        let _second = Counted::new(&board.queued);
        let response = board.busy_response("info");
        let detail = response.error_detail.unwrap();
        assert_eq!(detail.code, "daemon_busy");
        assert!(detail.message.contains("Session `work` is busy"));
        assert!(detail.message.contains("`view` has been running for"));
        assert!(detail.message.contains("1 more command is queued"));
        assert!(detail.message.contains("`info` was not started"));
        assert!(detail
            .hint
            .unwrap()
            .contains("`eoka --session work status`"));
        assert_eq!(
            detail.context.unwrap().action_status,
            Some(eoka_protocol::ActionStatus::NotAttempted)
        );
    }

    #[test]
    fn queued_and_waiting_requests_keep_the_daemon_alive() {
        let board = board();
        assert!(!board.has_waiters());
        {
            let _queued = Counted::new(&board.queued);
            assert!(board.has_waiters());
        }
        assert!(!board.has_waiters());
    }
}
