use eoka_protocol::{DomainArgs, ModeArgs};
use serde_json::{json, Value};

use super::Engine;
use super::EngineError;
use eoka_protocol::Response;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScriptPolicyMode {
    AllowAll,
    BlockAll,
}

pub struct ScriptPolicyState {
    mode: ScriptPolicyMode,
    allow: Vec<String>,
    block: Vec<String>,
    ever_disabled: bool,
}

impl ScriptPolicyState {
    pub fn new(mode: ScriptPolicyMode, allow: Vec<String>, block: Vec<String>) -> Self {
        Self {
            mode,
            allow,
            block,
            ever_disabled: false,
        }
    }

    pub fn resolve(&self, host: &str) -> bool {
        match self.mode {
            ScriptPolicyMode::BlockAll => self.allow.iter().any(|entry| host_matches(entry, host)),
            ScriptPolicyMode::AllowAll => !self.block.iter().any(|entry| host_matches(entry, host)),
        }
    }

    pub fn is_active(&self) -> bool {
        self.mode == ScriptPolicyMode::BlockAll
            || !self.allow.is_empty()
            || !self.block.is_empty()
            || self.ever_disabled
    }

    pub fn note_applied(&mut self, enabled: bool) {
        if !enabled {
            self.ever_disabled = true;
        }
    }

    pub fn set_mode(&mut self, mode: ScriptPolicyMode) {
        self.mode = mode;
    }

    pub fn add_allow(&mut self, domain: String) {
        if !self.allow.iter().any(|d| d.eq_ignore_ascii_case(&domain)) {
            self.allow.push(domain);
        }
    }

    pub fn add_block(&mut self, domain: String) {
        if !self.block.iter().any(|d| d.eq_ignore_ascii_case(&domain)) {
            self.block.push(domain);
        }
    }

    pub fn remove(&mut self, domain: &str) -> bool {
        let before = self.allow.len() + self.block.len();
        self.allow.retain(|d| !d.eq_ignore_ascii_case(domain));
        self.block.retain(|d| !d.eq_ignore_ascii_case(domain));
        self.allow.len() + self.block.len() < before
    }

    pub fn status_json(&self) -> Value {
        json!({
            "mode": match self.mode {
                ScriptPolicyMode::AllowAll => "allow-all",
                ScriptPolicyMode::BlockAll => "block-all",
            },
            "allow": self.allow,
            "block": self.block,
        })
    }
}

impl Default for ScriptPolicyState {
    fn default() -> Self {
        Self::new(ScriptPolicyMode::AllowAll, Vec::new(), Vec::new())
    }
}

fn host_matches(entry: &str, host: &str) -> bool {
    let entry = entry.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    host == entry || host.ends_with(&format!(".{entry}"))
}

pub fn parse_mode(raw: &str) -> Result<ScriptPolicyMode, String> {
    match raw {
        "block-all" => Ok(ScriptPolicyMode::BlockAll),
        "allow-all" => Ok(ScriptPolicyMode::AllowAll),
        other => Err(format!(
            "Unknown js mode '{}'. Use block-all or allow-all.",
            other
        )),
    }
}

impl Engine {
    pub(super) async fn cmd_js_mode(&mut self, args: &ModeArgs) -> Result<Response, EngineError> {
        let mode = parse_mode(args.mode.as_str())?;
        self.script_policy.set_mode(mode);
        Ok(Response::ok(self.script_policy.status_json()))
    }

    pub(super) async fn cmd_js_allow(
        &mut self,
        args: &DomainArgs,
    ) -> Result<Response, EngineError> {
        let domain = args.domain.clone();
        self.script_policy.add_allow(domain);
        Ok(Response::ok(self.script_policy.status_json()))
    }

    pub(super) async fn cmd_js_block(
        &mut self,
        args: &DomainArgs,
    ) -> Result<Response, EngineError> {
        let domain = args.domain.clone();
        self.script_policy.add_block(domain);
        Ok(Response::ok(self.script_policy.status_json()))
    }

    pub(super) async fn cmd_js_remove(
        &mut self,
        args: &DomainArgs,
    ) -> Result<Response, EngineError> {
        let domain = args.domain.as_str();
        if self.script_policy.remove(domain) {
            Ok(Response::ok(self.script_policy.status_json()))
        } else {
            Err(format!("'{}' not found in either list", domain).into())
        }
    }

    pub(super) async fn cmd_js_list(&mut self) -> Result<Response, EngineError> {
        Ok(Response::ok(self.script_policy.status_json()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_all_denies_by_default_and_allows_exceptions() {
        let policy = ScriptPolicyState::new(
            ScriptPolicyMode::BlockAll,
            vec!["example.com".into()],
            vec![],
        );
        assert!(!policy.resolve("evil.example"));
        assert!(policy.resolve("example.com"));
        assert!(policy.resolve("sub.example.com"));
    }

    #[test]
    fn allow_all_permits_by_default_and_blocks_exceptions() {
        let policy = ScriptPolicyState::new(
            ScriptPolicyMode::AllowAll,
            vec![],
            vec!["evil.example".into()],
        );
        assert!(policy.resolve("example.com"));
        assert!(!policy.resolve("evil.example"));
        assert!(!policy.resolve("sub.evil.example"));
    }

    #[test]
    fn host_match_is_anchored_not_substring() {
        assert!(!host_matches(
            "example.com",
            "evil-example.com.attacker.net"
        ));
        assert!(host_matches("example.com", "example.com"));
        assert!(host_matches("example.com", "www.example.com"));
        assert!(!host_matches("example.com", "notexample.com"));
    }

    #[test]
    fn default_policy_is_inactive() {
        assert!(!ScriptPolicyState::default().is_active());
    }

    #[test]
    fn ever_disabled_keeps_policy_active_after_reverting_to_default() {
        let mut policy = ScriptPolicyState::default();
        assert!(!policy.is_active());
        policy.note_applied(false);
        assert!(policy.is_active());
        policy.note_applied(true);
        assert!(
            policy.is_active(),
            "note_applied(true) must not clear the sticky flag — only future is_active() checks matter"
        );
    }

    #[test]
    fn empty_host_falls_through_to_mode_default_not_silently_allowed() {
        let block_all = ScriptPolicyState::new(ScriptPolicyMode::BlockAll, vec![], vec![]);
        assert!(!block_all.resolve(""));

        let allow_all = ScriptPolicyState::new(ScriptPolicyMode::AllowAll, vec![], vec![]);
        assert!(allow_all.resolve(""));
    }

    #[test]
    fn remove_clears_from_either_list() {
        let mut policy = ScriptPolicyState::default();
        policy.add_block("evil.example".into());
        assert!(policy.remove("evil.example"));
        assert!(policy.resolve("evil.example"));
        assert!(!policy.remove("evil.example"));
    }
}
