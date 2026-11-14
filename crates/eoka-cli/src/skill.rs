use crate::{cli::SkillAction, protocol};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const NAME: &str = "eoka";
const MANIFEST: &str = "SKILL.md";
const FILES: &[(&str, &str)] = &[
    (MANIFEST, include_str!("../skill/SKILL.md")),
    (
        "references/actions.md",
        include_str!("../skill/references/actions.md"),
    ),
    (
        "references/captcha.md",
        include_str!("../skill/references/captcha.md"),
    ),
    (
        "references/downloads.md",
        include_str!("../skill/references/downloads.md"),
    ),
];
const DEFAULT_SKILL_ROOT: &str = ".agents/skills";
const HOME_SKILL_ROOTS: &[&str] = &[
    DEFAULT_SKILL_ROOT,
    ".claude/skills",
    ".codex/skills",
    ".pi/agent/skills",
    ".config/opencode/skills",
    ".cursor/skills",
    ".gemini/skills",
];
const AGENT_HOME_VARIABLES: &[&str] = &["CODEX_HOME", "CLAUDE_CONFIG_DIR"];
pub(crate) const FIRST_INSTALL_HINT: &str = "npx skills add https://github.com/shrimp-software/eoka-tools/tree/main/crates/eoka-cli/skill --skill eoka --global";

pub(crate) struct Search {
    pub(crate) home: Option<PathBuf>,
    pub(crate) agent_homes: Vec<PathBuf>,
}

impl Search {
    pub(crate) fn from_environment() -> Self {
        let directory = |name: &str| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            home: directory("HOME"),
            agent_homes: AGENT_HOME_VARIABLES
                .iter()
                .filter_map(|name| directory(name))
                .collect(),
        }
    }

    fn candidates(&self) -> Vec<PathBuf> {
        let under_home = self.home.iter().flat_map(|home| {
            HOME_SKILL_ROOTS
                .iter()
                .map(move |root| home.join(root).join(NAME))
        });
        let under_agent_homes = self
            .agent_homes
            .iter()
            .map(|agent_home| agent_home.join("skills").join(NAME));
        under_home.chain(under_agent_homes).collect()
    }

    fn installed(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for candidate in self.candidates() {
            let Ok(directory) = std::fs::canonicalize(&candidate) else {
                continue;
            };
            if holds_this_skill(&directory) && !found.contains(&directory) {
                found.push(directory);
            }
        }
        found
    }
}

fn declares_this_skill(manifest: &str) -> bool {
    let mut lines = manifest.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return false;
    }
    lines
        .take_while(|line| line.trim_end() != "---")
        .filter_map(|line| line.strip_prefix("name:"))
        .any(|value| value.trim().trim_matches(['"', '\'']) == NAME)
}

fn holds_this_skill(directory: &Path) -> bool {
    std::fs::read_to_string(directory.join(MANIFEST))
        .is_ok_and(|manifest| declares_this_skill(&manifest))
}

fn differing_files(directory: &Path) -> Vec<&'static str> {
    FILES
        .iter()
        .filter(|(relative, shipped)| {
            std::fs::read_to_string(directory.join(relative))
                .map_or(true, |installed| installed != *shipped)
        })
        .map(|(relative, _)| *relative)
        .collect()
}

fn shipped(relative: &str) -> &'static str {
    FILES
        .iter()
        .find(|(name, _)| *name == relative)
        .map_or("", |(_, contents)| contents)
}

fn replace_file(path: &Path, contents: &str) -> Result<(), String> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let failed = |error: std::io::Error| format!("{}: {error}", target.display());
    let directory = target
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", target.display()))?;
    std::fs::create_dir_all(directory).map_err(failed)?;
    let staged = directory.join(format!(".eoka-skill-{}", std::process::id()));
    let written =
        std::fs::write(&staged, contents).and_then(|()| std::fs::rename(&staged, &target));
    if written.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    written.map_err(failed)
}

fn copy_state(directory: &Path, write: bool) -> Value {
    let path = directory.to_string_lossy();
    let files = differing_files(directory);
    if files.is_empty() {
        return json!({"path": path, "status": "current"});
    }
    if !write {
        return json!({"path": path, "status": "outdated", "files": files});
    }
    let replaced = files
        .iter()
        .try_for_each(|relative| replace_file(&directory.join(relative), shipped(relative)));
    match replaced {
        Ok(()) => json!({"path": path, "status": "updated", "files": files}),
        Err(error) => json!({"path": path, "status": "failed", "files": files, "error": error}),
    }
}

fn describe(copies: &[Value]) -> String {
    if copies.is_empty() {
        return "No installed agent skill was found. Add one with: eoka skill install".to_string();
    }
    copies
        .iter()
        .map(|copy| {
            let path = copy["path"].as_str().unwrap_or_default();
            match copy["status"].as_str().unwrap_or_default() {
                "current" => format!("Agent skill at {path} is current"),
                "updated" => format!("Updated the agent skill at {path}"),
                "outdated" => {
                    format!("Agent skill at {path} is out of date. Run: eoka skill refresh")
                }
                _ => format!(
                    "Could not update the agent skill at {path}: {}",
                    copy["error"].as_str().unwrap_or("unknown error")
                ),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn report(copies: Vec<Value>) -> Value {
    json!({
        "skill_version": env!("CARGO_PKG_VERSION"),
        "text": describe(&copies),
        "copies": copies,
    })
}

pub(crate) fn installed_copies(search: &Search, write: bool) -> Value {
    report(
        search
            .installed()
            .iter()
            .map(|directory| copy_state(directory, write))
            .collect(),
    )
}

fn install(directory: Option<&Path>, search: &Search) -> Result<Value, String> {
    let directory = match (directory, &search.home) {
        (Some(directory), _) => directory.to_path_buf(),
        (None, Some(home)) => home.join(DEFAULT_SKILL_ROOT).join(NAME),
        (None, None) => return Err("HOME is not set; name the skill directory".to_string()),
    };
    if directory.join(MANIFEST).exists() && !holds_this_skill(&directory) {
        return Err(format!(
            "{} holds a different skill. Nothing was written.",
            directory.display()
        ));
    }
    std::fs::create_dir_all(&directory)
        .and_then(|()| std::fs::canonicalize(&directory))
        .map(|directory| report(vec![copy_state(&directory, true)]))
        .map_err(|error| format!("Cannot create {}: {error}", directory.display()))
}

fn response(outcome: Result<Value, String>) -> protocol::Response {
    match outcome {
        Ok(data) if any_failed(&data) => {
            protocol::Response::err(data["text"].as_str().unwrap_or_default())
        }
        Ok(data) => protocol::Response::ok(data),
        Err(error) => protocol::Response::err(error),
    }
}

fn any_failed(data: &Value) -> bool {
    data["copies"]
        .as_array()
        .is_some_and(|copies| copies.iter().any(|copy| copy["status"] == "failed"))
}

pub(crate) fn run(action: &SkillAction) -> protocol::Response {
    let search = Search::from_environment();
    response(match action {
        SkillAction::Install { dir } => install(dir.as_deref(), &search),
        SkillAction::Refresh => Ok(installed_copies(&search, true)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(home: &Path) -> Search {
        Search {
            home: Some(home.to_path_buf()),
            agent_homes: Vec::new(),
        }
    }

    fn stale_copy(directory: &Path) {
        std::fs::create_dir_all(directory.join("references")).unwrap();
        std::fs::write(
            directory.join(MANIFEST),
            "---\nname: eoka\ndescription: \"old\"\n---\n\n# old guide\n",
        )
        .unwrap();
        std::fs::write(
            directory.join("references/actions.md"),
            shipped("references/actions.md"),
        )
        .unwrap();
    }

    fn statuses(data: &Value) -> Vec<(String, String)> {
        data["copies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|copy| {
                (
                    copy["path"].as_str().unwrap().to_string(),
                    copy["status"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn every_file_in_the_skill_directory_ships_in_the_binary() {
        fn collect(directory: &Path, root: &Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(&path, root, found);
                } else {
                    let relative = path.strip_prefix(root).unwrap();
                    found.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("skill");
        let mut on_disk = Vec::new();
        collect(&root, &root, &mut on_disk);
        on_disk.sort();
        let mut embedded: Vec<String> = FILES.iter().map(|(name, _)| name.to_string()).collect();
        embedded.sort();
        assert_eq!(
            embedded, on_disk,
            "list every file under crates/eoka-cli/skill in FILES in crates/eoka-cli/src/skill.rs"
        );
        assert!(declares_this_skill(shipped(MANIFEST)));
    }

    #[test]
    fn the_shipped_guide_names_this_cli_version() {
        let mut version = env!("CARGO_PKG_VERSION").split('.');
        let expected = format!(
            "CLI\n**{}.{} /",
            version.next().unwrap(),
            version.next().unwrap()
        );
        let guide = shipped(MANIFEST);
        assert!(
            guide.contains(&expected) || guide.contains(&expected.replace('\n', " ")),
            "crates/eoka-cli/skill/SKILL.md must say it targets {}",
            expected.replace('\n', " ")
        );
    }

    #[test]
    fn only_a_manifest_named_eoka_counts_as_this_skill() {
        assert!(declares_this_skill("---\nname: eoka\n---\n"));
        assert!(declares_this_skill(
            "---\ndescription: x\nname: \"eoka\"\n---\nbody"
        ));
        assert!(declares_this_skill("---\r\nname: 'eoka'\r\n---\r\n"));
        for other in [
            "---\nname: eoka-extras\n---\n",
            "---\nname: herdr\n---\n",
            "# eoka\n\nname: eoka\n",
            "---\ndescription: x\n---\nname: eoka\n",
            "",
        ] {
            assert!(!declares_this_skill(other), "{other:?}");
        }
    }

    #[test]
    fn refresh_rewrites_only_what_differs_and_keeps_other_files() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let copy = home.join(".agents/skills/eoka");
        stale_copy(&copy);
        std::fs::write(copy.join("notes.md"), "mine").unwrap();

        let checked = installed_copies(&search(&home), false);
        assert_eq!(
            statuses(&checked),
            [(copy.to_string_lossy().to_string(), "outdated".to_string())]
        );
        assert!(!holds_current_skill(&copy), "a check must not write");

        let refreshed = installed_copies(&search(&home), true);
        assert_eq!(refreshed["copies"][0]["status"], "updated");
        assert_eq!(
            refreshed["copies"][0]["files"],
            json!([MANIFEST, "references/captcha.md", "references/downloads.md"])
        );
        assert!(holds_current_skill(&copy));
        assert_eq!(
            std::fs::read_to_string(copy.join("notes.md")).unwrap(),
            "mine"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&copy)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(".eoka-skill-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        let again = installed_copies(&search(&home), true);
        assert_eq!(again["copies"][0]["status"], "current");
        assert!(again["text"].as_str().unwrap().contains("is current"));
    }

    fn holds_current_skill(directory: &Path) -> bool {
        differing_files(directory).is_empty()
    }

    #[test]
    fn a_symlinked_agent_directory_is_the_same_copy() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let copy = home.join(".agents/skills/eoka");
        stale_copy(&copy);
        std::fs::create_dir_all(home.join(".claude/skills")).unwrap();
        std::os::unix::fs::symlink(
            "../../.agents/skills/eoka",
            home.join(".claude/skills/eoka"),
        )
        .unwrap();

        let refreshed = installed_copies(&search(&home), true);
        assert_eq!(
            statuses(&refreshed),
            [(copy.to_string_lossy().to_string(), "updated".to_string())]
        );
        assert!(holds_current_skill(&home.join(".claude/skills/eoka")));
    }

    #[test]
    fn a_symlinked_file_is_written_through() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let copy = home.join(".claude/skills/eoka");
        stale_copy(&copy);
        let shared = home.join("shared-guide.md");
        std::fs::rename(copy.join(MANIFEST), &shared).unwrap();
        std::os::unix::fs::symlink(&shared, copy.join(MANIFEST)).unwrap();

        installed_copies(&search(&home), true);
        assert!(copy.join(MANIFEST).is_symlink());
        assert_eq!(std::fs::read_to_string(&shared).unwrap(), shipped(MANIFEST));
    }

    #[test]
    fn other_skills_and_agent_homes_are_respected() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let foreign = home.join(".codex/skills/eoka");
        std::fs::create_dir_all(&foreign).unwrap();
        std::fs::write(foreign.join(MANIFEST), "---\nname: other\n---\n").unwrap();
        let relocated = home.join("relocated-claude");
        stale_copy(&relocated.join("skills/eoka"));

        let search = Search {
            home: Some(home.clone()),
            agent_homes: vec![relocated.clone()],
        };
        let refreshed = installed_copies(&search, true);
        assert_eq!(
            statuses(&refreshed),
            [(
                relocated.join("skills/eoka").to_string_lossy().to_string(),
                "updated".to_string()
            )]
        );
        assert_eq!(
            std::fs::read_to_string(foreign.join(MANIFEST)).unwrap(),
            "---\nname: other\n---\n"
        );

        let refused = install(Some(&foreign), &search).unwrap_err();
        assert!(refused.contains("holds a different skill"), "{refused}");
    }

    #[test]
    fn nothing_installed_names_the_install_command() {
        let home = tempfile::tempdir().unwrap();
        let found = installed_copies(&search(home.path()), true);
        assert_eq!(found["copies"], json!([]));
        assert!(found["text"]
            .as_str()
            .unwrap()
            .contains("eoka skill install"));
        assert!(std::fs::read_dir(home.path()).unwrap().next().is_none());
    }

    #[test]
    fn install_writes_the_default_directory_and_then_refresh_finds_it() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let installed = install(None, &search(&home)).unwrap();
        let copy = home.join(".agents/skills/eoka");
        assert_eq!(
            statuses(&installed),
            [(copy.to_string_lossy().to_string(), "updated".to_string())]
        );
        assert!(holds_current_skill(&copy));
        assert_eq!(
            installed_copies(&search(&home), true)["copies"][0]["status"],
            "current"
        );

        let elsewhere = home.join("project/.claude/skills/eoka");
        install(Some(&elsewhere), &search(&home)).unwrap();
        assert!(holds_current_skill(&elsewhere));

        let homeless = Search {
            home: None,
            agent_homes: Vec::new(),
        };
        assert!(install(None, &homeless).unwrap_err().contains("HOME"));
    }

    #[test]
    fn a_copy_that_cannot_be_written_fails_the_command() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let copy = home.join(".agents/skills/eoka");
        stale_copy(&copy);
        std::fs::remove_file(copy.join("references/actions.md")).unwrap();
        std::fs::remove_dir(copy.join("references")).unwrap();
        std::fs::write(copy.join("references"), "a file where a directory belongs").unwrap();

        let outcome = installed_copies(&search(&home), true);
        assert_eq!(outcome["copies"][0]["status"], "failed");
        let failed = response(Ok(outcome));
        assert!(!failed.ok);
        assert!(
            failed
                .error
                .as_deref()
                .unwrap()
                .contains("Could not update the agent skill"),
            "{:?}",
            failed.error
        );
    }
}
