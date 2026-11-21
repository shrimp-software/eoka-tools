use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use eoka::cdp::transport::CdpMessage;
use eoka::cdp::Transport;
use eoka::Page;
use eoka_protocol::{ActionStatus, ErrorContext, ErrorDetail, Response};
use serde_json::{json, Value};
use tokio::sync::{broadcast, watch};

pub const NAVIGATION_GRACE: Duration = Duration::from_secs(5);
pub const READ_LIMIT: Duration = Duration::from_secs(15);
const PROBE_LIMIT: Duration = Duration::from_millis(400);
const PROBE_REUSE: Duration = Duration::from_secs(1);
const CONTROL_LIMIT: Duration = Duration::from_secs(2);
const SETTLE_LIMIT: Duration = Duration::from_secs(1);
const RECENTLY_STOPPED: Duration = Duration::from_secs(5);
const TRACKED_FRAMES: usize = 256;

#[derive(Clone, Debug)]
pub struct Dialog {
    pub kind: String,
    pub message: String,
    pub url: String,
    pub since: Instant,
}

#[derive(Clone, Debug)]
pub struct Navigation {
    pub url: Option<String>,
    pub since: Instant,
}

#[derive(Clone, Debug)]
pub enum Blocker {
    Dialog(Dialog),
    Navigation(Navigation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Free,
    Gate,
    Observe,
    Capture,
    Read,
    Script,
    Wait,
    Navigate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    NotStarted,
    ReadInterrupted,
    ActionInterrupted,
}

impl Policy {
    pub fn interrupted(self) -> Stage {
        match self {
            Policy::Read | Policy::Wait | Policy::Capture | Policy::Observe => {
                Stage::ReadInterrupted
            }
            _ => Stage::ActionInterrupted,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Catalog,
    Cli,
    Mcp,
    Sidecar,
}

static SURFACE: OnceLock<Surface> = OnceLock::new();

pub fn set_surface(surface: Surface) {
    let _ = SURFACE.set(surface);
}

fn surface() -> Surface {
    SURFACE.get().copied().unwrap_or(Surface::Catalog)
}

#[derive(Clone, Copy)]
pub struct Remedies<'a> {
    surface: Surface,
    session: Option<&'a str>,
}

impl<'a> Remedies<'a> {
    pub fn new(session: Option<&'a str>) -> Self {
        Self {
            surface: surface(),
            session,
        }
    }

    pub fn for_surface(surface: Surface, session: Option<&'a str>) -> Self {
        Self { surface, session }
    }

    fn command(&self, cli: &str, mcp: &str, catalog: &str) -> String {
        match self.surface {
            Surface::Cli => match self.session.filter(|name| *name != "default") {
                Some(name) => format!("`eoka --session {name} {cli}`"),
                None => format!("`eoka {cli}`"),
            },
            Surface::Mcp => format!("`{mcp}`"),
            Surface::Catalog | Surface::Sidecar => format!("`{catalog}`"),
        }
    }

    pub fn name(&self, command: &str, arguments: &Value) -> String {
        let command = match command {
            "action_command" => arguments["command"].as_str().unwrap_or(command),
            command => command,
        };
        let operation = eoka_protocol::operation_by_cmd(command);
        match self.surface {
            Surface::Cli => operation.map_or_else(
                || command.replace('.', " ").replace('_', "-"),
                |operation| operation.cli_command(),
            ),
            Surface::Catalog | Surface::Sidecar => operation
                .map_or(command, |operation| operation.path)
                .to_owned(),
            Surface::Mcp => command.to_owned(),
        }
    }

    pub fn accept(&self) -> String {
        self.command("dialog accept", "accept_dialog", "dialog.accept")
    }

    pub fn dismiss(&self) -> String {
        self.command("dialog dismiss", "dismiss_dialog", "dialog.dismiss")
    }

    pub fn view(&self) -> String {
        self.command("view", "agent_observe", "agent.observe")
    }

    pub fn reload(&self) -> String {
        self.command("reload", "reload", "reload")
    }

    pub fn open(&self) -> String {
        self.command("open URL", "navigate", "open")
    }

    pub fn wait(&self) -> String {
        self.command(
            "wait --url TEXT --timeout 30000",
            "wait",
            "wait with url and timeout",
        )
    }

    pub fn new_tab(&self) -> String {
        if self.surface == Surface::Sidecar {
            return "`browser.new_page`".into();
        }
        self.command("tab new URL", "new_tab", "tab.new")
    }

    pub fn status(&self) -> String {
        if self.surface == Surface::Sidecar {
            return "`browser.tabs`".into();
        }
        self.command("status", "list_tabs", "tab.list")
    }

    pub fn operation(&self, id: &str) -> String {
        self.command(
            &format!("operations get {id}"),
            &format!("operation_get {id}"),
            &format!("operations.get {id}"),
        )
    }
}

struct Ended {
    frame: String,
    navigation: Navigation,
    committed: bool,
    at: Instant,
}

#[derive(Default)]
struct SessionState {
    dialog: Option<Dialog>,
    navigations: HashMap<String, Navigation>,
    ended: Option<Ended>,
    probed: Option<Instant>,
}

#[derive(Default)]
struct Sessions(HashMap<String, SessionState>);

impl Sessions {
    fn apply(&mut self, session: Option<&str>, method: &str, params: &Value, now: Instant) -> bool {
        if method == "Target.detachedFromTarget" {
            return params["sessionId"]
                .as_str()
                .is_some_and(|detached| self.0.remove(detached).is_some());
        }
        let Some(session) = session else {
            return false;
        };
        let changed = self.record(session, method, params, now);
        if changed {
            if let Some(state) = self.0.get_mut(session) {
                state.probed = None;
            }
        }
        changed
    }

    fn record(&mut self, session: &str, method: &str, params: &Value, now: Instant) -> bool {
        match method {
            "Page.javascriptDialogOpening" => {
                self.0.entry(session.to_owned()).or_default().dialog = Some(Dialog {
                    kind: params["type"].as_str().unwrap_or("alert").to_owned(),
                    message: params["message"].as_str().unwrap_or_default().to_owned(),
                    url: params["url"].as_str().unwrap_or_default().to_owned(),
                    since: now,
                });
                true
            }
            "Page.javascriptDialogClosed" => self
                .0
                .get_mut(session)
                .is_some_and(|state| state.dialog.take().is_some()),
            "Page.frameStartedLoading" | "Page.frameStartedNavigating" => {
                let Some(frame) = params["frameId"].as_str() else {
                    return false;
                };
                if params["navigationType"]
                    .as_str()
                    .is_some_and(|kind| kind.to_ascii_lowercase().contains("samedocument"))
                {
                    return false;
                }
                let state = self.0.entry(session.to_owned()).or_default();
                if state.navigations.len() >= TRACKED_FRAMES
                    && !state.navigations.contains_key(frame)
                {
                    return false;
                }
                let navigation = state
                    .navigations
                    .entry(frame.to_owned())
                    .or_insert(Navigation {
                        url: None,
                        since: now,
                    });
                if let Some(url) = params["url"].as_str() {
                    navigation.url = Some(url.to_owned());
                }
                true
            }
            "Page.frameNavigated" => {
                self.finish(session, params["frame"]["id"].as_str(), true, now)
            }
            "Page.frameStoppedLoading" | "Page.frameDetached" => {
                self.finish(session, params["frameId"].as_str(), false, now)
            }
            _ => false,
        }
    }

    fn finish(
        &mut self,
        session: &str,
        frame: Option<&str>,
        committed: bool,
        now: Instant,
    ) -> bool {
        let (Some(frame), Some(state)) = (frame, self.0.get_mut(session)) else {
            return false;
        };
        let Some(navigation) = state.navigations.remove(frame) else {
            return false;
        };
        state.ended = Some(Ended {
            frame: frame.to_owned(),
            navigation,
            committed,
            at: now,
        });
        true
    }

    fn stopped(&self, session: &str, target: &str, within: Duration) -> Option<Navigation> {
        self.0
            .get(session)?
            .ended
            .as_ref()
            .filter(|ended| {
                ended.frame == target && !ended.committed && ended.at.elapsed() <= within
            })
            .map(|ended| ended.navigation.clone())
    }

    fn current(&self, session: &str, target: &str) -> Option<Blocker> {
        let state = self.0.get(session)?;
        if let Some(dialog) = &state.dialog {
            return Some(Blocker::Dialog(dialog.clone()));
        }
        state
            .navigations
            .get(target)
            .cloned()
            .map(Blocker::Navigation)
    }

    fn clear(&mut self, session: &str, target: &str) {
        if let Some(state) = self.0.get_mut(session) {
            state.dialog = None;
            state.navigations.remove(target);
        }
    }
}

struct Watcher {
    transport: Weak<Transport>,
    sessions: Mutex<Sessions>,
    changed: watch::Sender<u64>,
}

impl Watcher {
    fn sessions(&self) -> std::sync::MutexGuard<'_, Sessions> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn bump(&self) {
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }
}

fn registry() -> &'static Mutex<HashMap<usize, Arc<Watcher>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, Arc<Watcher>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

fn watcher(page: &Page) -> Arc<Watcher> {
    let transport = page.session().transport();
    let key = Arc::as_ptr(transport) as usize;
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = registry.get(&key) {
        if existing
            .transport
            .upgrade()
            .is_some_and(|live| Arc::ptr_eq(&live, transport))
        {
            return existing.clone();
        }
    }
    let watcher = Arc::new(Watcher {
        transport: Arc::downgrade(transport),
        sessions: Default::default(),
        changed: watch::channel(0).0,
    });
    registry.insert(key, watcher.clone());
    tokio::spawn(follow(key, watcher.clone(), transport.subscribe()));
    watcher
}

async fn follow(key: usize, watcher: Arc<Watcher>, mut events: broadcast::Receiver<CdpMessage>) {
    loop {
        match events.recv().await {
            Ok(CdpMessage::Event {
                method,
                params,
                session_id,
            }) => {
                if !(method.starts_with("Page.") || method == "Target.detachedFromTarget") {
                    continue;
                }
                let changed = watcher.sessions().apply(
                    session_id.as_deref(),
                    &method,
                    &params,
                    Instant::now(),
                );
                if changed {
                    watcher.bump();
                }
            }
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if registry
        .get(&key)
        .is_some_and(|current| Arc::ptr_eq(current, &watcher))
    {
        registry.remove(&key);
    }
}

pub fn watch(page: &Page) {
    watcher(page);
}

pub fn current(page: &Page) -> Option<Blocker> {
    watcher(page)
        .sessions()
        .current(page.session().session_id(), page.target_id())
}

pub fn stopped_recently(page: &Page, within: Duration) -> Option<Navigation> {
    watcher(page)
        .sessions()
        .stopped(page.session().session_id(), page.target_id(), within)
}

fn clear(page: &Page) {
    let watcher = watcher(page);
    watcher
        .sessions()
        .clear(page.session().session_id(), page.target_id());
    watcher.bump();
}

async fn responsive(page: &Page) -> bool {
    tokio::time::timeout(
        PROBE_LIMIT,
        page.session().send::<_, Value>(
            "Runtime.evaluate",
            &json!({"expression":"1","returnByValue":true}),
        ),
    )
    .await
    .is_ok_and(|answer| answer.is_ok())
}

pub async fn confirmed(page: &Page, grace: Duration) -> Option<Blocker> {
    let blocker = match current(page)? {
        Blocker::Navigation(navigation) if navigation.since.elapsed() < grace => return None,
        blocker => blocker,
    };
    let session = page.session().session_id();
    let watcher = watcher(page);
    let fresh = watcher
        .sessions()
        .0
        .get(session)
        .and_then(|state| state.probed)
        .is_some_and(|probed| probed.elapsed() < PROBE_REUSE);
    if fresh {
        return Some(blocker);
    }
    if responsive(page).await {
        clear(page);
        return None;
    }
    if let Some(state) = watcher.sessions().0.get_mut(session) {
        state.probed = Some(Instant::now());
    }
    Some(blocker)
}

pub async fn interrupted(page: &Page, navigation: bool) -> Blocker {
    let watcher = watcher(page);
    let mut changes = watcher.changed.subscribe();
    loop {
        let pause = match current(page) {
            Some(Blocker::Dialog(_)) => Some(Duration::ZERO),
            Some(Blocker::Navigation(pending)) if navigation => {
                Some(NAVIGATION_GRACE.saturating_sub(pending.since.elapsed()))
            }
            _ => None,
        };
        let due = async {
            match pause {
                Some(pause) => tokio::time::sleep(pause).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = due => {
                match confirmed(page, NAVIGATION_GRACE).await {
                    Some(Blocker::Navigation(_)) if !navigation => {}
                    Some(blocker) => return blocker,
                    None => {}
                }
            }
            changed = changes.changed() => {
                if changed.is_err() {
                    std::future::pending::<()>().await;
                }
            }
        }
    }
}

pub enum Stop {
    Blocked(Blocker),
    Unresponsive(Duration),
    Frozen,
}

impl Stop {
    pub fn blocker(&self) -> Option<&Blocker> {
        match self {
            Stop::Blocked(blocker) => Some(blocker),
            Stop::Unresponsive(_) | Stop::Frozen => None,
        }
    }

    pub fn detail(&self, remedies: &Remedies<'_>, command: &str, stage: Stage) -> ErrorDetail {
        match self {
            Stop::Blocked(blocker) => blocker.detail(remedies, command, stage),
            Stop::Unresponsive(limit) => stalled(
                remedies,
                format!(
                    "`{command}` got no answer from the page within {} s.",
                    limit.as_secs()
                ),
            ),
            Stop::Frozen => stalled(
                remedies,
                format!("`{command}` timed out and the page does not answer a liveness probe."),
            ),
        }
    }
}

pub async fn admit(
    operations: &crate::operations::OperationManager,
    page: &Page,
    policy: Policy,
) -> Option<Blocker> {
    watch(page);
    if policy == Policy::Free {
        return None;
    }
    let grace = if policy == Policy::Navigate {
        Duration::ZERO
    } else {
        NAVIGATION_GRACE
    };
    match confirmed(page, grace).await? {
        Blocker::Navigation(_) if policy == Policy::Navigate => {
            supersede_navigation(operations, page).await;
            None
        }
        Blocker::Navigation(_) if matches!(policy, Policy::Wait | Policy::Capture) => None,
        blocker => Some(blocker),
    }
}

pub async fn race<T>(
    page: &Page,
    policy: Policy,
    limit: Option<Duration>,
    interruptible: bool,
    work: impl Future<Output = T>,
) -> Result<T, Stop> {
    let deadline = async {
        match limit {
            Some(limit) => tokio::time::sleep(limit).await,
            None => std::future::pending().await,
        }
    };
    let interruption = async {
        match interruptible {
            true => interrupted(page, policy != Policy::Wait).await,
            false => std::future::pending().await,
        }
    };
    tokio::select! {
        biased;
        finished = work => Ok(finished),
        blocker = interruption => Err(Stop::Blocked(blocker)),
        _ = deadline => Err(match current(page) {
            Some(blocker) => Stop::Blocked(blocker),
            None => Stop::Unresponsive(limit.unwrap_or_default()),
        }),
    }
}

pub async fn after_timeout(page: &Page, probes: bool) -> Option<Stop> {
    match confirmed(page, Duration::ZERO).await {
        Some(blocker) => Some(Stop::Blocked(blocker)),
        None if probes && !responsive(page).await => Some(Stop::Frozen),
        None => None,
    }
}

tokio::task_local! {
    static ABANDON: watch::Receiver<bool>;
}

pub async fn abandonable<F: Future>(caller_left: watch::Receiver<bool>, work: F) -> F::Output {
    ABANDON.scope(caller_left, work).await
}

pub async fn abandoned() {
    let Ok(mut caller_left) = ABANDON.try_with(Clone::clone) else {
        return std::future::pending().await;
    };
    loop {
        if *caller_left.borrow() {
            return;
        }
        if caller_left.changed().await.is_err() {
            return std::future::pending().await;
        }
    }
}

fn abandoned_response(command: &str) -> Response {
    Response::err_detail(
        ErrorDetail::new(
            "caller_left",
            format!("The caller disconnected before `{command}` answered, so the session stopped waiting for it."),
        )
        .retryable(false),
    )
}

pub async fn stop_loading(page: &Page) -> bool {
    let stopped = tokio::time::timeout(
        CONTROL_LIMIT,
        page.session()
            .send::<_, Value>("Page.stopLoading", &json!({})),
    )
    .await
    .is_ok_and(|answer| answer.is_ok());
    if stopped {
        let watcher = watcher(page);
        if let Some(state) = watcher.sessions().0.get_mut(page.session().session_id()) {
            state.navigations.remove(page.target_id());
        }
        watcher.bump();
    }
    stopped
}

pub async fn abandon_navigation(page: &Page, grace: Duration) -> Option<Navigation> {
    let Some(Blocker::Navigation(navigation)) = confirmed(page, grace).await else {
        return None;
    };
    stop_loading(page).await.then_some(navigation)
}

pub fn release_when_finished(
    page: Page,
    operations: crate::operations::OperationManager,
    id: String,
    navigation_action: bool,
) {
    tokio::spawn(async move {
        let finished = operations
            .wait(&id, 330_000)
            .await
            .is_ok_and(|record| !matches!(record.state, eoka_protocol::OperationState::Running));
        if finished {
            let grace = if navigation_action {
                Duration::ZERO
            } else {
                NAVIGATION_GRACE
            };
            abandon_navigation(&page, grace).await;
        }
    });
}

pub async fn wait_unless_dialog(
    operations: &crate::operations::OperationManager,
    page: &Page,
    id: &str,
    budget: u64,
) -> anyhow::Result<eoka_protocol::OperationRecord> {
    tokio::select! {
        biased;
        record = operations.wait(id, budget) => record,
        _ = interrupted(page, false) => operations.get(id).await,
        _ = abandoned() => operations.get(id).await,
    }
}

fn navigates(action: &str) -> bool {
    matches!(
        action,
        "navigate" | "navigate_with_headers" | "reload" | "back" | "forward" | "history"
    )
}

async fn running_on(
    operations: &crate::operations::OperationManager,
    page: &Page,
) -> Vec<eoka_protocol::OperationRecord> {
    operations
        .list(eoka_protocol::OperationListArgs {
            limit: Some(100),
            before_ms: None,
        })
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|record| {
            matches!(record.state, eoka_protocol::OperationState::Running)
                && record.target_id == page.target_id()
        })
        .collect()
}

pub async fn supersede_navigation(operations: &crate::operations::OperationManager, page: &Page) {
    let mut canceled = Vec::new();
    for record in running_on(operations, page).await {
        if navigates(&record.action) && operations.cancel(&record.id).await.is_ok() {
            canceled.push(record.id);
        }
    }
    stop_loading(page).await;
    for id in canceled {
        settle(operations, &id).await;
    }
}

pub async fn answer(
    operations: &crate::operations::OperationManager,
    page: &Page,
    accept: Option<Option<&str>>,
) -> eoka::Result<String> {
    match accept {
        Some(prompt_text) => {
            page.accept_dialog(prompt_text).await?;
            Ok("Dialog accepted".into())
        }
        None => dismiss(operations, page).await,
    }
}

pub async fn dismiss(
    operations: &crate::operations::OperationManager,
    page: &Page,
) -> eoka::Result<String> {
    let leaving =
        matches!(current(page), Some(Blocker::Dialog(dialog)) if dialog.kind == "beforeunload");
    page.dismiss_dialog().await?;
    if !leaving {
        return Ok("Dialog dismissed".into());
    }
    let mut stopped = Vec::new();
    for record in running_on(operations, page).await {
        if operations.cancel(&record.id).await.is_ok() {
            stopped.push(record);
        }
    }
    for record in &stopped {
        settle(operations, &record.id).await;
    }
    Ok(match stopped.as_slice() {
        [] => "Dialog dismissed".into(),
        records => format!(
            "Dialog dismissed. The page stays and the navigation is cancelled, so {} stopped: {}.",
            if records.len() == 1 {
                "the operation that started it was"
            } else {
                "the operations waiting on it were"
            },
            records
                .iter()
                .map(|record| format!("{} {}", record.action, record.id))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

pub async fn finish_navigation(
    operations: &crate::operations::OperationManager,
    page: &Page,
    navigation_action: bool,
    timed_out: bool,
) -> Option<Navigation> {
    let mut running = running_on(operations, page).await;
    if navigation_action && !running.is_empty() {
        for record in running.iter().filter(|record| navigates(&record.action)) {
            settle(operations, &record.id).await;
        }
        running = running_on(operations, page).await;
    }
    if !matches!(current(page), Some(Blocker::Navigation(_))) {
        return (navigation_action && timed_out && running.is_empty())
            .then(|| stopped_recently(page, RECENTLY_STOPPED))
            .flatten();
    }
    if !running.is_empty() {
        for record in running {
            let navigation = navigation_action || navigates(&record.action);
            release_when_finished(page.clone(), operations.clone(), record.id, navigation);
        }
        return None;
    }
    let grace = if navigation_action {
        Duration::ZERO
    } else {
        NAVIGATION_GRACE
    };
    abandon_navigation(page, grace).await
}

fn operation_id(response: &Response) -> Option<String> {
    let data = response.data.as_ref()?;
    data["operation_id"]
        .as_str()
        .or_else(|| data["id"].as_str())
        .map(str::to_owned)
}

fn timed_out(response: &Response) -> bool {
    let code = response
        .error_detail
        .as_ref()
        .map_or("", |detail| detail.code.as_str());
    let text = response
        .error
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let observation = response
        .data
        .as_ref()
        .and_then(|data| data["observation_error"].as_str())
        .unwrap_or_default();
    matches!(
        code,
        "operation_pending" | "action_timeout" | "timeout" | "dialog_open" | "navigation_pending"
    ) || observation.contains("timeout")
        || observation.contains("dialog_open")
        || text.contains("dialog_open")
        || text.contains("navigation_pending")
        || text.contains("timed out")
        || text.contains("timeout")
        || text.contains("deadline")
}

fn replace_error(response: &mut Response, detail: ErrorDetail) {
    if let Some(data) = response.data.as_mut().filter(|data| data.is_object()) {
        if data.get("observation").is_some() || data.get("observation_error").is_some() {
            data["operation_error"] = json!(detail);
            if data["observation"].is_null() {
                data["observation_error"] = json!(format!("{}: {}", detail.code, detail.message));
            }
        }
    }
    response.ok = false;
    response.error = Some(detail.message.clone());
    response.error_detail = Some(detail);
}

fn with_operation(
    mut detail: ErrorDetail,
    remedies: &Remedies<'_>,
    id: Option<&str>,
) -> ErrorDetail {
    if let Some(id) = id {
        detail
            .context
            .get_or_insert_with(Default::default)
            .operation_id
            .get_or_insert_with(|| id.to_owned());
        if let Some(hint) = detail.hint.as_mut() {
            hint.push_str(&format!(
                " The operation keeps its ID: inspect it with {}.",
                remedies.operation(id)
            ));
        }
    }
    detail
}

impl super::Engine {
    pub(super) fn named(
        &self,
        remedies: &Remedies<'_>,
        command: &str,
        arguments: &Value,
    ) -> String {
        match &self.spoken {
            Some(spoken) => spoken.clone(),
            None => remedies.name(command, arguments),
        }
    }

    pub fn current_page(&self) -> Option<Page> {
        self.state
            .as_ref()
            .and_then(|state| state.current_tab())
            .map(|tab| tab.page.clone())
    }

    pub(super) async fn refuse_blocked(
        &mut self,
        command: &str,
        arguments: &Value,
        policy: Policy,
    ) -> Option<Response> {
        let page = self.current_page()?;
        if command == "open" && self.state.as_ref().is_some_and(|state| state.is_live) {
            watch(&page);
            return None;
        }
        let blocker = admit(&self.operations, &page, policy).await?;
        let remedies = Remedies::new(Some(&self.session_name));
        let mut response = Response::err_detail(blocker.detail(
            &remedies,
            &self.named(&remedies, command, arguments),
            Stage::NotStarted,
        ));
        if matches!(policy, Policy::Gate | Policy::Navigate) {
            response.data = Some(json!({
                "status": eoka_protocol::OperationStatus::NotDispatched,
                "operation_id": arguments["operation_id"],
            }));
        }
        Some(response)
    }

    pub(super) async fn dispatch_guarded(
        &mut self,
        request: eoka_protocol::Request,
        arguments: &Value,
    ) -> Result<Response, super::EngineError> {
        let command = request.cmd();
        let policy = policy(command, arguments);
        let sleeps = matches!(command, "wait" | "network_wait") && policy == Policy::Free;
        if !sleeps && !matches!(policy, Policy::Read | Policy::Wait | Policy::Capture) {
            return Box::pin(self.dispatch(request)).await;
        }
        let page = self.current_page();
        let session = self.session_name.clone();
        let remedies = Remedies::new(Some(&session));
        let name = self.named(&remedies, command, arguments);
        guard(
            page.as_ref(),
            remedies,
            &name,
            policy,
            limit(policy, arguments),
            Box::pin(self.dispatch(request)),
        )
        .await
    }

    pub(super) async fn diagnose_blocked(
        &mut self,
        command: &str,
        arguments: &Value,
        policy: Policy,
        response: &mut Response,
    ) {
        let session = self.session_name.clone();
        let remedies = Remedies::new(Some(&session));
        explain(response, &remedies);
        if response.ok || policy == Policy::Free {
            return;
        }
        let Some(page) = self.current_page() else {
            return;
        };
        watch(&page);
        let id = operation_id(response);
        let navigation_action = policy == Policy::Navigate;
        let pending = || matches!(current(&page), Some(Blocker::Navigation(_)));
        let dialog = matches!(current(&page), Some(Blocker::Dialog(_)));
        if let Some(id) = id
            .as_deref()
            .filter(|_| !dialog && (navigation_action || pending()))
        {
            settle(&self.operations, id).await;
            let record = self.operations.get(id).await.ok();
            let running = record.as_ref().is_some_and(|record| {
                matches!(record.state, eoka_protocol::OperationState::Running)
            });
            if running && pending() {
                release_when_finished(
                    page.clone(),
                    self.operations.clone(),
                    id.to_owned(),
                    navigation_action,
                );
            }
            let grace = if navigation_action {
                Duration::ZERO
            } else {
                NAVIGATION_GRACE
            };
            let stopped = match running {
                true => None,
                false => match abandon_navigation(&page, grace).await {
                    Some(navigation) => Some(navigation),
                    None if navigation_action && timed_out(response) => {
                        stopped_recently(&page, RECENTLY_STOPPED)
                    }
                    None => None,
                },
            };
            if let Some(navigation) = stopped {
                if let Some(record) = record {
                    let status = record.status();
                    if let Some(data) = response.data.as_mut().filter(|data| data.is_object()) {
                        if data.get("observation").is_some()
                            || data.get("observation_error").is_some()
                        {
                            data["operation"] = json!(record);
                        } else if let Ok(Value::Object(fields)) = serde_json::to_value(&record) {
                            for (key, value) in fields {
                                data[key] = value;
                            }
                        }
                        data["status"] = json!(status);
                    }
                }
                replace_error(
                    response,
                    navigation_timeout(&remedies, &navigation, Some(id)),
                );
                return;
            }
        }
        if !timed_out(response) {
            return;
        }
        let Some(stop) = after_timeout(&page, true).await else {
            return;
        };
        let name = self.named(&remedies, command, arguments);
        let detail = stop.detail(&remedies, &name, policy.interrupted());
        replace_error(response, with_operation(detail, &remedies, id.as_deref()));
    }
}

pub async fn settle(operations: &crate::operations::OperationManager, id: &str) {
    let _ = operations.wait(id, SETTLE_LIMIT.as_millis() as u64).await;
}

fn seconds(since: Instant) -> String {
    let elapsed = since.elapsed();
    if elapsed < Duration::from_secs(10) {
        format!("{:.1}", elapsed.as_secs_f64())
    } else {
        elapsed.as_secs().to_string()
    }
}

fn quoted(text: &str) -> String {
    let mut short: String = text.chars().take(200).collect();
    if short.len() < text.len() {
        short.push('…');
    }
    format!("{short:?}")
}

impl Blocker {
    pub fn code(&self) -> &'static str {
        match self {
            Blocker::Dialog(_) => "dialog_open",
            Blocker::Navigation(_) => "navigation_pending",
        }
    }

    pub fn summary(&self) -> Value {
        match self {
            Blocker::Dialog(dialog) => json!({
                "kind": "dialog",
                "dialog_type": dialog.kind,
                "message": dialog.message.chars().take(200).collect::<String>(),
                "url": dialog.url,
                "for_ms": dialog.since.elapsed().as_millis() as u64,
            }),
            Blocker::Navigation(navigation) => json!({
                "kind": "navigation",
                "url": navigation.url,
                "for_ms": navigation.since.elapsed().as_millis() as u64,
            }),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Blocker::Dialog(dialog) if dialog.kind == "beforeunload" => {
                "A \"leave this page?\" (beforeunload) dialog is open on this tab".into()
            }
            Blocker::Dialog(dialog) => format!(
                "A JavaScript {} dialog is open on this tab ({})",
                dialog.kind,
                quoted(&dialog.message)
            ),
            Blocker::Navigation(navigation) => format!(
                "This tab has been waiting {} s for a response from {}",
                seconds(navigation.since),
                navigation.url.as_deref().unwrap_or("a navigation")
            ),
        }
    }

    pub fn remedy(&self, remedies: &Remedies<'_>) -> String {
        match self {
            Blocker::Dialog(dialog) if dialog.kind == "beforeunload" => format!(
                "Run {} to leave the page or {} to stay, then {}.",
                remedies.accept(),
                remedies.dismiss(),
                remedies.view()
            ),
            Blocker::Dialog(_) => format!(
                "Run {} or {}, then {}.",
                remedies.accept(),
                remedies.dismiss(),
                remedies.view()
            ),
            Blocker::Navigation(_) => format!(
                "Keep waiting with {}, or abandon the load: {} returns to the page that is still displayed and {} goes elsewhere.",
                remedies.wait(),
                remedies.reload(),
                remedies.open()
            ),
        }
    }

    pub fn detail(&self, remedies: &Remedies<'_>, command: &str, stage: Stage) -> ErrorDetail {
        let consequence = if stage == Stage::NotStarted {
            format!("`{command}` was not started")
        } else {
            format!("`{command}` returned without a result")
        };
        let reason = match self {
            Blocker::Dialog(_) => {
                "Chrome answers no page commands until the dialog is accepted or dismissed"
            }
            Blocker::Navigation(_) => {
                "Chrome answers no page commands until that navigation commits or is abandoned"
            }
        };
        let mut hint = self.remedy(remedies);
        if stage == Stage::ActionInterrupted {
            hint.push_str(
                " Do not repeat the interrupted command blindly: its page effects may already have happened.",
            );
        }
        let detail = ErrorDetail::new(
            self.code(),
            format!("{}. {reason}, so {consequence}.", self.describe()),
        )
        .retryable(false)
        .hint(hint);
        match stage {
            Stage::NotStarted => detail.context(ErrorContext {
                action_status: Some(ActionStatus::NotAttempted),
                ..Default::default()
            }),
            Stage::ActionInterrupted => detail.context(ErrorContext {
                action_status: Some(ActionStatus::MayHaveExecuted),
                ..Default::default()
            }),
            Stage::ReadInterrupted => detail,
        }
    }
}

fn stalled(remedies: &Remedies<'_>, observed: String) -> ErrorDetail {
    ErrorDetail::new(
        "page_unresponsive",
        format!("{observed} No dialog or pending navigation is known, so the page's main thread is busy or frozen."),
    )
    .retryable(false)
    .hint(format!(
        "Retry once the page settles. If it stays frozen, continue in a fresh tab with {}. {} clears a dialog that was already open before this session attached.",
        remedies.new_tab(),
        remedies.dismiss()
    ))
}

pub fn navigation_timeout(
    remedies: &Remedies<'_>,
    navigation: &Navigation,
    operation_id: Option<&str>,
) -> ErrorDetail {
    let timeout = match remedies.surface {
        Surface::Cli => "`--action-timeout-ms`",
        _ => "`timeout_ms`",
    };
    ErrorDetail::new(
        "navigation_timeout",
        format!(
            "No response from {} after {} s. The pending load was stopped, so the tab still shows the page it had before and accepts commands.",
            navigation.url.as_deref().unwrap_or("the requested URL"),
            seconds(navigation.since)
        ),
    )
    .retryable(false)
    .hint(format!(
        "Retry with a larger {timeout} if the site is slow, or check the URL, network and proxy. {} shows the current page.",
        remedies.view()
    ))
    .context(ErrorContext {
        operation_id: operation_id.map(str::to_owned),
        phase: Some("navigation".into()),
        ..Default::default()
    })
}

pub fn policy(command: &str, arguments: &Value) -> Policy {
    use eoka_protocol::OperationId as Id;
    let Some(operation) = eoka_protocol::operation_by_cmd(command) else {
        return Policy::Free;
    };
    let navigates = |kind: Option<&str>| {
        matches!(
            kind,
            Some("navigate" | "navigate_with_headers" | "reload" | "back" | "forward" | "history")
        )
    };
    match operation.id {
        Id::Open | Id::Back | Id::Forward | Id::Reload => Policy::Navigate,
        Id::ActionCommand => match arguments["command"].as_str() {
            Some("back" | "forward" | "reload") => Policy::Navigate,
            _ => Policy::Gate,
        },
        Id::ActionPerform | Id::AgentStep if navigates(arguments["action"]["kind"].as_str()) => {
            Policy::Navigate
        }
        Id::Wait if arguments["ms"].is_u64() => Policy::Free,
        Id::Wait => Policy::Wait,
        Id::Snapshot
        | Id::Observe
        | Id::Info
        | Id::Frames
        | Id::Text
        | Id::Find
        | Id::Content
        | Id::GetText
        | Id::GetAttribute
        | Id::Exists
        | Id::Storage
        | Id::DumpStorage
        | Id::Console
        | Id::Errors
        | Id::SpaInfo
        | Id::WasmInfo
        | Id::WasmRead
        | Id::WasmFind => Policy::Read,
        Id::Eval | Id::Exec | Id::FrameEval | Id::Fetch => Policy::Script,
        Id::Screenshot if arguments["annotate"].as_bool().unwrap_or(false) => Policy::Gate,
        Id::Screenshot => Policy::Capture,
        Id::AgentObserve => Policy::Observe,
        Id::AgentStep
        | Id::ActionPerform
        | Id::Click
        | Id::DblClick
        | Id::Fill
        | Id::Select
        | Id::Hover
        | Id::Key
        | Id::ClickShadow
        | Id::MouseDown
        | Id::MouseMove
        | Id::MouseUp
        | Id::KeyDown
        | Id::KeyUp
        | Id::Scroll
        | Id::SetStorage
        | Id::SaveState
        | Id::SpaNavigate
        | Id::WasmWrite
        | Id::CaptchaDetect
        | Id::CaptchaInject
        | Id::DownloadStart => Policy::Gate,
        Id::SessionHello
        | Id::OperationGet
        | Id::OperationList
        | Id::OperationWait
        | Id::OperationCancel
        | Id::Emulate
        | Id::RecordingStart
        | Id::RecordingGet
        | Id::RecordingStop
        | Id::RecordingCancel
        | Id::ReleaseAllInputs
        | Id::DownloadList
        | Id::DownloadGet
        | Id::DownloadWait
        | Id::DownloadCancel
        | Id::Cookies
        | Id::SetCookie
        | Id::DeleteCookie
        | Id::ClearCookies
        | Id::LoadState
        | Id::Headers
        | Id::TabList
        | Id::TabNew
        | Id::TabSwitch
        | Id::TabClose
        | Id::TabAttach
        | Id::CloneFrom
        | Id::FakeCamera
        | Id::InterceptAdd
        | Id::InterceptList
        | Id::InterceptRemove
        | Id::InterceptLog
        | Id::JsMode
        | Id::JsAllow
        | Id::JsBlock
        | Id::JsRemove
        | Id::JsList
        | Id::NetworkRecordStart
        | Id::NetworkRecordStop
        | Id::NetworkRecordStatus
        | Id::NetworkLog
        | Id::NetworkShow
        | Id::NetworkWait
        | Id::NetworkExport
        | Id::NetworkClear
        | Id::Close
        | Id::Shutdown
        | Id::CaptchaDatadome
        | Id::CaptchaTurnstile
        | Id::DialogAccept
        | Id::DialogDismiss => Policy::Free,
    }
}

pub fn limit(policy: Policy, arguments: &Value) -> Option<Duration> {
    match policy {
        Policy::Read => Some(READ_LIMIT),
        Policy::Wait => Some(Duration::from_millis(
            arguments["timeout"]
                .as_u64()
                .unwrap_or(10_000)
                .saturating_add(2_000),
        )),
        _ => None,
    }
}

pub async fn guard<E>(
    page: Option<&Page>,
    remedies: Remedies<'_>,
    command: &str,
    policy: Policy,
    limit: Option<Duration>,
    work: impl Future<Output = Result<Response, E>>,
) -> Result<Response, E> {
    let Some(page) = page.filter(|_| policy != Policy::Free) else {
        return tokio::select! {
            biased;
            result = work => result,
            _ = abandoned() => Ok(abandoned_response(command)),
        };
    };
    tokio::select! {
        biased;
        raced = race(page, policy, limit, true, work) => match raced {
            Ok(result) => result,
            Err(stop) => Ok(Response::err_detail(stop.detail(
                &remedies,
                command,
                policy.interrupted(),
            ))),
        },
        _ = abandoned() => Ok(abandoned_response(command)),
    }
}

pub fn explain(response: &mut Response, remedies: &Remedies<'_>) {
    let Some(detail) = response.error_detail.as_mut() else {
        return;
    };
    if detail.hint.is_some() {
        return;
    }
    let hint = match detail.code.as_str() {
        "expectation_required" if remedies.surface == Surface::Mcp => return,
        "expectation_required" => match remedies.surface {
            Surface::Cli => "Nothing was sent to the page. State the result you expect and run it again, for example `--expect '{\"kind\":\"url\",\"value\":\"/next\",\"contains\":true}'` (or `\"expect\":{...}` inside `step`). For deliberate unverified input use `step` with `\"mode\":\"explore\"`, then read the page with `view`.".to_owned(),
            _ => "Nothing was sent to the page. Pass an `expect` condition that is false now and true after the action, or use explore mode for deliberate unverified input and observe afterwards.".to_owned(),
        },
        _ => return,
    };
    detail.hint = Some(hint);
}

#[cfg(test)]
mod tests;
