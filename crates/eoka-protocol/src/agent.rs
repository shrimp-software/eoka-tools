use crate::{
    Destination, ErrorDetail, Expectation, InputMode, OperationRecord, OperationStatus, PageContext,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentObserveArgs {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[schemars(description = "Maximum labeled controls, 1 through 200. Increase if truncated.")]
    #[serde(default = "control_limit")]
    #[schemars(range(min = 1, max = 200))]
    pub max_controls: usize,
    #[serde(default = "observe_timeout")]
    #[schemars(range(min = 50, max = 60000))]
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
            cursor: None,
            role: None,
            name: None,
            max_controls: control_limit(),
            timeout_ms: observe_timeout(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Explore,
    #[default]
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentAction {
    Click {
        target: String,
    },
    Fill {
        target: String,
        text: String,
        #[serde(default)]
        input_mode: InputMode,
    },
    Type {
        target: String,
        text: String,
        #[serde(default)]
        input_mode: InputMode,
    },
    Focus {
        target: String,
    },
    Select {
        target: String,
        value: String,
    },
    Hover {
        target: String,
    },
    ScrollIntoView {
        target: String,
    },
    PressKey {
        key: String,
    },
    Navigate {
        url: String,
    },
    Back,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentStepArgs {
    #[serde(default)]
    pub destination: Destination,
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
    #[schemars(range(min = 50, max = 60000))]
    pub timeout_ms: u64,
    #[schemars(
        description = "Independent action deadline, 1 through 300000 milliseconds. A shorter response wait never cancels or repeats the action."
    )]
    #[serde(default = "action_timeout")]
    #[schemars(range(min = 1, max = 300000))]
    pub action_timeout_ms: u64,
    #[serde(default = "control_limit")]
    #[schemars(range(min = 1, max = 200))]
    pub max_controls: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentControl {
    pub target: String,
    pub role: String,
    pub name: String,
    pub disabled: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub focused: bool,
    pub editable: bool,
    pub readonly: bool,
    pub required: bool,
    pub invalid: bool,
    pub validation_message: Option<String>,
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
    pub next_cursor: Option<String>,
    pub total_controls: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentStepResult {
    pub status: OperationStatus,
    pub operation_id: String,
    pub operation: Option<OperationRecord>,
    pub operation_error: Option<ErrorDetail>,
    pub observation: Option<AgentObservation>,
    pub observation_error: Option<String>,
}
