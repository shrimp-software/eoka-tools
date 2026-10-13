use crate::operations::store::{timestamp, Store};
use anyhow::{anyhow, Result};
use eoka_protocol::{
    ActionOutcome, DownloadRecord, DownloadState, OperationRecord, OperationState,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Evidence {
    pub guid: Option<String>,
    pub native_complete: bool,
    pub received: u64,
    pub total: Option<u64>,
    pub expected: Option<u64>,
    pub sha256: Option<String>,
    pub publish_intent: bool,
    pub device: Option<u64>,
    pub inode: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct Journal {
    pub store: Arc<Store>,
    pub id: String,
}

impl Journal {
    pub async fn snapshot(&self) -> Result<DownloadRecord> {
        let id = self.id.clone();
        self.store
            .run(move |connection| {
                let json: String = connection.query_row(
                    "SELECT record FROM download_records WHERE id=?",
                    [id],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&json)?)
            })
            .await
    }
    pub async fn save(&self, mut record: DownloadRecord) -> Result<()> {
        if let Some(context) = record.error.as_mut().and_then(|e| e.context.as_mut()) {
            context.page = None;
        }
        let runtime = self.store.runtime.clone();
        self.store.run(move |connection| {
            connection.execute("INSERT INTO download_records(id,runtime,record) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record",params![record.id,runtime,serde_json::to_string(&record)?])?;
            Ok(())
        }).await
    }
    pub async fn completed_event(&self, evidence: Evidence) -> Result<()> {
        let id = self.id.clone();
        self.store
            .run(move |connection| save_evidence(connection, &id, &evidence))
            .await
    }
    pub fn intent(&self, hash: String, device: u64, inode: u64) -> Result<()> {
        let mut connection = self
            .store
            .connection
            .lock()
            .map_err(|_| anyhow!("Store unavailable"))?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut evidence = read_evidence(&tx, &self.id)?;
        if !evidence.native_complete {
            return Err(anyhow!("Native completion was not recorded"));
        }
        evidence.sha256 = Some(hash);
        evidence.device = Some(device);
        evidence.inode = Some(inode);
        evidence.publish_intent = true;
        save_evidence(&tx, &self.id, &evidence)?;
        tx.commit()?;
        Ok(())
    }
    pub fn complete(&self) -> Result<()> {
        let mut connection = self
            .store
            .connection
            .lock()
            .map_err(|_| anyhow!("Store unavailable"))?;
        complete(&mut connection, &self.id)
    }
}

fn save_evidence(connection: &Connection, id: &str, evidence: &Evidence) -> Result<()> {
    if connection.execute(
        "UPDATE download_records SET evidence=? WHERE id=?",
        params![serde_json::to_string(evidence)?, id],
    )? != 1
    {
        return Err(anyhow!("Download journal missing"));
    }
    Ok(())
}
fn read_evidence(connection: &Connection, id: &str) -> Result<Evidence> {
    let json: Option<String> = connection.query_row(
        "SELECT evidence FROM download_records WHERE id=?",
        [id],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(
        &json.ok_or_else(|| anyhow!("Download evidence missing"))?,
    )?)
}
fn complete(connection: &mut Connection, id: &str) -> Result<()> {
    let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let json: String = tx.query_row(
        "SELECT record FROM download_records WHERE id=?",
        [id],
        |r| r.get(0),
    )?;
    let mut record: DownloadRecord = serde_json::from_str(&json)?;
    let evidence = read_evidence(&tx, id)?;
    if !evidence.native_complete || !evidence.publish_intent {
        return Err(anyhow!("Publication evidence missing"));
    }
    record.native_guid = evidence.guid;
    record.native_complete = true;
    record.sha256 = evidence.sha256;
    record.state = DownloadState::Completed;
    record.published_path = Some(record.output.clone());
    record.updated_at_ms = timestamp();
    record.error = None;
    tx.execute(
        "UPDATE download_records SET record=? WHERE id=?",
        params![serde_json::to_string(&record)?, id],
    )?;
    let json: Option<String> = tx
        .query_row("SELECT record FROM operations WHERE id=?", [id], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(json) = json {
        let mut op: OperationRecord = serde_json::from_str(&json)?;
        op.state = OperationState::Finished;
        op.outcome = ActionOutcome::VerifiedComplete;
        op.finished_at_ms = Some(timestamp());
        op.error_code = None;
        tx.execute(
            "UPDATE operations SET state='finished',finished=?,record=? WHERE id=?",
            params![op.finished_at_ms, serde_json::to_string(&op)?, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn recover(connection: &mut Connection, runtime: &str) -> Result<()> {
    let rows: Vec<(String, String, Option<String>)> = connection
        .prepare("SELECT id,record,evidence FROM download_records WHERE runtime=?")?
        .query_map([runtime], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, json, evidence) in rows {
        let mut record: DownloadRecord = serde_json::from_str(&json)?;
        if matches!(
            record.state,
            DownloadState::Completed | DownloadState::Canceled
        ) {
            continue;
        }
        let evidence = evidence
            .as_deref()
            .map(serde_json::from_str::<Evidence>)
            .transpose()?;
        let recovered = match evidence {
            Some(e) if e.native_complete => recover_file(connection, &id, &record, e),
            _ => Err(anyhow!(
                "No durable native completion event; partial bytes retained"
            )),
        };
        if recovered.is_err() {
            record.state = DownloadState::Failed;
            record.updated_at_ms = timestamp();
            record.error=Some(super::failure("download_interrupted","Runtime interrupted; completion could not be proven. Retained files were not deleted and the trigger was not replayed."));
            connection.execute(
                "UPDATE download_records SET record=? WHERE id=?",
                params![serde_json::to_string(&record)?, id],
            )?;
        }
    }
    Ok(())
}

fn recover_file(
    connection: &mut Connection,
    id: &str,
    record: &DownloadRecord,
    mut evidence: Evidence,
) -> Result<()> {
    let guid = evidence
        .guid
        .as_ref()
        .filter(|g| super::files::safe_guid(g))
        .ok_or_else(|| anyhow!("Invalid GUID"))?;
    let staging = Path::new(
        record
            .staging_path
            .as_deref()
            .ok_or_else(|| anyhow!("Staging path missing"))?,
    );
    let path = staging.join(guid);
    let output = Path::new(&record.output);
    let source = if path.exists() { &path } else { output };
    let (hash, device, inode) =
        super::files::verify(source, evidence.received, evidence.total, evidence.expected)
            .map_err(|e| anyhow!(e.message))?;
    if evidence
        .sha256
        .as_ref()
        .is_some_and(|expected| expected != &hash)
    {
        return Err(anyhow!("Artifact digest changed"));
    }
    if source == output
        && (!evidence.publish_intent
            || evidence.device != Some(device)
            || evidence.inode != Some(inode))
    {
        return Err(anyhow!("Existing output is not the published artifact"));
    }
    if output.symlink_metadata().is_ok() {
        let (out_hash, out_dev, out_inode) =
            super::files::verify(output, evidence.received, evidence.total, evidence.expected)
                .map_err(|e| anyhow!(e.message))?;
        if !evidence.publish_intent
            || evidence.device != Some(out_dev)
            || evidence.inode != Some(out_inode)
            || out_hash != hash
        {
            return Err(anyhow!("Unrelated output exists"));
        }
    } else {
        evidence.sha256 = Some(hash);
        evidence.device = Some(device);
        evidence.inode = Some(inode);
        evidence.publish_intent = true;
        save_evidence(connection, id, &evidence)?;
        std::fs::hard_link(&path, output)?;
    }
    std::fs::File::open(
        output
            .parent()
            .ok_or_else(|| anyhow!("Output has no parent"))?,
    )?
    .sync_all()?;
    complete(connection, id)?;
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_dir(staging);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_crash_child() {
        let Ok(root) = std::env::var("EOKA_TEST_DOWNLOAD_ROOT") else {
            return;
        };
        let stage = std::env::var("EOKA_TEST_DOWNLOAD_STAGE").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let root = Path::new(&root);
            let store = Store::open(root.join("state")).await.unwrap();
            let id = "ccf86eb4-e786-4f8f-9910-6a6f04c39857";
            let staging = root.join("staging");
            std::fs::create_dir(&staging).unwrap();
            let source = staging.join("guid");
            std::fs::write(&source, b"download").unwrap();
            let output = root.join("result");
            let journal = Journal {
                store: store.clone(),
                id: id.into(),
            };
            let record = DownloadRecord {
                native_guid: None,
                native_complete: false,
                sha256: None,
                id: id.into(),
                tab_id: "page".into(),
                state: DownloadState::Downloading,
                output: output.to_string_lossy().into(),
                published_path: None,
                staging_path: Some(staging.to_string_lossy().into()),
                received_bytes: 8,
                total_bytes: Some(8),
                suggested_filename: None,
                source_url: None,
                created_at_ms: timestamp(),
                updated_at_ms: timestamp(),
                error: None,
            };
            journal.save(record).await.unwrap();
            if stage != "partial" {
                journal
                    .completed_event(Evidence {
                        guid: Some("guid".into()),
                        native_complete: true,
                        received: 8,
                        total: Some(8),
                        expected: Some(8),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
            }
            if matches!(
                stage.as_str(),
                "intent" | "linked" | "complete" | "unrelated" | "tampered"
            ) {
                let (hash, dev, ino) =
                    super::super::files::verify(&source, 8, Some(8), Some(8)).unwrap();
                journal.intent(hash, dev, ino).unwrap();
            }
            if matches!(stage.as_str(), "linked" | "complete") {
                std::fs::hard_link(&source, &output).unwrap();
            }
            if stage == "complete" {
                std::fs::File::open(root).unwrap().sync_all().unwrap();
                journal.complete().unwrap();
                std::fs::remove_file(&source).unwrap();
            }
            if stage == "unrelated" {
                std::fs::write(&output, b"download").unwrap();
            }
            if stage == "tampered" {
                std::fs::write(&source, b"modified").unwrap();
            }
            std::process::exit(87);
        });
    }

    #[tokio::test]
    async fn publication_recovery_requires_native_completion_and_the_original_artifact() {
        for stage in [
            "partial",
            "native_complete",
            "intent",
            "linked",
            "complete",
            "unrelated",
            "tampered",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "downloads::durable::tests::publication_crash_child",
                    "--nocapture",
                ])
                .env("EOKA_TEST_DOWNLOAD_ROOT", directory.path())
                .env("EOKA_TEST_DOWNLOAD_STAGE", stage)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(87));
            let store = Store::open(directory.path().join("state")).await.unwrap();
            let record: DownloadRecord = store
                .run(|c| {
                    let json: String =
                        c.query_row("SELECT record FROM download_records", [], |r| r.get(0))?;
                    Ok(serde_json::from_str(&json)?)
                })
                .await
                .unwrap();
            let completed = matches!(stage, "native_complete" | "intent" | "linked" | "complete");
            assert_eq!(
                record.state,
                if completed {
                    DownloadState::Completed
                } else {
                    DownloadState::Failed
                },
                "stage {stage}"
            );
            if completed {
                assert_eq!(
                    std::fs::read(directory.path().join("result")).unwrap(),
                    b"download"
                );
            }
            if stage == "partial" {
                assert!(!directory.path().join("result").exists());
                assert!(directory.path().join("staging/guid").exists());
            }
            if stage == "unrelated" {
                assert_eq!(
                    std::fs::read(directory.path().join("result")).unwrap(),
                    b"download"
                );
                assert!(directory.path().join("staging/guid").exists());
            }
        }
    }
}
