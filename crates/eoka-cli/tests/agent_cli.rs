use serde_json::{json, Value};
use std::process::Command;

struct Session {
    name: String,
    directory: tempfile::TempDir,
}

impl Session {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_eoka"));
        command
            .args(["--agent", "--session", &self.name])
            .args(args)
            .env("EOKA_STATE_DIR", self.directory.path())
            .env("EOKA_IDLE_TIMEOUT", "60000");
        if args.contains(&"open") {
            command.arg("--no-geo-align");
        }
        for name in [
            "EOKA_CDP",
            "EOKA_AUTO_CONNECT",
            "EOKA_FROM_PROFILE",
            "EOKA_PROXY",
            "EOKA_PROXY_FILE",
            "EOKA_PERSIST",
        ] {
            command.env_remove(name);
        }
        command
    }

    fn run(&self, args: &[&str]) -> Value {
        let output = self.command(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["ok"], true, "{result}");
        result["data"].clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.command(&["close"]).output();
    }
}

#[test]
#[ignore = "requires Chrome; CLI agent loop through SDK and daemon"]
fn view_step_and_recovery_work_through_the_actual_cli() {
    let session = Session {
        name: format!("agent-cli-{}", uuid::Uuid::new_v4()),
        directory: tempfile::tempdir().unwrap(),
    };
    session.run(&["open", "about:blank"]);
    session.run(&["exec", "--raw", "document.body.innerHTML='<button id=go>Save</button><p id=status>Ready</p>';window.hits=0;document.querySelector('#go').onclick=()=>{window.hits++;document.querySelector('#status').textContent='Saved'}"]);
    let view = session.run(&["view"]);
    let target = view["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|control| control["name"] == "Save")
        .unwrap()["target"]
        .clone();
    let request = json!({
        "operation_id": uuid::Uuid::new_v4().to_string(),
        "action": {"kind":"click", "target":target},
        "expect":{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}
    })
    .to_string();
    let result = session.run(&["step", &request]);
    assert_eq!(result["operation"]["outcome"], "verified_complete");
    assert!(result["observation"]["page"]["text_excerpt"]
        .as_str()
        .unwrap()
        .contains("Saved"));
    session.run(&["step", &request]);
    let view = session.run(&["view"]);
    assert!(view["controls"]
        .as_array()
        .unwrap()
        .iter()
        .any(|control| control["target"] == target));
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 1);
}

#[test]
#[ignore = "requires headed Chrome and DISPLAY; session and completion contracts"]
fn headed_session_is_reused_and_failed_actions_are_not_successful() {
    let session = Session {
        name: format!("identity-{}", uuid::Uuid::new_v4()),
        directory: tempfile::tempdir().unwrap(),
    };
    session.run(&["--headed", "open", "about:blank"]);
    session.run(&["exec", "()=>{window.marker=42;window.hits=0;document.body.innerHTML='<button onclick=hits++>Go</button>'}"]);
    assert_eq!(session.run(&["eval", "()=>window.marker"]), 42);
    let doctor = session.run(&["doctor"]);
    assert_eq!(doctor["daemon"]["mode"], "headed");
    assert_eq!(doctor["session"], session.name);
    let output = session.command(&["--headless", "info"]).output().unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["error_detail"]["code"], "session_mode_conflict");
    assert_eq!(session.run(&["eval", "()=>window.marker"]), 42);
    let view = session.run(&["view"]);
    let target = &view["controls"][0]["target"];
    session.run(&["snapshot"]);
    session.run(&["hover", target.as_str().unwrap()]);
    let request = json!({"action":{"kind":"click","target":target}}).to_string();
    let output = session.command(&["step", &request]).output().unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(result["data"]["status"], "not_dispatched");
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 0);
    let request = json!({"action":{"kind":"click","target":target},"mode":"explore"}).to_string();
    let output = session.command(&["step", &request]).output().unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(result["data"]["status"], "outcome_unknown");
    assert_eq!(session.run(&["eval", "()=>window.hits"]), 1);
    let output = session
        .command(&["exec", "async()=>{throw new Error('exception-marker')}"])
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("exception-marker"));
}
