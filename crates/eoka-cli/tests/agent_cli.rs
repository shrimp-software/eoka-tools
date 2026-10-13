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
            .args(["--agent", "--no-geo-align", "--session", &self.name])
            .args(args)
            .env("EOKA_STATE_DIR", self.directory.path())
            .env("EOKA_IDLE_TIMEOUT", "60000");
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
    session.run(&["eval", "document.body.innerHTML='<button id=go>Save</button><p id=status>Ready</p>';window.hits=0;document.querySelector('#go').onclick=()=>{window.hits++;document.querySelector('#status').textContent='Saved'}"]);
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
        "action": {"kind":"click", "target":target}
    })
    .to_string();
    let result = session.run(&["step", &request]);
    assert_eq!(result["operation"]["outcome"], "outcome_unknown");
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
    assert_eq!(session.run(&["eval", "window.hits"]), "1");
}

#[test]
#[ignore = "requires Chrome; CLI observation pagination"]
fn view_continuation_and_step_keep_later_controls_reachable() {
    let session = Session {
        name: format!("agent-pages-{}", uuid::Uuid::new_v4()),
        directory: tempfile::tempdir().unwrap(),
    };
    session.run(&["open", "about:blank"]);
    session.run(&["eval", "document.body.innerHTML=Array.from({length:205},(_,i)=>`<button style='display:block' onclick='window.hits++'>Button ${i}</button>`).join('');window.hits=0"]);
    let first = session.run(&["view", "--max-controls", "200"]);
    assert_eq!(first["next_offset"], 200);
    let last = session.run(&["view", "--offset", "200"]);
    assert_eq!(last["controls"].as_array().unwrap().len(), 5);
    assert!(last["next_offset"].is_null());
    let request = json!({
        "action":{"kind":"click","target":last["controls"][4]["target"]},
        "offset":200
    })
    .to_string();
    let result = session.run(&["step", &request]);
    assert!(result["operation_error"].is_null(), "{result}");
    assert_eq!(result["operation"]["receipt"]["dispatch"], "acknowledged");
    assert_eq!(result["observation"]["controls"][4]["name"], "Button 204");
    assert_eq!(session.run(&["eval", "window.hits"]), "1");
}
