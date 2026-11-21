use super::*;

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

pub(super) fn abandoned_response(command: &str) -> Response {
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

impl crate::engine::Engine {
    pub(in crate::engine) fn named(
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

    pub(in crate::engine) async fn refuse_blocked(
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

    pub(in crate::engine) async fn dispatch_guarded(
        &mut self,
        request: eoka_protocol::Request,
        arguments: &Value,
    ) -> Result<Response, crate::engine::EngineError> {
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

    pub(in crate::engine) async fn diagnose_blocked(
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
