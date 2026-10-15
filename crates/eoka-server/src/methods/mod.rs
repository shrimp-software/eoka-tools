pub mod browser;
pub mod page;

use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) use eoka_protocol::sidecar::PageIdParams;
use eoka_protocol::sidecar::ServerError;

pub(crate) fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, ServerError> {
    serde_json::from_value(params).map_err(|e| ServerError::invalid_params(e.to_string()))
}
