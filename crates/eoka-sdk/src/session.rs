use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}

fn runtime_dir() -> PathBuf {
    if let Some(base) = std::env::var_os("XDG_RUNTIME_DIR").filter(|base| !base.is_empty()) {
        return PathBuf::from(base).join("eoka");
    }
    let base = std::env::var_os("TMPDIR")
        .filter(|base| !base.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    base.join(format!("eoka-{}", effective_uid()))
}

pub fn socket_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{}.sock", session))
}

pub fn pid_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{}.pid", session))
}

fn lock_path(session: &str) -> PathBuf {
    runtime_dir().join(format!("eoka-{}.lock", session))
}

pub fn trusted_socket_path(session: &str) -> io::Result<PathBuf> {
    ensure_runtime_dir()?;
    Ok(socket_path(session))
}

pub fn profile_dir(session: &str) -> PathBuf {
    if let Ok(base) = std::env::var("EOKA_PROFILE_DIR") {
        return PathBuf::from(base).join(session);
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            if home.is_empty() {
                std::env::temp_dir()
            } else {
                PathBuf::from(home).join(".local/state")
            }
        });
    base.join("eoka/profiles").join(session)
}

pub fn ensure_runtime_dir() -> io::Result<()> {
    secure_private_dir(&runtime_dir())
}

fn secure_private_dir(path: &Path) -> io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("Runtime path {} is not a real directory", path.display()),
        ));
    }
    if metadata.uid() != effective_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "Runtime directory {} belongs to another user",
                path.display()
            ),
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    options
}

pub fn write_pid_file(session: &str) -> io::Result<()> {
    ensure_runtime_dir()?;
    let mut file = private_file_options()
        .write(true)
        .truncate(true)
        .open(pid_path(session))?;
    file.write_all(std::process::id().to_string().as_bytes())
}

pub fn open_daemon_log(session: &str) -> io::Result<(File, PathBuf)> {
    ensure_runtime_dir()?;
    let path = socket_path(session).with_extension("log");
    let file = private_file_options().append(true).open(&path)?;
    Ok((file, path))
}

pub struct DaemonLock {
    _file: File,
}

pub fn acquire_daemon_lock(session: &str) -> io::Result<DaemonLock> {
    ensure_runtime_dir()?;
    let file = private_file_options()
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path(session))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        return Err(if error.kind() == io::ErrorKind::WouldBlock {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Session {session} already has a running daemon"),
            )
        } else {
            error
        });
    }
    Ok(DaemonLock { _file: file })
}

pub fn list_sessions() -> Vec<String> {
    if ensure_runtime_dir().is_err() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(runtime_dir()) else {
        return Vec::new();
    };
    let mut sessions: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_prefix("eoka-")
                .and_then(|rest| rest.strip_suffix(".sock"))
                .map(str::to_owned)
        })
        .collect();
    sessions.sort();
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("eoka-session-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn private_dir_is_created_owner_only_and_tightened() {
        let root = temp_root("mode");
        let dir = root.join("created");
        secure_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        secure_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_dir_rejects_a_symlink() {
        let root = temp_root("link");
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = root.join("link");
        symlink(&real, &link).unwrap();
        assert!(secure_private_dir(&link).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_dir_rejects_a_directory_owned_by_another_user() {
        if effective_uid() == 0 {
            return;
        }
        let error = secure_private_dir(Path::new("/usr")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("another user"));
    }

    #[test]
    fn private_files_do_not_follow_symlinks() {
        let root = temp_root("nofollow");
        let victim = root.join("victim");
        std::fs::write(&victim, "IMPORTANT").unwrap();
        let link = root.join("planted");
        symlink(&victim, &link).unwrap();
        assert!(private_file_options()
            .write(true)
            .truncate(true)
            .open(&link)
            .is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "IMPORTANT");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_session_lock_cannot_be_acquired_twice() {
        let session = format!("lock-test-{}", uuid::Uuid::new_v4());
        let first = acquire_daemon_lock(&session).unwrap();
        let second = acquire_daemon_lock(&session).err().unwrap();
        assert_eq!(second.kind(), std::io::ErrorKind::AlreadyExists);
        drop(first);
        drop(acquire_daemon_lock(&session).unwrap());
        std::fs::remove_file(lock_path(&session)).unwrap();
    }
}
