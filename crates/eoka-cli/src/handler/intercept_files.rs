use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const FILE_TIMEOUT: Duration = Duration::from_millis(500);
pub(super) static FILE_SLOT: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));

#[cfg(test)]
pub(super) static FILE_READ_STARTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn open_regular(path: &Path, write: bool) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .read(!write)
        .write(write)
        .create(write)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err(std::io::Error::other("unsupported interception file"));
    }
    Ok(file)
}

async fn bounded<T: Send + 'static>(
    operation: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    bounded_on(FILE_SLOT.clone(), operation).await
}

async fn bounded_on<T: Send + 'static>(
    slot: Arc<tokio::sync::Semaphore>,
    operation: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    tokio::time::timeout(FILE_TIMEOUT, async {
        let permit = slot
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| std::io::Error::other("interception file worker unavailable"))?;
        let (send, receive) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("interception-file".into())
            .spawn(move || {
                let _permit = permit;
                let _ = send.send(operation());
            })?;
        receive
            .await
            .map_err(|_| std::io::Error::other("interception file worker failed"))?
    })
    .await
    .map_err(|_| std::io::Error::other("interception file deadline exceeded"))?
}

pub(super) async fn read(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(test)]
    FILE_READ_STARTED.notify_one();
    let path = path.to_owned();
    bounded(move || {
        let file = open_regular(&path, false)?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(std::io::Error::other(
                "interception response limit exceeded",
            ));
        }
        Ok(bytes)
    })
    .await
}

struct BoundedBytes(Vec<u8>);

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES - self.0.len() {
            return Err(std::io::Error::other("interception capture limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) async fn capture(path: &Path, body: serde_json::Value) -> std::io::Result<()> {
    let path = path.to_owned();
    bounded(move || {
        let mut bytes = BoundedBytes(Vec::new());
        serde_json::to_writer_pretty(&mut bytes, &body)?;
        let mut file = open_regular(&path, true)?;
        file.set_len(0)?;
        file.write_all(&bytes.0)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn special_and_oversized_files_are_rejected_and_binary_files_work() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        for path in [
            fifo.as_path(),
            Path::new("/dev/zero"),
            Path::new("/dev/null"),
        ] {
            assert!(read(path).await.is_err());
            assert!(capture(path, serde_json::json!({"fixture":true}))
                .await
                .is_err());
        }
        let regular = dir.path().join("regular");
        std::fs::File::create(&regular)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(read(&regular).await.is_err());
        let bytes: Vec<u8> = (0..=255).collect();
        std::fs::write(&regular, &bytes).unwrap();
        assert_eq!(read(&regular).await.unwrap(), bytes);
        capture(&regular, serde_json::json!({"fixture":true}))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&read(&regular).await.unwrap()).unwrap()
                ["fixture"],
            true
        );
        assert!(capture(&regular, serde_json::json!("x".repeat(MAX_BYTES)))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn delayed_worker_is_bounded_and_does_not_block_the_runtime() {
        let start = tokio::time::Instant::now();
        let operation = bounded_on(Arc::new(tokio::sync::Semaphore::new(1)), || {
            std::thread::sleep(Duration::from_millis(800));
            Ok(())
        });
        let service = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert!(start.elapsed() < FILE_TIMEOUT);
        };
        let (result, ()) = tokio::join!(operation, service);
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_millis(750));
    }
}
