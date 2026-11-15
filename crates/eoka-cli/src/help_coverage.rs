use crate::cli::Cli;
use clap::CommandFactory;

fn undocumented(command: &clap::Command, path: &str, gaps: &mut Vec<String>) {
    for arg in command.get_arguments() {
        if arg.is_hide_set() || arg.get_help().is_some() || arg.get_long_help().is_some() {
            continue;
        }
        let name = match arg.get_long() {
            Some(long) => format!("--{long}"),
            None => format!("<{}>", arg.get_id().as_str().to_uppercase()),
        };
        gaps.push(format!("{path} {name}").trim().to_string());
    }
    for sub in command.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        let sub_path = format!("{path} {}", sub.get_name()).trim().to_string();
        if sub.get_about().is_none() && sub.get_long_about().is_none() {
            gaps.push(format!("{sub_path} (command)"));
        }
        undocumented(sub, &sub_path, gaps);
    }
}

#[test]
fn every_cli_command_and_arg_has_help() {
    let mut gaps = Vec::new();
    undocumented(&Cli::command(), "", &mut gaps);
    assert!(
        gaps.is_empty(),
        "add help text in crates/eoka-cli/src/cli.rs for:\n{}",
        gaps.iter()
            .map(|gap| format!("  {gap}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn render(command: &mut clap::Command, path: &str, out: &mut String) {
    use std::fmt::Write as _;
    let _ = write!(
        out,
        "=== {path} usage\n{}\n=== {path} help\n{}\n=== {path} long help\n{}\n",
        command.render_usage(),
        command.render_help(),
        command.render_long_help()
    );
    let names: Vec<String> = command
        .get_subcommands()
        .map(|sub| sub.get_name().to_owned())
        .collect();
    for name in names {
        let sub_path = format!("{path} {name}");
        if let Some(sub) = command.find_subcommand_mut(&name) {
            render(sub, &sub_path, out);
        }
    }
}

#[test]
fn cli_help_matches_the_lock() {
    let mut command = Cli::command().term_width(100);
    command.build();
    let mut rendered = String::new();
    render(&mut command, "eoka", &mut rendered);
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/locks/help.txt");
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        std::fs::write(&lock, &rendered).unwrap();
        return;
    }
    let locked = std::fs::read_to_string(&lock).unwrap_or_default();
    assert!(
        locked == rendered,
        "CLI help changed. If that is intended, rerun with EOKA_UPDATE_LOCKS=1 and list it in the pull request."
    );
}
