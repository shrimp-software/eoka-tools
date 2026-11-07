use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::thread;

fn eoka_bin() -> &'static str {
    env!("CARGO_BIN_EXE_eoka")
}

fn run(session: &str, args: &[&str]) -> std::process::Output {
    command(session, args)
        .output()
        .expect("failed to spawn eoka binary")
}

fn command(session: &str, args: &[&str]) -> Command {
    let mut command = Command::new(eoka_bin());
    command.arg("--session").arg(session).args(args);
    command
}

fn clean_session(session: &str) {
    let _ = run(session, &["kill"]);
}

fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn tools_manifest_matches_protocol_catalog() {
    let session = format!("eoka-manifest-test-{}", std::process::id());
    let output = run(&session, &["tools", "manifest", "--json"]);
    assert_success(&output, "tools manifest");

    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("manifest output should be JSON");
    let expected = serde_json::json!({
        "tools": eoka_protocol::manifest_for_operations("eoka", false)
    });

    assert_eq!(actual, expected);
}

#[test]
fn tack_executes_without_browser_tools() {
    let session = format!("eoka-tack-test-{}", std::process::id());
    let output = run(&session, &["tack", "--raw-json", "return 5"]);
    assert_success(&output, "tack");

    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("tack output should be JSON");

    assert_eq!(actual["ok"], serde_json::json!(true));
    assert_eq!(actual["data"]["ok"], serde_json::json!(true));
    assert_eq!(actual["data"]["result"], serde_json::json!(5));
}

#[test]
fn agent_mode_implies_json_and_meta() {
    let session = format!("eoka-agent-test-{}", std::process::id());
    let output = run(&session, &["--agent", "doctor"]);
    assert_success(&output, "doctor");

    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor output should be JSON");

    assert_eq!(actual["ok"], serde_json::json!(true));
    assert_eq!(actual["meta"]["session"], serde_json::json!(session));
    assert_eq!(actual["meta"]["cmd"], serde_json::json!("doctor"));
    assert!(actual["data"]["tools"]["default"].as_u64().unwrap() > 0);
}

#[test]
fn agent_mode_formats_early_launch_errors_as_json() {
    let session = format!("eoka-agent-early-error-test-{}", std::process::id());
    let output = run(&session, &["--agent", "--cdp", "not-a-cdp-spec", "doctor"]);

    assert!(!output.status.success());
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("early error should be JSON");

    assert_eq!(actual["ok"], serde_json::json!(false));
    assert_eq!(
        actual["error_detail"]["code"],
        serde_json::json!("invalid_input")
    );
}

#[test]
fn tack_capability_exposes_opt_in_network_tools() {
    let session = format!("eoka-tack-network-test-{}", std::process::id());
    let output = run(
        &session,
        &[
            "tack",
            "--raw-json",
            "--capability",
            "network",
            "const hits = await tools.search({ query: \"network.record\", limit: 5 }); return hits.data.map(t => t.path);",
        ],
    );
    assert_success(&output, "tack network capability");

    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("tack output should be JSON");
    let paths = actual["data"]["result"].as_array().unwrap();

    assert!(paths
        .iter()
        .any(|path| path.as_str() == Some("eoka.network.record.start")));
}

#[test]
fn tack_rejects_unknown_capability() {
    let session = format!("eoka-tack-bad-capability-test-{}", std::process::id());
    let output = run(
        &session,
        &["tack", "--raw-json", "--capability", "nope", "return 1"],
    );

    assert!(!output.status.success());
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("error output should be JSON");

    assert_eq!(actual["ok"], serde_json::json!(false));
    assert_eq!(
        actual["error_detail"]["code"],
        serde_json::json!("invalid_input")
    );
}

fn local_har_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
    let addr = listener.local_addr().expect("test server address");
    thread::spawn(move || {
        for stream in listener.incoming().take(12).flatten() {
            handle_har_request(stream);
        }
    });
    format!("http://{addr}")
}

fn handle_har_request(mut stream: TcpStream) {
    let mut buffer = [0_u8; 8192];
    let read = stream.read(&mut buffer).unwrap_or_default();
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, body) = if path.starts_with("/api/data") {
        (
            "HTTP/1.1 201 Created",
            "application/json",
            r#"{"ok":true,"source":"e2e"}"#,
        )
    } else if path == "/page" {
        (
            "HTTP/1.1 200 OK",
            "text/html",
            r#"<html><body><script>
fetch('/api/data?from=page', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({from: 'page'})
});
</script></body></html>"#,
        )
    } else {
        ("HTTP/1.1 404 Not Found", "text/plain", "not found")
    };
    let response = format!(
        "{status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[test]
#[ignore = "requires Chrome"]
fn spawns_daemon_and_completes_a_real_command() {
    let session = format!("eoka-e2e-test-{}", std::process::id());

    clean_session(&session);

    let open = run(&session, &["--json", "open", "about:blank"]);
    assert_success(&open, "open");
    let open_json: serde_json::Value =
        serde_json::from_slice(&open.stdout).expect("open output should be JSON");
    assert_eq!(open_json["ok"], serde_json::json!(true));

    let status = run(&session, &["--json", "status"]);
    assert!(status.status.success());
    let status_json: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("status output should be JSON");
    assert_eq!(status_json["data"]["running"], serde_json::json!(true));

    let kill = run(&session, &["--json", "kill"]);
    assert!(kill.status.success());
    let kill_json: serde_json::Value =
        serde_json::from_slice(&kill.stdout).expect("kill output should be JSON");
    assert_eq!(kill_json["data"]["killed"], serde_json::json!(true));

    let status_after = run(&session, &["--json", "status"]);
    let status_after_json: serde_json::Value =
        serde_json::from_slice(&status_after.stdout).expect("status output should be JSON");
    assert_eq!(
        status_after_json["data"]["running"],
        serde_json::json!(false)
    );
}

#[test]
#[ignore = "requires Chrome"]
fn network_record_exports_local_fetch_to_har() {
    let session = format!("eoka-har-e2e-test-{}", std::process::id());
    let base_url = local_har_server();
    let page_url = format!("{base_url}/page");
    let har_path = std::env::temp_dir().join(format!("{session}.har"));
    let json_path = std::env::temp_dir().join(format!("{session}.json"));
    let har_arg = har_path.to_string_lossy().into_owned();
    let json_arg = json_path.to_string_lossy().into_owned();

    clean_session(&session);
    let _ = std::fs::remove_file(&har_path);
    let _ = std::fs::remove_file(&json_path);

    assert_success(
        &run(&session, &["open", "about:blank"]),
        "create recording session",
    );
    let start = run(
        &session,
        &[
            "--json",
            "network",
            "record",
            "start",
            "--pattern",
            "*/api/*",
        ],
    );
    assert_success(&start, "network record start");
    let started: serde_json::Value = serde_json::from_slice(&start.stdout).unwrap();
    assert_eq!(started["data"]["status"]["active"], true);

    let open = run(&session, &["--json", "open", &page_url]);
    assert_success(&open, "open");

    let wait = run(
        &session,
        &[
            "--json",
            "network",
            "wait",
            "--pattern",
            "*/api/*",
            "--status",
            "201",
            "--timeout",
            "5000",
            "--include-existing",
        ],
    );
    assert_success(&wait, "network wait");
    let wait_json: serde_json::Value =
        serde_json::from_slice(&wait.stdout).expect("network wait should be JSON");
    assert_eq!(wait_json["data"]["matched"], serde_json::json!(true));
    assert_eq!(wait_json["data"]["entry"]["status"], 201);
    assert!(wait_json["data"]["meta"]["namespace"]
        .as_str()
        .unwrap_or_default()
        .starts_with("session:eoka-har-e2e-test-"));

    let log = run(
        &session,
        &[
            "--json",
            "network",
            "log",
            "--pattern",
            "*/api/*",
            "--compact",
        ],
    );
    assert_success(&log, "network log");
    let log_json: serde_json::Value =
        serde_json::from_slice(&log.stdout).expect("network log should be JSON");
    assert!(
        log_json["data"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["url"]
                .as_str()
                .unwrap_or_default()
                .contains("/api/data?from=page")),
        "network log did not include api request: {log_json}"
    );

    // The network log can list the request before its response has finished
    // loading, and the HAR only includes finished entries; export again until
    // the request lands.
    let mut entry = None;
    for _ in 0..20 {
        let _ = std::fs::remove_file(&har_path);
        let save = run(&session, &["--json", "network", "har", &har_arg]);
        assert_success(&save, "network har");
        let har: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&har_path).expect("har file should exist"))
                .expect("har should be valid JSON");
        entry = har["log"]["entries"]
            .as_array()
            .expect("har entries should be an array")
            .iter()
            .find(|entry| {
                entry["request"]["url"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("/api/data?from=page")
            })
            .cloned();
        if entry.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let entry = entry.expect("har should include api request");

    assert_eq!(entry["request"]["method"], "POST");
    assert_eq!(entry["response"]["status"], 201);
    assert_eq!(
        entry["response"]["content"]["text"],
        r#"{"ok":true,"source":"e2e"}"#
    );
    assert_eq!(entry["_eoka"]["resource_type"], "Fetch");

    let export = run(
        &session,
        &["--json", "network", "export", &json_arg, "--format", "json"],
    );
    assert_success(&export, "network export json");
    let exported: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&json_path).expect("json export should exist"))
            .expect("json export should be valid JSON");
    assert_eq!(exported["version"], 1);
    assert!(exported["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["url"]
            .as_str()
            .unwrap_or_default()
            .contains("/api/data?from=page")));

    let stop = run(&session, &["--json", "network", "record", "stop"]);
    assert_success(&stop, "network record stop");
    let stopped: serde_json::Value = serde_json::from_slice(&stop.stdout).unwrap();
    assert_eq!(stopped["data"]["status"]["active"], false);
    let clear = run(&session, &["--json", "network", "clear"]);
    assert_success(&clear, "network clear");
    let log = run(&session, &["--json", "network", "log"]);
    assert_success(&log, "network log after clear");
    let log: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
    assert_eq!(log["data"]["entries"].as_array().unwrap().len(), 0);
    assert_success(
        &run(&session, &["open", "about:blank"]),
        "navigation after recorder stop",
    );

    clean_session(&session);
    let _ = std::fs::remove_file(har_path);
    let _ = std::fs::remove_file(json_path);
}
