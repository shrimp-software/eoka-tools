use std::path::{Path, PathBuf};

use eoka_protocol::sidecar::{Artifact, ArtifactContainer, ErrorCode, RecordingFailureStage};
use tokio::io::AsyncWriteExt;

pub const MAX_SAFE_BYTES: u64 = 9_007_199_254_740_991;

#[derive(Debug)]
pub struct FileFailure {
    pub stage: RecordingFailureStage,
    pub message: String,
    pub partial_path: Option<String>,
}

impl FileFailure {
    fn new(
        stage: RecordingFailureStage,
        error: impl std::fmt::Display,
        path: Option<&Path>,
    ) -> Self {
        Self {
            stage,
            message: error.to_string(),
            partial_path: path.map(|path| path.to_string_lossy().into_owned()),
        }
    }

    pub fn server_error(self) -> eoka_protocol::sidecar::ServerError {
        let mut error =
            eoka_protocol::sidecar::ServerError::new(ErrorCode::FileOutput, self.message);
        error.data.get_or_insert_with(Default::default).partial_path = self.partial_path;
        error.data.get_or_insert_with(Default::default).detail =
            Some(eoka_protocol::ErrorDetail::new(
                format!("file_{:?}", self.stage).to_ascii_lowercase(),
                error.message.clone(),
            ));
        error
    }
}

pub struct StagedArtifact {
    pub output: PathBuf,
    pub partial: PathBuf,
    file: Option<tokio::fs::File>,
    bytes: u64,
    sealed: bool,
    identity: (u64, u128),
}

impl StagedArtifact {
    pub async fn prepare(output: &str) -> Result<Self, FileFailure> {
        let output = output.to_owned();
        tokio::task::spawn_blocking(move || {
            let path = Path::new(&output);
            if !path.is_absolute() {
                return Err(FileFailure::new(
                    RecordingFailureStage::Start,
                    "output path must be absolute",
                    None,
                ));
            }
            let parent = path.parent().ok_or_else(|| {
                FileFailure::new(
                    RecordingFailureStage::Start,
                    "output needs a parent directory",
                    None,
                )
            })?;
            let name = path.file_name().ok_or_else(|| {
                FileFailure::new(
                    RecordingFailureStage::Start,
                    "output needs a filename",
                    None,
                )
            })?;
            let parent = std::fs::canonicalize(parent)
                .map_err(|error| FileFailure::new(RecordingFailureStage::Start, error, None))?;
            let output = parent.join(name);
            match std::fs::symlink_metadata(&output) {
                Ok(_) => {
                    return Err(FileFailure::new(
                        RecordingFailureStage::Start,
                        "output already exists",
                        None,
                    ))
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(FileFailure::new(RecordingFailureStage::Start, error, None))
                }
            }
            let partial = parent.join(format!(".eoka-capture-{}.partial", uuid::Uuid::new_v4()));
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options
                .open(&partial)
                .map_err(|error| FileFailure::new(RecordingFailureStage::Start, error, None))?;
            let identity = crate::file_publication::identity(&file).map_err(|error| {
                FileFailure::new(RecordingFailureStage::Start, error, Some(&partial))
            })?;
            Ok(Self {
                output,
                partial,
                file: Some(tokio::fs::File::from_std(file)),
                bytes: 0,
                sealed: false,
                identity,
            })
        })
        .await
        .map_err(|error| FileFailure::new(RecordingFailureStage::Start, error, None))?
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), FileFailure> {
        let bytes = self
            .bytes
            .checked_add(data.len() as u64)
            .filter(|bytes| *bytes <= MAX_SAFE_BYTES)
            .ok_or_else(|| {
                FileFailure::new(
                    RecordingFailureStage::Write,
                    "artifact exceeds the JavaScript-safe byte limit",
                    Some(&self.partial),
                )
            })?;
        let file = self.file.as_mut().ok_or_else(|| {
            FileFailure::new(
                RecordingFailureStage::Write,
                "staging file is closed",
                Some(&self.partial),
            )
        })?;
        file.write_all(data).await.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Write, error, Some(&self.partial))
        })?;
        self.bytes = bytes;
        Ok(())
    }

    pub(crate) fn recording_file(&mut self) -> Result<&mut tokio::fs::File, FileFailure> {
        self.file.as_mut().ok_or_else(|| {
            FileFailure::new(
                RecordingFailureStage::Close,
                "recording staging file is closed",
                Some(&self.partial),
            )
        })
    }

    pub async fn seal(&mut self) -> Result<(), FileFailure> {
        let Some(mut file) = self.file.take() else {
            return if self.sealed {
                Ok(())
            } else {
                Err(FileFailure::new(
                    RecordingFailureStage::Close,
                    "staging file was abandoned or failed closure",
                    Some(&self.partial),
                ))
            };
        };
        let flushed = async {
            file.flush().await?;
            file.sync_all().await
        }
        .await;
        let closed = crate::file_publication::close_file(file.into_std().await);
        flushed.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Flush, error, Some(&self.partial))
        })?;
        closed.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Close, error, Some(&self.partial))
        })?;
        self.sealed = true;
        Ok(())
    }

    pub async fn abandon(&mut self) -> Result<(), FileFailure> {
        if let Some(mut file) = self.file.take() {
            let flushed = file.flush().await;
            let closed = crate::file_publication::close_file(file.into_std().await);
            flushed.map_err(|error| {
                FileFailure::new(RecordingFailureStage::Flush, error, Some(&self.partial))
            })?;
            closed.map_err(|error| {
                FileFailure::new(RecordingFailureStage::Close, error, Some(&self.partial))
            })?;
        }
        Ok(())
    }

    fn verify(&self, path: &Path) -> Result<(), FileFailure> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            FileFailure::new(RecordingFailureStage::Publish, error, Some(&self.partial))
        })?;
        if !metadata.is_file() {
            return Err(FileFailure::new(
                RecordingFailureStage::Publish,
                "artifact is not a regular file",
                Some(&self.partial),
            ));
        }
        let file = std::fs::File::open(path).map_err(|error| {
            FileFailure::new(RecordingFailureStage::Publish, error, Some(&self.partial))
        })?;
        let metadata = file.metadata();
        #[cfg(not(unix))]
        let identity = crate::file_publication::identity(&file);
        let closed = crate::file_publication::close_file(file);
        let metadata = metadata.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Publish, error, Some(&self.partial))
        })?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (metadata.dev(), u128::from(metadata.ino()))
        };
        #[cfg(not(unix))]
        let identity = identity.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Publish, error, Some(&self.partial))
        })?;
        closed.map_err(|error| {
            FileFailure::new(RecordingFailureStage::Close, error, Some(&self.partial))
        })?;
        if !metadata.is_file() || metadata.len() != self.bytes || identity != self.identity {
            return Err(FileFailure::new(
                RecordingFailureStage::Publish,
                "staged artifact identity or byte length changed",
                Some(&self.partial),
            ));
        }
        Ok(())
    }

    pub fn publish(
        &self,
        mime_type: &str,
        container: Option<ArtifactContainer>,
    ) -> Result<Artifact, FileFailure> {
        if !self.sealed {
            return Err(FileFailure::new(
                RecordingFailureStage::Publish,
                "staging file must be successfully flushed and closed before publication",
                Some(&self.partial),
            ));
        }
        self.verify(&self.partial)?;
        crate::file_publication::publish_no_replace(&self.partial, &self.output).map_err(
            |error| FileFailure::new(RecordingFailureStage::Publish, error, Some(&self.partial)),
        )?;
        if let Err(failure) = self.verify(&self.output) {
            let rollback = std::fs::remove_file(&self.output);
            return Err(FileFailure::new(
                RecordingFailureStage::Publish,
                format!("{}; publication rollback: {rollback:?}", failure.message),
                Some(&self.partial),
            ));
        }
        if let Err(error) = std::fs::remove_file(&self.partial) {
            let rollback = std::fs::remove_file(&self.output);
            return Err(FileFailure::new(
                RecordingFailureStage::Publish,
                format!("staging cleanup failed: {error}; publication rollback: {rollback:?}"),
                Some(&self.partial),
            ));
        }
        Ok(Artifact {
            path: self.output.to_string_lossy().into_owned(),
            bytes: self.bytes,
            mime_type: mime_type.to_owned(),
            container,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collision_retains_partial_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.png");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        stage.write(b"complete image").await.unwrap();
        assert!(stage.publish("image/png", None).is_err());
        stage.seal().await.unwrap();
        std::fs::write(&output, b"previous").unwrap();
        let failure = stage.publish("image/png", None).unwrap_err();
        assert_eq!(failure.stage, RecordingFailureStage::Publish);
        assert_eq!(std::fs::read(&output).unwrap(), b"previous");
        assert_eq!(std::fs::read(&stage.partial).unwrap(), b"complete image");
    }

    #[tokio::test]
    async fn staged_bytes_publish_only_after_close() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.webp");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        stage.write(b"one").await.unwrap();
        stage.write(b"two").await.unwrap();
        assert!(!output.exists());
        stage.seal().await.unwrap();
        let artifact = stage.publish("image/webp", None).unwrap();
        assert_eq!(artifact.bytes, 6);
        assert_eq!(std::fs::read(output).unwrap(), b"onetwo");
        assert!(!stage.partial.exists());
    }

    #[tokio::test]
    async fn failed_write_and_symlink_destinations_are_not_success() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.mp4");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        stage.bytes = MAX_SAFE_BYTES;
        assert_eq!(
            stage.write(b"x").await.unwrap_err().stage,
            RecordingFailureStage::Write
        );
        stage.abandon().await.unwrap();
        assert!(!output.exists());
        assert!(stage.partial.exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path().join("missing"), &output).unwrap();
            assert!(StagedArtifact::prepare(output.to_str().unwrap())
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn borrowed_recording_file_keeps_original_identity_when_path_is_replaced() {
        use std::io::SeekFrom;
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.mp4");
        let retained = root.path().join("retained");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        stage.write(b"original").await.unwrap();
        stage.recording_file().unwrap().flush().await.unwrap();
        std::fs::rename(&stage.partial, &retained).unwrap();
        std::fs::write(&stage.partial, b"replaced").unwrap();
        let file = stage.recording_file().unwrap();
        file.seek(SeekFrom::Start(0)).await.unwrap();
        let mut bytes = [0; 8];
        file.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"original");
        file.seek(SeekFrom::Start(0)).await.unwrap();
        file.write_all(b"retained").await.unwrap();
        stage.seal().await.unwrap();
        assert!(stage.recording_file().is_err());
        assert!(stage
            .publish("video/mp4", Some(ArtifactContainer::Mp4))
            .is_err());
        assert_eq!(std::fs::read(&retained).unwrap(), b"retained");
        assert_eq!(std::fs::read(&stage.partial).unwrap(), b"replaced");
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn replaced_staging_files_and_abandoned_files_cannot_publish() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("capture.png");
        let mut stage = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        stage.write(b"original").await.unwrap();
        stage.seal().await.unwrap();
        std::fs::rename(&stage.partial, root.path().join("original")).unwrap();
        std::fs::write(&stage.partial, b"replaced").unwrap();
        assert!(stage.publish("image/png", None).is_err());
        assert!(!output.exists());
        let mut abandoned = StagedArtifact::prepare(output.to_str().unwrap())
            .await
            .unwrap();
        abandoned.write(b"partial").await.unwrap();
        abandoned.abandon().await.unwrap();
        assert!(abandoned.seal().await.is_err());
        assert!(abandoned.publish("image/png", None).is_err());
        assert!(!output.exists());
    }
}
