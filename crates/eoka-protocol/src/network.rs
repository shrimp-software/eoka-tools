use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkMeta {
    pub namespace: String,
    pub active: bool,
    pub entry_count: usize,
    pub in_flight: usize,
    pub last_id: u64,
    pub next_since: u64,
    pub body_pending: usize,
    pub body_failed: usize,
    pub warnings: Vec<String>,
    pub suggested_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkStatus {
    pub active: bool,
    pub session_id: Option<String>,
    pub target_id: Option<String>,
    pub patterns: Vec<String>,
    pub entry_count: usize,
    pub in_flight: usize,
    pub captured_body_bytes: usize,
    pub body_pending: usize,
    pub body_failed: usize,
    pub body_limit_bytes: usize,
    pub per_body_limit_bytes: usize,
    pub lagged_events: u64,
    pub dropped_events: u64,
    pub started_at: Option<u64>,
    pub last_id: u64,
    pub next_since: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkRecording {
    pub meta: NetworkMeta,
    pub status: NetworkStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkCleared {
    pub cleared: bool,
    pub meta: NetworkMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkLog {
    pub meta: NetworkMeta,
    pub count: usize,
    pub entries: Vec<NetworkLogRow>,
    pub filters: NetworkLogFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkLogFilters {
    pub limit: usize,
    pub pattern: Option<String>,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub since: Option<u64>,
    pub compact: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum NetworkLogRow {
    Full(NetworkLogEntry),
    Compact(NetworkCompactEntry),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkLogEntry {
    pub id: u64,
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub resource_type: Option<String>,
    pub duration_ms: f64,
    pub request_body: Option<NetworkBodySummary>,
    pub response_body: Option<NetworkBodySummary>,
    pub complete: bool,
    pub failed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkCompactEntry {
    pub id: u64,
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkBodySummary {
    pub bytes: usize,
    pub base64: bool,
    pub omitted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkBody {
    pub bytes: usize,
    pub base64: bool,
    pub text: Option<String>,
    pub omitted: Option<String>,
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkEntryDetail<B> {
    pub id: u64,
    pub session_id: String,
    pub target_id: String,
    pub request_id: String,
    pub hop: u32,
    pub url: String,
    pub method: String,
    pub resource_type: Option<String>,
    pub started_at: Option<f64>,
    pub duration_ms: f64,
    pub request: NetworkRequestDetail<B>,
    pub response: NetworkResponseDetail<B>,
    pub failure: Option<String>,
    pub canceled: bool,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkRequestDetail<B> {
    pub headers: BTreeMap<String, String>,
    pub headers_text: Option<String>,
    pub post_data: Option<B>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkResponseDetail<B> {
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub protocol: Option<String>,
    pub mime_type: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub headers_text: Option<String>,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u64>,
    pub encoded_data_length: Option<f64>,
    pub timing: Option<BTreeMap<String, serde_json::Number>>,
    pub redirect_url: Option<String>,
    pub body: Option<B>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkShow {
    pub meta: NetworkMeta,
    pub entry: NetworkShowEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum NetworkShowEntry {
    Metadata(NetworkEntryDetail<NetworkBodySummary>),
    WithBody(NetworkEntryDetail<NetworkBody>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkMatch {
    pub meta: NetworkMeta,
    pub entry: NetworkLogEntry,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetworkExport {
    pub meta: NetworkMeta,
    pub path: String,
    pub format: String,
    pub entries: usize,
    pub exported: usize,
    pub skipped_in_flight: usize,
    pub body_pending: usize,
    pub body_omitted: usize,
    pub warnings: Vec<String>,
}
