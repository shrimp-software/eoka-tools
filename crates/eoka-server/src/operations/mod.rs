pub mod commands;
pub mod sidecar;
pub(crate) mod store;

use crate::engine::blockers::{self, Blocker};
use anyhow::{anyhow, Context, Result};
use eoka::{
    Action, ActionOptions, ActionOutcome, ActionReceipt, DispatchObserver, DispatchState, Error,
    Page,
};
use eoka_protocol::{OperationListArgs, OperationRecord, OperationState};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use store::{timestamp, Store};
use tokio::sync::{watch, OnceCell};

#[derive(Clone, Default)]
pub struct OperationManager {
    inner: Arc<Manager>,
}
#[derive(Default)]
struct Manager {
    store: OnceCell<Arc<Store>>,
    directory: Option<PathBuf>,
    jobs: Mutex<HashMap<String, ActiveOperation>>,
    halts_on_dialog: AtomicBool,
    interrupted_targets: Mutex<HashSet<String>>,
}

struct ActiveOperation {
    target_id: String,
    cancel: watch::Sender<bool>,
}

#[derive(Default)]
pub(crate) struct Execution {
    pub canonical: Option<Vec<u8>>,
    pub exploratory: bool,
}

pub(crate) struct Intent {
    pub store: Arc<Store>,
    pub id: String,
}
impl DispatchObserver for Intent {
    fn before_dispatch(
        &self,
        preparation: bool,
    ) -> Pin<Box<dyn Future<Output = eoka::Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.store
                .update(self.id.clone(), move |record| {
                    if preparation {
                        record.preparation_intent = true;
                    } else {
                        record.dispatch_intent = true;
                        record.outcome = ActionOutcome::OutcomeUnknown;
                    }
                })
                .await
                .and_then(|record| {
                    if matches!(record.state, OperationState::Running)
                        && record.runtime_id == self.store.runtime
                        && (if preparation {
                            record.preparation_intent
                        } else {
                            record.dispatch_intent
                        })
                    {
                        Ok(())
                    } else {
                        Err(anyhow!("Operation is no longer owned and running"))
                    }
                })
                .map_err(|error| {
                    Error::transport_io(
                        "Could not durably record dispatch intent",
                        std::io::Error::other(error),
                    )
                })
        })
    }
}

struct TrackedIntent {
    intent: Intent,
    dispatched: Arc<AtomicBool>,
}
impl DispatchObserver for TrackedIntent {
    fn before_dispatch(
        &self,
        preparation: bool,
    ) -> Pin<Box<dyn Future<Output = eoka::Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.intent.before_dispatch(preparation).await?;
            if !preparation {
                self.dispatched.store(true, Ordering::Release);
            }
            Ok(())
        })
    }
}

impl OperationManager {
    pub fn in_directory(directory: PathBuf) -> Self {
        Self {
            inner: Arc::new(Manager {
                directory: Some(directory),
                ..Default::default()
            }),
        }
    }
    pub fn halt_on_dialog(&self) {
        self.inner.halts_on_dialog.store(true, Ordering::Release);
    }

    async fn dialog_opened(&self, page: &Page) -> Blocker {
        if self.inner.halts_on_dialog.load(Ordering::Acquire) {
            blockers::interrupted(page, false).await
        } else {
            std::future::pending().await
        }
    }

    fn note_interrupted(&self, target_id: &str, interrupted: bool) -> bool {
        self.inner
            .interrupted_targets
            .lock()
            .map(|mut targets| {
                if interrupted {
                    targets.insert(target_id.to_owned())
                } else {
                    targets.remove(target_id)
                }
            })
            .unwrap_or(false)
    }

    async fn release_interrupted_input(&self, page: &Page) {
        if !self.note_interrupted(page.target_id(), false) {
            return;
        }
        if matches!(blockers::current(page), Some(Blocker::Dialog(_))) {
            self.note_interrupted(page.target_id(), true);
            return;
        }
        let released =
            tokio::time::timeout(INTERRUPTED_INPUT_RELEASE, page.release_all_inputs()).await;
        if !matches!(released, Ok(Ok(()))) {
            tracing::warn!(
                target_id = page.target_id(),
                "Could not release input left by an action a dialog interrupted"
            );
        }
    }

    pub(crate) async fn store(&self) -> Result<&Arc<Store>> {
        self.inner
            .store
            .get_or_try_init(|| async {
                Store::open(match &self.inner.directory {
                    Some(path) => path.clone(),
                    None => store::state_directory()?,
                })
                .await
            })
            .await
    }
    pub async fn submit(
        &self,
        page: Page,
        id: String,
        action: Action,
        options: ActionOptions,
    ) -> Result<OperationRecord> {
        self.submit_with(page, id, action, options, Execution::default())
            .await
    }

    pub(crate) async fn submit_with(
        &self,
        page: Page,
        id: String,
        action: Action,
        options: ActionOptions,
        execution: Execution,
    ) -> Result<OperationRecord> {
        let manager = self.clone();
        tokio::spawn(async move {
            manager
                .submit_owned(page, id, action, options, execution)
                .await
        })
        .await
        .context("Operation admission task failed; inspect its ID before retrying")
        .map_err(commands::uncertain)?
    }

    async fn submit_owned(
        &self,
        page: Page,
        id: String,
        action: Action,
        mut options: ActionOptions,
        execution: Execution,
    ) -> Result<OperationRecord> {
        let parsed = uuid::Uuid::parse_str(&id)
            .context("operation_id must be a UUID generated before sending the request")?;
        if parsed.to_string() != id {
            return Err(anyhow!("operation_id must be a canonical lowercase UUID"));
        }
        let store = self.store().await?.clone();
        let canonical = match execution.canonical {
            Some(canonical) => canonical,
            None => serde_json::to_vec(&serde_json::json!({"action":action,"options":options}))?,
        };
        let initial = OperationRecord {
            id: id.clone(),
            runtime_id: store.runtime.clone(),
            target_id: page.target_id().into(),
            action: action.kind().into(),
            state: OperationState::Running,
            outcome: ActionOutcome::NotDispatched,
            dispatch_intent: false,
            preparation_intent: false,
            started_at_ms: timestamp(),
            finished_at_ms: None,
            receipt: None,
            error_code: None,
            error: None,
        };
        let (record, created) = store
            .admit(initial, canonical)
            .await
            .map_err(commands::uncertain)?;
        if !created {
            return Ok(record);
        }
        blockers::watch(&page);
        self.release_interrupted_input(&page).await;
        let (cancel, mut canceled) = watch::channel(false);
        self.inner
            .jobs
            .lock()
            .map_err(|_| commands::uncertain(anyhow!("Operation registry unavailable")))?
            .insert(
                id.clone(),
                ActiveOperation {
                    target_id: page.target_id().into(),
                    cancel,
                },
            );
        let dispatched = Arc::new(AtomicBool::new(false));
        options.observer = Some(Arc::new(TrackedIntent {
            intent: Intent {
                store: store.clone(),
                id: id.clone(),
            },
            dispatched: dispatched.clone(),
        }));
        let manager = self.clone();
        tokio::spawn(async move {
            let sensitive = action_values(&action);
            let navigation = navigates(&action);
            let kind = action.kind();
            let frames = crate::capture::Frames::hold(&page);
            let mut dialog = None;
            let result = tokio::select! {
                biased;
                _ = canceled.changed() => None,
                opened = manager.dialog_opened(&page) => {
                    dialog = Some(opened);
                    None
                },
                result = async {
                    if execution.exploratory { page.explore(action, options).await }
                    else { page.act(action, options).await }
                } => Some(result),
            };
            let unpainted = match &result {
                Some(Err(error)) if awaited_frames(kind, error) => frames_stalled(&page).await,
                _ => None,
            };
            drop(frames);
            if dialog.is_some() {
                manager.note_interrupted(page.target_id(), true);
            } else if navigation
                && navigation_unanswered(&result, dispatched.load(Ordering::Acquire))
            {
                abandon_navigation(&page).await;
            }
            let interruption = dialog
                .map(|dialog| interruption(&dialog, kind, &id, dispatched.load(Ordering::Acquire)));
            let diagnostic = result
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(|error| diagnostic(error, &sensitive, &id))
                .map(|detail| without_frames(detail, kind, unpainted));
            let finish = store
                .update(id.clone(), move |record| {
                    record.state = OperationState::Finished;
                    record.finished_at_ms = Some(timestamp());
                    record.error = diagnostic;
                    match result {
                        Some(Ok(receipt)) => {
                            record.outcome = receipt.outcome;
                            record.receipt = Some(receipt);
                        }
                        Some(Err(Error::Action { receipt, code, .. })) => {
                            record.outcome = receipt.outcome;
                            record.receipt = Some(*receipt);
                            record.error_code = Some(code);
                        }
                        None if interruption.is_some() => {
                            record.outcome = if record.dispatch_intent {
                                ActionOutcome::OutcomeUnknown
                            } else {
                                ActionOutcome::NotDispatched
                            };
                            record.receipt = Some(interrupted_receipt(record));
                            record.error_code = interruption.as_ref().map(|detail| detail.code.clone());
                            record.error = interruption;
                        }
                        other => {
                            record.outcome = if record.dispatch_intent {
                                ActionOutcome::OutcomeUnknown
                            } else {
                                ActionOutcome::NotDispatched
                            };
                            record.error_code = Some(
                                if other.is_none() {
                                    "operation_canceled"
                                } else {
                                    "action_failed"
                                }
                                .into(),
                            );
                            if record.error.is_none() {
                                record.error = Some(eoka_protocol::ErrorDetail::new(record.error_code.as_deref().unwrap_or("action_failed"), "Operation canceled; inspect dispatch evidence before further input").retryable(false));
                            }
                        }
                    }
                })
                .await;
            if let Err(error) = finish {
                tracing::error!(operation_id = %id, %error, "Could not persist operation completion; outcome remains uncertain");
            }
            if let Ok(mut jobs) = manager.inner.jobs.lock() {
                jobs.remove(&id);
            }
        });
        Ok(record)
    }
    pub async fn get(&self, id: &str) -> Result<OperationRecord> {
        let store = self.store().await?;
        store.recover().await?;
        store.get(id.into()).await
    }
    pub async fn list(&self, args: OperationListArgs) -> Result<Vec<OperationRecord>> {
        let limit = args.limit.unwrap_or(100).clamp(1, 500);
        let before = args.before_ms.unwrap_or(i64::MAX as u64);
        self.store()
            .await?
            .run(move |connection| {
                let mut query = connection.prepare(
                    "SELECT record FROM operations WHERE started < ?
                     ORDER BY started DESC,id LIMIT ?",
                )?;
                let rows = query.query_map(rusqlite::params![before, limit], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
            })
            .await
    }
    pub async fn wait(&self, id: &str, timeout_ms: u64) -> Result<OperationRecord> {
        if timeout_ms > 86_400_000 {
            return Err(anyhow!("Wait timeout exceeds 24 hours"));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let record = self.get(id).await?;
            if !matches!(record.state, OperationState::Running)
                || tokio::time::Instant::now() >= deadline
            {
                return Ok(record);
            }
            tokio::time::sleep_until(
                deadline.min(tokio::time::Instant::now() + Duration::from_millis(25)),
            )
            .await;
        }
    }
    pub async fn cancel(&self, id: &str) -> Result<OperationRecord> {
        let record = self.get(id).await?;
        self.cancel_admitted(id, record)
    }

    pub(crate) fn cancel_admitted(
        &self,
        id: &str,
        record: OperationRecord,
    ) -> Result<OperationRecord> {
        if matches!(record.state, OperationState::Running) {
            let jobs = self
                .inner
                .jobs
                .lock()
                .map_err(|_| anyhow!("Operation registry unavailable"))?;
            let active = jobs.get(id).ok_or_else(|| {
                anyhow!("operation_owned_elsewhere: request cancellation from the owning runtime")
            })?;
            active.cancel.send_replace(true);
        }
        Ok(record)
    }
    pub async fn stop(&self) {
        self.stop_where(|_| true).await;
    }

    pub async fn stop_target(&self, target_id: &str) {
        self.stop_where(|active| active == target_id).await;
    }

    fn signal(&self, selected: impl Fn(&str) -> bool) -> Vec<String> {
        if let Ok(jobs) = self.inner.jobs.lock() {
            jobs.iter()
                .filter(|(_, active)| selected(&active.target_id))
                .map(|(id, active)| {
                    active.cancel.send_replace(true);
                    id.clone()
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    }

    async fn stop_where(&self, selected: impl Fn(&str) -> bool) {
        let ids = self.signal(selected);
        for id in ids {
            if let Err(error) = self.wait(&id, 5000).await {
                tracing::warn!(operation_id = %id, %error, "Could not confirm operation shutdown");
            }
        }
    }
    pub fn is_active(&self) -> bool {
        self.inner
            .jobs
            .lock()
            .map(|j| !j.is_empty())
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests;

fn action_values(action: &Action) -> Vec<String> {
    match action {
        Action::Fill { value, .. } => vec![value.clone()],
        Action::Type { text, .. } | Action::InsertText { text } => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn failure_phase(description: &str) -> &'static str {
    if description.contains("preparation") {
        "preparation"
    } else if description.contains("queue admission") {
        "queue"
    } else if description.contains("input dispatch") {
        "dispatch"
    } else if description.contains("verification") {
        "verification"
    } else {
        "execution"
    }
}

fn diagnostic(
    error: &Error,
    sensitive: &[String],
    operation_id: &str,
) -> eoka_protocol::ErrorDetail {
    let (code, source) = match error {
        Error::Action { code, source, .. } => (code.as_str(), source.as_ref()),
        error => ("action_failed", error),
    };
    let description = source.to_string();
    let phase = failure_phase(&description);
    let message = redact(&description, sensitive);
    eoka_protocol::ErrorDetail::new(code, message.chars().take(8192).collect::<String>())
        .retryable(false)
        .context(eoka_protocol::ErrorContext {
            operation_id: Some(operation_id.to_owned()),
            phase: Some(phase.into()),
            ..Default::default()
        })
}

const INTERRUPTED_INPUT_RELEASE: Duration = Duration::from_secs(5);
const FRAME_PROBE: Duration = Duration::from_millis(1500);
const FRAME_PROBE_SCRIPT: &str = "(()=>Promise.race([new Promise(resolve=>requestAnimationFrame(()=>resolve(true))),new Promise(resolve=>setTimeout(()=>resolve(false),500))]).then(frame=>({frame,hidden:document.visibilityState==='hidden'})))()";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unpainted {
    Hidden,
    Visible,
}

fn awaited_frames(action: &str, error: &Error) -> bool {
    let source = match error {
        Error::Action { source, .. } => source.as_ref(),
        error => error,
    };
    matches!(
        action,
        "click" | "double_click" | "hover" | "drag" | "scroll_into_view"
    ) && matches!(source, Error::Timeout(description) if description.starts_with("Action deadline exceeded during preparation"))
}

async fn frames_stalled(page: &Page) -> Option<Unpainted> {
    let probed = tokio::time::timeout(
        FRAME_PROBE,
        page.evaluate::<serde_json::Value>(FRAME_PROBE_SCRIPT),
    )
    .await
    .ok()?
    .ok()?;
    if probed["frame"] != false {
        return None;
    }
    Some(if probed["hidden"] == true {
        Unpainted::Hidden
    } else {
        Unpainted::Visible
    })
}

fn without_frames(
    mut detail: eoka_protocol::ErrorDetail,
    action: &str,
    unpainted: Option<Unpainted>,
) -> eoka_protocol::ErrorDetail {
    let Some(unpainted) = unpainted else {
        return detail;
    };
    let (page, hint) = match unpainted {
        Unpainted::Hidden => (
            "the tab is hidden (document.visibilityState is hidden) and ran no animation frame",
            "Bring this tab to the front, or act on the tab that is in front, then repeat the action.",
        ),
        Unpainted::Visible => (
            "the page ran no animation frame",
            "The page is not rendering; check that it is not frozen or replacing requestAnimationFrame, then repeat the action.",
        ),
    };
    detail.message = format!(
        "{}: {page}, and a {} waits two frames to confirm its target has stopped moving",
        detail.message,
        action.replace('_', " ")
    );
    detail.hint = Some(hint.to_owned());
    detail
}

fn interruption(
    dialog: &Blocker,
    action: &str,
    operation_id: &str,
    dispatched: bool,
) -> eoka_protocol::ErrorDetail {
    let remedy = dialog.remedy(&blockers::Remedies::new(None));
    let (consequence, hint, status) = if dispatched {
        (
            format!("It opened after the {action} input was dispatched: the input was sent to the page once and is not repeated, and the {action} stopped waiting for its result."),
            format!("{remedy} Do not repeat the {action} blindly: its page effects may already have happened."),
            eoka_protocol::ActionStatus::MayHaveExecuted,
        )
    } else {
        (
            format!("It opened before the {action} input was dispatched, so nothing was sent to the page."),
            remedy,
            eoka_protocol::ActionStatus::NotAttempted,
        )
    };
    eoka_protocol::ErrorDetail::new(
        dialog.code(),
        format!("{}. {consequence}", dialog.describe()),
    )
    .retryable(false)
    .hint(hint)
    .context(eoka_protocol::ErrorContext {
        operation_id: Some(operation_id.to_owned()),
        phase: Some("execution".into()),
        action_status: Some(status),
        ..Default::default()
    })
}

fn interrupted_receipt(record: &OperationRecord) -> ActionReceipt {
    let finished_at_ms = record.finished_at_ms.unwrap_or_else(timestamp);
    ActionReceipt {
        destination_target_id: None,
        started_at_ms: record.started_at_ms,
        finished_at_ms,
        download_guid: None,
        action: record.action.clone(),
        target_id: record.target_id.clone(),
        outcome: record.outcome,
        dispatch: if record.dispatch_intent {
            DispatchState::MayHaveBeenSent
        } else {
            DispatchState::NotSent
        },
        preparation_may_have_executed: record.preparation_intent || record.dispatch_intent,
        verification: None,
        elapsed_ms: finished_at_ms.saturating_sub(record.started_at_ms),
        already_satisfied: false,
        cleanup_pending: record.dispatch_intent,
    }
}

const REDACTED: &str = "[redacted]";

fn word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn echoed_forms(value: &str) -> Vec<String> {
    let mut forms = vec![value.to_owned()];
    let escaped = serde_json::to_string(value)
        .ok()
        .and_then(|quoted| Some(quoted.get(1..quoted.len().checked_sub(1)?)?.to_owned()));
    if let Some(escaped) = escaped.filter(|escaped| escaped != value) {
        forms.push(escaped);
    }
    forms
}

fn redact_standalone(message: &str, value: &str) -> String {
    let mut redacted = String::with_capacity(message.len());
    let mut rest = 0;
    let mut search = 0;
    while let Some(found) = message[search..].find(value) {
        let start = search + found;
        let end = start + value.len();
        let embedded = message[..start]
            .chars()
            .next_back()
            .is_some_and(word_character)
            || message[end..].chars().next().is_some_and(word_character);
        if embedded {
            search = start + message[start..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        redacted.push_str(&message[rest..start]);
        redacted.push_str(REDACTED);
        rest = end;
        search = end;
    }
    redacted.push_str(&message[rest..]);
    redacted
}

pub(crate) fn redact(message: &str, sensitive: &[String]) -> String {
    sensitive
        .iter()
        .filter(|value| !value.is_empty())
        .flat_map(|value| echoed_forms(value))
        .fold(message.to_owned(), |message, form| {
            redact_standalone(&message, &form)
        })
}

fn navigates(action: &Action) -> bool {
    matches!(
        action,
        Action::Navigate { .. }
            | Action::NavigateWithHeaders { .. }
            | Action::Reload
            | Action::Back
            | Action::Forward
            | Action::History { .. }
    )
}

fn navigation_unanswered(result: &Option<eoka::Result<ActionReceipt>>, dispatched: bool) -> bool {
    match result {
        None => dispatched,
        Some(Err(Error::Action { receipt, .. })) => {
            receipt.dispatch == DispatchState::MayHaveBeenSent
        }
        Some(_) => false,
    }
}

const NAVIGATION_ABANDON_DEADLINE: Duration = Duration::from_secs(2);

pub(crate) async fn abandon_navigation(page: &Page) {
    let stopped = tokio::time::timeout(
        NAVIGATION_ABANDON_DEADLINE,
        page.session()
            .send::<_, serde_json::Value>("Page.stopLoading", &serde_json::json!({})),
    )
    .await;
    match stopped {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            tracing::warn!(target_id = page.target_id(), %error, "Could not stop an unanswered navigation");
        }
        Err(_) => {
            tracing::warn!(
                target_id = page.target_id(),
                "Stopping an unanswered navigation exceeded its deadline"
            );
        }
    }
}
