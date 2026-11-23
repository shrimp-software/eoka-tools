mod support;

use serde_json::{json, Value};
use support::{Reply, Session, Site};

const HAR_PAGE: &str = r#"<html><body><script>
fetch('/api/data?from=page', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({from: 'page'})
});
</script></body></html>"#;

fn har_route(path: &str) -> Option<Reply> {
    if path.starts_with("/api/data") {
        Some(Reply {
            status: "201 Created",
            content_type: "application/json",
            body: r#"{"ok":true,"source":"e2e"}"#,
        })
    } else if path == "/page" {
        Reply::html(HAR_PAGE)
    } else {
        Reply::not_found()
    }
}

fn succeeds(session: &Session, args: &[&str]) {
    let output = session.command(args).output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn tools_manifest_matches_protocol_catalog() {
    let session = Session::new("eoka-manifest-test");
    let (output, actual, _) = session.output(&["tools", "manifest", "--json"]);
    assert!(output.status.success(), "{actual}");
    let expected = json!({
        "tools": eoka_protocol::manifest_for_operations("eoka", false)
    });

    assert_eq!(actual, expected);
}

#[test]
fn the_tool_manifest_agents_read_is_locked() {
    let session = Session::new("eoka-manifest-lock");
    let (output, manifest, _) = session.output(&["tools", "manifest", "--all", "--json"]);
    assert!(output.status.success(), "{manifest}");
    let actual: String = manifest["tools"]
        .as_array()
        .expect("the manifest lists tools")
        .iter()
        .map(|tool| tool.to_string() + "\n")
        .collect();
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/locks/manifest.jsonl");
    if std::env::var_os("EOKA_UPDATE_LOCKS").is_some() {
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&lock).unwrap_or_default();
    let changed: Vec<String> = expected
        .lines()
        .filter(|line| !actual.lines().any(|other| other == *line))
        .map(|line| format!("- {}", line.chars().take(240).collect::<String>()))
        .chain(
            actual
                .lines()
                .filter(|line| !expected.lines().any(|other| other == *line))
                .map(|line| format!("+ {}", line.chars().take(240).collect::<String>())),
        )
        .collect();
    assert!(
        expected == actual,
        "The tool manifest changed. If that is intended, rerun with EOKA_UPDATE_LOCKS=1 and list it in the pull request.\n{}",
        changed.join("\n")
    );
}

#[test]
fn tack_executes_without_browser_tools() {
    let session = Session::new("eoka-tack-test");
    let actual = session.run(&["tack", "--raw-json", "return 5"]);

    assert_eq!(actual["ok"], json!(true));
    assert_eq!(actual["result"], json!(5));
}

#[test]
fn a_malformed_step_says_what_a_step_looks_like() {
    let output = Session::new("malformed-step")
        .command(&[
            "step",
            r#"{"action":{"kind":"click","target":"text:Go"},"expect":{"kind":"focused","value":true}}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing field `target`"), "{stderr}");
    assert!(stderr.contains(r#""mode":"explore""#), "{stderr}");
    assert!(stderr.contains(r#"{"kind":"url""#), "{stderr}");
}

#[test]
fn agent_mode_implies_json_and_meta() {
    let session = Session::new("eoka-agent-test");
    let (output, actual, _) = session.output(&["--agent", "doctor"]);
    assert!(output.status.success(), "{actual}");

    assert_eq!(actual["ok"], json!(true));
    assert_eq!(actual["meta"]["session"], json!(session.name));
    assert_eq!(actual["meta"]["cmd"], json!("doctor"));
    assert!(actual["data"]["tools"]["default"].as_u64().unwrap() > 0);
}

#[test]
fn agent_mode_formats_early_launch_errors_as_json() {
    let session = Session::new("eoka-agent-early-error-test");
    let (output, actual, _) = session.output(&["--agent", "--cdp", "not-a-cdp-spec", "doctor"]);

    assert!(!output.status.success());
    assert_eq!(actual["ok"], json!(false));
    assert_eq!(actual["error_detail"]["code"], json!("invalid_input"));
}

#[test]
fn tack_capability_exposes_opt_in_network_tools() {
    let session = Session::new("eoka-tack-network-test");
    let actual = session.run(&[
        "tack",
        "--raw-json",
        "--capability",
        "network",
        "const hits = await tools.search({ query: \"network.record\", limit: 5 }); return hits.data.map(t => t.path);",
    ]);
    let paths = actual["result"].as_array().unwrap();

    assert!(paths
        .iter()
        .any(|path| path.as_str() == Some("eoka.network.record.start")));
}

#[test]
fn tack_rejects_unknown_capability() {
    let session = Session::new("eoka-tack-bad-capability-test");
    let (output, actual, _) =
        session.output(&["tack", "--raw-json", "--capability", "nope", "return 1"]);

    assert!(!output.status.success());
    assert_eq!(actual["ok"], json!(false));
    assert_eq!(actual["error_detail"]["code"], json!("invalid_input"));
}

#[test]
#[ignore = "requires Chrome"]
fn spawns_daemon_and_completes_a_real_command() {
    let session = Session::new("eoka-e2e-test");

    session.run(&["--json", "open", "about:blank"]);
    assert_eq!(session.run(&["--json", "status"])["running"], json!(true));
    assert_eq!(session.run(&["--json", "kill"])["killed"], json!(true));
    let (_, status_after, _) = session.output(&["--json", "status"]);
    assert_eq!(status_after["data"]["running"], json!(false));
}

#[test]
#[ignore = "requires Chrome; the JavaScript policy applies to reloads and new tabs"]
fn the_javascript_policy_follows_reloads_and_new_tabs() {
    let session = Session::new("eoka-js-policy-test");
    let page = "data:text/html,<title>plain</title><script>document.title='scripted'</script>";
    let title = || {
        session.run(&["--json", "info"])["title"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    succeeds(&session, &["--no-js", "open", page]);
    assert_eq!(title(), "plain");
    succeeds(&session, &["tab", "new", page]);
    assert_eq!(title(), "plain");
    succeeds(&session, &["js", "mode", "allow-all"]);
    succeeds(&session, &["reload"]);
    assert_eq!(title(), "scripted");
}

#[test]
#[ignore = "requires Chrome"]
fn network_record_exports_local_fetch_to_har() {
    let session = Session::new("eoka-har-e2e-test");
    let site = Site::start(har_route);
    let page_url = site.url("/page");
    let har_path = session.directory.path().join("capture.har");
    let json_path = session.directory.path().join("capture.json");
    let har_arg = har_path.to_string_lossy().into_owned();
    let json_arg = json_path.to_string_lossy().into_owned();

    succeeds(&session, &["open", "about:blank"]);
    let started = session.run(&[
        "--json",
        "network",
        "record",
        "start",
        "--pattern",
        "*/api/*",
    ]);
    assert_eq!(started["status"]["active"], true);

    session.run(&["--json", "open", &page_url]);

    let waited = session.run(&[
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
    ]);
    assert_eq!(waited["matched"], json!(true));
    assert_eq!(waited["entry"]["status"], 201);
    assert!(waited["meta"]["namespace"]
        .as_str()
        .unwrap_or_default()
        .starts_with("session:eoka-har-e2e-test-"));

    let log = session.run(&[
        "--json",
        "network",
        "log",
        "--pattern",
        "*/api/*",
        "--compact",
    ]);
    assert!(
        log["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["url"]
                .as_str()
                .unwrap_or_default()
                .contains("/api/data?from=page")),
        "network log did not include api request: {log}"
    );

    let mut entry = None;
    for _ in 0..20 {
        let _ = std::fs::remove_file(&har_path);
        session.run(&["--json", "network", "har", &har_arg]);
        let har: Value =
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

    session.run(&["--json", "network", "export", &json_arg, "--format", "json"]);
    let exported: Value =
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

    let stopped = session.run(&["--json", "network", "record", "stop"]);
    assert_eq!(stopped["status"]["active"], false);
    session.run(&["--json", "network", "clear"]);
    let log = session.run(&["--json", "network", "log"]);
    assert_eq!(log["entries"].as_array().unwrap().len(), 0);
    succeeds(&session, &["open", "about:blank"]);
}
