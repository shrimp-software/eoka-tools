use serde_json::Value;

use crate::methods::{blocked, browser, page};
use crate::state::AppState;
use eoka_protocol::sidecar::{
    catalog_operation, catalog_request, method_names, validate_params, validate_safe_numbers,
    AgentObserveParams, AgentStepParams, ErrorCode, HelloParams, HelloResult, Method, ServerError,
    TransportLimits, CAPABILITIES, MAX_IN_FLIGHT, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
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
    if let Some(feature) = eoka_protocol::sidecar::method_feature(method)
        .filter(|feature| !crate::features::built(feature))
    {
        return Err(ServerError::new(
            ErrorCode::UnknownMethod,
            crate::features::unavailable_message(feature),
        ));
    }
    let parsed = Method::parse(method);
    let addressed = match parsed {
        Some(Method::ServerHello | Method::ServerShutdown) => None,
        None if catalog_operation(method).is_some() => None,
        _ => params
            .get("pageId")
            .and_then(Value::as_str)
            .and_then(|id| state.page(id).ok()),
    };
    match addressed {
        Some(page) => {
            let call = blocked::Call::new(parsed, method, &params);
            let routed = Box::pin(route(state, parsed, method, params));
            blocked::guarded(&state.operations, &page, call, routed).await
        }
        None => Box::pin(route(state, parsed, method, params)).await,
    }
}

pub fn clears_a_blocked_page(state: &AppState, method: &str, params: &Value) -> bool {
    match Method::parse(method) {
        Some(Method::BrowserCloseTab) => params
            .get("pageId")
            .and_then(Value::as_str)
            .and_then(|id| state.page(id).ok())
            .is_some_and(|page| crate::engine::blockers::current(&page).is_some()),
        Some(_) => false,
        None => matches!(method, "dialog.accept" | "dialog.dismiss"),
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
    let arguments = request.args_json();
    let accept = (method == "dialog.accept").then_some(arguments["prompt_text"].as_str());
    let reply = crate::engine::blockers::answer(&state.operations, &page, accept).await?;
    Ok(Value::String(reply))
}

async fn route(
    state: &AppState,
    parsed: Option<Method>,
    method: &str,
    params: Value,
) -> Result<Value, ServerError> {
    if let Some(method) = parsed {
        return run(state, method, params).await;
    }
    if matches!(method, "dialog.accept" | "dialog.dismiss") {
        return answer_dialog(state, method, &params).await;
    }
    let operation = catalog_operation(method).ok_or_else(|| ServerError::unknown_method(method))?;
    let (page_id, request) = catalog_request(operation, &params)?;
    state.catalog(&page_id, request, None).await
}

async fn run(state: &AppState, method: Method, params: Value) -> Result<Value, ServerError> {
    use crate::operations::sidecar::{self, OperationControl};
    use crate::recordings::{self, RecordingControl};
    match method {
        Method::ServerHello => hello(params),
        Method::ServerShutdown => {
            validate_params(method.name(), &params)?;
            state.shutdown().await?;
            Ok(serde_json::json!({}))
        }
        Method::BrowserLaunch => browser::launch(state, params).await,
        Method::BrowserConnect => browser::connect(state, params).await,
        Method::BrowserNewPage => browser::new_page(state, params).await,
        Method::BrowserTabs => {
            validate_params(method.name(), &params)?;
            browser::tabs(state, params).await
        }
        Method::BrowserCloseTab => {
            if let Some(page_id) = params.get("pageId").and_then(Value::as_str) {
                state.operations.stop_target(page_id).await;
            }
            browser::close_tab(state, params).await
        }
        Method::BrowserClose => {
            validate_params(method.name(), &params)?;
            browser::close(state, params).await
        }
        Method::ActionsPerform => sidecar::perform(state, params).await,
        Method::OperationsGet => {
            sidecar::control(&state.operations, OperationControl::Get, params).await
        }
        Method::OperationsList => {
            sidecar::control(&state.operations, OperationControl::List, params).await
        }
        Method::OperationsWait => {
            sidecar::control(&state.operations, OperationControl::Wait, params).await
        }
        Method::OperationsCancel => {
            sidecar::control(&state.operations, OperationControl::Cancel, params).await
        }
        Method::AgentObserve => {
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
                .map_err(|error| agent_error(error, method.name(), &page))?;
            serde_json::to_value(result).map_err(|error| ServerError::internal(error.to_string()))
        }
        Method::AgentStep => {
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
                .map_err(|error| agent_error(error, method.name(), &page))?;
            drop(browser);
            if let Some(observation) = &result.observation {
                if let Err(error) = state.attach_page(&observation.tab_id).await {
                    result.observation_error =
                        Some(format!("Destination attachment failed: {}", error.message));
                }
            }
            serde_json::to_value(result).map_err(|error| ServerError::internal(error.to_string()))
        }
        Method::PageCaptureState => page::capture_state(state, params).await,
        Method::PageRestoreState => page::restore_state(state, params).await,
        Method::RecordingStart => {
            let params: eoka_protocol::sidecar::RecordingStartParams =
                serde_json::from_value(params)
                    .map_err(|error| ServerError::invalid_params(error.to_string()))?;
            let page = state.page(&params.page_id);
            let record = state.recordings.start(params, page).await?;
            serde_json::to_value(record).map_err(|error| ServerError::internal(error.to_string()))
        }
        Method::RecordingGet => {
            recordings::control(&state.recordings, RecordingControl::Get, params).await
        }
        Method::RecordingStop => {
            recordings::control(&state.recordings, RecordingControl::Stop, params).await
        }
        Method::RecordingCancel => {
            recordings::control(&state.recordings, RecordingControl::Cancel, params).await
        }
        Method::PageSolveCaptcha => solve_captcha(state, params).await,
    }
}

#[cfg(feature = "anti-captcha")]
async fn solve_captcha(state: &AppState, params: Value) -> Result<Value, ServerError> {
    page::solve_captcha(state, params).await
}

#[cfg(not(feature = "anti-captcha"))]
async fn solve_captcha(_: &AppState, _: Value) -> Result<Value, ServerError> {
    Err(ServerError::unknown_method(Method::PageSolveCaptcha.name()))
}

pub fn hello(params: Value) -> Result<Value, ServerError> {
    validate_safe_numbers(&params)?;
    let params: HelloParams = serde_json::from_value(params)
        .map_err(|error| ServerError::invalid_params(error.to_string()))?;
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
        methods: method_names()
            .into_iter()
            .filter(|method| crate::features::method_built(method))
            .map(str::to_owned)
            .collect(),
        capabilities: CAPABILITIES.iter().copied().map(str::to_owned).collect(),
        limits: TransportLimits {
            max_message_bytes: MAX_MESSAGE_BYTES,
            max_in_flight: MAX_IN_FLIGHT,
        },
    })
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

#[cfg(test)]
mod stack_tests {
    use super::*;

    #[test]
    fn a_call_is_small_enough_to_run_on_a_worker_thread_stack() {
        let state = AppState::new();
        let sidecar = dispatch_shared(&state, "text", Value::Null);
        assert!(std::mem::size_of_val(&sidecar) < 16 * 1024);
        let catalog = state.catalog("page", eoka_protocol::Request::Info, None);
        assert!(std::mem::size_of_val(&catalog) < 16 * 1024);
    }
}
