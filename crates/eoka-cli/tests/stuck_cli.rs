mod support;

use serde_json::Value;
use std::process::Stdio;
use std::time::{Duration, Instant};
use support::{Reply, Session, Site};

const HOME: &str = "<title>home</title><h1>Home</h1><a id=\"hang\" href=\"/hang\">Slow link</a>";
const LEAVE: &str = "<title>leave</title><textarea id=\"note\"></textarea><a id=\"summary\" href=\"/summary\">Summary</a><script>note.addEventListener('input',()=>{window.onbeforeunload=e=>{e.preventDefault();e.returnValue=''}})</script>";
const SUMMARY: &str = "<title>summary</title><h1>Summary</h1>";
const DIALOG: &str = "<title>dialog</title><button id=\"alert\" onclick=\"alert('saved')\">Save</button><button id=\"plain\" onclick=\"document.title='clicked'\">Plain</button>";

fn route(path: &str) -> Option<Reply> {
    match path {
        "/hang" => None,
        "/dialog" => Reply::html(DIALOG),
        "/leave" => Reply::html(LEAVE),
        "/summary" => Reply::html(SUMMARY),
        "/" => Reply::html(HOME),
        _ => Reply::not_found(),
    }
}

fn session(prefix: &str) -> Session {
    Session::agent(prefix).env(eoka_protocol::env::QUEUE_WAIT_MS, 1000)
}

impl Session {
    fn refused(&self, args: &[&str], code: &str, within: Duration) -> Value {
        let (output, response, elapsed) = self.output(args);
        assert!(!output.status.success(), "{args:?} succeeded: {response}");
        assert_eq!(
            response["error_detail"]["code"], code,
            "{args:?}: {response}"
        );
        assert!(
            elapsed < within,
            "{args:?} took {elapsed:?}, expected under {within:?}: {response}"
        );
        response
    }
}

fn hint(response: &Value) -> &str {
    response["error_detail"]["hint"]
        .as_str()
        .unwrap_or_default()
}

#[test]
#[ignore = "requires Chrome; an open JavaScript dialog is named instead of timing out"]
fn an_open_dialog_is_reported_with_the_command_that_clears_it() {
    let site = Site::start(route);
    let session = session("stuck-dialog");
    let fast = Duration::from_secs(8);
    session.run(&["open", &site.url("/dialog")]);

    let step = r#"{"action":{"kind":"click","target":"css:#alert"},"mode":"explore"}"#;
    let (output, response, elapsed) = session.output(&["step", step]);
    assert_eq!(output.status.code(), Some(3), "{response}");
    assert!(elapsed < fast, "step waited {elapsed:?}: {response}");
    assert_eq!(
        response["error_detail"]["code"], "dialog_open",
        "{response}"
    );
    assert!(response["error"].as_str().unwrap().contains("saved"));
    assert!(hint(&response).contains("dialog accept"), "{response}");
    assert!(hint(&response).contains(&session.name), "{response}");
    let operation = response["data"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    for read in [
        vec!["view"],
        vec!["info"],
        vec!["text"],
        vec!["snapshot", "-i"],
        vec!["eval", "() => document.title"],
        vec!["screenshot"],
    ] {
        let refused = session.refused(&read, "dialog_open", fast);
        assert!(hint(&refused).contains("dialog dismiss"), "{refused}");
        assert_eq!(
            refused["error_detail"]["context"]["action_status"], "not_attempted",
            "{refused}"
        );
    }
    let click = r#"{"action":{"kind":"click","target":"css:#plain"},"mode":"explore"}"#;
    let refused = session.refused(&["step", click], "dialog_open", fast);
    assert_eq!(refused["data"]["status"], "not_dispatched", "{refused}");

    let status = session.run(&["status"]);
    assert_eq!(status["blocker"]["kind"], "dialog", "{status}");
    assert_eq!(status["blocker"]["dialog_type"], "alert", "{status}");

    session.run(&["dialog", "accept"]);
    let view = session.run(&["view"]);
    assert_eq!(view["page"]["title"], "dialog", "{view}");
    assert!(session.run(&["status"])["blocker"].is_null());
    let (_, record, _) =
        session.output(&["operations", "wait", &operation, "--timeout-ms", "5000"]);
    assert_eq!(record["data"]["dispatch_intent"], true, "{record}");
    assert_eq!(record["data"]["state"], "finished", "{record}");
    assert_eq!(session.run(&["eval", "() => document.title"]), "dialog");

    let confirm = session.refused(&["eval", "() => confirm('really?')"], "dialog_open", fast);
    assert!(confirm["error"].as_str().unwrap().contains("really?"));
    assert_eq!(
        confirm["error_detail"]["context"]["action_status"], "may_have_executed",
        "{confirm}"
    );
    session.run(&["dialog", "dismiss"]);
    assert_eq!(session.run(&["info"])["title"], "dialog");
}

#[test]
#[ignore = "requires Chrome; staying on a page stops the click that tried to leave it"]
fn dismissing_a_leave_page_dialog_frees_the_tab_for_the_next_click() {
    let site = Site::start(route);
    let session = session("stuck-leave");
    let fast = Duration::from_secs(5);
    session.run(&["open", &site.url("/leave")]);
    session.run(&[
        "step",
        r#"{"action":{"kind":"fill","target":"css:#note","text":"draft"}}"#,
    ]);
    let leave = r#"{"action":{"kind":"click","target":"css:#summary"},"expect":{"kind":"url","value":"/summary","contains":true}}"#;

    let first = session.refused(&["step", leave], "dialog_open", fast);
    let operation = first["data"]["operation_id"].as_str().unwrap().to_owned();
    assert_eq!(
        session.run(&["status"])["blocker"]["dialog_type"],
        "beforeunload"
    );

    let dismissed = session.run(&["dialog", "dismiss"]);
    assert!(
        dismissed.as_str().unwrap().contains(&operation),
        "{dismissed}"
    );
    let (_, record, _) =
        session.output(&["operations", "wait", &operation, "--timeout-ms", "1000"]);
    assert_eq!(record["data"]["state"], "finished", "{record}");
    assert_eq!(session.run(&["info"])["title"], "leave");

    let again = session.refused(&["step", leave], "dialog_open", fast);
    assert_eq!(
        again["data"]["operation"]["dispatch_intent"], true,
        "{again}"
    );
    session.run(&["dialog", "accept"]);
    let (_, record, _) = session.output(&[
        "operations",
        "wait",
        again["data"]["operation_id"].as_str().unwrap(),
        "--timeout-ms",
        "10000",
    ]);
    assert_eq!(record["data"]["outcome"], "verified_complete", "{record}");
    assert_eq!(session.run(&["info"])["title"], "summary");
}

#[test]
#[ignore = "requires Chrome; a server that never answers cannot wedge the tab"]
fn a_silent_server_is_reported_and_the_tab_stays_usable() {
    let site = Site::start(route);
    let session = session("stuck-navigation");
    session.run(&["open", &site.url("/")]);

    let (output, response, elapsed) =
        session.output(&["--action-timeout-ms", "2000", "open", &site.url("/hang")]);
    assert_eq!(output.status.code(), Some(4), "{response}");
    assert_eq!(
        response["error_detail"]["code"], "navigation_timeout",
        "{response}"
    );
    assert!(response["error"].as_str().unwrap().contains("/hang"));
    assert!(elapsed < Duration::from_secs(10), "open took {elapsed:?}");
    assert_eq!(response["data"]["status"], "outcome_unknown", "{response}");
    assert_eq!(session.run(&["view"])["page"]["title"], "home");

    let click =
        r#"{"action":{"kind":"click","target":"css:#hang"},"mode":"explore","timeout_ms":2000}"#;
    let (_, clicked, _) = session.output(&["step", click]);
    assert_ne!(clicked["data"]["status"], "not_dispatched", "{clicked}");
    let waiting = Instant::now();
    let pending = loop {
        let (output, response, elapsed) = session.output(&["info"]);
        assert!(elapsed < Duration::from_secs(8), "info took {elapsed:?}");
        if !output.status.success() {
            break response;
        }
        assert!(
            waiting.elapsed() < Duration::from_secs(20),
            "the pending navigation was never reported: {response}"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert_eq!(
        pending["error_detail"]["code"], "navigation_pending",
        "{pending}"
    );
    assert!(pending["error"].as_str().unwrap().contains("/hang"));
    assert!(hint(&pending).contains("reload"), "{pending}");
    assert!(hint(&pending).contains("open URL"), "{pending}");
    std::thread::sleep(Duration::from_secs(5));
    let fast = Duration::from_secs(4);
    session.refused(&["view"], "navigation_pending", fast);
    session.refused(&["eval", "() => 1"], "navigation_pending", fast);
    session.refused(&["text"], "navigation_pending", fast);
    let status = session.run(&["status"]);
    assert_eq!(status["blocker"]["kind"], "navigation", "{status}");
    assert!(session.run(&["tab", "list"]).is_string());

    let started = Instant::now();
    session.run(&["--action-timeout-ms", "10000", "reload"]);
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(session.run(&["view"])["page"]["title"], "home");

    session.run(&["eval", "() => { location.href = '/hang'; return 1 }"]);
    std::thread::sleep(Duration::from_secs(6));
    session.refused(&["info"], "navigation_pending", fast);
    session.run(&["--action-timeout-ms", "10000", "open", &site.url("/dialog")]);
    assert_eq!(session.run(&["info"])["title"], "dialog");
}

#[test]
#[ignore = "requires Chrome; a busy session answers with what it is doing"]
fn a_busy_session_says_what_it_is_running_and_frees_itself_when_the_caller_leaves() {
    let site = Site::start(route);
    let session = session("stuck-busy");
    session.run(&["open", &site.url("/")]);

    let mut sleeper = session
        .command(&["wait", "20000"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if session.run(&["status"])["busy"]["cmd"] == "wait" {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "wait never ran");
        std::thread::sleep(Duration::from_millis(50));
    }
    for read in [vec!["view"], vec!["info"]] {
        let busy = session.refused(&read, "daemon_busy", Duration::from_secs(5));
        assert!(busy["error"].as_str().unwrap().contains("`wait`"), "{busy}");
        assert!(hint(&busy).contains("status"), "{busy}");
        assert_eq!(
            busy["error_detail"]["context"]["action_status"], "not_attempted",
            "{busy}"
        );
    }
    assert!(session
        .run(&["operations", "list", "--limit", "1"])
        .is_array());

    sleeper.kill().unwrap();
    sleeper.wait().unwrap();
    let freed = Instant::now();
    loop {
        let (output, response, _) = session.output(&["info"]);
        if output.status.success() {
            assert_eq!(response["data"]["title"], "home", "{response}");
            break;
        }
        assert!(
            freed.elapsed() < Duration::from_secs(5),
            "the abandoned wait still held the session: {response}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(session.run(&["status"])["busy"].is_null());
}

#[test]
#[ignore = "requires Chrome; download queries answer while a command runs"]
fn download_queries_answer_while_another_command_runs() {
    let site = Site::start(route);
    let session = session("stuck-downloads");
    session.run(&["open", &site.url("/")]);
    let busy = session
        .command(&["eval", "() => new Promise(done => setTimeout(done, 4000))"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let (output, response, elapsed) = session.output(&["download", "list"]);
    assert!(output.status.success(), "{response}");
    assert_eq!(response["data"], serde_json::json!([]), "{response}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    let mut busy = busy;
    busy.wait().unwrap();
}
