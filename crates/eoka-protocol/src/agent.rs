use crate::{Expectation, OperationRecord, PageContext};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentObserveArgs {
    #[schemars(
        description = "Maximum labeled controls per page, 1 through 200. Continue with next_offset when truncated."
    )]
    #[serde(default = "control_limit")]
    pub max_controls: usize,
    #[schemars(
        description = "Zero-based control offset, normally the previous next_offset. Page changes can reorder controls; restart at zero after changes."
    )]
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "observe_timeout")]
    pub timeout_ms: u64,
}

fn control_limit() -> usize {
    40
}
fn observe_timeout() -> u64 {
    3000
}
fn step_timeout() -> u64 {
    10000
}
fn action_timeout() -> u64 {
    30000
}

impl Default for AgentObserveArgs {
    fn default() -> Self {
        Self {
            max_controls: control_limit(),
            offset: 0,
            timeout_ms: observe_timeout(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    #[default]
    Explore,
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentAction {
    Click { target: String },
    Fill { target: String, text: String },
    Type { target: String, text: String },
    Select { target: String, value: String },
    Hover { target: String },
    ScrollIntoView { target: String },
    PressKey { key: String },
    Navigate { url: String },
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentStepArgs {
    #[schemars(
        description = "Optional canonical UUID. SDK/CLI allocate before sending; direct MCP callers should supply one when recovery from a lost response is required. Reuse only with identical action arguments."
    )]
    #[serde(default)]
    pub operation_id: Option<String>,
    pub action: AgentAction,
    #[schemars(
        description = "Explore permits input without a predicted result and reports unverified outcomes honestly. Verify requires explicit expectations for actions without intrinsic completion evidence."
    )]
    #[serde(default)]
    pub mode: AgentMode,
    #[serde(default)]
    pub expect: Option<Expectation>,
    #[serde(default = "step_timeout")]
    pub timeout_ms: u64,
    #[schemars(
        description = "Independent action deadline, 1 through 300000 milliseconds. A shorter response wait never cancels or repeats the action."
    )]
    #[serde(default = "action_timeout")]
    pub action_timeout_ms: u64,
    #[serde(default = "control_limit")]
    pub max_controls: usize,
    #[schemars(
        description = "Zero-based control offset for the refreshed observation. This affects only observation, not operation identity or execution."
    )]
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentControl {
    pub target: String,
    pub role: String,
    pub name: String,
    pub disabled: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentTab {
    pub id: String,
    pub url: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentObservation {
    pub observation_id: String,
    pub tab_id: String,
    pub page: Option<PageContext>,
    pub tabs: Vec<AgentTab>,
    pub controls: Vec<AgentControl>,
    pub truncated: bool,
    #[serde(default)]
    pub next_offset: Option<usize>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentStepResult {
    pub operation_id: String,
    pub operation: Option<OperationRecord>,
    pub operation_error: Option<String>,
    pub observation: Option<AgentObservation>,
    pub observation_error: Option<String>,
}
