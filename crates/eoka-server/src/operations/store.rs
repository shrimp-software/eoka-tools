use anyhow::{anyhow, Context, Result};
use eoka_protocol::{ActionOutcome, OperationRecord, OperationState};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::Sha256;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct Store {
    pub(crate) connection: Mutex<Connection>,
    pub runtime: String,
    key: Vec<u8>,
    directory: PathBuf,
    last_recovery: Mutex<std::time::Instant>,
    _lock: File,
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

pub fn state_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("EOKA_STATE_DIR") {
        return Ok(path.into());
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("eoka"));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("eoka"));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow!("EOKA_STATE_DIR or HOME is required for durable operations"))?;
    #[cfg(target_os = "macos")]
    let suffix = "Library/Application Support/eoka";
    #[cfg(not(target_os = "macos"))]
    let suffix = ".local/state/eoka";
    Ok(PathBuf::from(home).join(suffix))
}

fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(anyhow!("State lock must be a regular file"));
    }
    Ok(file)
}

impl Store {
    pub async fn open(path: PathBuf) -> Result<Arc<Self>> {
        tokio::task::spawn_blocking(move || Self::open_sync(&path)).await?
    }

    fn open_sync(path: &Path) -> Result<Arc<Self>> {
        let mut directory = std::fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(path)?;
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(anyhow!("State directory must be a real directory"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(anyhow!("State directory belongs to another user"));
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
        let setup_lock = private_file(&path.join("store.lock"))?;
        setup_lock.lock_exclusive()?;
        let db_path = path.join("operations.sqlite3");
        drop(private_file(&db_path)?);
        let mut connection = Connection::open(db_path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS secrets (id INTEGER PRIMARY KEY, key BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS operations (
                 id TEXT PRIMARY KEY, digest BLOB NOT NULL, runtime TEXT NOT NULL,
                 state TEXT NOT NULL, started INTEGER NOT NULL, finished INTEGER,
                 record TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS operations_started ON operations(started);
             CREATE TABLE IF NOT EXISTS download_records (
                 id TEXT PRIMARY KEY, runtime TEXT NOT NULL, record TEXT NOT NULL, evidence TEXT
             );",
        )?;
        let key = match connection
            .query_row("SELECT key FROM secrets WHERE id=1", [], |row| row.get(0))
            .optional()?
        {
            Some(key) => key,
            None => {
                let key = [
                    uuid::Uuid::new_v4().as_bytes().as_slice(),
                    uuid::Uuid::new_v4().as_bytes().as_slice(),
                ]
                .concat();
                connection.execute("INSERT INTO secrets(id,key) VALUES(1,?)", [&key])?;
                key
            }
        };
        let runtime = uuid::Uuid::new_v4().to_string();
        let runtime_lock = private_file(&path.join(format!("runtime-{runtime}.lock")))?;
        runtime_lock.lock_exclusive()?;
        recover_abandoned(&mut connection, path)?;
        let cutoff = timestamp().saturating_sub(30 * 24 * 60 * 60 * 1000);
        let rows: Vec<(String, String)> = connection
            .prepare("SELECT id,record FROM operations WHERE finished < ? AND state != 'expired'")?
            .query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, json) in rows {
            let mut record: OperationRecord = serde_json::from_str(&json)?;
            record.state = OperationState::Expired;
            record.target_id.clear();
            record.receipt = None;
            connection.execute(
                "UPDATE operations SET state='expired',record=? WHERE id=?",
                params![serde_json::to_string(&record)?, id],
            )?;
        }
        #[cfg(unix)]
        File::open(path)?.sync_all()?;
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
            runtime,
            key,
            _lock: runtime_lock,
            directory: path.into(),
            last_recovery: Mutex::new(std::time::Instant::now()),
        }))
    }

    pub async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = store
                .connection
                .lock()
                .map_err(|_| anyhow!("Operation store unavailable"))?;
            f(&mut connection)
        })
        .await?
    }

    pub async fn recover(self: &Arc<Self>) -> Result<()> {
        {
            let mut last = self
                .last_recovery
                .lock()
                .map_err(|_| anyhow!("Recovery clock unavailable"))?;
            if last.elapsed() < std::time::Duration::from_secs(1) {
                return Ok(());
            }
            *last = std::time::Instant::now();
        }
        let directory = self.directory.clone();
        self.run(move |connection| {
            let lock = private_file(&directory.join("store.lock"))?;
            lock.lock_exclusive()?;
            recover_abandoned(connection, &directory)
        })
        .await
    }

    pub fn digest(&self, canonical: &[u8]) -> Result<Vec<u8>> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).context("Invalid store key")?;
        mac.update(canonical);
        Ok(mac.finalize().into_bytes().to_vec())
    }

    pub async fn lookup(
        self: &Arc<Self>,
        id: String,
        canonical: Vec<u8>,
    ) -> Result<Option<OperationRecord>> {
        let digest = self.digest(&canonical)?;
        self.run(move |connection| bound_operation(connection, &id, &digest))
            .await
    }

    pub async fn admit(
        self: &Arc<Self>,
        record: OperationRecord,
        canonical: Vec<u8>,
    ) -> Result<(OperationRecord, bool)> {
        let digest = self.digest(&canonical)?;
        self.run(move |connection| {
            let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let Some(existing) = bound_operation(&tx, &record.id, &digest)? {
                return Ok((existing, false));
            }
            tx.execute(
                "INSERT INTO operations(id,digest,runtime,state,started,record) VALUES(?,?,?,'running',?,?)",
                params![record.id, digest, record.runtime_id, record.started_at_ms, serde_json::to_string(&record)?],
            )?;
            tx.commit()?;
            Ok((record, true))
        }).await
    }

    pub async fn get(self: &Arc<Self>, id: String) -> Result<OperationRecord> {
        self.run(move |connection| {
            let json: String = connection
                .query_row("SELECT record FROM operations WHERE id=?", [id], |r| {
                    r.get(0)
                })
                .context("operation_not_found")?;
            Ok(serde_json::from_str(&json)?)
        })
        .await
    }

    pub async fn update(
        self: &Arc<Self>,
        id: String,
        f: impl FnOnce(&mut OperationRecord) + Send + 'static,
    ) -> Result<OperationRecord> {
        self.run(move |connection| {
            let tx =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let json: String =
                tx.query_row("SELECT record FROM operations WHERE id=?", [&id], |r| {
                    r.get(0)
                })?;
            let mut record: OperationRecord = serde_json::from_str(&json)?;
            if matches!(record.state, OperationState::Running) {
                f(&mut record);
            }
            let state = match record.state {
                OperationState::Running => "running",
                OperationState::Finished => "finished",
                OperationState::Interrupted => "interrupted",
                OperationState::Expired => "expired",
            };
            tx.execute(
                "UPDATE operations SET state=?,finished=?,record=? WHERE id=?",
                params![
                    state,
                    record.finished_at_ms,
                    serde_json::to_string(&record)?,
                    id
                ],
            )?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }
}

fn bound_operation(
    connection: &Connection,
    id: &str,
    digest: &[u8],
) -> Result<Option<OperationRecord>> {
    let prior: Option<(Vec<u8>, String)> = connection
        .query_row(
            "SELECT digest,record FROM operations WHERE id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match prior {
        Some((existing, json)) if existing == digest => Ok(Some(serde_json::from_str(&json)?)),
        Some(_) => Err(anyhow!(
            "operation_id_conflict: ID is already bound to different arguments"
        )),
        None => Ok(None),
    }
}

fn recover_abandoned(connection: &mut Connection, path: &Path) -> Result<()> {
    let abandoned = connection
        .prepare(
            "SELECT DISTINCT runtime FROM operations WHERE state='running'
             UNION SELECT DISTINCT runtime FROM download_records",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for owner in abandoned {
        uuid::Uuid::parse_str(&owner).context("Invalid runtime identity in store")?;
        let lock = private_file(&path.join(format!("runtime-{owner}.lock")))?;
        match lock.try_lock_exclusive() {
            Ok(()) => {
                let tx = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let rows: Vec<(String, String)> = tx
                    .prepare(
                        "SELECT id,record FROM operations WHERE runtime=? AND state='running'",
                    )?
                    .query_map([&owner], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                for (id, json) in rows {
                    let mut record: OperationRecord = serde_json::from_str(&json)?;
                    record.state = OperationState::Interrupted;
                    record.outcome = if record.dispatch_intent {
                        ActionOutcome::OutcomeUnknown
                    } else {
                        ActionOutcome::NotDispatched
                    };
                    record.finished_at_ms = Some(timestamp());
                    record.error_code = Some("runtime_interrupted".into());
                    tx.execute(
                        "UPDATE operations SET state='interrupted',finished=?,record=? WHERE id=?",
                        params![record.finished_at_ms, serde_json::to_string(&record)?, id],
                    )?;
                }
                tx.commit()?;
                crate::downloads::durable::recover(connection, &owner)?;
            }
            Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(())
}
