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
const CONTROL_LIMIT: Duration = Duration::from_secs(2);
const SETTLE_LIMIT: Duration = Duration::from_secs(1);
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
            Surface::Catalog => format!("`{catalog}`"),
        }
    }

    pub fn name(&self, command: &str, arguments: &Value) -> String {
        let command = match command {
            "action_command" => arguments["command"].as_str().unwrap_or(command),
            command => command,
        };
        match (self.surface, command) {
            (Surface::Cli, "agent_observe") => "view".into(),
            (Surface::Cli, "agent_step") => "step".into(),
            (Surface::Cli, "action_perform") => "action".into(),
            (Surface::Cli, command) => command.replace('_', "-"),
            (_, command) => command.to_owned(),
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
        self.command("tab new URL", "new_tab", "tabs.new")
    }

    pub fn status(&self) -> String {
        self.command("status", "list_tabs", "tabs.list")
    }

    pub fn operation(&self, id: &str) -> String {
        self.command(
            &format!("operations get {id}"),
            &format!("operation_get {id}"),
            &format!("operations.get {id}"),
        )
    }
}

#[derive(Default)]
struct SessionState {
    dialog: Option<Dialog>,
    navigations: HashMap<String, Navigation>,
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
            "Page.frameNavigated" => self.finish(session, params["frame"]["id"].as_str()),
            "Page.frameStoppedLoading" | "Page.frameDetached" => {
                self.finish(session, params["frameId"].as_str())
            }
            _ => false,
        }
    }

    fn finish(&mut self, session: &str, frame: Option<&str>) -> bool {
        let Some(frame) = frame else {
            return false;
        };
        self.0
            .get_mut(session)
            .is_some_and(|state| state.navigations.remove(frame).is_some())
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
    if responsive(page).await {
        clear(page);
        return None;
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

pub async fn finish_navigation(
    operations: &crate::operations::OperationManager,
    page: &Page,
    navigation_action: bool,
) -> Option<Navigation> {
    if !matches!(current(page), Some(Blocker::Navigation(_))) {
        return None;
    }
    let mut running = false;
    for record in running_on(operations, page).await {
        running = true;
        let navigation = navigation_action || navigates(&record.action);
        release_when_finished(page.clone(), operations.clone(), record.id, navigation);
    }
    if running {
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
    pub fn current_page(&self) -> Option<Page> {
        self.state
            .as_ref()
            .and_then(|state| state.current_tab())
            .map(|tab| tab.page.clone())
    }

    pub fn blocker_summary(&self) -> Option<Value> {
        let page = self.current_page()?;
        current(&page).map(|blocker| blocker.summary())
    }

    pub(super) async fn refuse_blocked(
        &mut self,
        command: &str,
        arguments: &Value,
        policy: Policy,
    ) -> Option<Response> {
        let page = self.current_page()?;
        watch(&page);
        if policy == Policy::Free {
            return None;
        }
        let grace = if policy == Policy::Navigate {
            Duration::ZERO
        } else {
            NAVIGATION_GRACE
        };
        let blocker = match confirmed(&page, grace).await? {
            Blocker::Navigation(_) if policy == Policy::Navigate => {
                supersede_navigation(&self.operations, &page).await;
                return None;
            }
            Blocker::Navigation(_) if matches!(policy, Policy::Wait | Policy::Capture) => {
                return None
            }
            blocker => blocker,
        };
        let remedies = Remedies::new(Some(&self.session_name));
        let mut response = Response::err_detail(blocker.detail(
            &remedies,
            &remedies.name(command, arguments),
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
        command: &str,
        arguments: &Value,
    ) -> Result<Response, super::EngineError> {
        let policy = policy(command, arguments);
        let sleeps = command == "wait" && policy == Policy::Free;
        if !sleeps && !matches!(policy, Policy::Read | Policy::Wait | Policy::Capture) {
            return self.dispatch(command, arguments).await;
        }
        let page = self.current_page();
        let session = self.session_name.clone();
        let remedies = Remedies::new(Some(&session));
        let name = remedies.name(command, arguments);
        guard(
            page.as_ref(),
            remedies,
            &name,
            policy,
            limit(policy, arguments),
            self.dispatch(command, arguments),
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
        if let (Some(id), Some(Blocker::Navigation(_))) = (id.as_deref(), current(&page)) {
            settle(&self.operations, id).await;
            let record = self.operations.get(id).await.ok();
            let running = record.as_ref().is_some_and(|record| {
                matches!(record.state, eoka_protocol::OperationState::Running)
            });
            if running {
                release_when_finished(
                    page.clone(),
                    self.operations.clone(),
                    id.to_owned(),
                    navigation_action,
                );
            } else {
                let grace = if navigation_action {
                    Duration::ZERO
                } else {
                    NAVIGATION_GRACE
                };
                if let Some(navigation) = abandon_navigation(&page, grace).await {
                    if let Some(record) = record {
                        let status = record.status();
                        if let Some(data) = response.data.as_mut().filter(|data| data.is_object()) {
                            if data.get("observation").is_some()
                                || data.get("observation_error").is_some()
                            {
                                data["operation"] = json!(record);
                            } else if let Ok(Value::Object(fields)) = serde_json::to_value(&record)
                            {
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
        }
        if !timed_out(response) {
            return;
        }
        let name = remedies.name(command, arguments);
        let detail = match confirmed(&page, Duration::ZERO).await {
            Some(blocker) => blocker.detail(&remedies, &name, policy.interrupted()),
            None if !responsive(&page).await => frozen(&remedies, &name),
            None => return,
        };
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

pub fn unresponsive(remedies: &Remedies<'_>, command: &str, limit: Duration) -> ErrorDetail {
    ErrorDetail::new(
        "page_unresponsive",
        format!(
            "`{command}` got no answer from the page within {} s. No dialog or pending navigation is known, so the page's main thread is busy or frozen.",
            limit.as_secs()
        ),
    )
    .retryable(false)
    .hint(format!(
        "Retry once the page settles. If it stays frozen, continue in a fresh tab with {}. {} clears a dialog that was already open before this session attached.",
        remedies.new_tab(),
        remedies.dismiss()
    ))
}

pub fn frozen(remedies: &Remedies<'_>, command: &str) -> ErrorDetail {
    ErrorDetail::new(
        "page_unresponsive",
        format!(
            "`{command}` timed out and the page does not answer a liveness probe. No dialog or pending navigation is known, so the page's main thread is busy or frozen."
        ),
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
    let navigates = |kind: Option<&str>| {
        matches!(
            kind,
            Some("navigate" | "navigate_with_headers" | "reload" | "back" | "forward" | "history")
        )
    };
    match command {
        "open" | "back" | "forward" | "reload" => Policy::Navigate,
        "action_command" => match arguments["command"].as_str() {
            Some("back" | "forward" | "reload") => Policy::Navigate,
            _ => Policy::Gate,
        },
        "action_perform" | "agent_step" if navigates(arguments["action"]["kind"].as_str()) => {
            Policy::Navigate
        }
        "wait" => {
            if arguments["ms"].is_u64() {
                Policy::Free
            } else {
                Policy::Wait
            }
        }
        "snapshot" | "observe" | "info" | "frames" | "text" | "find" | "content" | "get_text"
        | "get_attribute" | "exists" | "storage" | "dump_storage" | "console" | "errors"
        | "spa_info" | "wasm_info" | "wasm_read" | "wasm_find" => Policy::Read,
        "eval" | "exec" | "frame_eval" | "fetch" => Policy::Script,
        "screenshot" => {
            if arguments["annotate"].as_bool().unwrap_or(false) {
                Policy::Gate
            } else {
                Policy::Capture
            }
        }
        "agent_observe" => Policy::Observe,
        "agent_step" | "action_perform" | "click" | "dblclick" | "fill" | "select" | "hover"
        | "key" | "click_shadow" | "mouse_down" | "mouse_move" | "mouse_up" | "key_down"
        | "key_up" | "scroll" | "set_storage" | "save_state" | "spa_navigate" | "wasm_write"
        | "captcha_detect" | "captcha_inject" | "download_start" => Policy::Gate,
        _ => Policy::Free,
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
    let deadline = async {
        match limit {
            Some(limit) => tokio::time::sleep(limit).await,
            None => std::future::pending().await,
        }
    };
    let Some(page) = page.filter(|_| policy != Policy::Free) else {
        return tokio::select! {
            biased;
            result = work => result,
            _ = abandoned() => Ok(abandoned_response(command)),
        };
    };
    tokio::select! {
        biased;
        result = work => result,
        _ = abandoned() => Ok(abandoned_response(command)),
        blocker = interrupted(page, policy != Policy::Wait) => {
            Ok(Response::err_detail(blocker.detail(&remedies, command, policy.interrupted())))
        }
        _ = deadline => {
            let limit = limit.unwrap_or_default();
            Ok(Response::err_detail(match current(page) {
                Some(blocker) => blocker.detail(&remedies, command, policy.interrupted()),
                None => unresponsive(&remedies, command, limit),
            }))
        }
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
mod tests {
    use super::*;

    fn event(sessions: &mut Sessions, method: &str, params: Value) -> bool {
        sessions.apply(Some("session"), method, &params, Instant::now())
    }

    #[test]
    fn a_dialog_blocks_until_it_closes() {
        let mut sessions = Sessions::default();
        assert!(sessions.current("session", "top").is_none());
        assert!(event(
            &mut sessions,
            "Page.javascriptDialogOpening",
            json!({"type":"confirm","message":"Delete?","url":"https://example.test/"}),
        ));
        let Some(Blocker::Dialog(dialog)) = sessions.current("session", "top") else {
            panic!("dialog expected")
        };
        assert_eq!(dialog.kind, "confirm");
        assert_eq!(dialog.message, "Delete?");
        assert!(event(
            &mut sessions,
            "Page.javascriptDialogClosed",
            json!({"result":true})
        ));
        assert!(sessions.current("session", "top").is_none());
        assert!(!event(
            &mut sessions,
            "Page.javascriptDialogClosed",
            json!({"result":true})
        ));
    }

    #[test]
    fn a_navigation_is_pending_from_start_until_commit_or_stop() {
        let mut sessions = Sessions::default();
        event(
            &mut sessions,
            "Page.frameStartedLoading",
            json!({"frameId":"top"}),
        );
        event(
            &mut sessions,
            "Page.frameStartedNavigating",
            json!({"frameId":"top","url":"https://slow.test/","navigationType":"differentDocument"}),
        );
        let Some(Blocker::Navigation(navigation)) = sessions.current("session", "top") else {
            panic!("navigation expected")
        };
        assert_eq!(navigation.url.as_deref(), Some("https://slow.test/"));
        assert!(event(
            &mut sessions,
            "Page.frameNavigated",
            json!({"frame":{"id":"top","url":"https://slow.test/"}}),
        ));
        assert!(sessions.current("session", "top").is_none());
        event(
            &mut sessions,
            "Page.frameStartedLoading",
            json!({"frameId":"top"}),
        );
        assert!(sessions.current("session", "top").is_some());
        assert!(event(
            &mut sessions,
            "Page.frameStoppedLoading",
            json!({"frameId":"top"})
        ));
        assert!(sessions.current("session", "top").is_none());
    }

    #[test]
    fn subframes_other_sessions_and_same_document_changes_do_not_block_the_tab() {
        let mut sessions = Sessions::default();
        event(
            &mut sessions,
            "Page.frameStartedLoading",
            json!({"frameId":"child"}),
        );
        assert!(sessions.current("session", "top").is_none());
        assert!(!event(
            &mut sessions,
            "Page.frameStartedNavigating",
            json!({"frameId":"top","url":"https://app.test/#tab","navigationType":"sameDocument"}),
        ));
        assert!(sessions.current("session", "top").is_none());
        sessions.apply(
            Some("other"),
            "Page.javascriptDialogOpening",
            &json!({"type":"alert","message":"x"}),
            Instant::now(),
        );
        assert!(sessions.current("session", "top").is_none());
        assert!(sessions.current("other", "top").is_some());
    }

    #[test]
    fn a_dialog_outranks_a_pending_navigation_and_detach_forgets_the_session() {
        let mut sessions = Sessions::default();
        event(
            &mut sessions,
            "Page.frameStartedNavigating",
            json!({"frameId":"top","url":"https://slow.test/","navigationType":"differentDocument"}),
        );
        event(
            &mut sessions,
            "Page.javascriptDialogOpening",
            json!({"type":"beforeunload","message":""}),
        );
        assert!(matches!(
            sessions.current("session", "top"),
            Some(Blocker::Dialog(_))
        ));
        assert!(sessions.apply(
            None,
            "Target.detachedFromTarget",
            &json!({"sessionId":"session"}),
            Instant::now(),
        ));
        assert!(sessions.current("session", "top").is_none());
    }

    #[test]
    fn errors_name_the_blocker_and_the_next_command() {
        let dialog = Blocker::Dialog(Dialog {
            kind: "alert".into(),
            message: "Saved".into(),
            url: String::new(),
            since: Instant::now(),
        });
        let cli = Remedies::for_surface(Surface::Cli, Some("work"));
        let detail = dialog.detail(&cli, "view", Stage::NotStarted);
        assert_eq!(detail.code, "dialog_open");
        assert!(detail.message.contains("\"Saved\""));
        assert!(detail.message.contains("`view` was not started"));
        let hint = detail.hint.unwrap();
        assert!(hint.contains("`eoka --session work dialog accept`"));
        assert!(hint.contains("`eoka --session work dialog dismiss`"));
        assert_eq!(
            detail.context.unwrap().action_status,
            Some(ActionStatus::NotAttempted)
        );
        let navigation = Blocker::Navigation(Navigation {
            url: Some("https://slow.test/".into()),
            since: Instant::now(),
        });
        let mcp = Remedies::for_surface(Surface::Mcp, None);
        let detail = navigation.detail(&mcp, "page_text", Stage::ReadInterrupted);
        assert!(detail.context.is_none());
        assert!(dialog
            .detail(&mcp, "exec", Stage::ActionInterrupted)
            .hint
            .unwrap()
            .contains("Do not repeat"));
        assert_eq!(detail.code, "navigation_pending");
        assert!(detail.message.contains("https://slow.test/"));
        assert!(detail.message.contains("returned without a result"));
        assert!(detail.hint.unwrap().contains("`reload`"));
        let default = Remedies::for_surface(Surface::Cli, Some("default"));
        assert!(unresponsive(&default, "info", READ_LIMIT)
            .hint
            .unwrap()
            .contains("`eoka tab new URL`"));
    }

    #[test]
    fn commands_are_classified_by_what_they_need_from_the_page() {
        let none = json!({});
        assert_eq!(policy("dialog_accept", &none), Policy::Free);
        assert_eq!(policy("tab_list", &none), Policy::Free);
        assert_eq!(policy("close", &none), Policy::Free);
        assert_eq!(policy("info", &none), Policy::Read);
        assert_eq!(policy("eval", &none), Policy::Script);
        assert_eq!(policy("open", &none), Policy::Navigate);
        assert_eq!(policy("click", &none), Policy::Gate);
        assert_eq!(policy("agent_observe", &none), Policy::Observe);
        assert_eq!(Policy::Observe.interrupted(), Stage::ReadInterrupted);
        assert_eq!(Policy::Script.interrupted(), Stage::ActionInterrupted);
        assert_eq!(policy("screenshot", &none), Policy::Capture);
        assert_eq!(
            policy("screenshot", &json!({"annotate":true})),
            Policy::Gate
        );
        assert_eq!(policy("wait", &json!({"ms":500})), Policy::Free);
        assert_eq!(policy("wait", &json!({"text":"Done"})), Policy::Wait);
        assert_eq!(
            policy("action_command", &json!({"command":"reload"})),
            Policy::Navigate
        );
        assert_eq!(
            policy("action_command", &json!({"command":"click"})),
            Policy::Gate
        );
        assert_eq!(
            policy("agent_step", &json!({"action":{"kind":"navigate"}})),
            Policy::Navigate
        );
        assert_eq!(
            policy("agent_step", &json!({"action":{"kind":"click"}})),
            Policy::Gate
        );
        assert_eq!(limit(Policy::Read, &none), Some(READ_LIMIT));
        assert_eq!(
            limit(Policy::Wait, &json!({"timeout":4000})),
            Some(Duration::from_secs(6))
        );
        assert_eq!(limit(Policy::Script, &none), None);
    }

    #[test]
    fn a_missing_expectation_says_how_to_supply_one() {
        let mut response = Response::err_detail(ErrorDetail::new(
            "expectation_required",
            "expectation_required: action requires an expected result",
        ));
        explain(
            &mut response,
            &Remedies::for_surface(Surface::Cli, Some("work")),
        );
        let hint = response.error_detail.unwrap().hint.unwrap();
        assert!(hint.contains("--expect"));
        assert!(hint.contains("explore"));
    }
}
