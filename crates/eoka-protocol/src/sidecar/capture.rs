use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

dto!(ScreenshotClip {
    #[schemars(range(min = 0))]
    x: f64,
    #[schemars(range(min = 0))]
    y: f64,
    #[schemars(schema_with = "positive_scale_schema")]
    width: f64,
    #[schemars(schema_with = "positive_scale_schema")]
    height: f64,
});
dto!(ScreenshotParams {
    page_id: String,
    #[serde(skip_serializing_if = "Option::is_none")] format: Option<ScreenshotFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 100))] quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")] clip: Option<ScreenshotClip>,
    #[serde(skip_serializing_if = "Option::is_none")] full_page: Option<bool>,
});
dto!(ScreenshotToFileParams {
    page_id: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")] format: Option<ScreenshotFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 100))] quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")] clip: Option<ScreenshotClip>,
    #[serde(skip_serializing_if = "Option::is_none")] full_page: Option<bool>,
});

fn screenshot_options(
    format: Option<ScreenshotFormat>,
    quality: Option<u8>,
    clip: Option<&ScreenshotClip>,
    full_page: Option<bool>,
) -> eoka::ScreenshotOptions {
    eoka::ScreenshotOptions {
        format: match format.unwrap_or(ScreenshotFormat::Png) {
            ScreenshotFormat::Png => eoka::ScreenshotFormat::Png,
            ScreenshotFormat::Jpeg => eoka::ScreenshotFormat::Jpeg,
            ScreenshotFormat::Webp => eoka::ScreenshotFormat::Webp,
        },
        quality,
        clip: clip.map(|clip| eoka::ScreenshotClip {
            x: clip.x,
            y: clip.y,
            width: clip.width,
            height: clip.height,
        }),
        full_page: full_page.unwrap_or(false),
    }
}

pub(super) fn validate_screenshot(
    format: Option<ScreenshotFormat>,
    quality: Option<u8>,
    clip: Option<&ScreenshotClip>,
    full_page: Option<bool>,
) -> Result<(), ServerError> {
    if quality.is_some_and(|quality| quality > 100)
        || (quality.is_some() && format.unwrap_or(ScreenshotFormat::Png) == ScreenshotFormat::Png)
    {
        return Err(ServerError::invalid_params(
            "quality must be between 0 and 100 and is only supported for JPEG or WebP",
        ));
    }
    if let Some(clip) = clip {
        if full_page.unwrap_or(false) {
            return Err(ServerError::invalid_params(
                "clip and fullPage cannot be combined",
            ));
        }
        if !clip.x.is_finite()
            || !clip.y.is_finite()
            || clip.x < 0.0
            || clip.y < 0.0
            || !clip.width.is_finite()
            || !clip.height.is_finite()
            || clip.width <= 0.0
            || clip.height <= 0.0
        {
            return Err(ServerError::invalid_params(
                "clip requires finite nonnegative x/y and finite positive width/height",
            ));
        }
    }
    Ok(())
}

fn validate_output_path(path: &str) -> Result<(), ServerError> {
    if !std::path::Path::new(path).is_absolute() || path.as_bytes().contains(&0) {
        return Err(ServerError::invalid_params(
            "path must be an absolute file path without NUL bytes",
        ));
    }
    Ok(())
}

impl ScreenshotParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_screenshot(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
    pub fn options(&self) -> eoka::ScreenshotOptions {
        screenshot_options(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
}

impl ScreenshotToFileParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_screenshot(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )?;
        validate_output_path(&self.path)
    }
    pub fn options(&self) -> eoka::ScreenshotOptions {
        screenshot_options(
            self.format,
            self.quality,
            self.clip.as_ref(),
            self.full_page,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactContainer {
    Mp4,
}

dto!(Artifact {
    path: String,
    bytes: u64,
    mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")] container: Option<ArtifactContainer>,
});

impl Artifact {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_output_path(&self.path)?;
        if self.bytes > MAX_SAFE_INTEGER {
            return Err(ServerError::invalid_params(
                "artifact bytes exceed the JavaScript-safe integer limit",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordingStartParams {
    pub page_id: String,
    #[schemars(regex(
        pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    ))]
    pub recording_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2147483647))]
    pub max_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 2147483647))]
    pub max_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 120))]
    pub fps: Option<u32>,
}

dto!(RecordingIdParams {
    #[schemars(regex(
        pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    ))]
    recording_id: String,
});

fn validate_recording_id(id: &str) -> Result<(), ServerError> {
    if id.len() != 36
        || !id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return Err(ServerError::invalid_params("recordingId must be a UUID"));
    }
    Ok(())
}

impl RecordingStartParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)?;
        validate_output_path(&self.path)?;
        if self
            .max_width
            .into_iter()
            .chain(self.max_height)
            .any(|dimension| dimension == 0 || dimension > i32::MAX as u32)
        {
            return Err(ServerError::invalid_params(
                "maxWidth/maxHeight must be positive and no greater than 2147483647",
            ));
        }
        if self
            .fps
            .is_some_and(|fps| fps == 0 || fps > MAX_RECORDING_FPS)
        {
            return Err(ServerError::invalid_params(
                "fps must be an integer between 1 and 120",
            ));
        }
        Ok(())
    }
}

impl RecordingIdParams {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    Running,
    Finalizing,
    Completed,
    Cancelled,
    Failed,
}

impl RecordingState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecordingFailureStage {
    Start,
    Stop,
    Read,
    Write,
    Flush,
    Close,
    Publish,
    Cancel,
    Disconnect,
}

dto!(RecordingFailure {
    stage: RecordingFailureStage,
    code: ErrorCode,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")] partial_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] cleanup_uncertain: Option<bool>,
});
dto!(RecordingRecord {
    recording_id: String,
    page_id: String,
    state: RecordingState,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")] artifact: Option<Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")] failure: Option<RecordingFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")] frames: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] duration_ms: Option<u64>,
});

impl RecordingRecord {
    pub fn validate(&self) -> Result<(), ServerError> {
        validate_recording_id(&self.recording_id)?;
        validate_output_path(&self.path)?;
        if self.state == RecordingState::Completed {
            let artifact = self.artifact.as_ref().ok_or_else(|| {
                ServerError::invalid_params("completed recording requires a published artifact")
            })?;
            artifact.validate()?;
            if artifact.path != self.path
                || artifact.container != Some(ArtifactContainer::Mp4)
                || artifact.mime_type != "video/mp4"
                || self.failure.is_some()
            {
                return Err(ServerError::invalid_params(
                    "completed recording requires its destination MP4 artifact and no failure",
                ));
            }
        } else if self.artifact.is_some() || self.frames.is_some() || self.duration_ms.is_some() {
            return Err(ServerError::invalid_params(
                "only completed recordings may contain an artifact, frames or duration",
            ));
        }
        if self.state == RecordingState::Failed && self.failure.is_none() {
            return Err(ServerError::invalid_params(
                "failed recording requires failure evidence",
            ));
        }
        if self.failure.is_some() && !self.state.is_terminal() {
            return Err(ServerError::invalid_params(
                "nonterminal recording cannot contain failure evidence",
            ));
        }
        if let Some(partial) = self
            .failure
            .as_ref()
            .and_then(|failure| failure.partial_path.as_ref())
        {
            validate_output_path(partial)?;
            if partial == &self.path {
                return Err(ServerError::invalid_params(
                    "partial artifact cannot use the destination success path",
                ));
            }
        }
        Ok(())
    }
}
