pub use eoka::{
    Action, ActionOptions, ActionOutcome, ActionReceipt, DispatchState, Expectation, LocatorQuery,
    LocatorSpec,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionPerformArgs {
    pub operation_id: String,
    pub action: Action,
    #[serde(default)]
    pub options: ActionOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OperationIdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OperationWaitArgs {
    pub id: String,
    #[serde(default = "wait_timeout")]
    #[schemars(range(max = 86400000))]
    pub timeout_ms: u64,
}
fn wait_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OperationListArgs {
    #[serde(default)]
    pub before_ms: Option<u64>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Running,
    Finished,
    Interrupted,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OperationRecord {
    pub id: String,
    pub runtime_id: String,
    pub target_id: String,
    pub action: String,
    pub state: OperationState,
    pub outcome: ActionOutcome,
    pub dispatch_intent: bool,
    pub preparation_intent: bool,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub receipt: Option<ActionReceipt>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expectation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCommandArgs {
    pub operation_id: String,
    pub command: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ActionBatchArgs {
    pub actions: Vec<ActionPerformArgs>,
}
