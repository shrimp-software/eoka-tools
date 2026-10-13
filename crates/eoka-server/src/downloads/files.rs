use super::failure;
use eoka_protocol::ErrorDetail;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Destination {
    pub output: PathBuf,
    pub staging: PathBuf,
}

impl Destination {
    pub fn prepare(output: &str, id: &str, expected: Option<u64>) -> Result<Self, ErrorDetail> {
        let path = Path::new(output);
        let parent = path
            .parent()
            .ok_or_else(|| failure("invalid_input", "Output needs a parent directory"))?;
        let name = path
            .file_name()
            .ok_or_else(|| failure("invalid_input", "Output needs a filename"))?;
        let parent = std::fs::canonicalize(parent).map_err(io_failure)?;
        let output = parent.join(name);
        match std::fs::symlink_metadata(&output) {
            Ok(_) => {
                return Err(failure(
                    "download_destination_exists",
                    "Output already exists",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_failure(error)),
        }
        ensure_space(&parent, expected.unwrap_or(0))?;
        let staging = parent.join(format!(".eoka-download-{id}"));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&staging).map_err(io_failure)?;
        Ok(Self { output, staging })
    }

    #[cfg(test)]
    pub fn publish(
        &self,
        guid: &str,
        received: u64,
        total: Option<u64>,
        expected: Option<u64>,
        gate: &Mutex<Publication>,
    ) -> Result<(), ErrorDetail> {
        self.publish_durable(guid, received, total, expected, gate, None)
    }

    pub(super) fn publish_durable(
        &self,
        guid: &str,
        received: u64,
        total: Option<u64>,
        expected: Option<u64>,
        gate: &Mutex<Publication>,
        journal: Option<&super::durable::Journal>,
    ) -> Result<(), ErrorDetail> {
        if !safe_guid(guid) {
            return Err(failure(
                "download_invalid_event",
                "Invalid native download identifier",
            ));
        }
        let path = self.staging.join(guid);
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(&path).map_err(io_failure)?;
        let metadata = file.metadata().map_err(io_failure)?;
        if !metadata.is_file() {
            return Err(failure(
                "download_invalid_file",
                "Chrome did not produce a regular file",
            ));
        }
        let size = metadata.len();
        if size != received
            || total.is_some_and(|n| n != size)
            || expected.is_some_and(|n| n != size)
        {
            return Err(failure(
                "download_size_mismatch",
                "Saved file size does not match expected transfer size",
            ));
        }
        file.sync_all().map_err(io_failure)?;
        let (hash, device, inode) = verify(&path, received, total, expected)?;
        if let Some(journal) = journal {
            journal.intent(hash, device, inode).map_err(|_| {
                failure(
                    "download_store_failed",
                    "Could not persist publication intent",
                )
            })?;
        }
        let mut state = gate.lock().unwrap_or_else(|e| e.into_inner());
        if *state == Publication::Canceled {
            return Err(failure(
                "download_canceled",
                "Download canceled before publication",
            ));
        }
        std::fs::hard_link(&path, &self.output).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                failure(
                    "download_destination_exists",
                    "Output appeared during transfer; staged file retained",
                )
            } else {
                io_failure(error)
            }
        })?;
        *state = Publication::Publishing;
        drop(state);
        let parent = self
            .output
            .parent()
            .ok_or_else(|| failure("download_io_error", "Output has no parent directory"))?;
        std::fs::File::open(parent)
            .and_then(|parent| parent.sync_all())
            .map_err(io_failure)?;
        let (_, published_device, published_inode) =
            verify(&self.output, received, total, expected)?;
        if (device, inode) != (published_device, published_inode) {
            return Err(failure(
                "download_invalid_file",
                "Published file identity changed",
            ));
        }
        if let Some(journal) = journal {
            journal.complete().map_err(|_| {
                failure(
                    "download_store_failed",
                    "Could not persist completed publication",
                )
            })?;
        }
        *gate.lock().unwrap_or_else(|e| e.into_inner()) = Publication::Published;
        std::fs::remove_file(path).map_err(io_failure)?;
        Ok(())
    }

    pub fn retained_path(&self) -> Option<String> {
        match std::fs::remove_dir(&self.staging) {
            Ok(()) => None,
            Err(_) if self.staging.exists() => Some(self.staging.to_string_lossy().into_owned()),
            Err(_) => None,
        }
    }
}

impl Drop for Destination {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.staging);
    }
}

pub(super) fn verify(
    path: &Path,
    received: u64,
    total: Option<u64>,
    expected: Option<u64>,
) -> Result<(String, u64, u64), ErrorDetail> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(io_failure)?;
    let metadata = file.metadata().map_err(io_failure)?;
    if !metadata.is_file()
        || metadata.len() != received
        || total.is_some_and(|n| n != received)
        || expected.is_some_and(|n| n != received)
    {
        return Err(failure(
            "download_size_mismatch",
            "Artifact is not a regular file with the recorded size",
        ));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buffer).map_err(io_failure)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        read += n as u64;
    }
    if read != received {
        return Err(failure(
            "download_size_mismatch",
            "Artifact changed during verification",
        ));
    }
    file.sync_all().map_err(io_failure)?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let identity = (0, 0);
    Ok((format!("{:x}", hash.finalize()), identity.0, identity.1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Publication {
    Open,
    Canceled,
    Publishing,
    Published,
}

pub fn safe_guid(guid: &str) -> bool {
    !guid.is_empty()
        && guid.len() <= 128
        && guid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

pub fn io_failure(error: std::io::Error) -> ErrorDetail {
    failure(
        if error.raw_os_error() == Some(libc::ENOSPC) {
            "download_no_space"
        } else {
            "download_io_error"
        },
        error.to_string(),
    )
}

#[cfg(unix)]
pub fn ensure_space(path: &Path, needed: u64) -> Result<(), ErrorDetail> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| failure("invalid_input", "Invalid output path"))?;
    let mut result = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), result.as_mut_ptr()) } != 0 {
        return Err(io_failure(std::io::Error::last_os_error()));
    }
    let result = unsafe { result.assume_init() };
    let available = (result.f_bavail as u128) * (result.f_frsize as u128);
    if available < u128::from(needed.max(1024 * 1024)) {
        return Err(failure(
            "download_no_space",
            "Insufficient free space on destination filesystem",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_space(_path: &Path, _needed: u64) -> Result<(), ErrorDetail> {
    Err(failure(
        "download_unsupported",
        "Downloads currently require a Unix filesystem",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_verifies_size_and_never_replaces_existing_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.bin");
        let destination = Destination::prepare(path.to_str().unwrap(), "test", None).unwrap();
        let gate = Mutex::new(Publication::Open);
        std::fs::write(destination.staging.join("guid"), b"content").unwrap();
        assert_eq!(
            destination
                .publish("guid", 7, None, Some(8), &gate)
                .unwrap_err()
                .code,
            "download_size_mismatch"
        );
        assert!(!path.exists());
        std::fs::write(&path, b"existing").unwrap();
        assert_eq!(
            destination
                .publish("guid", 7, None, Some(7), &gate)
                .unwrap_err()
                .code,
            "download_destination_exists"
        );
        assert_eq!(std::fs::read(path).unwrap(), b"existing");
        assert!(destination.retained_path().is_some());
    }

    #[test]
    fn canceled_publication_retains_verified_bytes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.bin");
        let destination = Destination::prepare(path.to_str().unwrap(), "cancel", None).unwrap();
        std::fs::write(destination.staging.join("guid"), b"content").unwrap();
        let gate = Mutex::new(Publication::Canceled);
        assert_eq!(
            destination
                .publish("guid", 7, None, None, &gate)
                .unwrap_err()
                .code,
            "download_canceled"
        );
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(destination.staging.join("guid")).unwrap(),
            b"content"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_nonregular_artifacts_are_rejected() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("result.bin");
        symlink(root.path().join("missing"), &path).unwrap();
        assert!(Destination::prepare(path.to_str().unwrap(), "occupied", None).is_err());
        std::fs::remove_file(&path).unwrap();
        let destination = Destination::prepare(path.to_str().unwrap(), "safe", None).unwrap();
        let gate = Mutex::new(Publication::Open);
        std::fs::write(root.path().join("source"), b"content").unwrap();
        symlink(root.path().join("source"), destination.staging.join("guid")).unwrap();
        assert!(destination.publish("guid", 7, None, None, &gate).is_err());
        std::fs::remove_file(destination.staging.join("guid")).unwrap();
        std::fs::create_dir(destination.staging.join("guid")).unwrap();
        assert!(destination.publish("guid", 0, None, None, &gate).is_err());
        assert!(destination
            .publish("../source", 7, None, None, &gate)
            .is_err());
        assert!(!path.exists());
    }
}
