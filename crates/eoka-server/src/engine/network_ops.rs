use super::*;

impl Engine {
    pub(super) fn network_recorder(&mut self) -> Result<NetworkRecorder, String> {
        if let Some(recorder) = self.network.clone() {
            return Ok(recorder);
        }
        let session = self.require_tab()?.page.session().clone();
        let recorder = NetworkRecorder::spawn(
            format!("session:{}", self.session_name),
            session.transport().clone(),
        );
        self.network = Some(recorder.clone());
        Ok(recorder)
    }

    pub(super) async fn sync_network_recording(&mut self) -> Result<(), String> {
        let Some(recorder) = self.network.clone() else {
            return Ok(());
        };
        if !recorder.is_active().await {
            return Ok(());
        }
        let session = self.require_tab()?.page.session().clone();
        recorder.update_session(&session).await
    }

    pub(super) async fn sync_current_target_features(&mut self) -> Result<(), String> {
        self.sync_fetch_interception().await?;
        self.sync_network_recording().await
    }

    pub async fn is_network_recording(&self) -> bool {
        match self.network.as_ref() {
            Some(recorder) => recorder.is_active().await,
            None => false,
        }
    }

    pub(super) async fn cmd_network_record_start(
        &mut self,
        args: &NetworkRecordStartArgs,
    ) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        if self
            .state
            .as_ref()
            .and_then(|state| state.current_tab())
            .is_none()
        {
            self.require_state_mut()?.ensure_blank_tab().await?;
        }
        let patterns = Some(args.patterns.clone())
            .filter(|patterns| !patterns.is_empty())
            .unwrap_or_else(|| NetworkConfig::default().patterns);
        let max_body_bytes = args.max_body_bytes;
        let clear = args.clear;
        let config = NetworkConfig {
            patterns,
            capture_bodies: !args.no_bodies,
            max_body_bytes,
            ..NetworkConfig::default()
        };
        let session = self.require_tab()?.page.session().clone();
        let recorder = self.network_recorder()?;
        if clear {
            let _ = recorder.clear().await?;
        }
        Ok(typed(recorder.start(&session, config).await?))
    }

    pub(super) async fn cmd_network_record_stop(&mut self) -> Result<Response, EngineError> {
        let Some(recorder) = self.network.clone() else {
            return Ok(typed(self.network_idle()));
        };
        let session = self
            .state
            .as_ref()
            .and_then(|state| state.current_tab())
            .map(|tab| tab.page.session().clone());
        Ok(typed(recorder.stop(session).await?))
    }

    pub(super) async fn cmd_network_record_status(&mut self) -> Result<Response, EngineError> {
        match self.network.as_ref() {
            Some(recorder) => Ok(typed(recorder.status().await)),
            None => Ok(typed(self.network_idle())),
        }
    }

    pub(super) async fn cmd_network_log(
        &mut self,
        args: &NetworkLogArgs,
    ) -> Result<Response, EngineError> {
        let limit = args.limit.unwrap_or(100).min(1000);
        let Some(recorder) = self.network.as_ref() else {
            return Ok(typed(eoka_protocol::NetworkLog {
                meta: self.network_empty_meta(),
                count: 0,
                entries: Vec::new(),
                filters: eoka_protocol::NetworkLogFilters {
                    limit,
                    pattern: args.pattern.clone(),
                    method: args.method.clone(),
                    status: args.status,
                    since: args.since,
                    compact: args.compact,
                },
            }));
        };
        let pattern = args.pattern.as_deref();
        let method = args.method.as_deref();
        let status = args.status;
        let since = args.since;
        let compact = args.compact;
        Ok(typed(
            recorder
                .log(limit, pattern, method, status, since, compact)
                .await,
        ))
    }

    pub(super) async fn cmd_network_show(
        &mut self,
        args: &NetworkShowArgs,
    ) -> Result<Response, EngineError> {
        let id = args.id;
        let body = args.body;
        let max_body = args.max_body;
        let Some(recorder) = self.network.as_ref() else {
            return Err(format!("Network entry #{} not found", id).into());
        };
        if body {
            recorder.settle(5000).await;
        }
        recorder
            .show(id, body, max_body)
            .await
            .map(typed)
            .ok_or_else(|| format!("Network entry #{} not found", id).into())
    }

    pub(super) async fn cmd_network_export(
        &mut self,
        args: &NetworkExportArgs,
    ) -> Result<Response, EngineError> {
        let path = args.path.as_str();
        let format = args.format.as_str();
        let settle_ms = args.settle_ms;
        let Some(recorder) = self.network.as_ref() else {
            return Err("No network recorder has been started".into());
        };
        Ok(typed(
            recorder
                .export(std::path::Path::new(path), format, settle_ms)
                .await?,
        ))
    }

    pub(super) async fn cmd_network_wait(
        &mut self,
        args: &NetworkWaitArgs,
    ) -> Result<Response, EngineError> {
        let Some(recorder) = self.network.as_ref() else {
            return Err("No network recorder has been started".into());
        };
        let timeout = args.timeout.unwrap_or(10_000);
        let pattern = args.pattern.as_deref();
        let method = args.method.as_deref();
        let status = args.status;
        let since = args.since;
        let include_existing = args.include_existing;
        recorder
            .wait(pattern, method, status, since, include_existing, timeout)
            .await
            .map(typed)
            .ok_or_else(|| {
                format!("Timed out waiting {}ms for matching network entry", timeout).into()
            })
    }

    pub(super) async fn cmd_network_clear(&mut self) -> Result<Response, EngineError> {
        match self.network.as_ref() {
            Some(recorder) => Ok(typed(recorder.clear().await?)),
            None => Ok(typed(eoka_protocol::NetworkCleared {
                cleared: true,
                meta: self.network_empty_meta(),
            })),
        }
    }

    fn network_idle(&self) -> eoka_protocol::NetworkRecording {
        eoka_protocol::NetworkRecording {
            meta: self.network_empty_meta(),
            status: network::idle_status(),
        }
    }

    pub(super) fn network_empty_meta(&self) -> eoka_protocol::NetworkMeta {
        eoka_protocol::NetworkMeta {
            namespace: format!("session:{}", self.session_name),
            active: false,
            entry_count: 0,
            in_flight: 0,
            last_id: 0,
            next_since: 0,
            body_pending: 0,
            body_failed: 0,
            warnings: Vec::new(),
            suggested_commands: vec![
                "eoka network record start".into(),
                "eoka network log --since 0 --compact".into(),
            ],
        }
    }

    pub(super) async fn cmd_intercept_add(
        &mut self,
        args: &InterceptAddArgs,
    ) -> Result<Response, EngineError> {
        let url_pattern = args.url_pattern.clone();
        let capture = args.capture.as_deref().map(std::path::PathBuf::from);
        let respond = args.respond.as_deref().map(std::path::PathBuf::from);
        let status = args.status;

        let id = self
            .intercept
            .add_rule(url_pattern.clone(), capture, respond, status);
        self.sync_fetch_interception().await?;

        Ok(Response::ok_text(format!(
            "Added intercept rule #{} for \"{}\"",
            id, url_pattern
        )))
    }

    pub(super) async fn cmd_intercept_list(&mut self) -> Result<Response, EngineError> {
        self.drain_fetch_events().await;
        Ok(typed(self.intercept.list()))
    }

    pub(super) async fn cmd_intercept_remove(
        &mut self,
        args: &IdArgs,
    ) -> Result<Response, EngineError> {
        let id = args.id.as_str();
        if id == "all" {
            self.intercept.remove_all();
            self.sync_fetch_interception().await?;
            Ok(Response::ok_text("Removed all intercept rules"))
        } else {
            let id_num: usize = id.parse().map_err(|_| "Invalid rule ID")?;
            if self.intercept.remove_rule(id_num) {
                self.sync_fetch_interception().await?;
                Ok(Response::ok_text(format!("Removed rule #{}", id_num)))
            } else {
                Err(format!("Rule #{} not found", id_num).into())
            }
        }
    }

    pub(super) async fn cmd_intercept_log(
        &mut self,
        args: &ClearFlagArgs,
    ) -> Result<Response, EngineError> {
        self.drain_fetch_events().await;
        if let Some(recorder) = self.network.clone() {
            self.intercept
                .resolve_network_links(|session_id, network_id, url, method| {
                    let recorder = recorder.clone();
                    let session_id = session_id.map(str::to_string);
                    let network_id = network_id.map(str::to_string);
                    let url = url.to_string();
                    let method = method.to_string();
                    async move {
                        recorder
                            .entry_id_for_network(
                                session_id.as_deref(),
                                network_id.as_deref(),
                                &url,
                                &method,
                            )
                            .await
                    }
                })
                .await;
        }
        let clear = args.clear;
        let result = self.intercept.log();
        if clear {
            self.intercept.clear_log();
        }
        Ok(typed(result))
    }
}
