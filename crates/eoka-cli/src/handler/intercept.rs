use std::{collections::VecDeque, path::PathBuf};

use serde_json::{json, Value};

use super::network::url_matches;

#[derive(Clone)]
pub struct InterceptRule {
    pub id: usize,
    pub url_pattern: String,
    pub capture_path: Option<PathBuf>,
    pub respond_path: Option<PathBuf>,
    pub respond_status: u16,
}

impl InterceptRule {
    pub fn matches_url(&self, url: &str) -> bool {
        url_matches(&self.url_pattern, url)
    }
}

#[derive(Clone)]
pub struct InterceptLogEntry {
    pub rule_id: usize,
    pub url: String,
    pub method: String,
    pub has_body: bool,
    pub action: String,
    pub session_id: Option<String>,
    pub network_id: Option<String>,
    pub network_entry_id: Option<u64>,
}

const MAX_LOG_ENTRIES: usize = 1000;

#[derive(Default)]
pub(super) struct InterceptLog {
    entries: VecDeque<InterceptLogEntry>,
}

impl InterceptLog {
    pub(super) fn push(&mut self, entry: InterceptLogEntry) {
        if self.entries.len() == MAX_LOG_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }
}

impl IntoIterator for InterceptLog {
    type Item = InterceptLogEntry;
    type IntoIter = std::collections::vec_deque::IntoIter<InterceptLogEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

pub struct InterceptState {
    rules: Vec<InterceptRule>,
    log: InterceptLog,
    next_id: usize,
    pub enabled: bool,
}

impl InterceptState {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            log: InterceptLog::default(),
            next_id: 1,
            enabled: false,
        }
    }

    pub fn add_rule(
        &mut self,
        url_pattern: String,
        capture_path: Option<PathBuf>,
        respond_path: Option<PathBuf>,
        respond_status: u16,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.rules.push(InterceptRule {
            id,
            url_pattern,
            capture_path,
            respond_path,
            respond_status,
        });
        id
    }

    pub fn remove_rule(&mut self, id: usize) -> bool {
        let len = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < len
    }

    pub fn remove_all(&mut self) {
        self.rules.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn fetch_patterns_json(&self) -> Value {
        Value::Array(
            self.rules
                .iter()
                .map(|r| {
                    json!({
                        "urlPattern": r.url_pattern,
                        "requestStage": "Request",
                    })
                })
                .collect(),
        )
    }

    pub fn rules_snapshot(&self) -> Vec<InterceptRule> {
        self.rules.clone()
    }

    pub fn add_log(&mut self, entry: InterceptLogEntry) {
        self.log.push(entry);
    }

    pub fn clear_log(&mut self) {
        self.log.entries.clear();
    }

    pub async fn resolve_network_links<F, Fut>(&mut self, mut resolve: F)
    where
        F: FnMut(Option<&str>, Option<&str>, &str, &str) -> Fut,
        Fut: std::future::Future<Output = Option<u64>>,
    {
        for entry in &mut self.log.entries {
            if entry.network_entry_id.is_some() {
                continue;
            }
            entry.network_entry_id = resolve(
                entry.session_id.as_deref(),
                entry.network_id.as_deref(),
                &entry.url,
                &entry.method,
            )
            .await;
        }
    }

    pub fn list_json(&self) -> Value {
        let rules: Vec<Value> = self
            .rules
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "pattern": r.url_pattern,
                    "capture": r.capture_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "respond": r.respond_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "status": r.respond_status,
                })
            })
            .collect();
        Value::Array(rules)
    }

    pub fn log_json(&self) -> Value {
        let entries: Vec<Value> = self
            .log
            .entries
            .iter()
            .map(|e| {
                json!({
                    "rule_id": e.rule_id,
                    "url": e.url,
                    "method": e.method,
                    "has_body": e.has_body,
                    "action": e.action,
                    "session_id": e.session_id,
                    "network_id": e.network_id,
                    "network_entry_id": e.network_entry_id,
                })
            })
            .collect();
        Value::Array(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_and_state_logs_retain_only_the_latest_entries() {
        let mut pending = InterceptLog::default();
        for id in 0..MAX_LOG_ENTRIES * 3 {
            pending.push(InterceptLogEntry {
                rule_id: id,
                url: format!("https://fixture.invalid/{id}"),
                method: "GET".into(),
                has_body: false,
                action: "continue".into(),
                session_id: None,
                network_id: None,
                network_entry_id: None,
            });
            assert!(pending.entries.len() <= MAX_LOG_ENTRIES);
        }
        assert_eq!(pending.entries.len(), MAX_LOG_ENTRIES);
        let mut state = InterceptState::new();
        for entry in pending {
            state.add_log(entry.clone());
            state.add_log(entry);
        }
        let logs = state.log_json();
        assert_eq!(logs.as_array().unwrap().len(), MAX_LOG_ENTRIES);
        assert_eq!(logs[0]["rule_id"], MAX_LOG_ENTRIES * 5 / 2);
        assert_eq!(
            logs[MAX_LOG_ENTRIES - 1]["rule_id"],
            MAX_LOG_ENTRIES * 3 - 1
        );
        state.clear_log();
        assert_eq!(state.log_json(), json!([]));
    }

    #[tokio::test]
    async fn retained_logs_can_still_resolve_network_links() {
        let mut state = InterceptState::new();
        state.add_log(InterceptLogEntry {
            rule_id: 1,
            url: "https://fixture.invalid/".into(),
            method: "GET".into(),
            has_body: false,
            action: "continue".into(),
            session_id: Some("session".into()),
            network_id: Some("network".into()),
            network_entry_id: None,
        });
        state
            .resolve_network_links(|session, network, url, method| {
                assert_eq!(session, Some("session"));
                assert_eq!(network, Some("network"));
                assert_eq!(url, "https://fixture.invalid/");
                assert_eq!(method, "GET");
                std::future::ready(Some(42))
            })
            .await;
        assert_eq!(state.log_json()[0]["network_entry_id"], 42);
        state
            .resolve_network_links(|_, _, _, _| async {
                panic!("already resolved entries must not be queried again");
            })
            .await;
    }

    #[test]
    fn glob_matching() {
        assert!(url_matches(
            "*/api/data*",
            "https://example.com/api/data?foo=1"
        ));
        assert!(url_matches(
            "*/biometrics/*",
            "https://api.facetec.com/biometrics/process-request"
        ));
        assert!(!url_matches("*/api/data*", "https://example.com/other"));
        assert!(url_matches("example.com", "https://example.com/foo"));
    }

    #[test]
    fn fetch_patterns_include_rule_patterns() {
        let mut state = InterceptState::new();
        state.add_rule("*/api/*".into(), None, None, 200);

        assert_eq!(
            state.fetch_patterns_json(),
            json!([{ "urlPattern": "*/api/*", "requestStage": "Request" }])
        );
    }

    #[test]
    fn fetch_patterns_are_empty_when_no_rules_exist() {
        let state = InterceptState::new();

        assert!(state.is_empty());
        assert_eq!(state.fetch_patterns_json(), json!([]));
    }

    #[test]
    fn fetch_patterns_include_all_active_rules() {
        let mut state = InterceptState::new();
        state.add_rule("*/api/*".into(), None, None, 200);
        state.add_rule("*/graphql".into(), None, None, 200);

        assert_eq!(
            state.fetch_patterns_json(),
            json!([
                { "urlPattern": "*/api/*", "requestStage": "Request" },
                { "urlPattern": "*/graphql", "requestStage": "Request" },
            ])
        );
    }
}
