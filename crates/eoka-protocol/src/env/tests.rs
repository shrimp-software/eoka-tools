use super::*;
use std::path::{Path, PathBuf};

const NOT_RUNTIME_PREFIXES: [&str; 2] = ["EOKA_TEST_", "EOKA_DOWNLOAD_TEST_"];
const NOT_RUNTIME: [&str; 3] = [
    "EOKA_UPDATE_LOCKS",
    "EOKA_CORE_VERSION",
    "EOKA_ABRUPT_PARENT_FIXTURE",
];
const DOC_START: &str = "<!-- generated:env -->";
const DOC_END: &str = "<!-- /generated:env -->";

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                rust_sources(&path, found);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

fn quoted_names(source: &str) -> Vec<&str> {
    let opening = "\"EOKA_";
    source
        .match_indices(opening)
        .filter_map(|(start, _)| {
            let name = &source[start + 1..];
            let length = name
                .find(|character: char| {
                    !(character.is_ascii_uppercase()
                        || character.is_ascii_digit()
                        || character == '_')
                })
                .unwrap_or(name.len());
            (length > "EOKA_".len() && name[length..].starts_with('"')).then_some(&name[..length])
        })
        .collect()
}

fn not_runtime(name: &str) -> bool {
    NOT_RUNTIME.contains(&name)
        || NOT_RUNTIME_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

#[test]
fn registered_names_are_unique_and_prefixed() {
    let mut names: Vec<&str> = ALL.iter().map(|variable| variable.name).collect();
    assert!(names
        .iter()
        .all(|name| name.starts_with("EOKA_") && !not_runtime(name)));
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ALL.len());
}

#[test]
fn every_eoka_variable_is_read_through_the_registry() {
    let registry = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/env.rs");
    let mut sources = Vec::new();
    rust_sources(&repository().join("crates"), &mut sources);
    let mut stray = Vec::new();
    for path in sources {
        if path.canonicalize().unwrap() == registry.canonicalize().unwrap() {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for name in quoted_names(&source) {
            if not_runtime(name) {
                continue;
            }
            let fix = match ALL.iter().find(|variable| variable.name == name) {
                Some(_) => format!("use eoka_protocol::env::{}", &name["EOKA_".len()..]),
                None => "register it in crates/eoka-protocol/src/env.rs".to_owned(),
            };
            stray.push(format!("{}: \"{name}\": {fix}", path.display()));
        }
    }
    assert!(
        stray.is_empty(),
        "EOKA_* names written as literals:\n{}",
        stray.join("\n")
    );
}

fn table() -> String {
    let rows: String = ALL
        .iter()
        .map(|variable| {
            format!(
                "| `{}` | {} | {} |\n",
                variable.name,
                variable.read_by.join(", "),
                variable.description
            )
        })
        .collect();
    format!(
        "{DOC_START}\n<!-- Generated from crates/eoka-protocol/src/env.rs; update with EOKA_UPDATE_LOCKS=1 cargo test -p eoka-protocol env::. Do not edit. -->\n\n| Variable | Read by | Purpose |\n| --- | --- | --- |\n{rows}{DOC_END}"
    )
}

#[test]
fn the_environment_table_in_the_cli_guide_is_current() {
    let path = repository().join("docs/cli.md");
    let doc = std::fs::read_to_string(&path).unwrap();
    let start = doc
        .find(DOC_START)
        .expect("docs/cli.md has a generated:env block");
    let end = doc[start..]
        .find(DOC_END)
        .expect("the generated:env block is closed")
        + start
        + DOC_END.len();
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        let current = format!("{}{}{}", &doc[..start], table(), &doc[end..]);
        std::fs::write(&path, current).unwrap();
        return;
    }
    assert_eq!(
        doc[start..end],
        table(),
        "docs/cli.md lists stale environment variables; update with EOKA_UPDATE_LOCKS=1 cargo test -p eoka-protocol env::"
    );
}
