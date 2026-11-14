use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const STALE_GUIDE: &str = "---\nname: eoka\ndescription: \"old\"\n---\n\n# old guide\n";

struct Home {
    directory: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("home")).unwrap();
        std::fs::create_dir(directory.path().join("runtime")).unwrap();
        Self { directory }
    }

    fn path(&self) -> PathBuf {
        std::fs::canonicalize(self.directory.path().join("home")).unwrap()
    }

    fn eoka(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_eoka"))
            .args(arguments)
            .env("HOME", self.path())
            .env("XDG_RUNTIME_DIR", self.directory.path().join("runtime"))
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("EOKA_CDP")
            .output()
            .unwrap()
    }

    fn json(&self, arguments: &[&str]) -> Value {
        let output = self.eoka(arguments);
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{error}: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }
}

fn shipped(relative: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skill")
            .join(relative),
    )
    .unwrap()
}

#[test]
fn refresh_brings_a_stale_installed_skill_up_to_this_binary() {
    let home = Home::new();
    let copy = home.path().join(".agents/skills/eoka");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join("SKILL.md"), STALE_GUIDE).unwrap();
    std::fs::write(copy.join("notes.md"), "mine").unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        "../../.agents/skills/eoka",
        home.path().join(".claude/skills/eoka"),
    )
    .unwrap();

    let refreshed = home.json(&["--json", "skill", "refresh"]);
    assert_eq!(refreshed["ok"], true, "{refreshed}");
    let copies = refreshed["data"]["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 1, "{refreshed}");
    assert_eq!(copies[0]["status"], "updated");
    assert_eq!(copies[0]["path"], copy.to_string_lossy().as_ref());
    assert_eq!(
        refreshed["data"]["skill_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        refreshed["data"]["text"],
        format!("Updated the agent skill at {}", copy.display())
    );

    for relative in [
        "SKILL.md",
        "references/actions.md",
        "references/captcha.md",
        "references/downloads.md",
    ] {
        assert_eq!(
            std::fs::read_to_string(home.path().join(".claude/skills/eoka").join(relative))
                .unwrap(),
            shipped(relative),
            "{relative}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(copy.join("notes.md")).unwrap(),
        "mine"
    );

    let again = home.eoka(&["skill", "refresh"]);
    assert!(again.status.success());
    assert_eq!(
        String::from_utf8_lossy(&again.stdout).trim(),
        format!("Agent skill at {} is current", copy.display())
    );
}

#[test]
fn refresh_without_an_installed_skill_writes_nothing() {
    let home = Home::new();
    let output = home.eoka(&["skill", "refresh"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("eoka skill install"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(std::fs::read_dir(home.path()).unwrap().next().is_none());
}

#[test]
fn install_writes_the_skill_and_refuses_another_skills_directory() {
    let home = Home::new();
    let installed = home.json(&["--json", "skill", "install"]);
    assert_eq!(installed["ok"], true, "{installed}");
    assert_eq!(
        std::fs::read_to_string(home.path().join(".agents/skills/eoka/SKILL.md")).unwrap(),
        shipped("SKILL.md")
    );

    let named = home.path().join("project/.claude/skills/eoka");
    let output = home.eoka(&["skill", "install", &named.to_string_lossy()]);
    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(named.join("references/actions.md")).unwrap(),
        shipped("references/actions.md")
    );

    let foreign = home.path().join(".claude/skills/herdr");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("SKILL.md"), "---\nname: herdr\n---\n").unwrap();
    let refused = home.eoka(&["skill", "install", &foreign.to_string_lossy()]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("holds a different skill"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(foreign.join("SKILL.md")).unwrap(),
        "---\nname: herdr\n---\n"
    );
}

#[test]
fn a_copy_that_cannot_be_written_exits_nonzero_and_says_why() {
    let home = Home::new();
    let copy = home.path().join(".agents/skills/eoka");
    std::fs::create_dir_all(&copy).unwrap();
    std::fs::write(copy.join("SKILL.md"), STALE_GUIDE).unwrap();
    std::fs::write(copy.join("references"), "a file where a directory belongs").unwrap();

    let output = home.eoka(&["skill", "refresh"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Could not update the agent skill at"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
