mod blocked;
#[cfg(test)]
mod decoded;
mod error;
mod helpers;
mod reply;
mod routes;
#[cfg(feature = "anti-captcha")]
mod solve_captcha;
#[cfg(test)]
mod surface;
#[cfg(test)]
mod through_client;

use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext},
    model::*,
    tool_handler, ServerHandler,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use eoka_protocol::{OperationId, OperationNeeds, OperationReply, Request as Operation};

use error::{internal, invalid, is_transport_error_msg, AgentError};

#[derive(Clone)]
pub struct EokaServer {
    engine: Arc<Mutex<eoka_server::engine::Engine>>,
    unhealthy: Arc<std::sync::atomic::AtomicBool>,
    running: Arc<std::sync::Mutex<Option<(String, std::time::Instant)>>>,
    busy_limit: std::time::Duration,
    turn: Arc<Mutex<()>>,
    headless: bool,
    tool_router: ToolRouter<EokaServer>,
    downloads: eoka_server::downloads::DownloadManager,
    operations: eoka_server::operations::OperationManager,
}

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
        eoka_server::engine::blockers::set_surface(eoka_server::engine::blockers::Surface::Mcp);
        let mut engine = eoka_server::engine::Engine::new("mcp", launch_spec(headless, None));
        engine.patch_binary(true);
        Self {
            operations: engine.operation_manager(),
            downloads: engine.download_manager(),
            engine: Arc::new(Mutex::new(engine)),
            unhealthy: Default::default(),
            running: Default::default(),
            busy_limit: blocked::BUSY_LIMIT,
            turn: Default::default(),
            headless,
            tool_router: routes::router(include_opt_in),
        }
    }

    async fn catalog_tool(
        &self,
        tool: &str,
        route: routes::Route,
        arguments: Option<JsonObject>,
    ) -> Result<CallToolResult, ErrorData> {
        let operation = route.operation;
        let given = Value::Object(arguments.unwrap_or_default());
        if let Some(error) = route.unknown_argument(&given) {
            return Ok(CallToolResult::error(vec![ContentBlock::text(error)]));
        }
        let arguments = route.arguments(&given);
        let request = match eoka_protocol::request_from_cmd(operation.cmd, arguments.clone()) {
            Ok(request) => request,
            Err(error) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "failed to deserialize parameters: {error}"
                ))]))
            }
        };
        helpers::ensure_action_urls_allowed(&arguments)?;
        if operation.lock_free {
            return reply::envelope(self.query(operation.cmd, &arguments, &request).await);
        }
        if operation.id == OperationId::Close {
            self.unhealthy
                .store(false, std::sync::atomic::Ordering::Release);
        }
        if operation.auto_launch {
            self.ensure_browser().await?;
        }
        if operation.reply == OperationReply::Action {
            return self.action(tool, operation.cmd, arguments).await;
        }
        let (response, page) = self.respond(tool, request, operation.needs).await?;
        match operation.reply {
            OperationReply::Envelope => reply::envelope(response),
            OperationReply::Image => reply::image(response, page.as_ref()),
            _ => reply::unwrapped(response, page.as_ref()),
        }
    }

    async fn query(
        &self,
        cmd: &str,
        arguments: &Value,
        request: &Operation,
    ) -> eoka_protocol::Response {
        let response =
            match self.operations.query(cmd, arguments).await {
                Some(response) => response,
                None => self.downloads.query(request).await.unwrap_or_else(|| {
                    eoka_protocol::Response::err(format!("Unknown command: {cmd}"))
                }),
            };
        #[cfg(test)]
        decoded::record(cmd, &response);
        response
    }

    async fn action(
        &self,
        tool: &str,
        cmd: &str,
        mut arguments: Value,
    ) -> Result<CallToolResult, ErrorData> {
        let operation_id = arguments["operation_id"]
            .as_str()
            .map_or_else(reply::new_operation_id, str::to_owned);
        let unverified = reply::runs_unverified(cmd, &arguments);
        if unverified {
            arguments["exploratory"] = Value::Bool(true);
        }
        let request = if cmd == "action_perform" {
            arguments["operation_id"] = Value::String(operation_id);
            eoka_protocol::request_from_cmd(cmd, arguments).map_err(invalid)?
        } else {
            Operation::ActionCommand(eoka_protocol::ActionCommandArgs {
                operation_id,
                command: cmd.to_owned(),
                arguments,
            })
        };
        let (response, _) = self.respond(tool, request, OperationNeeds::Page).await?;
        reply::action(response, unverified)
    }

    async fn respond(
        &self,
        tool: &str,
        operation: Operation,
        needs: OperationNeeds,
    ) -> Result<(eoka_protocol::Response, Option<eoka_server::eoka::Page>), ErrorData> {
        let engine = self.engine.clone();
        let spoken = tool.to_owned();
        #[cfg(test)]
        let cmd = operation.cmd();
        let answered = tokio::spawn(async move {
            let mut engine = engine.lock_owned().await;
            if needs != OperationNeeds::Nothing && !engine.is_open() {
                return Err(AgentError::NoBrowser);
            }
            if needs == OperationNeeds::Page && engine.current_page().is_none() {
                return Err(AgentError::NoTab);
            }
            let response = Box::pin(engine.handle_as(Some(&spoken), operation)).await;
            Ok((response, engine.current_page()))
        })
        .await
        .map_err(internal)?;
        let (mut response, page) = answered?;
        #[cfg(test)]
        decoded::record(cmd, &response);
        let failure = response.error.clone().unwrap_or_default();
        let answered_late = failure.to_ascii_lowercase().contains("timed out")
            && page
                .as_ref()
                .is_some_and(|page| !page.session().transport().is_closed());
        if !response.ok && is_transport_error_msg(&failure) && !answered_late {
            eprintln!("[eoka-mcp] connection lost, marking unhealthy: {failure}");
            self.unhealthy
                .store(true, std::sync::atomic::Ordering::Release);
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

    async fn ensure_browser(&self) -> Result<(), ErrorData> {
        let mut engine = self.engine.lock().await;
        if self
            .unhealthy
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            eprintln!("[eoka-mcp] browser unhealthy, relaunching...");
            let _ = Box::pin(engine.handle_as(None, Operation::Close)).await;
        }
        if !engine.is_open() {
            let proxy = helpers::parse_proxy_env().map_err(internal)?;
            Box::pin(engine.launch_with(launch_spec(self.headless, proxy)))
                .await
                .map_err(internal)?;
        }
        Ok(())
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
        if routes::route(&tool).is_some_and(|route| route.operation.lock_free) {
            let context = ToolCallContext::new(self, request, context);
            return self.tool_router.call(context).await;
        }
        let arguments = request
            .arguments
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null);
        let policy = blocked::policy_for_tool(&tool, &arguments);
        let _turn = match self.take_turn(&tool).await {
            Ok(turn) => turn,
            Err(busy) => return Ok(busy.into()),
        };
        let page = self.engine.lock().await.current_page();
        if let Some(page) = &page {
            if let Some(refusal) = self.refuse_blocked(&tool, policy, page).await {
                return Ok(refusal.into());
            }
        }
        self.started(&tool);
        let context = ToolCallContext::new(self, request, context);
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
                 TARGETING: resolved when the action runs: Submit, text:Submit, placeholder:code, css:button, id:btn, role:button, or a ref from agent_observe. Numeric indices are not accepted.\n\
                 VERIFY: click and type_key take expect, a condition that is false now and true after the action. Without it the action runs and is reported as not verified.\n\
                 BLOCKED: a reply with dialog_open, navigation_pending or navigation_timeout names the tool that clears it.\n\
                 RETRY: actions are never replayed. Send the same operation_id again to ask about an action instead of repeating it.\n\n\
                 OBSERVE: filter='inputs'|'buttons', max=N\n\
                 BATCH: batch({actions:[{operation_id:'<uuid>', action:{kind:'click', target:{query:{kind:'css', selector:'button'}}}, options:{expect:{kind:'url', value:'/done', contains:true}}}]}) — typed actions as in action_perform; stops at the first one that is not verified\n\
                 SPA: spa_info, spa_navigate, history_go\n\
                 Tabs: list_tabs, new_tab, switch_tab (auto-attaches to popups), close_tab\n\
                 TIMING: wait_ms(ms) — pause between actions\n\
                 NAV INTERCEPT: click_intercept_nav, or click with intercept_nav — durable click with a history.pushState/replaceState diagnostic\n\
                 FETCH: fetch(url, method?, headers?, body?, redirect?, max_body?) — HTTP request from browser session, bypasses Cloudflare/bot detection\n\
                 COOKIES: cookies, set_cookie, delete_cookie, clear_cookies\n\
                 DIALOGS: accept_dialog, dismiss_dialog\n\
                 DOWNLOADS: download_start(url or click, output) saves a file and returns an id for download_wait. Clicking a download link does not save the file.\n\
                 STATE: save_state(path) — persist auth state (cookies+storage) to JSON, load_state(path, no_navigate?) — restore it\n\
                 STORAGE: local_storage_get/set, session_storage_get/set, dump_storage\n\
                 CONSOLE: console(clear?, level?) — read captured console output (auto-injects on first call)\n\
                 ERRORS: errors(clear?) — read captured JS errors and unhandled rejections\n\
                 NAVIGATE EXTRAS: navigate(url, headers?, user_agent?, bypass_csp?) — headers/UA/CSP override per navigation\n\
                 WAIT: wait_ms, wait_for_text, wait_for_network_idle"
            )
    }
}

async fn cleanup_browser(engine: &Arc<Mutex<eoka_server::engine::Engine>>) {
    let mut engine = engine.lock().await;
    if engine.is_open() {
        eprintln!("[eoka-mcp] closing browser...");
        let _ = Box::pin(engine.handle_as(None, Operation::Close)).await;
        eprintln!("[eoka-mcp] browser closed.");
    }
}

fn launch_spec(headless: bool, proxy: Option<String>) -> eoka_protocol::launch_spec::LaunchSpec {
    eoka_protocol::launch_spec::LaunchSpec::Launch {
        headless,
        from_profile: None,
        clone_state_from: None,
        no_stealth: false,
        proxy,
        no_js: false,
        js_allow: Vec::new(),
        js_block: Vec::new(),
        persist: false,
        geo_align: false,
        auto_solve_cloudflare: std::env::var("EOKA_AUTO_SOLVE_CLOUDFLARE")
            .is_ok_and(|value| value == "1" || value == "true"),
        sandbox: true,
    }
}

pub async fn run_server() -> anyhow::Result<()> {
    use rmcp::ServiceExt;

    let server = EokaServer::new();
    let engine = Arc::clone(&server.engine);
    let signal_engine = Arc::clone(&engine);
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("failed to register SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => eprintln!("[eoka-mcp] received SIGTERM"),
            _ = sigint.recv() => eprintln!("[eoka-mcp] received SIGINT"),
        }
        cleanup_browser(&signal_engine).await;
        std::process::exit(0);
    });

    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    eprintln!("[eoka-mcp] MCP connection closed, cleaning up.");
    cleanup_browser(&engine).await;
    Ok(())
}
