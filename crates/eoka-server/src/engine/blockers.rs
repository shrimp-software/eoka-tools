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

mod control;
mod explain;

pub use control::*;
pub use explain::*;

#[cfg(test)]
mod tests;
