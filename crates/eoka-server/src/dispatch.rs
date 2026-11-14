use serde_json::Value;

use crate::methods::{blocked, browser, page};
use crate::state::AppState;
use eoka_protocol::sidecar::{
    catalog_operation, catalog_request, method_names, validate_params, validate_safe_numbers,
    AgentObserveParams, AgentStepParams, ErrorCode, HelloParams, HelloResult, ProtocolVersion,
    ServerError, TransportLimits, CAPABILITY_CLOUDFLARE_AUTO_SOLVE, CAPABILITY_NATIVE_RECORDING,
    CAPABILITY_OPERATION_CATALOG, CAPABILITY_PAGE_BLOCKERS, CAPABILITY_SCREENSHOT_FILE,
    CAPABILITY_SCREENSHOT_OPTIONS, MAX_IN_FLIGHT, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};

pub async fn dispatch(
    state: &mut AppState,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    dispatch_shared(state, method, params).await
}

pub async fn dispatch_shared(
    state: &AppState,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    validate_safe_numbers(&params)?;
    if method == "server.hello" {
        return hello(params);
    }
    if method == "server.shutdown" {
        validate_params(method, &params)?;
        state.shutdown().await?;
        return Ok(serde_json::json!({}));
    }
    let addressed = if catalog_operation(method).is_some() {
        None
    } else {
        params
            .get("pageId")
            .and_then(Value::as_str)
            .and_then(|id| state.page(id).ok())
    };
    match addressed {
        Some(page) => {
            let call = blocked::Call::new(method, &params);
            blocked::guarded(&state.operations, &page, call, route(state, method, params)).await
        }
        None => route(state, method, params).await,
    }
}

pub fn clears_a_blocked_page(state: &AppState, method: &str, params: &Value) -> bool {
    match method {
        "dialog.accept" | "dialog.dismiss" => true,
        "page.close" | "browser.close_tab" => params
            .get("pageId")
            .and_then(Value::as_str)
            .and_then(|id| state.page(id).ok())
            .is_some_and(|page| crate::engine::blockers::current(&page).is_some()),
        _ => false,
    }
}

async fn answer_dialog(
    state: &AppState,
    method: &str,
    params: &Value,
) -> Result<Value, ServerError> {
    let operation = catalog_operation(method).ok_or_else(|| ServerError::unknown_method(method))?;
    let (page_id, request) = catalog_request(operation, params)?;
    let page = state.page(&page_id)?;
    if method == "dialog.accept" {
        page.accept_dialog(request.args_json()["prompt_text"].as_str())
            .await?;
        Ok(Value::String("Dialog accepted".into()))
    } else {
        page.dismiss_dialog().await?;
        Ok(Value::String("Dialog dismissed".into()))
    }
}

async fn route(state: &AppState, method: &str, params: Value) -> Result<Value, ServerError> {
    if matches!(method, "dialog.accept" | "dialog.dismiss") {
        return answer_dialog(state, method, &params).await;
    }
    if method == "agent.observe" {
        let args: AgentObserveParams = serde_json::from_value(params)
            .map_err(|error| ServerError::invalid_params(error.to_string()))?;
        let browser = state.browser()?;
        let page = state.page(&args.page_id)?;
        let agent = state.agent.clone();
        let result = agent
            .lock()
            .await
            .observe(&browser, &page, args.args)
            .await
            .map_err(|error| agent_error(error, method, &page))?;
        return serde_json::to_value(result)
            .map_err(|error| ServerError::internal(error.to_string()));
    }
    if method == "agent.step" {
        let args: AgentStepParams = serde_json::from_value(params)
            .map_err(|error| ServerError::invalid_params(error.to_string()))?;
        let browser = state.browser()?;
        let page = state.page(&args.page_id)?;
        let agent = state.agent.clone();
        let mut result = agent
            .lock()
            .await
            .step(&browser, &page, &state.operations, args.args)
            .await
            .map_err(|error| agent_error(error, method, &page))?;
        drop(browser);
        if let Some(observation) = &result.observation {
            if let Err(error) = state.attach_page(&observation.tab_id).await {
                result.observation_error =
                    Some(format!("Destination attachment failed: {}", error.message));
            }
        }
        return serde_json::to_value(result)
            .map_err(|error| ServerError::internal(error.to_string()));
    }
    if crate::operations::sidecar::supports(method) {
        return crate::operations::sidecar::dispatch(state, method, params).await;
    }
    if crate::recordings::supports_control(method) {
        return crate::recordings::control(&state.recordings, method, params).await;
    }
    if let Some(operation) = catalog_operation(method) {
        let (page_id, request) = catalog_request(operation, &params)?;
        return state.catalog(&page_id, operation, &request).await;
    }
    if matches!(method, "browser.tabs" | "browser.close") {
        validate_params(method, &params)?;
    }
    if matches!(method, "browser.close_tab" | "page.close") {
        if let Some(page_id) = params.get("pageId").and_then(Value::as_str) {
            state.operations.stop_target(page_id).await;
        }
    }
    match method {
        "browser.launch" => browser::launch(state, params).await,
        "browser.connect" => browser::connect(state, params).await,
        "browser.new_page" => browser::new_page(state, params).await,
        "browser.tabs" => browser::tabs(state, params).await,
        "browser.close_tab" => browser::close_tab(state, params).await,
        "browser.close" => browser::close(state, params).await,

        "page.text" => page::text(state, params).await,
        "page.content" => page::content(state, params).await,
        "page.title" => page::title(state, params).await,
        "page.url" => page::url(state, params).await,
        "page.get_text" => page::get_text(state, params).await,
        "page.get_attribute" => page::get_attribute(state, params).await,
        "page.exists" => page::exists(state, params).await,
        "page.wait_for" => page::wait_for(state, params).await,
        "page.wait_for_visible" => page::wait_for_visible(state, params).await,
        "page.wait_for_text" => page::wait_for_text(state, params).await,
        "page.evaluate" => page::evaluate(state, params).await,
        "page.execute" => page::execute(state, params).await,
        "page.evaluate_raw" => page::evaluate_raw(state, params).await,
        "page.execute_raw" => page::execute_raw(state, params).await,
        "page.frames" => page::frames(state, params).await,
        "page.evaluate_in_frame" => page::evaluate_in_frame(state, params).await,
        "page.click_shadow" => page::click_shadow(state, params).await,
        "page.solve_turnstile" => page::solve_turnstile(state, params).await,
        "page.fetch" => page::fetch(state, params).await,
        "page.fetch_bounded" => page::fetch_bounded(state, params).await,
        "page.capture_state" => page::capture_state(state, params).await,
        "page.restore_state" => page::restore_state(state, params).await,
        "page.screenshot" => page::screenshot(state, params).await,
        "page.screenshot_to_file" => page::screenshot_to_file(state, params).await,
        "recording.start" => {
            let params: eoka_protocol::sidecar::RecordingStartParams =
                serde_json::from_value(params)
                    .map_err(|error| ServerError::invalid_params(error.to_string()))?;
            let page = state.page(&params.page_id);
            let record = state.recordings.start(params, page).await?;
            serde_json::to_value(record).map_err(|error| ServerError::internal(error.to_string()))
        }
        "page.set_viewport" => page::set_viewport(state, params).await,
        "page.reset_viewport" => page::reset_viewport(state, params).await,
        "page.mouse_down" => page::mouse_down(state, params).await,
        "page.mouse_move" => page::mouse_move(state, params).await,
        "page.mouse_up" => page::mouse_up(state, params).await,
        "page.key_down" => page::key_down(state, params).await,
        "page.key_up" => page::key_up(state, params).await,
        "page.release_all_inputs" => page::release_all_inputs(state, params).await,
        "page.solve_captcha" => page::solve_captcha(state, params).await,
        "page.close" => page::close(state, params).await,

        _ => Err(ServerError::unknown_method(method)),
    }
}

pub fn hello(params: Value) -> Result<Value, ServerError> {
    negotiate(params).map(|(_, greeting)| greeting)
}

pub fn negotiate(params: Value) -> Result<(ProtocolVersion, Value), ServerError> {
    validate_safe_numbers(&params)?;
    let params: HelloParams = serde_json::from_value(params)
        .map_err(|error| ServerError::invalid_params(error.to_string()))?;
    let spoken = params.spoken();
    if params.protocol.major != PROTOCOL_VERSION.major
        || params.protocol.minor > PROTOCOL_VERSION.minor
    {
        return Err(ServerError::new(
            ErrorCode::ProtocolMismatch,
            format!(
                "unsupported protocol {}.{}; server supports {}.{}",
                params.protocol.major,
                params.protocol.minor,
                PROTOCOL_VERSION.major,
                PROTOCOL_VERSION.minor
            ),
        ));
    }
    serde_json::to_value(HelloResult {
        protocol: PROTOCOL_VERSION,
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: env!("EOKA_CORE_VERSION").to_owned(),
        methods: method_names().into_iter().map(str::to_owned).collect(),
        capabilities: vec![
            "durableActions".to_owned(),
            "operationCancellation".to_owned(),
            "gracefulShutdown".to_owned(),
            "explicitExecutablePath".to_owned(),
            "browserAttachment".to_owned(),
            "runtimeIsolation".to_owned(),
            "sandboxControl".to_owned(),
            "viewportEmulation".to_owned(),
            "boundedFetch".to_owned(),
            CAPABILITY_SCREENSHOT_OPTIONS.to_owned(),
            CAPABILITY_SCREENSHOT_FILE.to_owned(),
            CAPABILITY_NATIVE_RECORDING.to_owned(),
            CAPABILITY_CLOUDFLARE_AUTO_SOLVE.to_owned(),
            CAPABILITY_OPERATION_CATALOG.to_owned(),
            CAPABILITY_PAGE_BLOCKERS.to_owned(),
        ],
        limits: TransportLimits {
            max_message_bytes: MAX_MESSAGE_BYTES,
            max_in_flight: MAX_IN_FLIGHT,
        },
    })
    .map(|greeting| (spoken, greeting))
    .map_err(|error| ServerError::internal(error.to_string()))
}

fn agent_error(error: anyhow::Error, method: &str, page: &eoka::Page) -> ServerError {
    match error.downcast::<eoka::Error>() {
        Ok(error) => error.into(),
        Err(error) => {
            let message = format!("{error:#}");
            let named = message
                .split_once(": ")
                .filter(|(code, _)| ErrorCode::for_blocker_detail(code).is_some());
            match named {
                Some((code, description)) => blocked::named(code, description, method, page),
                None => ServerError::internal(message),
            }
        }
    }
}
