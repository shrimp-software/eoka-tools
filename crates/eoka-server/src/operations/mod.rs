pub mod commands;
pub mod sidecar;
pub(crate) mod store;

use anyhow::{anyhow, Result};
use eoka::{Action, ActionOptions, ActionOutcome, DispatchObserver, Error, Page};
use eoka_protocol::{OperationListArgs, OperationRecord, OperationState};
use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use store::{timestamp, Store};
use tokio::sync::{watch, OnceCell};

#[derive(Clone, Default)]
pub struct OperationManager {
    inner: Arc<Manager>,
}
#[derive(Default)]
struct Manager {
    store: OnceCell<Arc<Store>>,
    directory: Option<PathBuf>,
    jobs: Mutex<HashMap<String, watch::Sender<bool>>>,
}

pub(crate) struct Intent {
    pub store: Arc<Store>,
    pub id: String,
}
impl DispatchObserver for Intent {
    fn before_dispatch(
        &self,
        preparation: bool,
    ) -> Pin<Box<dyn Future<Output = eoka::Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.store
                .update(self.id.clone(), move |record| {
                    if preparation {
                        record.preparation_intent = true;
                    } else {
                        record.dispatch_intent = true;
                        record.outcome = ActionOutcome::OutcomeUnknown;
                    }
                })
                .await
                .and_then(|record| {
                    if matches!(record.state, OperationState::Running)
                        && record.runtime_id == self.store.runtime
                        && (if preparation {
                            record.preparation_intent
                        } else {
                            record.dispatch_intent
                        })
                    {
                        Ok(())
                    } else {
                        Err(anyhow!("Operation is no longer owned and running"))
                    }
                })
                .map_err(|_| Error::transport("Could not durably record dispatch intent"))
        })
    }
}

impl OperationManager {
    pub fn in_directory(directory: PathBuf) -> Self {
        Self {
            inner: Arc::new(Manager {
                directory: Some(directory),
                ..Default::default()
            }),
        }
    }
    pub(crate) async fn store(&self) -> Result<&Arc<Store>> {
        self.inner
            .store
            .get_or_try_init(|| async {
                Store::open(match &self.inner.directory {
                    Some(path) => path.clone(),
                    None => store::state_directory()?,
                })
                .await
            })
            .await
    }
    pub async fn submit(
        &self,
        page: Page,
        id: String,
        action: Action,
        options: ActionOptions,
    ) -> Result<OperationRecord> {
        let manager = self.clone();
        tokio::spawn(async move { manager.submit_owned(page, id, action, options).await })
            .await
            .map_err(|_| {
                anyhow!("Operation admission task failed; inspect its ID before retrying")
            })?
    }

    async fn submit_owned(
        &self,
        page: Page,
        id: String,
        action: Action,
        mut options: ActionOptions,
    ) -> Result<OperationRecord> {
        let parsed = uuid::Uuid::parse_str(&id).map_err(|_| {
            anyhow!("operation_id must be a UUID generated before sending the request")
        })?;
        if parsed.to_string() != id {
            return Err(anyhow!("operation_id must be a canonical lowercase UUID"));
        }
        let store = self.store().await?.clone();
        let canonical =
            serde_json::to_vec(&serde_json::json!({"action":action,"options":options}))?;
        let initial = OperationRecord {
            id: id.clone(),
            runtime_id: store.runtime.clone(),
            target_id: page.target_id().into(),
            action: action.kind().into(),
            state: OperationState::Running,
            outcome: ActionOutcome::NotDispatched,
            dispatch_intent: false,
            preparation_intent: false,
            started_at_ms: timestamp(),
            finished_at_ms: None,
            receipt: None,
            error_code: None,
        };
        let (record, created) = store.admit(initial, canonical).await?;
        if !created {
            return Ok(record);
        }
        let (cancel, mut canceled) = watch::channel(false);
        self.inner
            .jobs
            .lock()
            .map_err(|_| anyhow!("Operation registry unavailable"))?
            .insert(id.clone(), cancel);
        options.observer = Some(Arc::new(Intent {
            store: store.clone(),
            id: id.clone(),
        }));
        let manager = self.clone();
        tokio::spawn(async move {
            let result = tokio::select! {
                biased;
                _ = canceled.changed() => None,
                result = page.act(action,options) => Some(result),
            };
            let finish = store
                .update(id.clone(), move |record| {
                    record.state = OperationState::Finished;
                    record.finished_at_ms = Some(timestamp());
                    match result {
                        Some(Ok(receipt)) => {
                            record.outcome = receipt.outcome;
                            record.receipt = Some(receipt);
                        }
                        Some(Err(Error::Action { receipt, code, .. })) => {
                            record.outcome = receipt.outcome;
                            record.receipt = Some(*receipt);
                            record.error_code = Some(code);
                        }
                        other => {
                            record.outcome = if record.dispatch_intent {
                                ActionOutcome::OutcomeUnknown
                            } else {
                                ActionOutcome::NotDispatched
                            };
                            record.error_code = Some(
                                if other.is_none() {
                                    "operation_canceled"
                                } else {
                                    "action_failed"
                                }
                                .into(),
                            );
                        }
                    }
                })
                .await;
            if finish.is_err() {
                tracing::error!(operation_id=%id,"Could not persist operation completion; outcome remains uncertain");
            }
            if let Ok(mut jobs) = manager.inner.jobs.lock() {
                jobs.remove(&id);
            }
        });
        Ok(record)
    }
    pub async fn get(&self, id: &str) -> Result<OperationRecord> {
        let store = self.store().await?;
        store.recover().await?;
        store.get(id.into()).await
    }
    pub async fn list(&self, args: OperationListArgs) -> Result<Vec<OperationRecord>> {
        let limit = args.limit.unwrap_or(100).clamp(1, 500);
        self.store().await?.run(move |connection| {
            let rows=connection.prepare("SELECT record FROM operations WHERE started < ? ORDER BY started DESC,id LIMIT ?")?.query_map(rusqlite::params![args.before_ms.unwrap_or(i64::MAX as u64),limit],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(|s|Ok(serde_json::from_str(&s)?)).collect()
        }).await
    }
    pub async fn wait(&self, id: &str, timeout_ms: u64) -> Result<OperationRecord> {
        if timeout_ms > 86_400_000 {
            return Err(anyhow!("Wait timeout exceeds 24 hours"));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let record = self.get(id).await?;
            if !matches!(record.state, OperationState::Running)
                || tokio::time::Instant::now() >= deadline
            {
                return Ok(record);
            }
            tokio::time::sleep_until(
                deadline.min(tokio::time::Instant::now() + Duration::from_millis(25)),
            )
            .await;
        }
    }
    pub async fn cancel(&self, id: &str) -> Result<OperationRecord> {
        let record = self.get(id).await?;
        if matches!(record.state, OperationState::Running) {
            let jobs = self
                .inner
                .jobs
                .lock()
                .map_err(|_| anyhow!("Operation registry unavailable"))?;
            let sender = jobs.get(id).ok_or_else(|| {
                anyhow!("operation_owned_elsewhere: request cancellation from the owning runtime")
            })?;
            sender.send_replace(true);
        }
        Ok(record)
    }
    pub async fn stop(&self) {
        let ids = if let Ok(jobs) = self.inner.jobs.lock() {
            jobs.iter()
                .map(|(id, cancel)| {
                    cancel.send_replace(true);
                    id.clone()
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for id in ids {
            let _ = self.wait(&id, 5000).await;
        }
    }
    pub fn is_active(&self) -> bool {
        self.inner
            .jobs
            .lock()
            .map(|j| !j.is_empty())
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests;
