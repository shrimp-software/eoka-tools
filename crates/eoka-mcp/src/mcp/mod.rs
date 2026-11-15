mod agent;
mod blocked;
mod error;
mod helpers;
mod operations;
mod state;
#[cfg(test)]
mod surface;
#[cfg(test)]
mod through_client;
mod types;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka_protocol::{input_schema_for_cmd, operation_by_cmd, Request as Operation};
use eoka_server::{annotate, observe};

use error::{internal, invalid, is_transport_error_msg, AgentError};
use helpers::{element_list, resolve_js, text_ok, title_nonblocking};
use state::BrowserState;
use types::*;

const MCP_PROTOCOL_TOOL_ROUTES: &[(&str, &str)] = &[
    ("agent_observe", "agent_observe"),
    ("agent_step", "agent_step"),
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
    ("captcha_turnstile", "captcha_turnstile"),
];

/// Hand-written tools that cover a catalog command under a different name.
const MCP_OWN_TOOL_COMMANDS: &[(&str, &str)] = &[
    ("navigate", "open"),
    ("page_info", "info"),
    ("page_text", "text"),
    ("extract", "eval"),
    ("list_tabs", "tab_list"),
    ("local_storage_get", "storage"),
    ("local_storage_set", "set_storage"),
    ("accept_dialog", "dialog_accept"),
    ("dismiss_dialog", "dialog_dismiss"),
    ("detect_captcha", "captcha_detect"),
    ("inject_captcha_token", "captcha_inject"),
];

fn add_catalog_tools(router: &mut ToolRouter<EokaServer>, include_opt_in: bool) {
    let covered: std::collections::HashSet<&str> = MCP_PROTOCOL_TOOL_ROUTES
        .iter()
        .chain(MCP_OWN_TOOL_COMMANDS)
        .map(|(_, command)| *command)
        .collect();
    for operation in eoka_protocol::sidecar::page_operations() {
        let name = operation.name.replace('.', "_");
        let hidden =
            operation.exposure == eoka_protocol::OperationExposure::OptIn && !include_opt_in;
        if hidden || covered.contains(operation.cmd) || router.map.contains_key(name.as_str()) {
            continue;
        }
        let spoken = name.clone();
        let mut tool = Tool::new(
            name,
            operation.description,
            rmcp::model::object(input_schema_for_cmd(operation.cmd)),
        );
        tool.annotations = Some(
            ToolAnnotations::new()
                .read_only(operation.read_only)
                .destructive(operation.destructive),
        );
        router.add_route(rmcp::handler::server::router::tool::ToolRoute::new_dyn(
            tool,
            move |context: rmcp::handler::server::tool::ToolCallContext<'_, EokaServer>| {
                let spoken = spoken.clone();
                Box::pin(async move {
                    context
                        .service
                        .catalog_tool(operation, &spoken, context.arguments)
                        .await
                        .map(Into::into)
                })
            },
        ));
    }
}

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
    agent: Arc<Mutex<eoka_server::agent::AgentSession>>,
    state: Arc<Mutex<Option<BrowserState>>>,
    running: Arc<std::sync::Mutex<Option<(String, std::time::Instant)>>>,
    headless: bool,
    tool_router: ToolRouter<EokaServer>,
    downloads: eoka_server::downloads::DownloadManager,
    operations: eoka_server::operations::OperationManager,
}

impl EokaServer {
    async fn lock_browser(
        &self,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<BrowserState>>, ErrorData> {
        let guard = self.state.lock().await;
        if guard.is_none() {
            return Err(ErrorData::from(AgentError::NoBrowser));
        }
        Ok(guard)
    }

    async fn catalog_tool(
        &self,
        operation: &'static eoka_protocol::OperationSpec,
        spoken: &str,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, ErrorData> {
        drop(self.lock_browser().await?);
        let request = eoka_protocol::request_from_cmd(
            operation.cmd,
            Value::Object(arguments.unwrap_or_default()),
        )
        .map_err(invalid)?;
        self.engine_tool(spoken, request).await
    }

    async fn respond(
        &self,
        tool: &str,
        operation: Operation,
    ) -> Result<(eoka_protocol::Response, eoka_server::eoka::Page), ErrorData> {
        let read_only = operation.spec().read_only;
        let mut guard = self.lock_browser().await?;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let page_id = state
            .current_tab_id
            .clone()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let page = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?
            .page
            .clone();
        let server = state.server.clone();
        let spoken = tool.to_owned();
        let mut response =
            tokio::spawn(async move { server.respond(&page_id, operation, Some(&spoken)).await })
                .await
                .map_err(internal)?
                .unwrap_or_else(|error| match error.data.and_then(|data| data.detail) {
                    Some(detail) => eoka_protocol::Response::err_detail(detail),
                    None => eoka_protocol::Response::err(error.message),
                });
        if !read_only {
            if let Some(tab) = state.current_tab_mut() {
                tab.invalidate();
            }
        }
        let failure = response.error.clone().unwrap_or_default();
        let answered_late = failure.to_ascii_lowercase().contains("timed out")
            && !page.session().transport().is_closed();
        if !response.ok && is_transport_error_msg(&failure) && !answered_late {
            eprintln!("[eoka-mcp] connection lost, marking unhealthy: {failure}");
            state.unhealthy = true;
            let lost = " (connection lost - will relaunch on next call)";
            if let Some(error) = response.error.as_mut() {
                error.push_str(lost);
            }
            if let Some(detail) = response.error_detail.as_mut() {
                detail.message.push_str(lost);
            }
        }
        Ok((response, page))
    }

    async fn engine_tool(
        &self,
        tool: &str,
        operation: Operation,
    ) -> Result<CallToolResult, ErrorData> {
        let (response, page) = self.respond(tool, operation).await?;
        let error = match eoka_server::state::catalog_result(response, Some(&page)) {
            Ok(Value::String(text)) => return text_ok(text),
            Ok(data) => return text_ok(serde_json::to_string(&data).map_err(internal)?),
            Err(error) => error,
        };
        if let Some(detail) = blocked::blocked_detail(&error) {
            return Ok(blocked::refusal(detail));
        }
        Ok(CallToolResult::error(vec![ContentBlock::text(
            serde_json::to_string(&error).map_err(internal)?,
        )]))
    }

    async fn launched_tool(
        &self,
        tool: &str,
        operation: Operation,
    ) -> Result<CallToolResult, ErrorData> {
        self.ensure_browser().await?;
        self.engine_tool(tool, operation).await
    }

    async fn storage_get(
        &self,
        tool: &str,
        key: String,
        session_storage: bool,
    ) -> Result<CallToolResult, ErrorData> {
        let storage = eoka_protocol::StorageArgs {
            key: Some(key),
            session_storage,
        };
        self.engine_tool(tool, Operation::Storage(storage)).await
    }

    async fn storage_set(
        &self,
        tool: &str,
        entry: StorageSetRequest,
        session_storage: bool,
    ) -> Result<CallToolResult, ErrorData> {
        let storage = eoka_protocol::SetStorageArgs {
            key: entry.key,
            value: entry.value,
            session_storage,
        };
        self.engine_tool(tool, Operation::SetStorage(storage)).await
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
            let state =
                BrowserState::new(self.headless, self.agent.clone(), self.operations.clone())
                    .await
                    .map_err(internal)?;
            *guard = Some(state);
        }
        Ok(())
    }

    async fn check_transport_err<E: std::fmt::Display>(&self, e: E) -> ErrorData {
        let msg = e.to_string();
        let command_timeout = msg.to_ascii_lowercase().contains("timed out");
        if command_timeout && self.transport_alive().await {
            return internal(msg);
        }
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
        let include_opt_in = std::env::var("EOKA_MCP_ALL_TOOLS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(false);
        Self::listing(include_opt_in)
    }

    fn listing(include_opt_in: bool) -> Self {
        let headless = std::env::var("EOKA_HEADLESS")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
        let mut tool_router = Self::tool_router();
        apply_protocol_tool_metadata(&mut tool_router);
        add_catalog_tools(&mut tool_router, include_opt_in);
        eoka_server::engine::blockers::set_surface(eoka_server::engine::blockers::Surface::Mcp);
        Self {
            agent: Default::default(),
            state: Arc::new(Mutex::new(None)),
            running: Default::default(),
            headless,
            tool_router,
            downloads: Default::default(),
            operations: Default::default(),
        }
    }

    #[tool]
    async fn agent_observe(
        &self,
        req: Parameters<eoka_protocol::AgentObserveArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.agent_tool("agent_observe", Operation::AgentObserve(req.0))
            .await
    }

    #[tool]
    async fn agent_step(
        &self,
        req: Parameters<eoka_protocol::AgentStepArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        helpers::ensure_action_urls_allowed(&serde_json::to_value(&req.0).map_err(internal)?)?;
        self.agent_tool("agent_step", Operation::AgentStep(req.0))
            .await
    }

    #[tool]
    async fn action_perform(
        &self,
        req: Parameters<eoka_protocol::ActionPerformArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let arguments = serde_json::to_value(req.0).map_err(internal)?;
        helpers::ensure_action_urls_allowed(&arguments)?;
        self.action_tool("action_perform", "action_perform", arguments)
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
        if let Some(url) = &req.0.url {
            helpers::ensure_url_allowed(url)?;
        }
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
        let browser = state.browser.clone();
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let selector = if let Some(target) = req.0.click.as_deref() {
            match eoka_server::downloads::resolve_click(
                &*self.agent.lock().await,
                &tab.page,
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
        if let Some(url) = &req.0.url {
            helpers::ensure_url_allowed(url)?;
        }
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
        description = "Navigate to a URL and return a durable operation record. operation_id is optional and is generated when omitted. Launches browser on first call. \
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
            .unwrap_or_else(operations::new_operation_id);
        helpers::ensure_url_allowed(&req.0.url)?;
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
        let budget = options.timeout_ms.min(60_000);
        eoka_server::engine::blockers::watch(&page);
        let record = self
            .operations
            .submit(page.clone(), id.clone(), action, options)
            .await;
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                return protocol_tool_response(eoka_server::operations::commands::failure(
                    error,
                    Some(id),
                ))
            }
        };
        let record = if matches!(record.state, eoka_protocol::OperationState::Running) {
            match eoka_server::engine::blockers::wait_unless_dialog(
                &self.operations,
                &page,
                &id,
                budget,
            )
            .await
            {
                Ok(record) => record,
                Err(error) => {
                    return protocol_tool_response(eoka_server::operations::commands::failure(
                        eoka_server::operations::commands::uncertain(error),
                        Some(id),
                    ))
                }
            }
        } else {
            record
        };
        protocol_tool_response(eoka_protocol::Response::action_result(record))
    }

    #[tool(
        description = "List interactive elements. Optional filter: 'inputs' (form elements), 'buttons' (clickables), 'all'. Optional max limit. Use live targeting (text:, css:) to skip observe."
    )]
    async fn observe(&self, req: Parameters<ObserveRequest>) -> Result<CallToolResult, ErrorData> {
        observe::validate_filter(req.0.filter.as_deref()).map_err(invalid)?;
        self.engine_tool("observe", Operation::Observe(req.0)).await
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
        self.launched_tool("snapshot", Operation::Snapshot(req.0))
            .await
    }

    #[tool(
        description = "Click an element. Target: index (0), @e1 (snapshot ref), text:Submit, placeholder:Search, role:button, css:selector, id:my-btn, or plain text. Returns a durable operation ID. Never replays input."
    )]
    async fn click(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "click",
            "click",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(
        description = "Type text into an input. Target: index, text:Label, placeholder:Enter code, css:input, id:field. Returns a durable operation ID. Never replays input. Clears existing text."
    )]
    async fn fill(&self, req: Parameters<FillRequest>) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "fill",
            "fill",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(
        description = "Select dropdown option. Target: index, text:Label, css:select, id:dropdown. Value matches option value or visible text."
    )]
    async fn select(&self, req: Parameters<SelectRequest>) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "select",
            "select",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(
        description = "Hover over element to trigger tooltips, menus, or hover states. Target: index, text:Label, css:selector, etc."
    )]
    async fn hover(&self, req: Parameters<TargetRequest>) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "hover",
            "hover",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(
        description = "Press keyboard key. Common: Enter, Tab, Escape, ArrowDown, ArrowUp, Backspace, Space."
    )]
    async fn type_key(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "type_key",
            "key",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool]
    async fn mouse_down(
        &self,
        req: Parameters<MouseButtonRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("mouse_down", Operation::MouseDown(req.0))
            .await
    }

    #[tool]
    async fn mouse_move(
        &self,
        req: Parameters<MouseMoveRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("mouse_move", Operation::MouseMove(req.0))
            .await
    }

    #[tool]
    async fn mouse_up(
        &self,
        req: Parameters<MouseButtonRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("mouse_up", Operation::MouseUp(req.0))
            .await
    }

    #[tool]
    async fn key_down(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("key_down", Operation::KeyDown(req.0))
            .await
    }

    #[tool]
    async fn key_up(&self, req: Parameters<TypeKeyRequest>) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("key_up", Operation::KeyUp(req.0)).await
    }

    #[tool]
    async fn release_all_inputs(&self) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("release_all_inputs", Operation::ReleaseAllInputs)
            .await
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
        self.engine_tool("scroll", Operation::Scroll(req.0)).await
    }

    #[tool(
        description = "Find elements by text content (case-insensitive). Returns matching elements with indices."
    )]
    async fn find_text(
        &self,
        req: Parameters<FindTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("find_text", Operation::Find(req.0)).await
    }

    #[tool(
        description = "Invoke a JavaScript function with JSON arguments and return its awaited JSON result. Use code or file for the function; raw=true explicitly selects the raw script escape hatch."
    )]
    async fn extract(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = helpers::resolve_js_source(&req.0)?;
        let value: Value = if req.0.no_await {
            tab.page.evaluate_sync(&code).await
        } else if req.0.raw {
            tab.page.evaluate(&code).await
        } else {
            tab.page.evaluate_function(&code, &req.0.arguments).await
        }
        .map_err(internal)?;
        let size = serde_json::to_vec(&value).map_err(internal)?.len();
        let maximum = req.0.max_size.unwrap_or(1_048_576).min(1_048_576);
        if size > maximum {
            return Err(invalid("Evaluation result exceeds its JSON byte limit"));
        }
        protocol_tool_response(eoka_protocol::Response::ok(value))
    }

    #[tool(
        description = "Invoke and await a JavaScript function with JSON arguments, propagating exceptions. Use code or file; raw=true explicitly selects the raw script escape hatch."
    )]
    async fn exec(&self, req: Parameters<JsRequest>) -> Result<CallToolResult, ErrorData> {
        let guard = self.state.lock().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
        let code = helpers::resolve_js_source(&req.0)?;
        if req.0.no_await {
            tab.page.execute_sync(&code).await.map_err(internal)?;
            return protocol_tool_response(eoka_protocol::Response::err_detail(eoka_protocol::ErrorDetail::new("outcome_unknown", "Raw execution dispatched; asynchronous completion was not awaited. Inspect the page before further input.")));
        }
        if req.0.raw {
            tab.page.execute(&code).await.map_err(internal)?;
        } else {
            tab.page
                .execute_function(&code, &req.0.arguments)
                .await
                .map_err(internal)?;
        }
        text_ok("Executed successfully")
    }

    #[tool(
        description = "Make an HTTP request from inside the browser session. Uses the browser's real cookies, TLS fingerprint, and Cloudflare clearance — bypasses bot detection that blocks curl. \
        Returns status, headers, and body. Use instead of Bash(curl) for any target protected by Cloudflare or requiring authentication. \
        Set redirect='manual' to capture redirect URLs without following (useful for OAuth redirect_uri testing)."
    )]
    async fn fetch(&self, req: Parameters<FetchRequest>) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("fetch", Operation::Fetch(req.0)).await
    }

    #[tool(
        description = "Get all visible text on the page. Useful for reading content without elements."
    )]
    async fn page_text(&self) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("page_text", Operation::Text).await
    }

    #[tool(description = "Get current URL and page title.")]
    async fn page_info(&self) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("page_info", Operation::Info).await
    }

    #[tool(description = "Go back in browser history.")]
    async fn back(
        &self,
        req: Parameters<eoka_protocol::ActionSettings>,
    ) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "back",
            "back",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(description = "Go forward in browser history.")]
    async fn forward(
        &self,
        req: Parameters<eoka_protocol::ActionSettings>,
    ) -> Result<CallToolResult, ErrorData> {
        self.action_tool(
            "forward",
            "forward",
            serde_json::to_value(req.0).map_err(internal)?,
        )
        .await
    }

    #[tool(
        description = "Detect SPA router type and current route state. Returns router type (React Router, Next.js, Vue Router, etc.), current path, query params, and whether programmatic navigation is available."
    )]
    async fn spa_info(&self) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("spa_info", Operation::SpaInfo).await
    }

    #[tool(
        description = "Navigate SPA to a new path without page reload. Uses the detected router (React Router, Next.js, Vue Router, etc.) or falls back to History API. Much faster than full page navigation for SPAs."
    )]
    async fn spa_navigate(
        &self,
        req: Parameters<SpaNavigateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("spa_navigate", Operation::SpaNavigate(req.0))
            .await
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
            .unwrap_or_else(operations::new_operation_id);
        self.action_tool("history_go", "action_perform",serde_json::json!({"operation_id":id,"action":{"kind":"history","delta":req.0.delta},"options":{"timeout_ms":req.0.operation.timeout_ms.unwrap_or(30000),"expect":req.0.operation.expect}})).await
    }

    #[tool(description = "Get all cookies for the current page. Returns JSON array of cookies.")]
    async fn cookies(&self) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("cookies", Operation::Cookies).await
    }

    #[tool(description = "Set a cookie. Useful for restoring sessions or authentication.")]
    async fn set_cookie(
        &self,
        req: Parameters<SetCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let SetCookieRequest {
            name,
            value,
            domain,
            path,
        } = req.0;
        let cookie = eoka_protocol::SetCookieArgs {
            name,
            value,
            domain,
            path,
        };
        self.engine_tool("set_cookie", Operation::SetCookie(cookie))
            .await
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

    #[tool]
    async fn captcha_turnstile(
        &self,
        req: Parameters<eoka_protocol::CaptchaTurnstileArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.launched_tool("captcha_turnstile", Operation::CaptchaTurnstile(req.0))
            .await
    }

    #[tool(
        description = "Detect hCaptcha or reCAPTCHA on the current page. Returns captcha type and sitekey."
    )]
    async fn detect_captcha(
        &self,
        _req: Parameters<DetectCaptchaRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("detect_captcha", Operation::CaptchaDetect)
            .await
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
        let wait = eoka_protocol::WaitArgs {
            ms: Some(req.0.ms),
            ..Default::default()
        };
        self.engine_tool("wait_ms", Operation::Wait(wait)).await
    }

    #[tool(
        description = "Install a page navigation diagnostic and admit a durable click. Requires an explicit expect condition. Returns the operation record; wait with operation_wait, then inspect window.__eoka_nav using extract. Intercepts history.pushState/replaceState; location navigation cannot be reliably intercepted by page JavaScript. allow_nav=true preserves history navigation."
    )]
    async fn click_intercept_nav(
        &self,
        req: Parameters<ClickInterceptNavRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let id = req
            .0
            .operation
            .operation_id
            .clone()
            .unwrap_or_else(operations::new_operation_id);
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
            return self
                .action_tool("click_intercept_nav", "click", arguments)
                .await;
        }
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;
        let tab = state
            .current_tab_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;
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
        self.action_tool("click_intercept_nav", "click", arguments)
            .await
    }

    #[tool(description = "Delete a specific cookie by name.")]
    async fn delete_cookie(
        &self,
        req: Parameters<DeleteCookieRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let cookie = eoka_protocol::DeleteCookieArgs {
            name: req.0.name,
            domain: req.0.domain,
        };
        self.engine_tool("delete_cookie", Operation::DeleteCookie(cookie))
            .await
    }

    #[tool(description = "Clear all cookies for the current browser context.")]
    async fn clear_cookies(&self) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("clear_cookies", Operation::ClearCookies)
            .await
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
        let wait = eoka_protocol::WaitArgs {
            text: Some(req.0.text),
            timeout: req.0.timeout_ms,
            ..Default::default()
        };
        self.engine_tool("wait_for_text", Operation::Wait(wait))
            .await
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
        self.storage_get("local_storage_get", req.0.key, false)
            .await
    }

    #[tool(description = "Set a value in localStorage.")]
    async fn local_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.storage_set("local_storage_set", req.0, false).await
    }

    #[tool(description = "Get a value from sessionStorage. Returns null message if key not found.")]
    async fn session_storage_get(
        &self,
        req: Parameters<StorageKeyRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.storage_get("session_storage_get", req.0.key, true)
            .await
    }

    #[tool(description = "Set a value in sessionStorage.")]
    async fn session_storage_set(
        &self,
        req: Parameters<StorageSetRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.storage_set("session_storage_set", req.0, true).await
    }

    #[tool(
        description = "Dump all client-side storage as JSON: localStorage, sessionStorage, and cookies. Use to capture session state for later restore or analysis."
    )]
    async fn dump_storage(&self) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("dump_storage", Operation::DumpStorage)
            .await
    }

    #[tool(
        description = "Read captured console output (log/warn/error/info/debug). Auto-injects capture on first call."
    )]
    async fn console(&self, req: Parameters<ConsoleRequest>) -> Result<CallToolResult, ErrorData> {
        let console = eoka_protocol::ConsoleArgs {
            clear: req.0.clear.unwrap_or(false),
            level: req.0.level,
        };
        self.launched_tool("console", Operation::Console(console))
            .await
    }

    #[tool(
        description = "Read captured JavaScript errors (uncaught exceptions and unhandled promise rejections). Auto-injects capture on first call."
    )]
    async fn errors(&self, req: Parameters<ErrorsRequest>) -> Result<CallToolResult, ErrorData> {
        let errors = eoka_protocol::ClearFlagArgs {
            clear: req.0.clear.unwrap_or(false),
        };
        self.launched_tool("errors", Operation::Errors(errors))
            .await
    }

    #[tool(
        description = "Save browser auth state (cookies + localStorage + sessionStorage) to a JSON file. Use after login to persist sessions across restarts. Captures httpOnly cookies via CDP for full fidelity. The path must end in .json and can only replace an empty file or an earlier state file."
    )]
    async fn save_state(
        &self,
        req: Parameters<SaveStateRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.engine_tool("save_state", Operation::SaveState(req.0))
            .await
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

        let navigate = req.0.navigate.unwrap_or(true);
        if navigate {
            helpers::ensure_url_allowed(&saved.url)?;
        }
        self.ensure_browser().await?;
        let mut guard = self.state.lock().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| ErrorData::from(AgentError::NoBrowser))?;

        if navigate {
            let tab = state.ensure_tab(&saved.url).await.map_err(internal)?;
            helpers::wait_for_stable(&tab.page)
                .await
                .map_err(internal)?;
        }

        let tab = state
            .current_tab()
            .ok_or_else(|| ErrorData::from(AgentError::NoTab))?;

        let restored = helpers::restore_state(&tab.page, &saved).await?;

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
            "Loaded {} cookies, {} localStorage keys, {} sessionStorage keys from {}.{}",
            saved.cookies.len(),
            restored.local_restored,
            restored.session_restored,
            req.0.path,
            restored.notice(&saved.url)
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
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tool = request.name.to_string();
        let arguments = request
            .arguments
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null);
        let policy = blocked::policy_for_tool(&tool, &arguments);
        let page = match self.gate_page(&tool).await {
            Ok(page) => page,
            Err(busy) => return Ok(busy.into()),
        };
        if let Some(page) = &page {
            if let Some(refusal) = self.refuse_blocked(&tool, policy, page).await {
                return Ok(refusal.into());
            }
        }
        self.started(&tool);
        let context = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let call = self.tool_router.call(context);
        use eoka_server::engine::blockers::{self, Policy};
        let result = match &page {
            Some(page)
                if matches!(
                    policy,
                    Policy::Read | Policy::Script | Policy::Wait | Policy::Observe
                ) =>
            {
                match blockers::race(page, policy, None, true, call).await {
                    Ok(result) => result,
                    Err(stop) => Ok(blocked::stopped(&tool, policy, &stop).into()),
                }
            }
            _ => call.await,
        };
        self.finished();
        self.explain_blocked(&tool, policy, result).await
    }

    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
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

    #[test]
    fn every_catalog_operation_is_an_mcp_tool() {
        let router = {
            let mut router = EokaServer::tool_router();
            apply_protocol_tool_metadata(&mut router);
            add_catalog_tools(&mut router, true);
            router
        };
        for (tool, command) in MCP_OWN_TOOL_COMMANDS.iter().chain(MCP_PROTOCOL_TOOL_ROUTES) {
            assert!(router.map.contains_key(*tool), "{tool} is not registered");
            assert!(
                operation_by_cmd(command).is_some(),
                "{command} is not cataloged"
            );
        }
        let covered: std::collections::HashSet<&str> = MCP_PROTOCOL_TOOL_ROUTES
            .iter()
            .chain(MCP_OWN_TOOL_COMMANDS)
            .map(|(_, command)| *command)
            .collect();
        // Every operation that acts on a page, including those the sidecar
        // serves under a method of its own such as recording.
        for operation in eoka_protocol::sidecar::page_operations() {
            let name = operation.name.replace('.', "_");
            assert!(
                covered.contains(operation.cmd) || router.map.contains_key(name.as_str()),
                "catalog operation {} has no MCP tool",
                operation.cmd
            );
        }
        // Opt-in operations stay out of the default tool list.
        let mut default_router = EokaServer::tool_router();
        add_catalog_tools(&mut default_router, false);
        assert!(default_router.map.contains_key("content"));
        assert!(default_router.map.contains_key("reload"));
        for opt_in in [
            "click_shadow",
            "recording_start",
            "clone_from",
            "fake_camera",
        ] {
            assert!(!default_router.map.contains_key(opt_in), "{opt_in}");
            assert!(router.map.contains_key(opt_in), "{opt_in}");
        }
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
