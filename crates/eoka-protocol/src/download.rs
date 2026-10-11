use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DownloadStartArgs {
    pub click: Option<String>,
    pub url: Option<String>,
    #[schemars(
        description = "Absolute output filename. Must not exist; its parent directory must exist."
    )]
    pub output: String,
    pub expect_bytes: Option<u64>,
    #[serde(default = "default_download_timeout")]
    #[schemars(range(min = 1, max = 300000))]
    pub start_timeout_ms: u64,
}

impl DownloadStartArgs {
    pub fn validate(&self) -> Result<(), String> {
        if self.click.is_some() == self.url.is_some() {
            return Err("Exactly one of click or url is required".into());
        }
        if self
            .click
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err("Click target must not be empty".into());
        }
        if !std::path::Path::new(&self.output).is_absolute() {
            return Err("Download output must be an absolute filename".into());
        }
        if self.start_timeout_ms == 0 || self.start_timeout_ms > 300_000 {
            return Err("start_timeout_ms must be between 1 and 300000".into());
        }
        Ok(())
    }
}

pub fn default_download_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DownloadIdArgs {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DownloadWaitArgs {
    pub id: String,
    #[serde(default = "default_download_timeout")]
    #[schemars(range(min = 1, max = 86400000))]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Pending,
    Downloading,
    Verifying,
    Completed,
    Canceled,
    Failed,
}

impl DownloadState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Canceled | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DownloadRecord {
    pub id: String,
    pub tab_id: String,
    pub state: DownloadState,
    pub output: String,
    pub published_path: Option<String>,
    pub staging_path: Option<String>,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub suggested_filename: Option<String>,
    pub source_url: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub error: Option<crate::ErrorDetail>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_inputs_reject_ambiguous_triggers_and_relative_paths() {
        let mut args = DownloadStartArgs {
            click: Some("@e1".into()),
            url: None,
            output: "/tmp/file".into(),
            expect_bytes: None,
            start_timeout_ms: 30_000,
        };
        assert!(args.validate().is_ok());
        args.url = Some("https://example.com/file".into());
        assert!(args.validate().is_err());
        args.url = None;
        args.output = "relative".into();
        assert!(args.validate().is_err());
    }
}
