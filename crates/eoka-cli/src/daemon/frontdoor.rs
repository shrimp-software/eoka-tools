use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eoka_protocol::wire;
use eoka_server::downloads::DownloadManager;
use eoka_server::engine::blockers::{self, Blocker, Remedies, Surface};
use eoka_server::operations::OperationManager;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    net::{
        unix::{OwnedReadHalf, OwnedWriteHalf},
        UnixListener,
    },
    sync::{mpsc, OwnedMutexGuard},
    task::JoinSet,
};

use crate::protocol::{Request, Response};

pub(super) type Reader = BufReader<OwnedReadHalf>;

pub(super) struct Admitted {
    pub reader: Reader,
    pub writer: Replier,
    pub request: Request,
    pub arguments: Value,
    pub turn: OwnedMutexGuard<()>,
}

pub(super) struct Replier {
    writer: OwnedWriteHalf,
    id: Value,
}

impl Replier {
    pub async fn send(&mut self, response: &Response) -> std::io::Result<()> {
        wire::write_frame(&mut self.writer, &wire::reply_frame(&self.id, response)).await
    }
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

    async fn blocker(&self) -> Option<Blocker> {
        let page = self.page()?;
        blockers::confirmed(&page, Duration::ZERO).await
    }

    fn busy(&self) -> Option<(String, Duration)> {
        locked(&self.running)
            .as_ref()
            .map(|running| (running.command.clone(), running.since.elapsed()))
    }

    async fn activity(&self) -> Value {
        let blocker = self.blocker().await;
        json!({
            "busy": self.busy().map(|(command, elapsed)| json!({
                "cmd": command,
                "for_ms": elapsed.as_millis() as u64,
            })),
            "queued": self.queued.load(Ordering::SeqCst),
            "blocker": blocker.map(|blocker| {
                let mut summary = blocker.summary();
                summary["description"] = json!(blocker.describe());
                summary["hint"] = json!(blocker.remedy(&self.remedies()));
                summary
            }),
        })
    }

    async fn hello(&self) -> Value {
        let mut hello = self.hello.clone();
        if hello.is_object() {
            hello["activity"] = self.activity().await;
        }
        hello
    }

    async fn busy_response(&self, command: &str) -> Response {
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
        let hint = match self.blocker().await {
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
    downloads: DownloadManager,
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
            downloads.clone(),
            accepted.clone(),
        ));
    }
}

fn mismatch() -> Response {
    Response::err_detail(eoka_protocol::ErrorDetail::new(
        "protocol_mismatch",
        "Client and daemon protocols differ; upgrade explicitly. No browser was restarted and no command was dispatched.",
    ))
}

type Incoming = Result<wire::Incoming, (Value, eoka_protocol::sidecar::ServerError)>;

async fn next_frame(reader: &mut Reader) -> Option<Incoming> {
    next_frame_from(reader).await
}

async fn next_frame_from<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Option<Incoming> {
    let frame = tokio::time::timeout(Duration::from_secs(5), async {
        if reader
            .fill_buf()
            .await?
            .first()
            .is_some_and(|byte| *byte != b'{')
        {
            return Err(std::io::Error::other("not a session frame"));
        }
        wire::read_frame(reader).await
    })
    .await;
    match frame {
        Ok(Ok(Some(frame))) => Some(wire::incoming(frame)),
        _ => None,
    }
}

async fn connection_task(
    stream: tokio::net::UnixStream,
    board: Arc<Board>,
    operations: OperationManager,
    downloads: DownloadManager,
    accepted: mpsc::Sender<Admitted>,
) {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = Replier {
        writer,
        id: Value::Null,
    };
    let hello = match next_frame(&mut reader).await {
        Some(Ok(hello)) => hello,
        Some(Err((id, error))) => {
            let _ = wire::write_frame(&mut writer.writer, &wire::error_frame(&id, &error)).await;
            return;
        }
        None => return,
    };
    writer.id = hello.id.clone();
    if hello.method != wire::HELLO || !hello.protocol_matches() {
        reply(&mut writer, mismatch()).await;
        return;
    }
    reply(&mut writer, Response::ok(board.hello().await)).await;
    let incoming = match next_frame(&mut reader).await {
        Some(Ok(incoming)) => incoming,
        Some(Err((id, error))) => {
            let _ = wire::write_frame(&mut writer.writer, &wire::error_frame(&id, &error)).await;
            return;
        }
        None => return,
    };
    writer.id = incoming.id.clone();
    let request = match incoming.request() {
        Ok(request) => request,
        Err(error) => {
            let _ = wire::write_frame(&mut writer.writer, &wire::error_frame(&incoming.id, &error))
                .await;
            return;
        }
    };
    if matches!(request, Request::SessionHello) {
        reply(&mut writer, Response::ok(board.hello().await)).await;
        return;
    }
    let arguments = incoming.params;
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
    if DownloadManager::is_query(&request) {
        tokio::select! {
            response = downloads.query(&request) => {
                if let Some(response) = response {
                    reply(&mut writer, response).await;
                }
            }
            _ = hung_up(&mut reader) => {}
        }
        return;
    }
    let turn = match board.turn.clone().try_lock_owned() {
        Ok(turn) => turn,
        Err(_) => {
            if let Some(response) = handle_dialog(&board, &operations, &request, &arguments).await {
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
                    reply(&mut writer, board.busy_response(&name).await).await;
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
            arguments,
            turn,
        })
        .await;
}

async fn handle_dialog(
    board: &Board,
    operations: &OperationManager,
    request: &Request,
    arguments: &Value,
) -> Option<Response> {
    let accept = match request.cmd() {
        "dialog_accept" => true,
        "dialog_dismiss" => false,
        _ => return None,
    };
    let page = board.page()?;
    let prompt_text = arguments["prompt_text"].as_str();
    let handled = tokio::time::timeout(
        Duration::from_secs(5),
        blockers::answer(operations, &page, accept.then_some(prompt_text)),
    )
    .await;
    Some(match handled {
        Ok(Ok(reply)) => Response::ok_text(reply),
        Ok(Err(error)) => Response::err(error.to_string()),
        Err(_) => Response::err("Timeout: the browser did not acknowledge the dialog command"),
    })
}

async fn hung_up(reader: &mut Reader) {
    let mut probe = [0u8; 1];
    let _ = reader.read(&mut probe).await;
}

async fn reply(writer: &mut Replier, response: Response) {
    match tokio::time::timeout(Duration::from_secs(5), writer.send(&response)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("[eoka] control response disconnected: {error}"),
        Err(_) => eprintln!("[eoka] control response timed out"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Arc<Board> {
        Board::new(
            "work",
            json!({"protocol": eoka_protocol::sidecar::PROTOCOL_VERSION,"mode":"headless"}),
        )
    }

    #[tokio::test]
    async fn hello_reports_what_the_session_is_doing() {
        let board = board();
        assert!(board.hello().await["activity"]["busy"].is_null());
        board.started("action_command", &json!({"command":"reload"}));
        let hello = board.hello().await;
        assert_eq!(
            hello["protocol"]["major"],
            eoka_protocol::sidecar::PROTOCOL_VERSION.major
        );
        assert_eq!(hello["activity"]["busy"]["cmd"], "reload");
        assert_eq!(hello["activity"]["queued"], 0);
        board.finished(None);
        assert!(board.hello().await["activity"]["busy"].is_null());
    }

    #[tokio::test]
    async fn a_busy_refusal_names_the_running_command_and_the_next_step() {
        let board = board();
        board.started("agent_observe", &Value::Null);
        let _first = Counted::new(&board.queued);
        let _second = Counted::new(&board.queued);
        let response = board.busy_response("info").await;
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

    #[tokio::test]
    async fn a_client_from_another_version_is_dropped_at_once() {
        let (client, server) = tokio::io::duplex(1024);
        let (_client_read, mut client_write) = tokio::io::split(client);
        let length_prefixed = [&22u32.to_be_bytes()[..], br#"{"cmd":"session_hello"}"#].concat();
        tokio::io::AsyncWriteExt::write_all(&mut client_write, &length_prefixed)
            .await
            .unwrap();
        let (server_read, _server_write) = tokio::io::split(server);
        let mut reader = BufReader::new(server_read);
        let started = Instant::now();
        assert!(next_frame_from(&mut reader).await.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
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
