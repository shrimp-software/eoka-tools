use crate::cli::{Cli, Command};
use clap::{error::ErrorKind, Parser};
use std::path::{Path, PathBuf};

struct Example {
    file: PathBuf,
    line: usize,
    argv: Vec<String>,
}

fn skill_docs() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("skill");
    let mut references: Vec<PathBuf> = std::fs::read_dir(root.join("references"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    references.sort();
    let mut docs = vec![root.join("SKILL.md")];
    docs.extend(references);
    docs
}

fn shell_examples(file: &Path) -> Vec<Example> {
    let text = std::fs::read_to_string(file).unwrap();
    let mut examples = Vec::new();
    let mut in_shell = false;
    let mut continued: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(fence) = trimmed.strip_prefix("```") {
            in_shell = !in_shell && matches!(fence, "bash" | "sh" | "shell");
            continue;
        }
        if !in_shell {
            continue;
        }
        let (start, command) = match continued.take() {
            Some((start, head)) => (start, format!("{head} {trimmed}")),
            None => (index + 1, trimmed.to_string()),
        };
        if let Some(head) = command.strip_suffix('\\') {
            continued = Some((start, head.trim_end().to_string()));
            continue;
        }
        if !command.starts_with("eoka ") {
            continue;
        }
        let argv = shlex::split(&command)
            .unwrap_or_else(|| panic!("{}:{start}: unbalanced quotes", file.display()));
        examples.push(Example {
            file: file.to_path_buf(),
            line: start,
            argv,
        });
    }
    examples
}

fn check(example: &Example) -> Result<(), String> {
    let cli = match Cli::try_parse_from(&example.argv) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error.render().to_string()),
    };
    match &cli.command {
        Some(Command::Step { request }) => {
            serde_json::from_str::<eoka_protocol::AgentStepArgs>(request)
                .map(drop)
                .map_err(|error| format!("step request: {error}"))
        }
        Some(Command::Action { request }) => {
            serde_json::from_str::<eoka_protocol::ActionPerformArgs>(request)
                .map(drop)
                .map_err(|error| format!("action request: {error}"))
        }
        Some(Command::Batch {
            input: Some(input), ..
        }) => crate::batch::parse_batch_requests(input).map(drop),
        _ => Ok(()),
    }
}

#[test]
fn skill_examples_parse_with_the_real_cli() {
    let examples: Vec<Example> = skill_docs()
        .iter()
        .flat_map(|file| shell_examples(file))
        .collect();
    assert!(
        examples.len() >= 5,
        "found only {} eoka examples in the skill docs",
        examples.len()
    );
    let failures: Vec<String> = examples
        .iter()
        .filter_map(|example| {
            check(example).err().map(|error| {
                format!(
                    "{}:{}: {}\n{}",
                    example.file.display(),
                    example.line,
                    shlex::try_join(example.argv.iter().map(String::as_str)).unwrap(),
                    error.trim_end()
                )
            })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "skill examples no longer match the CLI:\n\n{}",
        failures.join("\n\n")
    );
}

/// Every action kind the core library accepts must be described in the
/// actions reference, so a new kind cannot ship undocumented.
#[test]
fn every_action_kind_is_documented() {
    let bundle = eoka_protocol::sidecar::schema_bundle();
    let mut found = std::collections::BTreeSet::new();
    // Only the action variants themselves, not the locator kinds nested in them.
    for variant in bundle["types"]["Action"]["oneOf"].as_array().unwrap() {
        if let Some(kind) = variant["properties"]["kind"]["const"].as_str() {
            found.insert(kind.to_owned());
        }
    }
    assert!(
        found.contains("click"),
        "action kinds were not found in the schema"
    );

    let reference = Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/references/actions.md");
    let text = std::fs::read_to_string(&reference).unwrap();
    // A kind counts only when it names a row of the kinds table, so prose that
    // happens to mention `click` or `back` does not satisfy this.
    let rows: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|row| row.split('|').next())
        .collect();
    let undocumented: Vec<&String> = found
        .iter()
        .filter(|kind| !rows.iter().any(|cell| cell.contains(&format!("{kind}`"))))
        .collect();
    assert!(
        undocumented.is_empty(),
        "describe these action kinds in {}: {undocumented:?}",
        reference.display()
    );
}
