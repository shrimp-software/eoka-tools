mod error;
mod helpers;
mod operations;
mod state;
mod types;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde_json::Value;
use std::fmt::Write;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka_protocol::{input_schema_for_cmd, operation_by_cmd};
use eoka_server::{annotate, captcha, observe, snapshot, spa};

use error::{internal, invalid, is_transport_error_msg, AgentError};
use helpers::{
    auto_observe_if_needed, element_list, ensure_console_capture, resolve_js, resolve_target,
    text_ok, title_nonblocking,
};
use state::BrowserState;
use types::*;

const MCP_PROTOCOL_TOOL_ROUTES: &[(&str, &str)] = &[
    ("action_perform", "action_perform"),
    ("operation_get", "operation_get"),
    ("operation_list", "operation_list"),
    ("operation_wait", "operation_wait"),
    ("operation_cancel", "operation_cancel"),
    ("download_start", "download_start"),
    ("download_list", "download_list"),
    ("download_get", "download_get"),
    ("download_wait", "download_wait"),
    ("download_cancel", "download_cancel"),
    ("fetch", "fetch"),
    ("snapshot", "snapshot"),
    ("observe", "observe"),
    ("click", "click"),
    ("fill", "fill"),
    ("select", "select"),
    ("hover", "hover"),
    ("type_key", "key"),
    ("mouse_down", "mouse_down"),
    ("mouse_move", "mouse_move"),
    ("mouse_up", "mouse_up"),
    ("key_down", "key_down"),
    ("key_up", "key_up"),
    ("release_all_inputs", "release_all_inputs"),
    ("scroll", "scroll"),
    ("find_text", "find"),
    ("new_tab", "tab_new"),
    ("switch_tab", "tab_switch"),
    ("close_tab", "tab_close"),
    ("spa_navigate", "spa_navigate"),
    ("save_state", "save_state"),
];

fn apply_protocol_tool_metadata(router: &mut ToolRouter<EokaServer>) {
    for (route_name, command) in MCP_PROTOCOL_TOOL_ROUTES {
        let operation = operation_by_cmd(command).expect("MCP route must use a cataloged command");
        let route = router
            .map
            .get_mut(*route_name)
            .expect("MCP protocol route must be registered");
        route.attr.description = Some(operation.description.into());
        route.attr.input_schema = rmcp::model::object(input_schema_for_cmd(operation.cmd)).into();
        route.attr.annotations = Some(
            ToolAnnotations::new()
                .read_only(operation.read_only)
                .destructive(operation.destructive),
        );
    }
}

#[derive(Clone)]
pub struct EokaServer {
    state: Arc<Mutex<Option<BrowserState>>>,
    headless: bool,
    tool_router: ToolRouter<EokaServer>,
    downloads: eoka_server::downloads::DownloadManager,
    operations: eoka_server::operations::OperationManager,
}

impl EokaServer {
    fn core_mouse_button(button: eoka_protocol::MouseButton) -> eoka_server::eoka::MouseButton {
        match button {
            eoka_protocol::MouseButton::Left => eoka_server::eoka::MouseButton::Left,
            eoka_protocol::MouseButton::Middle => eoka_server::eoka::MouseButton::Middle,
            eoka_protocol::MouseButton::Right => eoka_server::eoka::MouseButton::Right,
            eoka_protocol::MouseButton::Back => eoka_server::eoka::MouseButton::Back,
            eoka_protocol::MouseButton::Forward => eoka_server::eoka::MouseButton::Forward,
        }
    }

    #[allow(dead_code)]
    async fn lock_browser(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<BrowserState>>, ErrorData> {
        let guard = self.state.lock().await;
        if guard.is_none() {
            return Err(ErrorData::from(AgentError::NoBrowser));
        }
        Ok(guard)
    }

    async fn ensure_browser(&self) -> Result<(), ErrorData> {
        let mut guard = self.state.lock().await;
        if guard.as_ref().map(|s| s.unhealthy).unwrap_or(false) {
            eprintln!("[eoka-mcp] browser unhealthy, relaunching...");
            self.operations.stop().await;
            self.downloads.stop().await;
            if let Some(state) = guard.take() {
                let _ = state.close().await;
            }
        }
        if guard.is_none() {
            let state = BrowserState::new(self.headless).await.map_err(internal)?;
            *guard = Some(state);
        }
        Ok(())
    }

    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        if is_transport_error_msg(&msg) {
            eprintln!("[eoka-mcp] connection lost, marking unhealthy: {}", msg);
            let mut guard = self.state.lock().await;
            if let Some(state) = guard.as_mut() {
                state.unhealthy = true;
            }
            ErrorData::internal_error(
                format!("{} (connection lost - will relaunch on next call)", msg),
                None::<Value>,
            )
        } else {
            internal(e)
        }
    }
}

#[tool_router]
impl EokaServer {
    pub fn new() -> Self {
        let headless = std::env::var("EOKA_HEADLESS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        let mut tool_router = Self::tool_router();
        apply_protocol_tool_metadata(&mut tool_router);
        Self {
            state: Arc::new(Mutex::new(None)),
            headless,
            tool_router,
            downloads: Default::default(),
            operations: Default::default(),
        }
    }

    #[tool]
    async fn action_perform(
        &self,
        req: Parameters<eoka_protocol::ActionPerformArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.perform_command(
            "action_perform",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }
    #[tool]
    async fn operation_get(
        &self,
        req: Parameters<eoka_protocol::OperationIdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        protocol_tool_response(
            self.operations
                .query(
                    "operation_get",
                    &serde_json::to_value(req.0).map_err(internal)?,
                )
                .await
                .ok_or_else(|| internal("Unknown operation"))?,
        )
    }
    #[tool]
    async fn operation_list(
        &self,
        req: Parameters<eoka_protocol::OperationListArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        protocol_tool_response(
            self.operations
                .query(
                    "operation_list",
                    &serde_json::to_value(req.0).map_err(internal)?,
                )
                .await
                .ok_or_else(|| internal("Unknown operation"))?,
        )
    }
    #[tool]
    async fn operation_wait(
        &self,
        req: Parameters<eoka_protocol::OperationWaitArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        protocol_tool_response(
            self.operations
                .query(
                    "operation_wait",
                    &serde_json::to_value(req.0).map_err(internal)?,
                )
                .await
                .ok_or_else(|| internal("Unknown operation"))?,
        )
    }
    #[tool]
    async fn operation_cancel(
        &self,
        req: Parameters<eoka_protocol::OperationIdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        protocol_tool_response(
            self.operations
                .query(
                    "operation_cancel",
                    &serde_json::to_value(req.0).map_err(internal)?,
                )
                .await
                .ok_or_else(|| internal("Unknown operation"))?,
        )
    }
    #[tool]
    async fn download_start(
        &self,
        req: Parameters<DownloadStartRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.downloads.existing(&req.0).await {
            Ok(Some(record)) => {
                return protocol_tool_response(eoka_server::downloads::start_response(Ok(record)))
            }
            Ok(None) => {}
            Err(error) => return protocol_tool_response(eoka_server::downloads::rejected(error)),
        }
        self.downloads
            .restore()
            .await
            .map_err(|e| internal(e.message))?;
        req.0.validate().map_err(invalid)?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        if self.downloads.is_active() {
            return protocol_tool_response(eoka_protocol::Response::err_detail(
                eoka_server::downloads::failure("download_busy", "One download is already active"),
            ));
        }
        let viewport = state.config.viewport_only;
        let browser = state.browser.clone();
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let selector = if let Some(target) = req.0.click.as_deref() {
            match eoka_server::downloads::resolve_click(
                &tab.page,
                &mut tab.elements,
                &tab.snapshot_refs,
                viewport,
                target,
            )
            .await
            {
                Ok(selector) => {
                    tab.invalidate();
                    Some(selector)
                }
                Err(error) => {
                    return protocol_tool_response(eoka_server::downloads::rejected(error))
                }
            }
        } else {
            None
        };
        let result = self
            .downloads
            .start(&browser, &tab.page, req.0, selector)
            .await;
        protocol_tool_response(eoka_server::downloads::start_response(result))
    }

    #[tool]
    async fn download_list(&self) -> Result<CallToolResult, ErrorData> {
        self.downloads
            .restore()
            .await
            .map_err(|e| internal(e.message))?;
        protocol_tool_response(eoka_protocol::Response::ok(
            serde_json::to_value(self.downloads.list()).map_err(internal)?,
        ))
    }

    #[tool]
    async fn download_get(
        &self,
        req: Parameters<DownloadIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.downloads
            .restore()
            .await
            .map_err(|e| internal(e.message))?;
        protocol_tool_response(match self.downloads.get(&req.0.id) {
            Ok(job) => eoka_server::downloads::record_response(job.snapshot()),
            Err(error) => eoka_protocol::Response::err_detail(error),
        })
    }

    #[tool]
    async fn download_cancel(
        &self,
        req: Parameters<DownloadIdRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.downloads
            .restore()
            .await
            .map_err(|e| internal(e.message))?;
        protocol_tool_response(eoka_server::downloads::start_response(
            self.downloads.cancel(&req.0.id),
        ))
    }

    #[tool]
    async fn download_wait(
        &self,
        req: Parameters<DownloadWaitRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.downloads
            .restore()
            .await
            .map_err(|e| internal(e.message))?;
        protocol_tool_response(self.downloads.wait(&req.0.id, req.0.timeout_ms).await)
    }

    #[tool(description = "List all open browser tabs. Returns tab IDs, titles, and URLs.")]
    async fn list_tabs(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard.as_ref().unwrap();

        let tabs = state.list_tabs().await.map_err(internal)?;
        let current_id = state.current_tab_id.as_deref();

        let mut out = String::new();
        for tab in tabs {
            let marker = if Some(tab.id.as_str()) == current_id {
                " *"
            } else {
                ""
            };
            out.push_str(&format!(
                "[{}]{} {}\n  {}\n",
                tab.id, marker, tab.title, tab.url
            ));
        }
        if out.is_empty() {
            out = "No tabs open.".into();
        }
        text_ok(out)
    }

    #[tool(description = "Open a new browser tab. Optionally navigate to URL. Returns new tab ID.")]
    async fn new_tab(&self, req: Parameters<NewTabRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().unwrap();

        let (tab_id, tab) = state
            .new_tab(req.0.url.as_deref())
            .await
            .map_err(internal)?;

        let url = tab.page.url().await.map_err(internal)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Opened new tab [{}]\nURL: {}\nTitle: {}",
            tab_id, url, title
        ))
    }

    #[tool(description = "Switch to a different browser tab by ID. Get IDs from list_tabs.")]
    async fn switch_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        state.switch_tab(&req.0.tab_id).await.map_err(internal)?;

        let tab = state.current_tab().unwrap();
        let url = tab.page.url().await.map_err(internal)?;
        let title = title_nonblocking(&tab.page).await;
        text_ok(format!(
            "Switched to tab [{}]\nURL: {}\nTitle: {}",
            req.0.tab_id, url, title
        ))
    }

    #[tool(description = "Close a browser tab by ID. Cannot close the last remaining tab.")]
    async fn close_tab(&self, req: Parameters<TabIdRequest>) -> Result<CallToolResult, ErrorData> {
        self.operations.stop().await;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        state.close_tab(&req.0.tab_id).await.map_err(internal)?;
        text_ok(format!("Closed tab [{}]", req.0.tab_id))
    }

    #[tool(
        description = "Navigate to a URL and return a durable operation record. Requires operation_id. Launches browser on first call. \
        Optional: headers (e.g. CVE-2025-29927 bypass), user_agent override, bypass_csp."
    )]
    async fn navigate(
        &self,
        req: Parameters<NavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = req
            .0
            .operation
            .operation_id
            .clone()
            .ok_or_else(|| invalid("operation_id is required"))?;
        self.ensure_browser().await?;
        let page = {
            let mut guard = self.state.lock().await;
            let state = guard
                .as_mut()
                .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
            if state.current_tab_id.is_none() {
                state.new_tab(None).await.map_err(internal)?;
            }
            let tab = state
                .current_tab_mut()
                .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
            if let Some(ua) = &req.0.user_agent {
                tab.page.set_user_agent(ua).await.map_err(internal)?;
            }
            if req.0.bypass_csp.unwrap_or(false) {
                tab.page.set_bypass_csp(true).await.map_err(internal)?;
            }
            tab.invalidate();
            tab.page.clone()
        };
        let action = match req.0.headers {
            Some(headers) => eoka_server::eoka::Action::NavigateWithHeaders {
                url: req.0.url,
                headers,
            },
            None => eoka_server::eoka::Action::Navigate {
                url: req.0.url,
                referrer: None,
            },
        };
        let options = eoka_server::eoka::ActionOptions {
            timeout_ms: req.0.operation.timeout_ms.unwrap_or(30000),
            expect: req.0.operation.expect,
            ..Default::default()
        };
        protocol_tool_response(eoka_server::operations::commands::response(
            self.operations.submit(page, id, action, options).await,
        ))
    }

    #[tool(
        description = "List interactive elements. Optional filter: 'inputs' (form elements), 'buttons' (clickables), 'all'. Optional max limit. Use live targeting (text:, css:) to skip observe."
    )]
    async fn observe(&self, req: Parameters<ObserveRequest>) -> Result<CallToolResult, ErrorData> {
        observe::validate_filter(req.0.filter.as_deref()).map_err(invalid)?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let (elements, data) = match observe::inspect(&tab.page, viewport_only, &req.0).await {
            Ok(result) => result,
            Err(error) => {
                drop(guard);
                return Err(self.check_transport_err(error).await);
            }
        };
        tab.elements = elements;
        match data {
            Value::String(text) => text_ok(text),
            data => protocol_tool_response(eoka_protocol::Response::ok(data)),
        }
    }

    #[tool(
        description = "Take annotated screenshot with numbered element boxes. Returns PNG image AND element list. Best way to see the page."
    )]
    async fn screenshot(&self) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(internal)?;
        }

        let png = match annotate::annotated_screenshot(&tab.page, &tab.elements).await {
            Ok(p) => p,
            Err(e) => {
                drop(guard);
                return Err(self.check_transport_err(e).await);
            }
        };
        let list = element_list(&tab.elements);
        let b64 = BASE64.encode(&png);
        Ok(CallToolResult::success(vec![
            ContentBlock::image(b64, "image/png"),
            ContentBlock::text(if list.is_empty() {
                "No interactive elements found.".into()
            } else {
                list
            }),
        ]))
    }

    #[tool]
    async fn snapshot(
        &self,
        req: Parameters<SnapshotRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let include_all = req.0.all;
        let result = snapshot::snapshot(&tab.page, include_all)
            .await
            .map_err(internal)?;

        tab.snapshot_refs.clear();
        for r in &result.refs {
            tab.snapshot_refs
                .insert(r.ref_label.clone(), r.backend_node_id);
        }

        text_ok(result.tree_text)
    }

    #[tool(
        description = "Click an element. Target: index (0), @e1 (snapshot ref), text:Submit, placeholder:Search, role:button, css:selector, id:my-btn, or plain text. Returns a durable operation ID. Never replays input."
    )]
    async fn click(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        self.perform_command("click", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(
        description = "Type text into an input. Target: index, text:Label, placeholder:Enter code, css:input, id:field. Returns a durable operation ID. Never replays input. Clears existing text."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        self.perform_command("fill", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(
        description = "Select dropdown option. Target: index, text:Label, css:select, id:dropdown. Value matches option value or visible text."
    )]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        self.perform_command("select", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(
        description = "Hover over element to trigger tooltips, menus, or hover states. Target: index, text:Label, css:selector, etc."
    )]
    async fn hover(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        self.perform_command("hover", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(
        description = "Press keyboard key. Common: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Space."
    )]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.perform_command("key", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool]
    async fn mouse_down(
        &self,
        req: Parameters<MouseButtonRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .mouse_down(req.0.x, req.0.y, Self::core_mouse_button(req.0.button))
            .await
            .map_err(internal)?;
        text_ok("Mouse button held".to_string())
    }

    #[tool]
    async fn mouse_move(
        &self,
        req: Parameters<MouseMoveRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .mouse_move(req.0.x, req.0.y)
            .await
            .map_err(internal)?;
        text_ok("Mouse moved".to_string())
    }

    #[tool]
    async fn mouse_up(
        &self,
        req: Parameters<MouseButtonRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .mouse_up(req.0.x, req.0.y, Self::core_mouse_button(req.0.button))
            .await
            .map_err(internal)?;
        text_ok("Mouse button released".to_string())
    }

    #[tool]
    async fn key_down(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.key_down(&req.0.key).await.map_err(internal)?;
        text_ok(format!("Held {}", req.0.key))
    }

    #[tool]
    async fn key_up(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.key_up(&req.0.key).await.map_err(internal)?;
        text_ok(format!("Released {}", req.0.key))
    }

    #[tool]
    async fn release_all_inputs(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.release_all_inputs().await.map_err(internal)?;
        text_ok("Released all held inputs".to_string())
    }

    #[tool(
        description = "Execute durable actions in sequence. Each action supplies its own operation_id, action and options. Stops on an uncertain or failed outcome; completed records remain inspectable."
    )]
    async fn batch(&self, req: Parameters<BatchRequest>) -> Result<CallToolResult, ErrorData> {
        let page = {
            let guard = self.state.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?
                .current_tab()
                .ok_or_else(|| ErrorData::from(AgentError::NoTab))?
                .page
                .clone()
        };
        let mut records = Vec::new();
        for request in req.0.actions {
            let mut record = self
                .operations
                .submit(
                    page.clone(),
                    request.operation_id.clone(),
                    request.action,
                    request.options,
                )
                .await
                .map_err(internal)?;
            if matches!(record.state, eoka_protocol::OperationState::Running) {
                record = self
                    .operations
                    .wait(&request.operation_id, 86_400_000)
                    .await
                    .map_err(internal)?;
            }
            let completed = record.outcome == eoka_protocol::ActionOutcome::VerifiedComplete;
            records.push(record);
            if !completed {
                break;
            }
        }
        protocol_tool_response(eoka_protocol::Response::ok(
            serde_json::to_value(records).map_err(internal)?,
        ))
    }

    #[tool(
        description = "Scroll page. Target: 'up', 'down', 'top', 'bottom', or element target (index, text:Label, css:selector) to scroll into view."
    )]
    async fn scroll(&self, req: Parameters<ScrollRequest>) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let vp = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        match req.0.target.as_str() {
            "up" => tab
                .page
                .execute_sync("window.scrollBy(0, -window.innerHeight * 0.8)")
                .await
                .map_err(internal)?,
            "down" => tab
                .page
                .execute_sync("window.scrollBy(0, window.innerHeight * 0.8)")
                .await
                .map_err(internal)?,
            "top" => tab
                .page
                .execute_sync("window.scrollTo(0, 0)")
                .await
                .map_err(internal)?,
            "bottom" => tab
                .page
                .execute_sync("window.scrollTo(0, document.body.scrollHeight)")
                .await
                .map_err(internal)?,
            target_str => {
                auto_observe_if_needed(tab, target_str, vp).await?;
                let resolved = resolve_target(tab, target_str).await?;
                let js = format!(
                    "document.querySelector({})?.scrollIntoView({{behavior:'smooth',block:'center'}})",
                    serde_json::to_string(&resolved.selector).unwrap()
                );
                tab.page.execute_sync(&js).await.map_err(internal)?;
            }
        }
        text_ok(format!("Scrolled {}", req.0.target))
    }

    #[tool(
        description = "Find elements by text content (case-insensitive). Returns matching elements with indices."
    )]
    async fn find_text(
        &self,
        req: Parameters<FindTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let config_viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        if tab.elements.is_empty() {
            tab.elements = observe::observe(&tab.page, config_viewport_only)
                .await
                .map_err(internal)?;
        }

        let needle = req.0.text.to_lowercase();
        let matches: Vec<_> = tab
            .elements
            .iter()
            .filter(|e| {
                e.text.to_lowercase().contains(&needle)
                    || e.placeholder
                        .as_ref()
                        .map(|p| p.to_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .collect();

        if matches.is_empty() {
            text_ok(format!("No elements found matching \"{}\"", req.0.text))
        } else {
            let out: String = matches.iter().map(|e| format!("{}\n", e)).collect();
            text_ok(out)
        }
    }

    #[tool(
        description = "Run JavaScript and return result. Supports multi-statement code; the last expression's value is returned as JSON. \
        Pass js= for inline code, or file= with an absolute path to load from disk (saves tokens for repeated/complex scripts)."
    )]
    async fn extract(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = resolve_js(&req.0)?;
        let escaped_js = serde_json::to_string(&code).map_err(internal)?;
        let js = format!("JSON.stringify(eval({}))", escaped_js);
        let json_str: String = tab.page.evaluate_sync(&js).await.map_err(internal)?;
        text_ok(json_str)
    }

    #[tool(
        description = "Execute JavaScript without expecting a return value. Use for side effects like clicking elements via JS. \
        Pass js= for inline code, or file= with an absolute path to load from disk (saves tokens for repeated/complex scripts)."
    )]
    async fn exec(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = resolve_js(&req.0)?;
        let _: String = tab.page.evaluate_sync(&code).await.unwrap_or_default();
        text_ok("Executed successfully")
    }

    #[tool(
        description = "Make an HTTP request from inside the browser session. Uses the browser's real cookies, TLS fingerprint, and Cloudflare clearance — bypasses bot detection that blocks curl. \
        Returns status, headers, and body. Use instead of Bash(curl) for any target protected by Cloudflare or requiring authentication. \
        Set redirect='manual' to capture redirect URLs without following (useful for OAuth redirect_uri testing)."
    )]
    async fn fetch(&self, req: Parameters<FetchRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let response = eoka_server::browser_fetch::fetch(&tab.page, req.0).await;
        protocol_tool_response(response)
    }

    #[tool(
        description = "Get all visible text on the page. Useful for reading content without elements."
    )]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        match tab.page.text().await {
            Ok(text) => text_ok(text),
            Err(e) => {
                drop(guard);
                Err(self.check_transport_err(e).await)
            }
        }
    }

    #[tool(description = "Get current URL and page title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        match tab.page.url().await {
            Ok(url) => {
                let title = title_nonblocking(&tab.page).await;
                text_ok(format!("URL: {}\nTitle: {}", url, title))
            }
            Err(e) => {
                drop(guard);
                Err(self.check_transport_err(e).await)
            }
        }
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(
        &self,
        req: Parameters<eoka_protocol::ActionSettings>,
    ) -> Result<CallToolResult, ErrorData> {
        self.perform_command("back", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(
        &self,
        req: Parameters<eoka_protocol::ActionSettings>,
    ) -> Result<CallToolResult, ErrorData> {
        self.perform_command("forward", serde_json::to_value(req.0).map_err(internal)?)
            .await
    }

    #[tool(
        description = "Detect SPA router type and current route state. Returns router type (React Router, Next.js, Vue Router, etc.), current path, query params, and whether programmatic navigation is available."
    )]
    async fn spa_info(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let info = spa::detect_router(&tab.page).await.map_err(internal)?;
        text_ok(info.to_string())
    }

    #[tool(
        description = "Navigate SPA to a new path without page reload. Uses the detected router (React Router, Next.js, Vue Router, etc.) or falls back to History API. Much faster than full page navigation for SPAs."
    )]
    async fn spa_navigate(
        &self,
        req: Parameters<SpaNavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let info = spa::detect_router(&tab.page).await.map_err(internal)?;
        let new_path = spa::spa_navigate(&tab.page, &info.router_type, &req.0.path)
            .await
            .map_err(internal)?;

        tab.invalidate();
        text_ok(format!(
            "Navigated to {} via {} (no page reload)",
            new_path, info.router_type
        ))
    }

    #[tool(
        description = "Navigate browser history by delta steps. Use delta=-1 for back, delta=1 for forward, delta=-2 for back twice, etc. Works with both SPAs and regular pages."
    )]
    async fn history_go(
        &self,
        req: Parameters<HistoryGoRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = req
            .0
            .operation
            .operation_id
            .clone()
            .ok_or_else(|| invalid("operation_id is required"))?;
        self.perform_command("action_perform",serde_json::json!({"operation_id":id,"action":{"kind":"history","delta":req.0.delta},"options":{"timeout_ms":req.0.operation.timeout_ms.unwrap_or(30000),"expect":req.0.operation.expect}})).await
    }

    #[tool(description = "Get all cookies for the current page. Returns JSON array of cookies.")]
    async fn cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let cookies = tab.page.cookies().await.map_err(internal)?;
        let json = serde_json::to_string_pretty(&cookies).map_err(internal)?;
        text_ok(json)
    }

    #[tool(description = "Set a cookie. Useful for restoring sessions or authentication.")]
    async fn set_cookie(
        &self,
        req: Parameters<SetCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .set_cookie(
                &req.0.name,
                &req.0.value,
                req.0.domain.as_deref(),
                req.0.path.as_deref(),
            )
            .await
            .map_err(internal)?;
        text_ok(format!("Cookie '{}' set", req.0.name))
    }

    #[tool(description = "Solve hCaptcha, reCAPTCHA, or AWS WAF CAPTCHAs using anti-captcha.com")]
    async fn solve_captcha(
        &self,
        req: Parameters<SolveCaptchaRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let solver = ::captcha::AntiCaptcha::new(req.0.api_key);

        let solution = match req.0.captcha_type.to_lowercase().as_str() {
            "hcaptcha" => solver.solve_hcaptcha(&req.0.website_url, &req.0.website_key).await,
            "recaptcha_v2" => solver.solve_recaptcha_v2(&req.0.website_url, &req.0.website_key).await,
            "recaptcha_v2_enterprise" => solver
                .solve_recaptcha_v2_enterprise(
                    &req.0.website_url,
                    &req.0.website_key,
                    req.0.enterprise_payload,
                    req.0.api_domain.as_deref(),
                )
                .await,
            "recaptcha_v3" => {
                let page_action = req.0.page_action.unwrap_or_else(|| "submit".to_string());
                let min_score = req.0.min_score.unwrap_or(0.3);
                solver
                    .solve_recaptcha_v3(
                        &req.0.website_url,
                        &req.0.website_key,
                        &page_action,
                        min_score,
                    )
                    .await
            }
            "amazon_waf" => {
                let iv = req.0.iv.as_deref().ok_or_else(|| internal("amazon_waf requires iv"))?;
                let context = req.0.context.as_deref().ok_or_else(|| internal("amazon_waf requires context"))?;
                solver.solve_amazon_waf(
                    &req.0.website_url, &req.0.website_key, iv, context,
                    req.0.captcha_script.as_deref(), req.0.challenge_script.as_deref(),
                ).await
            }
            _ => {
                return Err(internal(format!(
                    "Unknown captcha type: {}. Use 'hcaptcha', 'recaptcha_v2', 'recaptcha_v2_enterprise', 'recaptcha_v3', or 'amazon_waf'",
                    req.0.captcha_type
                )))
            }
        };

        match solution {
            Ok(solution) => {
                let token = solution
                    .token()
                    .ok_or_else(|| internal("Captcha solution contained no token"))?;
                text_ok(
                    serde_json::json!({
                        "token": token,
                        "user_agent": solution.user_agent,
                    })
                    .to_string(),
                )
            }
            Err(e) => Err(internal(format!("Failed to solve captcha: {}", e))),
        }
    }

    #[tool(
        description = "Detect hCaptcha or reCAPTCHA on the current page. Returns captcha type and sitekey."
    )]
    async fn detect_captcha(
        &self,
        req: Parameters<DetectCaptchaRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        if req.0.auto_detect.unwrap_or(true) {
            if let Some(info) = captcha::detect_captcha_on_page(&tab.page).await {
                text_ok(format!(
                    "Captcha detected!\nType: {}\nSitekey: {}",
                    info.captcha_type, info.sitekey
                ))
            } else {
                text_ok("No captcha detected on current page".to_string())
            }
        } else {
            text_ok("Captcha detection disabled".to_string())
        }
    }

    #[tool(description = "Inject solved captcha token into page (for hCaptcha or reCAPTCHA v2)")]
    async fn inject_captcha_token(
        &self,
        req: Parameters<JsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let code = resolve_js(&req.0)?;
        tab.page.execute_sync(&code).await.map_err(internal)?;

        text_ok("Captcha token injected")
    }

    #[tool(
        description = "Wait for a specified number of milliseconds. Useful for timing-sensitive operations where you need a pause between actions."
    )]
    async fn wait_ms(&self, req: Parameters<WaitMsRequest>) -> Result<CallToolResult, ErrorData> {
        let capped = req.0.ms.min(30_000);
        tokio::time::sleep(std::time::Duration::from_millis(capped)).await;
        if capped < req.0.ms {
            text_ok(format!(
                "Waited {}ms (capped from {}ms, max 30s)",
                capped, req.0.ms
            ))
        } else {
            text_ok(format!("Waited {}ms", capped))
        }
    }

    #[tool(
        description = "Install a page navigation diagnostic and admit a durable click. Requires operation_id and an explicit expect condition. Returns the operation record; wait with operation_wait, then inspect window.__eoka_nav using extract. Intercepts history.pushState/replaceState; location navigation cannot be reliably intercepted by page JavaScript. allow_nav=true preserves history navigation."
    )]
    async fn click_intercept_nav(
        &self,
        req: Parameters<ClickInterceptNavRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = req.0.operation.operation_id.clone().ok_or_else(|| {
            invalid("operation_id is required; generate a UUID before requesting an action")
        })?;
        if req.0.operation.expect.is_none() {
            return Err(invalid("expect is required to verify the click outcome"));
        }
        let arguments = serde_json::json!({
            "operation_id": id,
            "target": req.0.target,
            "expect": req.0.operation.expect,
            "timeout_ms": req.0.operation.timeout_ms,
        });
        if self.operations.get(&id).await.is_ok() {
            return self.perform_command("click", arguments).await;
        }
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let viewport_only = state.config.viewport_only;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        auto_observe_if_needed(tab, &req.0.target, viewport_only).await?;
        let allow_nav = req.0.allow_nav.unwrap_or(false);
        tab.page
            .execute_sync(&format!(
                r#"(() => {{
            window.__eoka_nav = null;
            window.__eoka_allow_nav = {allow_nav};
            if (window.__eoka_nav_installed) return;
            window.__eoka_nav_installed = true;
            for (const method of ['pushState', 'replaceState']) {{
                const original = history[method].bind(history);
                history[method] = function(state, title, url) {{
                    window.__eoka_nav = {{ method: 'history.' + method, url: String(url) }};
                    if (window.__eoka_allow_nav) return original(state, title, url);
                }};
            }}
        }})()"#
            ))
            .await
            .map_err(internal)?;
        drop(guard);
        self.perform_command("click", arguments).await
    }

    #[tool(description = "Delete a specific cookie by name.")]
    async fn delete_cookie(
        &self,
        req: Parameters<DeleteCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .delete_cookie(&req.0.name, req.0.domain.as_deref())
            .await
            .map_err(internal)?;
        text_ok(format!("Cookie '{}' deleted", req.0.name))
    }

    #[tool(description = "Clear all cookies for the current browser context.")]
    async fn clear_cookies(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.clear_all_cookies().await.map_err(internal)?;
        text_ok("All cookies cleared")
    }

    #[tool(
        description = "Accept a JavaScript alert/confirm/prompt dialog. Optionally provide text for prompt() dialogs."
    )]
    async fn accept_dialog(
        &self,
        req: Parameters<AcceptDialogRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page
            .accept_dialog(req.0.prompt_text.as_deref())
            .await
            .map_err(internal)?;
        text_ok("Dialog accepted")
    }

    #[tool(description = "Dismiss (cancel) a JavaScript alert/confirm/prompt dialog.")]
    async fn dismiss_dialog(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        tab.page.dismiss_dialog().await.map_err(internal)?;
        text_ok("Dialog dismissed")
    }

    #[tool(
        description = "Wait for text to appear on the page (case-insensitive). Errors on timeout."
    )]
    async fn wait_for_text(
        &self,
        req: Parameters<WaitForTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_text(&req.0.text, timeout_ms)
            .await
            .map_err(internal)?;
        text_ok(format!("Text \"{}\" found", req.0.text))
    }

    #[tool(
        description = "Wait for network to go idle (no XHR/fetch for idle_ms). Useful after actions that trigger API calls before reading the result."
    )]
    async fn wait_for_network_idle(
        &self,
        req: Parameters<WaitNetworkIdleRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let idle_ms = req.0.idle_ms.unwrap_or(500);
        let timeout_ms = req.0.timeout_ms.unwrap_or(10_000);
        tab.page
            .wait_for_network_idle(idle_ms, timeout_ms)
            .await
            .map_err(internal)?;
        text_ok("Network idle")
    }

    #[tool(description = "Get a value from localStorage. Returns null message if key not found.")]
    async fn local_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let js = format!(
            "localStorage.getItem({})",
            serde_json::to_string(&req.0.key).map_err(internal)?
        );
        let value: Option<String> = tab.page.evaluate(&js).await.map_err(internal)?;
        match value {
            Some(v) => text_ok(v),
            None => text_ok(format!("(key '{}' not in localStorage)", req.0.key)),
        }
    }

    #[tool(description = "Set a value in localStorage.")]
    async fn local_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let js = format!(
            "localStorage.setItem({}, {})",
            serde_json::to_string(&req.0.key).map_err(internal)?,
            serde_json::to_string(&req.0.value).map_err(internal)?
        );
        tab.page.execute(&js).await.map_err(internal)?;
        text_ok(format!("localStorage['{}'] set", req.0.key))
    }

    #[tool(description = "Get a value from sessionStorage. Returns null message if key not found.")]
    async fn session_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let js = format!(
            "sessionStorage.getItem({})",
            serde_json::to_string(&req.0.key).map_err(internal)?
        );
        let value: Option<String> = tab.page.evaluate(&js).await.map_err(internal)?;
        match value {
            Some(v) => text_ok(v),
            None => text_ok(format!("(key '{}' not in sessionStorage)", req.0.key)),
        }
    }

    #[tool(description = "Set a value in sessionStorage.")]
    async fn session_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let js = format!(
            "sessionStorage.setItem({}, {})",
            serde_json::to_string(&req.0.key).map_err(internal)?,
            serde_json::to_string(&req.0.value).map_err(internal)?
        );
        tab.page.execute(&js).await.map_err(internal)?;
        text_ok(format!("sessionStorage['{}'] set", req.0.key))
    }

    #[tool(
        description = "Dump all client-side storage as JSON: localStorage, sessionStorage, and cookies. Use to capture session state for later restore or analysis."
    )]
    async fn dump_storage(&self) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let js = r#"JSON.stringify({
            localStorage: Object.fromEntries(Object.entries(localStorage)),
            sessionStorage: Object.fromEntries(Object.entries(sessionStorage)),
            cookies: document.cookie
        })"#;
        let json: String = tab.page.evaluate(js).await.map_err(internal)?;
        text_ok(json)
    }

    #[tool(
        description = "Read captured console output (log/warn/error/info/debug). Auto-injects capture on first call."
    )]
    async fn console(&self, req: Parameters<ConsoleRequest>) -> Result<CallToolResult, ErrorData> {
        let req = req.0;
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or(AgentError::NoBrowser)?;
        let tab = state.current_tab_mut().ok_or(AgentError::NoTab)?;
        ensure_console_capture(tab).await?;

        let clear = req.clear.unwrap_or(false);
        let level_filter = req.level.as_deref();

        let js = if clear {
            "(() => { var c = window.__eoka_console || []; window.__eoka_console = []; return c; })()"
        } else {
            "(window.__eoka_console || [])"
        };
        let result: Value = tab.page.evaluate_sync(js).await.map_err(internal)?;

        let entries = result.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        if entries.is_empty() {
            return text_ok("No console output captured.");
        }

        let mut out = String::with_capacity(entries.len() * 60);
        for entry in entries {
            let level = entry.get("level").and_then(|v| v.as_str()).unwrap_or("log");
            if let Some(filter) = level_filter {
                if level != filter {
                    continue;
                }
            }
            let text = entry.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let ts = entry.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
            let _ = writeln!(out, "[{}] {}: {}", ts, level.to_uppercase(), text);
        }
        if out.is_empty() {
            return text_ok(format!(
                "No console output matching level '{}'.",
                level_filter.unwrap_or("all")
            ));
        }
        text_ok(out)
    }

    #[tool(
        description = "Read captured JavaScript errors (uncaught exceptions and unhandled promise rejections). Auto-injects capture on first call."
    )]
    async fn errors(&self, req: Parameters<ErrorsRequest>) -> Result<CallToolResult, ErrorData> {
        let req = req.0;
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard.as_mut().ok_or(AgentError::NoBrowser)?;
        let tab = state.current_tab_mut().ok_or(AgentError::NoTab)?;
        ensure_console_capture(tab).await?;

        let clear = req.clear.unwrap_or(false);
        let js = if clear {
            "(() => { var e = window.__eoka_errors || []; window.__eoka_errors = []; return e; })()"
        } else {
            "(window.__eoka_errors || [])"
        };
        let result: Value = tab.page.evaluate_sync(js).await.map_err(internal)?;

        let entries = result.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        if entries.is_empty() {
            return text_ok("No JavaScript errors captured.");
        }

        let mut out = String::with_capacity(entries.len() * 120);
        for entry in entries {
            let msg = entry
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            let source = entry.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let line = entry.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
            let col = entry.get("col").and_then(|v| v.as_u64()).unwrap_or(0);
            let stack = entry.get("stack").and_then(|v| v.as_str()).unwrap_or("");
            let ts = entry.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);

            let _ = write!(out, "[{}] {}", ts, msg);
            if !source.is_empty() {
                let _ = write!(out, " ({}:{}:{})", source, line, col);
            }
            out.push('\n');
            if !stack.is_empty() {
                let _ = writeln!(out, "  {}", stack.replace('\n', "\n  "));
            }
        }
        text_ok(out)
    }

    #[tool(
        description = "Save browser auth state (cookies + localStorage + sessionStorage) to a JSON file. Use after login to persist sessions across restarts. Captures httpOnly cookies via CDP for full fidelity."
    )]
    async fn save_state(
        &self,
        req: Parameters<SaveStateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let saved = helpers::capture_state(&tab.page).await?;
        let json = serde_json::to_string_pretty(&saved).map_err(internal)?;
        std::fs::write(&req.0.path, &json)
            .map_err(|e| invalid(format!("Failed to write '{}': {}", req.0.path, e)))?;

        text_ok(format!(
            "Saved {} cookies, {} localStorage keys, {} sessionStorage keys to {}",
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len(),
            req.0.path
        ))
    }

    #[tool(
        description = "Load browser auth state (cookies + localStorage + sessionStorage) from a JSON file. Restores a previously saved session — avoids re-login, 2FA, etc. By default navigates to the saved URL first."
    )]
    async fn load_state(
        &self,
        req: Parameters<LoadStateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let contents = std::fs::read_to_string(&req.0.path)
            .map_err(|e| invalid(format!("Failed to read '{}': {}", req.0.path, e)))?;
        let saved: helpers::SavedState = serde_json::from_str(&contents)
            .map_err(|e| invalid(format!("Invalid state JSON: {}", e)))?;

        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        let navigate = req.0.navigate.unwrap_or(true);

        if navigate {
            let tab = state.ensure_tab(&saved.url).await.map_err(internal)?;
            helpers::wait_for_stable(&tab.page)
                .await
                .map_err(internal)?;
        }

        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        helpers::restore_state(&tab.page, &saved).await?;

        if navigate {
            let _: String = tab
                .page
                .evaluate_sync("location.reload()")
                .await
                .unwrap_or_default();
            helpers::wait_for_stable(&tab.page)
                .await
                .map_err(internal)?;
        }

        text_ok(format!(
            "Loaded {} cookies, {} localStorage keys, {} sessionStorage keys from {}",
            saved.cookies.len(),
            saved.local_storage.len(),
            saved.session_storage.len(),
            req.0.path
        ))
    }

    #[tool(description = "Close the browser. Call when done to free resources.")]
    async fn close(&self) -> Result<CallToolResult, ErrorData> {
        self.operations.stop().await;
        self.downloads.stop().await;
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.take() {
            state.close().await.map_err(internal)?;
        }
        text_ok("Browser closed.")
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EokaServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2026_07_28)
            .with_server_info(Implementation::new("eoka-tools", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Browser automation.\n\n\
                 TARGETING: Index (0) uses cache. Everything else is LIVE (resolved at action time):\n\
                 Submit, text:Submit, placeholder:code, css:button, id:btn, role:button\n\n\
                 OBSERVE: filter='inputs'|'buttons', max=N\n\
                 BATCH: batch([{action:'fill',target:'placeholder:code',text:'X'},{action:'click',target:'Submit'}])\n\
                 AUTO-RETRY: click/fill retry once on stale\n\
                 SPA: spa_info, spa_navigate, history_go\n\
                 Tabs: list_tabs, new_tab, switch_tab (auto-attaches to popups), close_tab\n\
                 TIMING: wait_ms(ms) — pause between actions\n\
                 NAV INTERCEPT: click_intercept_nav — durable click with a history.pushState/replaceState diagnostic\n\
                 FETCH: fetch(url, method?, headers?, body?, redirect?, max_body?) — HTTP request from browser session, bypasses Cloudflare/bot detection\n\
                 COOKIES: cookies, set_cookie, delete_cookie, clear_cookies\n\
                 DIALOGS: accept_dialog, dismiss_dialog\n\
                 STATE: save_state(path) — persist auth state (cookies+storage) to JSON, load_state(path, navigate?) — restore it\n\
                 STORAGE: local_storage_get/set, session_storage_get/set, dump_storage\n\
                 CONSOLE: console(clear?, level?) — read captured console output (auto-injects on first call)\n\
                 ERRORS: errors(clear?) — read captured JS errors and unhandled rejections\n\
                 NAVIGATE EXTRAS: navigate(url, headers?, user_agent?, bypass_csp?) — headers/UA/CSP override per navigation\n\
                 WAIT: wait_ms, wait_for_text, wait_for_network_idle"
            )
    }
}

async fn cleanup_browser(
    state: &Arc<Mutex<Option<BrowserState>>>,
    downloads: &eoka_server::downloads::DownloadManager,
    operations: &eoka_server::operations::OperationManager,
) {
    operations.stop().await;
    downloads.stop().await;
    let mut guard = state.lock().await;
    if let Some(bs) = guard.take() {
        eprintln!("[eoka-mcp] closing browser...");
        let _ = bs.close().await;
        eprintln!("[eoka-mcp] browser closed.");
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = EokaServer::new();
    let state_handle = Arc::clone(&server.state);
    let downloads = server.downloads.clone();
    let signal_downloads = downloads.clone();
    let operations = server.operations.clone();
    let signal_operations = operations.clone();

    let sig_state = Arc::clone(&state_handle);
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("failed to register SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => eprintln!("[eoka-mcp] received SIGTERM"),
            _ = sigint.recv() => eprintln!("[eoka-mcp] received SIGINT"),
        }
        cleanup_browser(&sig_state, &signal_downloads, &signal_operations).await;
        std::process::exit(0);
    });

    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    eprintln!("[eoka-mcp] MCP connection closed, cleaning up.");
    cleanup_browser(&state_handle, &downloads, &operations).await;
    Ok(())
}

fn protocol_tool_response(response: eoka_protocol::Response) -> Result<CallToolResult, ErrorData> {
    let ok = response.ok;
    let text = serde_json::to_string(&response).map_err(internal)?;
    Ok(if ok {
        CallToolResult::success(vec![ContentBlock::text(text)])
    } else {
        CallToolResult::error(vec![ContentBlock::text(text)])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires Chrome; durable navigation diagnostic"]
    async fn intercepted_click_keeps_durable_identity_and_does_not_replay() {
        let server = EokaServer::new();
        let navigation_id = uuid::Uuid::new_v4().to_string();
        server.navigate(Parameters(NavigateRequest {
            operation: eoka_protocol::ActionSettings { operation_id: Some(navigation_id.clone()), ..Default::default() },
            url: "data:text/html,<button id='save' onclick=\"window.clicks=(window.clicks||0)+1;history.pushState({},'',String.fromCharCode(35)+'report');document.getElementById('status').textContent='Saved'\">Save</button><p id='status'></p>".into(),
            headers: None, user_agent: None, bypass_csp: None,
        })).await.unwrap();
        let navigation = server.operations.wait(&navigation_id, 30000).await.unwrap();
        assert_eq!(
            navigation.outcome,
            eoka_protocol::ActionOutcome::VerifiedComplete
        );
        let id = uuid::Uuid::new_v4().to_string();
        for _ in 0..2 {
            server
                .click_intercept_nav(Parameters(ClickInterceptNavRequest {
                    target: "css:#save".into(),
                    allow_nav: Some(false),
                    operation: eoka_protocol::ActionSettings {
                        operation_id: Some(id.clone()),
                        expect: Some(eoka_protocol::Expectation::Text {
                            target: eoka_protocol::LocatorSpec::css("#status"),
                            value: "Saved".into(),
                            contains: false,
                        }),
                        timeout_ms: Some(3000),
                    },
                }))
                .await
                .unwrap();
            let result = server.operations.wait(&id, 3000).await.unwrap();
            assert_eq!(
                result.outcome,
                eoka_protocol::ActionOutcome::VerifiedComplete
            );
        }
        let guard = server.state.lock().await;
        let page = &guard.as_ref().unwrap().current_tab().unwrap().page;
        let result: Value = page
            .evaluate_sync("({clicks:window.clicks,nav:window.__eoka_nav,hash:location.hash})")
            .await
            .unwrap();
        assert_eq!(result["clicks"], 1);
        assert_eq!(result["nav"]["url"], "#report");
        assert_eq!(result["hash"], "");
        drop(guard);
        server.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome; MCP observation and download evidence"]
    async fn mcp_observation_and_download_failures_share_page_context() {
        let server = EokaServer::new();
        let navigation_id = uuid::Uuid::new_v4().to_string();
        server.navigate(Parameters(NavigateRequest {
            operation: eoka_protocol::ActionSettings { operation_id:Some(navigation_id.clone()), ..Default::default() },
            url: "data:text/html,<title>MCP fixture</title><h1>Unavailable</h1><p role='alert'>Transfer unavailable</p><button>Features</button>".into(),
            headers: None, user_agent: None, bypass_csp: None,
        })).await.unwrap();
        let navigation = server.operations.wait(&navigation_id, 30000).await.unwrap();
        assert_eq!(
            navigation.outcome,
            eoka_protocol::ActionOutcome::VerifiedComplete
        );
        let structured = server
            .observe(Parameters(ObserveRequest {
                structured: true,
                max: Some(1),
                filter: None,
            }))
            .await
            .unwrap();
        let plain = server
            .observe(Parameters(ObserveRequest::default()))
            .await
            .unwrap();
        let failed = server
            .download_start(Parameters(DownloadStartRequest {
                operation_id: None,
                click: Some("text:Download".into()),
                url: None,
                output: std::env::temp_dir()
                    .join("eoka-mcp-not-created.bin")
                    .to_string_lossy()
                    .into_owned(),
                expect_bytes: None,
                start_timeout_ms: 1000,
            }))
            .await
            .unwrap();
        server.close().await.unwrap();
        let structured = serde_json::to_value(structured).unwrap();
        let response: eoka_protocol::Response =
            serde_json::from_str(structured["content"][0]["text"].as_str().unwrap()).unwrap();
        let data = response.data.unwrap();
        assert_eq!(data["page"]["title"], "MCP fixture");
        assert_eq!(data["page"]["messages"][0], "Transfer unavailable");
        assert_eq!(data["elements"].as_array().unwrap().len(), 1);
        let plain = serde_json::to_value(plain).unwrap();
        assert!(plain["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Page message: Transfer unavailable"));
        let failed = serde_json::to_value(failed).unwrap();
        assert_eq!(failed["isError"], true);
        let error: eoka_protocol::Response =
            serde_json::from_str(failed["content"][0]["text"].as_str().unwrap()).unwrap();
        let context = error.error_detail.unwrap().context.unwrap();
        assert_eq!(
            context.action_status,
            Some(eoka_protocol::ActionStatus::NotAttempted)
        );
        assert_eq!(context.page.unwrap().messages, ["Transfer unavailable"]);
    }

    #[test]
    fn mcp_protocol_descriptors_match_catalog() {
        let server = EokaServer::new();

        for (route_name, command) in MCP_PROTOCOL_TOOL_ROUTES {
            let operation = operation_by_cmd(command).unwrap();
            let route = server.tool_router.get(route_name).unwrap();
            assert_eq!(route.description.as_deref(), Some(operation.description));
            assert_eq!(
                serde_json::to_value(&*route.input_schema).unwrap(),
                input_schema_for_cmd(operation.cmd)
            );
            let annotations = route.annotations.as_ref().unwrap();
            assert_eq!(annotations.read_only_hint, Some(operation.read_only));
            assert_eq!(annotations.destructive_hint, Some(operation.destructive));
        }
    }
}
