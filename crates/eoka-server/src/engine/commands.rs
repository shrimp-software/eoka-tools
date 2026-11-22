use super::*;

impl Engine {
    pub(super) async fn cmd_open(&mut self, args: &OpenArgs) -> Result<Response, EngineError> {
        let requested_url = args.url.clone();
        let open_state = args
            .load_state
            .as_deref()
            .map(read_saved_state_file)
            .transpose()?;
        let url = resolve_open_url_for_state(&requested_url, open_state.as_ref())?;
        let state_init_js = open_state
            .as_ref()
            .filter(|saved| state_should_prime_open(saved, &url))
            .and_then(build_storage_seed_js);
        self.ensure_browser().await?;

        let headers: Option<HashMap<String, String>> = args
            .headers
            .as_ref()
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let user_agent = args.user_agent.as_deref();
        let bypass_csp = args.bypass_csp;
        let inject_js = args.inject_js.as_deref();
        let script_enabled = self.script_enabled_for(&url);
        let _has_extras = headers.is_some()
            || user_agent.is_some()
            || bypass_csp
            || inject_js.is_some()
            || open_state.is_some()
            || state_init_js.is_some()
            || script_enabled.is_some();

        let operations = self.operations.clone();
        let operation_id = args
            .operation
            .operation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let options =
            crate::operations::commands::options(&serde_json::to_value(&args.operation)?)?;
        let budget = options.timeout_ms;
        let drain = self.start_fetch_drain();
        let active_recorder = self.network.clone();
        let result = async {
            let state = self.require_state_mut()?;

            if state.is_live {
                state.new_tab(None).await.map_err(|e| e.to_string())?;
                if let Some(recorder) = active_recorder.as_ref() {
                    if recorder.is_active().await {
                        let session = state.current_tab().ok_or("No tab")?.page.session().clone();
                        recorder.update_session(&session).await?;
                    }
                }
            }

            {
                let tab = if state.current_tab_id.is_some() {
                    state.current_tab().ok_or("No tab")?
                } else {
                    state.ensure_blank_tab().await.map_err(|e| e.to_string())?
                };
                blockers::watch(&tab.page);
                if let Some(saved) = open_state.as_ref() {
                    restore_cookies(&tab.page, saved).await?;
                }
                if let Some(ua) = user_agent {
                    tab.page
                        .set_user_agent(ua)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if bypass_csp {
                    tab.page
                        .set_bypass_csp(true)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if let Some(ref h) = headers {
                    tab.page
                        .set_extra_headers(h.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if let Some(js) = inject_js {
                    let source = if js.ends_with(".js") {
                        std::fs::read_to_string(js)
                            .map_err(|e| format!("inject_js read error: {}", e))?
                    } else {
                        js.to_string()
                    };
                    tab.page
                        .session()
                        .send::<_, serde_json::Value>(
                            "Page.addScriptToEvaluateOnNewDocument",
                            &serde_json::json!({ "source": source }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                }
                let state_script_id = if let Some(source) = state_init_js.as_deref() {
                    let response: Value = tab
                        .page
                        .session()
                        .send(
                            "Page.addScriptToEvaluateOnNewDocument",
                            &json!({ "source": source }),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    response
                        .get("identifier")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                } else {
                    None
                };
                if let Some(enabled) = script_enabled {
                    tab.page
                        .set_javascript_enabled(enabled)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                tab.invalidate();
                let record = operations
                    .submit(
                        tab.page.clone(),
                        operation_id.clone(),
                        eoka::Action::Navigate {
                            url: url.clone(),
                            referrer: None,
                        },
                        options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let nav_result = if matches!(record.state, eoka_protocol::OperationState::Running) {
                    blockers::wait_unless_dialog(&operations, &tab.page, &operation_id, budget)
                        .await
                } else {
                    Ok(record)
                };
                if let Some(identifier) = state_script_id {
                    let _ = tab
                        .page
                        .session()
                        .send::<_, Value>(
                            "Page.removeScriptToEvaluateOnNewDocument",
                            &json!({ "identifier": identifier }),
                        )
                        .await;
                }
                if headers.is_some() {
                    let _ = tab.page.clear_extra_headers().await;
                }
                nav_result.map_err(|e| e.to_string())
            }
        }
        .await;
        self.stop_fetch_drain(drain).await;
        if let Some(enabled) = script_enabled {
            self.script_policy.note_applied(enabled);
        }
        let record = result?;
        self.sync_current_target_features().await?;
        Ok(Response::action_result(record))
    }

    pub(super) async fn cmd_snapshot(
        &mut self,
        args: &SnapshotArgs,
    ) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let include_all = args.all;
        let tab = self.require_tab()?;

        let result = snapshot::snapshot(&tab.page, include_all).await?;

        Ok(Response::ok_text(result.tree_text))
    }

    pub(super) async fn cmd_observe(
        &mut self,
        args: &ObserveArgs,
    ) -> Result<Response, EngineError> {
        let (tab, viewport_only) = self.tab_with_config()?;
        let (elements, data) = observe::inspect(&tab.page, viewport_only, args).await?;
        tab.set_elements(elements);
        Ok(typed(data))
    }

    pub(super) async fn cmd_screenshot(
        &mut self,
        capture: ScreenshotArgs,
    ) -> Result<Response, EngineError> {
        capture.validate().map_err(|error| error.message)?;
        if !capture.annotate {
            let page = self.require_tab()?.page.clone();
            return Ok(typed(super::page_ops::screenshot(&page, &capture).await?));
        }
        let (tab, viewport_only) = self.tab_with_config()?;
        let elements = &observed_elements(&tab, viewport_only).await?;
        let page = &tab.page;
        let mut result = super::page_ops::publish_screenshot(&capture, async {
            Ok(annotate::annotated_screenshot(page, elements).await?)
        })
        .await?;
        let mut list = String::new();
        for el in elements {
            let _ = writeln!(list, "{}", el);
        }
        match &mut result {
            eoka_protocol::ScreenshotResult::Inline(inline) => inline.elements = Some(list),
            eoka_protocol::ScreenshotResult::File(file) => file.elements = Some(list),
        }
        Ok(typed(result))
    }

    pub(super) async fn cmd_emulate(
        &mut self,
        args: &EmulateArgs,
    ) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let tab = self.require_tab()?;
        if args.reset || args.desktop {
            let page = tab.page.clone();
            return Ok(super::page_ops::viewport(&page, args).await?);
        }
        let session = tab.page.session();
        session
            .send::<_, Value>(
                "Emulation.setDeviceMetricsOverride",
                &json!({
                    "width": args.width,
                    "height": args.height,
                    "deviceScaleFactor": args.dpr,
                    "mobile": true,
                }),
            )
            .await?;
        let _ = session
            .send::<_, Value>(
                "Emulation.setTouchEmulationEnabled",
                &json!({ "enabled": true, "maxTouchPoints": 5 }),
            )
            .await;
        Ok(typed(eoka_protocol::Viewport {
            width: args.width,
            height: args.height,
            dpr: args.dpr,
            mobile: true,
        }))
    }

    pub(super) async fn cmd_recording_start(
        &mut self,
        args: RecordingStartArgs,
    ) -> Result<Response, EngineError> {
        let state = self
            .state
            .as_ref()
            .ok_or("No browser open. Use 'open' first.")?;
        let page_id = state.current_tab_id.clone().ok_or("Open a tab first")?;
        let page = self.require_tab()?.page.clone();
        let params = eoka_protocol::sidecar::RecordingStartParams {
            page_id,
            recording_id: args
                .recording_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            path: args.path,
            max_width: args.max_width,
            max_height: args.max_height,
            fps: args.fps,
        };
        let record = self.recordings.start(params, Ok(page)).await;
        recording_response(record.and_then(|record| {
            serde_json::to_value(record)
                .map_err(|e| eoka_protocol::sidecar::ServerError::internal(e.to_string()))
        }))
    }

    pub(super) async fn cmd_recording_control(
        &mut self,
        control: crate::recordings::RecordingControl,
        args: &RecordingIdArgs,
    ) -> Result<Response, EngineError> {
        let params = json!({"recordingId": args.recording_id});
        recording_response(crate::recordings::control(&self.recordings, control, params).await)
    }

    pub(super) async fn cmd_dialog(
        &mut self,
        accept: Option<Option<&str>>,
    ) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        tab.invalidate();
        let page = tab.page.clone();
        let reply = blockers::answer(&self.operations, &page, accept).await?;
        Ok(Response::ok_text(reply))
    }

    pub(super) async fn cmd_captcha_detect(&mut self) -> Result<Response, EngineError> {
        let page = &self.require_tab()?.page;
        let info = crate::captcha::detect_captcha_on_page(page).await;
        Ok(typed(eoka_protocol::CaptchaDetection {
            detected: info.is_some(),
            captcha_type: info.as_ref().map(|info| info.captcha_type.clone()),
            sitekey: info.map(|info| info.sitekey),
        }))
    }

    pub(super) async fn cmd_find(&mut self, args: &TextArgs) -> Result<Response, EngineError> {
        let text = args.text.as_str();
        let (tab, viewport_only) = self.tab_with_config()?;
        let elements = observed_elements(&tab, viewport_only).await?;

        let needle = text.to_lowercase();
        let matches: Vec<_> = elements
            .iter()
            .filter(|e| {
                e.text.to_lowercase().contains(&needle)
                    || e.placeholder
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(&needle))
            })
            .collect();

        if matches.is_empty() {
            Ok(Response::ok_text(format!(
                "No elements found matching \"{}\"",
                text
            )))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            Ok(Response::ok_text(out))
        }
    }

    pub(super) async fn cmd_scroll(&mut self, args: &TargetArgs) -> Result<Response, EngineError> {
        let target_str = args.target.as_str();
        let tab = self.require_tab()?;

        match target_str {
            "up" => {
                tab.page
                    .execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)")
                    .await
            }
            "down" => {
                tab.page
                    .execute_sync("window.scrollBy(0, window.innerHeight * 0.8)")
                    .await
            }
            "top" => tab.page.execute_sync("window.scrollTo(0, 0)").await,
            "bottom" => {
                tab.page
                    .execute_sync("window.scrollTo(0, document.body.scrollHeight)")
                    .await
            }
            _ => {
                let target = self
                    .agent
                    .lock()
                    .await
                    .resolve_target(&tab.page, target_str)
                    .await?;
                let id = uuid::Uuid::new_v4().to_string();
                self.operations
                    .submit(
                        tab.page.clone(),
                        id.clone(),
                        eoka::Action::ScrollIntoView { target },
                        Default::default(),
                    )
                    .await?;
                return Ok(Response::action_result(
                    self.operations.wait(&id, 30_000).await?,
                ));
            }
        }?;
        Ok(Response::ok_text(format!("Scrolled {}", target_str)))
    }

    pub(super) async fn cmd_captcha_inject(
        &mut self,
        args: &CaptchaInjectArgs,
    ) -> Result<Response, EngineError> {
        self.ensure_browser().await?;
        let token = args.token.as_str();
        let captcha_type = args.captcha_type.as_str();
        let callback = args.callback.as_deref();
        let click_after = args.click_after.as_deref().map(str::to_string);
        let kind = parse_captcha_inject_kind(captcha_type).map_err(|error| error.to_string())?;
        let js = build_captcha_inject_js(token, kind, callback);
        let result: String = {
            let tab = self.require_tab()?;
            tab.page.evaluate(&js).await?
        };
        let mut injected = eoka_protocol::CaptchaInjectResult {
            injection: serde_json::from_str(&result)?,
            click_after: None,
            next_action: None,
        };
        if let Some(target) = click_after {
            let (tab, viewport_only) = self.tab_with_config()?;
            let clicked = click_once(&tab, &target, viewport_only).await?;
            injected.click_after = Some(eoka_protocol::ClickAfter { target, clicked });
        } else {
            injected.next_action = Some(
                "If the page does not advance after callbacks run, click the continuation control again or pass --click-after.".into(),
            );
        }
        Ok(typed(injected))
    }

    pub(super) async fn cmd_fetch(&mut self, args: FetchArgs) -> Result<Response, EngineError> {
        let page = self.require_tab()?.page.clone();
        let drain = self.start_fetch_drain();
        let remedies = blockers::Remedies::new(Some(&self.session_name));
        let name = self.named(&remedies, "fetch", &Value::Null);
        let response = blockers::guard(
            Some(&page),
            remedies,
            &name,
            blockers::Policy::Script,
            None,
            async { Ok::<_, EngineError>(crate::browser_fetch::fetch(&page, args).await) },
        )
        .await;
        self.stop_fetch_drain(drain).await;
        response
    }

    pub(super) async fn cmd_headers(
        &mut self,
        args: &HeadersArgs,
    ) -> Result<Response, EngineError> {
        let headers: HashMap<String, String> =
            serde_json::from_str(&args.headers_json).map_err(|_| "Invalid headers JSON")?;

        let tab = self.require_tab()?;
        tab.page.set_extra_headers(headers.clone()).await?;
        Ok(Response::ok_text(format!(
            "Set {} extra headers",
            headers.len()
        )))
    }

    pub(super) async fn cmd_console(
        &mut self,
        args: &ConsoleArgs,
    ) -> Result<Response, EngineError> {
        let clear = args.clear;
        let level = args.level.as_deref();

        let tab = self.require_tab()?;
        ensure_console_capture(&tab).await?;

        let js = match level {
            Some(l) => format!(
                "JSON.stringify((__eoka_console || []).filter(e => e.level === {}))",
                json_str(l)
            ),
            None => "JSON.stringify(__eoka_console || [])".into(),
        };
        let parsed = eval_json_or_string(&tab.page, &js).await?;
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_console = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

    pub(super) async fn cmd_errors(
        &mut self,
        args: &ClearFlagArgs,
    ) -> Result<Response, EngineError> {
        let clear = args.clear;
        let tab = self.require_tab()?;
        ensure_console_capture(&tab).await?;

        let parsed = eval_json_or_string(&tab.page, "JSON.stringify(__eoka_errors || [])").await?;
        if clear {
            let _: String = tab
                .page
                .evaluate_sync("__eoka_errors = []")
                .await
                .unwrap_or_default();
        }
        Ok(Response::ok(parsed))
    }

    pub(super) async fn cmd_spa_info(&mut self) -> Result<Response, EngineError> {
        let tab = self.require_tab()?;
        let info = crate::spa::detect_router(&tab.page)
            .await
            .map_err(|e: eoka::Error| e.to_string())?;
        Ok(typed(eoka_protocol::SpaInfo {
            router_type: info.router_type.into(),
            current_path: info.current_path,
            can_navigate: info.can_navigate,
        }))
    }

    pub(super) async fn cmd_spa_navigate(
        &mut self,
        args: &PathStringArgs,
    ) -> Result<Response, EngineError> {
        let path = args.path.as_str();
        let drain = self.start_fetch_drain();
        let result = async {
            let tab = self.require_tab()?;
            let info = crate::spa::detect_router(&tab.page)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            crate::spa::spa_navigate(&tab.page, &info.router_type, path)
                .await
                .map_err(|e: eoka::Error| e.to_string())?;
            let _ = wait_for_stable(&tab.page).await;
            tab.invalidate();
            tab.page.url().await.map_err(|e| e.to_string())
        }
        .await;
        self.stop_fetch_drain(drain).await;
        let url = result?;
        Ok(Response::ok_text(format!("SPA navigated to: {}", url)))
    }

    pub(super) async fn cmd_fake_camera(
        &mut self,
        args: &FakeCameraArgs,
    ) -> Result<Response, EngineError> {
        let file_path = args.file.as_str();
        let loop_video = args.loop_video;

        let data = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read video file '{}': {}", file_path, e))?;

        let mime = if file_path.ends_with(".webm") {
            "video/webm"
        } else {
            "video/mp4"
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);

        let template = include_str!("js/fake_camera.js");
        let js = template
            .replace("/*VIDEO_DATA*/", &b64)
            .replace("/*MIME_TYPE*/", mime)
            .replace("/*LOOP*/", if loop_video { "true" } else { "false" });

        self.ensure_browser().await?;
        let tab = self.require_tab()?;

        tab.page
            .session()
            .send::<_, Value>(
                "Page.addScriptToEvaluateOnNewDocument",
                &json!({ "source": js }),
            )
            .await?;
        let _: String = tab.page.evaluate_sync(&js).await.unwrap_or_default();

        let _ = tab
            .page
            .session()
            .send::<_, Value>(
                "Browser.grantPermissions",
                &json!({ "permissions": ["videoCapture", "audioCapture"] }),
            )
            .await;

        Ok(Response::ok_text(format!(
            "Fake camera injected ({}KB {}, loop={})",
            data.len() / 1024,
            mime,
            loop_video
        )))
    }

    pub(super) async fn cmd_close(&mut self) -> Result<Response, EngineError> {
        self.downloads.stop().await;
        if let Some(recorder) = self.network.take() {
            let session = self
                .state
                .as_ref()
                .and_then(|state| state.current_tab())
                .map(|tab| tab.page.session().clone());
            let _ = recorder.stop(session).await;
        }
        self.disable_all_fetch_sessions().await;
        if let Err(error) = self.recordings.shutdown().await {
            eprintln!("[eoka] recording cleanup failed: {}", error.message);
        }
        if let Some(state) = self.state.take() {
            crate::capture::quiet().await;
            state.close().await?;
        }
        Ok(Response::ok_text("Browser closed"))
    }
}

async fn observed_elements(
    tab: &crate::tabs::Tab,
    viewport_only: bool,
) -> Result<Vec<crate::InteractiveElement>, EngineError> {
    let cached = tab.elements();
    if !cached.is_empty() {
        return Ok(cached);
    }
    let elements = observe::observe(&tab.page, viewport_only).await?;
    tab.set_elements(elements.clone());
    Ok(elements)
}
