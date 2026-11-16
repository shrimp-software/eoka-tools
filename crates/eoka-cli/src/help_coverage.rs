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
