pub use eoka::{
    Action, ActionOptions, ActionOutcome, ActionReceipt, Destination, DispatchState, Expectation,
    InputMode, LocatorQuery, LocatorSpec,
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
    #[serde(default)]
    pub error: Option<crate::ErrorDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Running,
    VerifiedComplete,
    NotDispatched,
    OutcomeUnknown,
}

impl OperationStatus {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::VerifiedComplete => 0,
            Self::NotDispatched => 1,
            Self::Running => 3,
            Self::OutcomeUnknown => 4,
        }
    }
}

impl OperationRecord {
    pub fn status(&self) -> OperationStatus {
        if matches!(self.state, OperationState::Running) {
            return OperationStatus::Running;
        }
        match self.outcome {
            ActionOutcome::VerifiedComplete if self.error_code.is_none() => {
                OperationStatus::VerifiedComplete
            }
            ActionOutcome::NotDispatched => OperationStatus::NotDispatched,
            _ => OperationStatus::OutcomeUnknown,
        }
    }
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
