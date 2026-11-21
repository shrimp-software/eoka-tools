use super::*;

impl Engine {
    pub(super) async fn disable_fetch_session(&mut self, session: &CdpSession) {
        let session_id = session.session_id().to_string();
        session.transport().remove_request_route(&session_id);
        self.drain_fetch_events().await;
        let _ = session
            .transport()
            .set_request_interception(&session_id, None)
            .await;
        self.fetch_sessions.remove(&session_id);
        if self.fetch_sessions.is_empty() {
            self.intercept.enabled = false;
            self.fetch_events = None;
            self.fetch_sender = None;
        }
    }

    pub(super) async fn disable_stale_fetch_sessions(&mut self, keep_session_id: Option<&str>) {
        let stale: Vec<CdpSession> = self
            .fetch_sessions
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != keep_session_id)
            .map(|(_, session)| session.clone())
            .collect();

        for session in stale {
            self.disable_fetch_session(&session).await;
        }
    }

    pub(super) async fn disable_all_fetch_sessions(&mut self) {
        self.disable_stale_fetch_sessions(None).await;
        self.fetch_sessions.clear();
        self.intercept.enabled = false;
        self.fetch_events = None;
        self.fetch_sender = None;
    }

    pub(super) fn fetch_command_transport(&self) -> Option<std::sync::Arc<eoka::cdp::Transport>> {
        self.fetch_sessions
            .values()
            .next()
            .map(|session| session.transport().clone())
            .or_else(|| {
                self.state
                    .as_ref()
                    .and_then(|state| state.current_tab())
                    .map(|tab| tab.page.session().transport().clone())
            })
    }

    pub(super) fn fetch_drain_config(&self) -> Option<FetchDrainConfig> {
        Some(FetchDrainConfig {
            transport: self.fetch_command_transport()?,
            failures: self.fetch_dropped.clone(),
            rules: self.intercept.rules_snapshot(),
        })
    }

    pub(super) fn start_fetch_drain(&mut self) -> Option<FetchDrainHandle> {
        if !self.intercept.enabled {
            return None;
        }
        let config = self.fetch_drain_config()?;
        let rx = self.fetch_events.take()?;
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let join = tokio::spawn(fetch_drain_until_stopped(rx, config, stop_rx));
        Some(FetchDrainHandle {
            stop: stop_tx,
            join,
        })
    }

    pub(super) async fn stop_fetch_drain(&mut self, handle: Option<FetchDrainHandle>) -> bool {
        let Some(mut handle) = handle else {
            return true;
        };
        let _ = handle.stop.send(());
        match tokio::time::timeout(std::time::Duration::from_secs(5), &mut handle.join).await {
            Ok(Ok((rx, logs))) => {
                if self.fetch_events.is_none() && self.intercept.enabled {
                    self.fetch_events = Some(rx);
                }
                for log in logs {
                    self.intercept.add_log(log);
                }
                true
            }
            failed => {
                self.fetch_dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for session in self.fetch_sessions.values() {
                    session
                        .transport()
                        .remove_request_route(session.session_id());
                }
                self.fetch_events = None;
                self.fetch_sender = None;
                if failed.is_err() {
                    if let Some(config) = self.fetch_drain_config() {
                        tokio::spawn(async move {
                            if let Ok((mut rx, mut logs)) = handle.join.await {
                                rx.receiver.close();
                                drain_fetch_receiver(&mut rx, &config, &mut logs).await;
                            }
                        });
                    }
                }
                false
            }
        }
    }

    pub(super) async fn sync_fetch_interception(&mut self) -> Result<(), String> {
        if self.intercept.is_empty() {
            self.disable_all_fetch_sessions().await;
            return Ok(());
        }

        self.ensure_browser().await?;
        let patterns = self.intercept.fetch_patterns_json();
        let session = {
            let tab = self.require_tab()?;
            tab.page.session().clone()
        };
        let session_id = session.session_id().to_string();
        self.disable_stale_fetch_sessions(Some(&session_id)).await;
        if self.fetch_sender.is_none() {
            let (sender, receiver) = tokio::sync::mpsc::channel(256);
            self.fetch_sender = Some(sender);
            self.fetch_events = Some(FetchQueue {
                receiver,
                transport: session.transport().clone(),
                failures: self.fetch_dropped.clone(),
            });
            for existing in self.fetch_sessions.values() {
                existing
                    .transport()
                    .remove_request_route(existing.session_id());
            }
            self.fetch_sessions.clear();
        }
        if !self.fetch_sessions.contains_key(&session_id) {
            session
                .transport()
                .install_request_route(
                    &session_id,
                    self.fetch_sender.as_ref().unwrap().clone(),
                    self.fetch_dropped.clone(),
                )
                .map_err(|error| error.to_string())?;
        }
        if let Err(error) = session
            .transport()
            .set_request_interception(
                &session_id,
                Some(patterns.as_array().cloned().unwrap_or_default()),
            )
            .await
        {
            session.transport().remove_request_route(&session_id);
            self.fetch_sessions.remove(&session_id);
            self.intercept.enabled = false;
            return Err(error.to_string());
        }
        self.intercept.enabled = true;
        self.fetch_sessions.insert(session_id, session);
        Ok(())
    }

    pub fn service_idle_fetch(&mut self) {
        if self.idle_fetch_drain.is_none() {
            self.idle_fetch_drain = self.start_fetch_drain();
        }
    }

    pub(crate) async fn drain_fetch_events(&mut self) {
        let drain = self
            .idle_fetch_drain
            .take()
            .or_else(|| self.start_fetch_drain());
        self.stop_fetch_drain(drain).await;
    }
}

#[derive(Clone)]
pub(super) struct FetchDrainConfig {
    pub(super) transport: std::sync::Arc<eoka::cdp::Transport>,
    pub(super) failures: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(super) rules: Vec<InterceptRule>,
}

pub(super) struct FetchQueue {
    pub(super) receiver: tokio::sync::mpsc::Receiver<RequestPause>,
    pub(super) transport: std::sync::Arc<eoka::cdp::Transport>,
    pub(super) failures: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

pub(super) fn hand_off_fetch_pause(
    event: RequestPause,
    transport: std::sync::Arc<eoka::cdp::Transport>,
    failures: std::sync::Arc<std::sync::atomic::AtomicU64>,
) {
    failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            let config = FetchDrainConfig {
                transport,
                failures,
                rules: Vec::new(),
            };
            continue_fetch_pause(&config, &event, &CommandDispatch::new()).await;
        });
    }
}

impl Drop for FetchQueue {
    fn drop(&mut self) {
        self.receiver.close();
        while let Ok(event) = self.receiver.try_recv() {
            hand_off_fetch_pause(event, self.transport.clone(), self.failures.clone());
        }
    }
}

pub(super) struct FetchPauseOwner {
    pub(super) event: Option<RequestPause>,
    pub(super) config: FetchDrainConfig,
    pub(super) dispatch: CommandDispatch,
}

impl Drop for FetchPauseOwner {
    fn drop(&mut self) {
        if let Some(event) = self.event.take() {
            if self.dispatch.may_have_been_sent() {
                self.config
                    .failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            hand_off_fetch_pause(
                event,
                self.config.transport.clone(),
                self.config.failures.clone(),
            );
        }
    }
}

pub(super) async fn process_owned_fetch_pause(
    mut owner: FetchPauseOwner,
) -> Option<InterceptLogEntry> {
    let result = match tokio::time::timeout(
        std::time::Duration::from_secs(1),
        process_fetch_paused_event(
            &owner.config,
            owner.event.as_ref().unwrap(),
            &owner.dispatch,
        ),
    )
    .await
    {
        Ok(log) => log,
        Err(_) => {
            owner
                .config
                .failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            None
        }
    };
    if !owner.dispatch.may_have_been_sent() {
        owner.dispatch = CommandDispatch::new();
        continue_fetch_pause(
            &owner.config,
            owner.event.as_ref().unwrap(),
            &owner.dispatch,
        )
        .await;
    }
    owner.event.take();
    result
}

pub(super) struct FetchDrainHandle {
    pub(super) stop: tokio::sync::oneshot::Sender<()>,
    pub(super) join: tokio::task::JoinHandle<(FetchQueue, InterceptLog)>,
}

pub(super) async fn fetch_drain_until_stopped(
    mut rx: FetchQueue,
    config: FetchDrainConfig,
    mut stop_rx: tokio::sync::oneshot::Receiver<()>,
) -> (FetchQueue, InterceptLog) {
    let mut logs = InterceptLog::default();
    loop {
        let event = tokio::select! {
            biased;
            _ = &mut stop_rx => break,
            event = rx.receiver.recv() => match event {
                Some(event) => event,
                None => break,
            },
        };
        if let Some(log) = process_owned_fetch_pause(FetchPauseOwner {
            event: Some(event),
            config: config.clone(),
            dispatch: CommandDispatch::new(),
        })
        .await
        {
            logs.push(log);
        }
    }
    drain_fetch_receiver(&mut rx, &config, &mut logs).await;
    (rx, logs)
}

pub(super) async fn drain_fetch_receiver(
    rx: &mut FetchQueue,
    config: &FetchDrainConfig,
    logs: &mut InterceptLog,
) -> bool {
    let mut batch = tokio::task::JoinSet::new();
    for _ in 0..rx.receiver.len() {
        let Ok(event) = rx.receiver.try_recv() else {
            break;
        };
        let owner = FetchPauseOwner {
            event: Some(event),
            config: config.clone(),
            dispatch: CommandDispatch::new(),
        };
        batch.spawn(process_owned_fetch_pause(owner));
    }
    while let Some(result) = batch.join_next().await {
        match result {
            Ok(Some(log)) => logs.push(log),
            Ok(None) => {}
            Err(_) => {
                config
                    .failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    rx.receiver.is_closed() && rx.receiver.is_empty()
}

pub(super) async fn process_fetch_paused_event(
    config: &FetchDrainConfig,
    event: &RequestPause,
    dispatch: &CommandDispatch,
) -> Option<InterceptLogEntry> {
    let params = &event.params;
    let request_id = params
        .get("requestId")
        .and_then(|v| v.as_str())?
        .to_string();
    let req_field = |name: &str| -> Option<String> {
        params.get("request")?.get(name)?.as_str().map(String::from)
    };
    let url = req_field("url").unwrap_or_default();
    let method = req_field("method").unwrap_or_else(|| "GET".to_string());
    let post_data = req_field("postData");
    let network_id = params
        .get("networkId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let session_id = event.session_id.clone();

    let matched = config.rules.iter().find(|rule| rule.matches_url(&url));
    let Some(rule) = matched else {
        continue_fetch_pause(config, event, dispatch).await;
        return None;
    };

    let mut capture_failed = false;
    if let Some(ref path) = rule.capture_path {
        let body = json!({
            "url": &url,
            "method": &method,
            "postData": &post_data,
            "headers": params.get("request").and_then(|r| r.get("headers")),
        });
        if intercept_files::capture(path, body).await.is_err() {
            config
                .failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            capture_failed = true;
            eprintln!("[eoka] interception capture failed");
        }
    }

    let action = if let Some(ref path) = rule.respond_path {
        if let Ok(body) = intercept_files::read(path).await {
            let fulfilled = config
                .transport
                .send_to_session_with_dispatch::<_, serde_json::Value>(
                    &session_id,
                    "Fetch.fulfillRequest",
                    &json!({
                        "requestId": request_id,
                        "responseCode": rule.respond_status,
                        "body": base64::engine::general_purpose::STANDARD
                            .encode(&body),
                    }),
                    dispatch,
                )
                .await;
            if fulfilled.is_err() {
                config
                    .failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                "response unconfirmed"
            } else {
                "responded"
            }
        } else {
            config
                .failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if continue_fetch_pause(config, event, dispatch).await {
                "continued (response read failed)"
            } else {
                "continuation and response read failed"
            }
        }
    } else {
        if continue_fetch_pause(config, event, dispatch).await {
            "continued"
        } else {
            "continuation failed"
        }
    };

    Some(InterceptLogEntry {
        rule_id: rule.id,
        url,
        method,
        has_body: post_data.is_some(),
        action: if capture_failed {
            format!("{action}; capture failed")
        } else {
            action.to_string()
        },
        session_id: Some(session_id),
        network_id,
        network_entry_id: None,
    })
}

pub(super) async fn continue_fetch_pause(
    config: &FetchDrainConfig,
    event: &RequestPause,
    dispatch: &CommandDispatch,
) -> bool {
    let confirmed = matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            event.continue_request_with_dispatch(&config.transport, dispatch)
        )
        .await,
        Ok(Ok(()))
    );
    if !confirmed {
        config
            .failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    confirmed
}
