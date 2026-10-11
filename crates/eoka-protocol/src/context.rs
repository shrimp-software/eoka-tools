use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PageContext {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub ready_state: String,
    pub headings: Vec<String>,
    pub messages: Vec<String>,
    pub text_excerpt: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    NotAttempted,
    MayHaveExecuted,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_status: Option<ActionStatus>,
}
