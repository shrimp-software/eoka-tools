use eoka_datadome::fixture::Fixture;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use tokio::process::Command;

struct Cli {
    name: String,
    port: Option<u16>,
}

impl Cli {
    fn new(label: &str, port: Option<u16>) -> Self {
        Self {
            name: format!("dd-cli-{label}-{}", std::process::id()),
            port,
        }
    }

    fn effective_name(&self) -> String {
        if self.port.is_some() {
            format!("{}-live", self.name)
        } else {
            self.name.clone()
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_eoka"));
        for name in [
            "EOKA_CDP",
            "EOKA_AUTO_CONNECT",
            "EOKA_FROM_PROFILE",
            "EOKA_PROXY",
            "EOKA_PROXY_FILE",
            "EOKA_PERSIST",
            "EOKA_NO_JS",
            "EOKA_NO_STEALTH",
        ] {
            cmd.env_remove(name);
        }
        cmd.env(
            "EOKA_CHROME_ARGS",
            "--host-resolver-rules=MAP * 127.0.0.1:--no-proxy-server:--site-per-process",
        );
        cmd.args(["--session", &self.name, "--json", "--no-geo-align"]);
        if let Some(port) = self.port {
            cmd.args(["--cdp", &port.to_string()]);
        }
        cmd.args(args).kill_on_drop(true);
        cmd
    }

    async fn run(&self, args: &[&str]) -> Value {
        let output = tokio::time::timeout(Duration::from_secs(35), self.command(args).output())
            .await
            .expect("CLI deadline")
            .unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "non-JSON CLI response: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
        assert_eq!(
            output.status.success(),
            response["ok"] == true,
            "{response}"
        );
        response
    }

    async fn ok(&self, args: &[&str]) -> Value {
        let response = self.run(args).await;
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }

    async fn eval(&self, expression: &str) -> Value {
        let data = self
            .ok(&["eval", &format!("JSON.stringify({expression})")])
            .await;
        let encoded: String = serde_json::from_str(data.as_str().unwrap()).unwrap();
        serde_json::from_str(&encoded).unwrap()
    }

    fn children(&self, pid: &str) -> Vec<u32> {
        let mut children = Vec::new();
        for task in std::fs::read_dir(format!("/proc/{}/task", pid.trim()))
            .unwrap()
            .flatten()
        {
            if let Ok(value) = std::fs::read_to_string(task.path().join("children")) {
                children.extend(
                    value
                        .split_whitespace()
                        .map(|child| child.parse::<u32>().unwrap()),
                );
            }
        }
        children.sort_unstable();
        children.dedup();
        children
    }

    fn pid_path(&self) -> PathBuf {
        eoka_sdk::session::pid_path(&self.effective_name())
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let mut command = self.command(&["kill"]);
        let _ = command.as_std_mut().output();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome CLI/daemon fixture, not live DataDome acceptance"]
async fn datadome_cli_preserves_owned_session_and_reports_outcomes() {
    let cli = Cli::new("owned", None);
    let missing = cli.run(&["captcha", "datadome"]).await;
    assert_eq!(missing["error_detail"]["code"], "session_unavailable");
    assert!(!cli.pid_path().exists());
    let mut pid = String::new();
    let mut children = Vec::new();
    let mut target = String::new();
    for (mode, expected) in [
        ("simple", "challenge_cleared"),
        ("image", "challenge_cleared"),
        ("absent", "not_present"),
        ("blocked", "blocked"),
        ("banned", "ip_banned"),
        ("unsupported", "failed"),
        ("reblock", "blocked"),
        ("timeout", "datadome_timeout"),
    ] {
        let fixture = if mode == "image" {
            Fixture::start(false, false, false).await
        } else {
            Fixture::scenario(mode).await
        };
        cli.ok(&["open", &fixture.url]).await;
        if pid.is_empty() {
            pid = std::fs::read_to_string(cli.pid_path()).unwrap();
            children = cli.children(&pid);
            assert!(!children.is_empty());
            cli.ok(&["network", "record", "start"]).await;
            cli.ok(&["network", "intercept", "add", "*/accepted"]).await;
        }
        let before = cli.eval("({marker:window.fixtureMarker,local:localStorage.getItem('fixture'),session:sessionStorage.getItem('fixture')})").await;
        let response = cli
            .run(&[
                "captcha",
                "datadome",
                "--timeout-ms",
                if mode == "timeout" { "5000" } else { "20000" },
            ])
            .await;
        if mode == "timeout" {
            assert_eq!(response["error_detail"]["code"], expected, "{response}");
            let held = cli.eval("document.body.dataset.pressed==='true' && document.body.dataset.released!=='true'").await;
            assert_eq!(held, false);
        } else {
            assert_eq!(response["data"]["outcome"], expected, "{mode}: {response}");
            let id = response["data"]["tab_id"].as_str().unwrap();
            if target.is_empty() {
                target = id.into();
            }
            assert_eq!(id, target);
        }
        assert_eq!(std::fs::read_to_string(cli.pid_path()).unwrap(), pid);
        assert_eq!(cli.children(&pid), children);
        assert_eq!(cli.eval("({marker:window.fixtureMarker,local:localStorage.getItem('fixture'),session:sessionStorage.getItem('fixture')})").await, before);
        assert_eq!(
            cli.eval("document.cookie.includes('other=retained')").await,
            true
        );
        assert_eq!(cli.ok(&["info"]).await["url"], fixture.url);
        if expected == "challenge_cleared" {
            assert_eq!(
                cli.eval("document.body.dataset.restored==='true'").await,
                true
            );
        }
        cli.ok(&["snapshot"]).await;
        cli.ok(&["click", "Continue"]).await;
        assert_eq!(
            cli.eval("document.body.dataset.continued==='true'").await,
            true
        );
        cli.ok(&["network", "record", "status"]).await;
        if mode == "simple" {
            tokio::time::timeout(Duration::from_secs(2), fixture.started.notified())
                .await
                .unwrap();
            cli.ok(&["exec", "setTimeout(()=>fetch('/accepted'),100)"])
                .await;
            tokio::time::timeout(Duration::from_secs(2), fixture.started.notified())
                .await
                .unwrap();
            cli.ok(&["eval", "fetch('/accepted').then(r=>r.text())"])
                .await;
        }
    }
    let logs = cli.ok(&["network", "intercept", "log"]).await;
    assert!(logs.to_string().contains("accepted"), "{logs}");
    cli.ok(&["network", "record", "stop"]).await;
    cli.ok(&["close"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome interception fixture"]
async fn interception_preserves_binary_responses_and_bounds_worker_logs() {
    let files = tempfile::tempdir().unwrap();
    let response_path = files.path().join("response.bin");
    let capture_path = files.path().join("request.json");
    let bytes: Vec<u8> = (0..=255).collect();
    std::fs::write(&response_path, &bytes).unwrap();
    let fixture = Fixture::scenario("absent").await;
    let cli = Cli::new("interception", None);
    cli.ok(&["open", &fixture.url]).await;
    cli.ok(&[
        "network",
        "intercept",
        "add",
        "*/binary",
        "--respond",
        response_path.to_str().unwrap(),
        "--capture",
        capture_path.to_str().unwrap(),
    ])
    .await;
    assert_eq!(
        cli.ok(&["captcha", "datadome"]).await["outcome"],
        "not_present"
    );
    let data = cli.ok(&["eval", "fetch('/binary',{method:'POST',body:'fixture-body'}).then(r=>r.arrayBuffer()).then(b=>Array.from(new Uint8Array(b)))"]).await;
    assert_eq!(
        serde_json::from_str::<Vec<u8>>(data.as_str().unwrap()).unwrap(),
        bytes
    );
    let capture: Value = serde_json::from_slice(&std::fs::read(&capture_path).unwrap()).unwrap();
    assert_eq!(capture["method"], "POST");
    assert_eq!(capture["postData"], "fixture-body");
    let logs = cli.ok(&["network", "intercept", "log", "--clear"]).await;
    assert_eq!(logs.as_array().unwrap().len(), 1);
    assert_eq!(logs[0]["action"], "responded");

    cli.ok(&[
        "network",
        "intercept",
        "add",
        "*/bulk*",
        "--respond",
        response_path.to_str().unwrap(),
    ])
    .await;
    let data = cli.ok(&["eval", "(async()=>{for(let batch=0;batch<11;batch++){await Promise.all(Array.from({length:100},(_,i)=>fetch('/bulk?i='+(batch*100+i)).then(r=>r.arrayBuffer())))}return 1100})()"] ).await;
    assert_eq!(data, "1100");
    let logs = cli.ok(&["network", "intercept", "log", "--clear"]).await;
    assert_eq!(logs.as_array().unwrap().len(), 1000);
    assert!(logs
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["action"] == "responded"));
    assert_eq!(
        cli.ok(&["network", "intercept", "log"]).await,
        serde_json::json!([])
    );
    cli.ok(&["exec", "window.streaming=true; window.streamingWork=(async()=>{while(window.streaming){await Promise.all(Array.from({length:8},()=>fetch('/bulk').then(r=>r.arrayBuffer())))}return 'done'})(); void 0"]).await;
    assert_eq!(
        cli.ok(&["captcha", "datadome"]).await["outcome"],
        "not_present"
    );
    cli.ok(&["eval", "window.streaming=false; window.streamingWork"])
        .await;
    cli.ok(&["snapshot"]).await;
    cli.ok(&["close"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome client-disconnect fixture"]
async fn accepted_request_finishes_after_cli_disconnect_without_replay() {
    let fixture = Fixture::scenario("simple").await;
    let cli = Cli::new("disconnect", None);
    cli.ok(&["open", &fixture.url]).await;
    let pid = std::fs::read_to_string(cli.pid_path()).unwrap();
    let mut child = cli
        .command(&["captcha", "datadome", "--timeout-ms", "20000"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), fixture.started.notified())
        .await
        .unwrap();
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    cli.ok(&["snapshot"]).await;
    assert_eq!(
        cli.eval("document.body.dataset.restored==='true'").await,
        true
    );
    assert_eq!(std::fs::read_to_string(cli.pid_path()).unwrap(), pid);
    assert_eq!(
        cli.ok(&["captcha", "datadome"]).await["outcome"],
        "not_present"
    );
    cli.ok(&["click", "Continue"]).await;
    cli.ok(&["close"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome borrowed-session fixture"]
async fn datadome_cli_preserves_borrowed_browser_other_tab_and_missing_target() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let browser = eoka::Browser::launch_with(|config| {
        config.extra_args.extend([
            format!("--remote-debugging-port={port}"),
            "--host-resolver-rules=MAP * 127.0.0.1".into(),
            "--no-proxy-server".into(),
            "--site-per-process".into(),
        ])
    })
    .await
    .unwrap();
    let fixture = Fixture::scenario("simple").await;
    let page = browser.new_page(&fixture.url).await.unwrap();
    let other = browser.new_page("about:blank").await.unwrap();
    other.execute_sync("window.untouched='yes'").await.unwrap();
    let cli = Cli::new("borrowed", Some(port));
    cli.ok(&["tab", "attach", page.target_id()]).await;
    let data = cli
        .ok(&["captcha", "datadome", "--timeout-ms", "20000"])
        .await;
    assert_eq!(data["outcome"], "challenge_cleared");
    assert_eq!(data["tab_id"], page.target_id());
    assert_eq!(
        other
            .evaluate_sync::<String>("window.untouched")
            .await
            .unwrap(),
        "yes"
    );
    browser.close_tab(page.target_id()).await.unwrap();
    let response = cli
        .run(&["captcha", "datadome", "--timeout-ms", "5000"])
        .await;
    assert_eq!(response["ok"], false);
    assert_eq!(
        other
            .evaluate_sync::<String>("window.untouched")
            .await
            .unwrap(),
        "yes"
    );
    cli.ok(&["close"]).await;
    other.execute_sync("window.stillAlive=true").await.unwrap();
    browser.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "local Chrome interception integrity regression"]
async fn failed_interception_files_suppress_clearance_and_idle_special_files_do_not_stall() {
    let files = tempfile::tempdir().unwrap();
    for option in ["--capture", "--respond"] {
        let fixture = Fixture::scenario("simple").await;
        let cli = Cli::new(
            if option == "--capture" {
                "bad-capture"
            } else {
                "bad-response"
            },
            None,
        );
        cli.ok(&["open", &fixture.url]).await;
        let missing = files.path().join("missing-parent/file");
        cli.ok(&[
            "network",
            "intercept",
            "add",
            "*/accepted",
            option,
            missing.to_str().unwrap(),
        ])
        .await;
        let response = cli.run(&["captcha", "datadome"]).await;
        assert_eq!(
            response["error_detail"]["code"], "datadome_cleanup_failed",
            "{response}"
        );
        assert_eq!(cli.eval("document.body.dataset.restored").await, "true");
        let logs = cli.ok(&["network", "intercept", "log"]).await;
        assert!(logs
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["action"].as_str().unwrap().contains("failed")));
        cli.ok(&["snapshot"]).await;
        cli.ok(&["close"]).await;
    }
    let fifo = files.path().join("fifo");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let fixture = Fixture::scenario("absent").await;
    let cli = Cli::new("idle-special", None);
    cli.ok(&["open", &fixture.url]).await;
    for path in [fifo.to_str().unwrap(), "/dev/zero"] {
        cli.ok(&["network", "intercept", "remove", "all"]).await;
        cli.ok(&[
            "network",
            "intercept",
            "add",
            "*/accepted",
            "--respond",
            path,
        ])
        .await;
        cli.ok(&["exec", "setTimeout(()=>fetch('/accepted').then(()=>document.body.dataset.fetched='true'),100); void 0"]).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        tokio::time::timeout(Duration::from_secs(5), cli.ok(&["snapshot"]))
            .await
            .unwrap();
    }
    let mut socket =
        tokio::net::UnixStream::connect(eoka_sdk::session::socket_path(&cli.effective_name()))
            .await
            .unwrap();
    eoka_sdk::write_msg(&mut socket, &eoka_sdk::Request::Shutdown)
        .await
        .unwrap();
    let shutdown: eoka_sdk::Response =
        tokio::time::timeout(Duration::from_secs(5), eoka_sdk::read_msg(&mut socket))
            .await
            .unwrap()
            .unwrap();
    assert!(shutdown.ok);
}
