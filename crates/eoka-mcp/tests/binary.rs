use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn eoka_mcp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_eoka-mcp"))
}

#[test]
fn version_flag_prints_the_crate_version() {
    let output = eoka_mcp().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("eoka-mcp {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn an_unknown_argument_is_a_usage_error() {
    let output = eoka_mcp().arg("serve").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn eoka_mcp_answers_the_mcp_handshake_on_stdio() {
    let mut child = eoka_mcp()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn eoka-mcp");
    let mut stdin = child.stdin.take().unwrap();
    for message in [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"binary-test","version":"0"}}}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ] {
        writeln!(stdin, "{message}").unwrap();
    }
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    let mut replies = Vec::new();
    while replies.len() < 2 {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "eoka-mcp closed stdout"
        );
        replies.push(serde_json::from_str::<serde_json::Value>(&line).unwrap());
    }
    drop(stdin);
    let _ = child.wait();
    assert_eq!(
        replies[0]["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "navigate"));
}
