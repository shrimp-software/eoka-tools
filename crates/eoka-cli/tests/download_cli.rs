use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    url: String,
    task: tokio::task::JoinHandle<()>,
    sent: Arc<AtomicU64>,
}

impl Fixture {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let sent = Arc::new(AtomicU64::new(0));
        let counter = sent.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let sent = counter.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        let Ok(n) = stream.read(&mut chunk).await else {
                            return;
                        };
                        if n == 0 || request.len() > 65536 {
                            return;
                        }
                        request.extend_from_slice(&chunk[..n]);
                    }
                    let request = String::from_utf8_lossy(&request);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    if path == "/redirect" {
                        let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /file\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                        return;
                    }
                    let authenticated = request.contains("session=accepted");
                    if path == "/" {
                        let body = br#"<a id="download" href="/file">Download</a><a id="slow" href="/slow">Slow</a><button id="blob" onclick="const a=document.createElement('a');a.href=window.URL.createObjectURL(new Blob(['fixture blob bytes']));a.download='blob.bin';a.click()">Blob</button><a id="popup" href="/file" target="_blank">Popup</a>"#;
                        let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=accepted; HttpOnly; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = stream.write_all(headers.as_bytes()).await;
                        let _ = stream.write_all(body).await;
                        return;
                    }
                    if path == "/html" || !authenticated {
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 18\r\nConnection: close\r\n\r\n<html>login</html>").await;
                        return;
                    }
                    let text = matches!(path, "/text" | "/endless" | "/controls");
                    let data = if path == "/text" {
                        "é世🙂".repeat(4096).into_bytes()
                    } else if path == "/controls" {
                        vec![0; 65536]
                    } else {
                        (0..65536).map(|n| (n % 251) as u8).collect::<Vec<_>>()
                    };
                    let count = match path {
                        "/empty" => 0,
                        "/slow" => 80,
                        "/large" => 1024,
                        "/endless" => 10000,
                        "/controls" => 256,
                        _ => 2,
                    };
                    let unknown = path == "/unknown" || path == "/endless";
                    let length = if unknown {
                        "Transfer-Encoding: chunked\r\n".into()
                    } else {
                        format!("Content-Length: {}\r\n", data.len() * count)
                    };
                    let disposition = if text {
                        ""
                    } else {
                        "Content-Disposition: attachment; filename=fixture.bin\r\n"
                    };
                    let mime = if text {
                        "text/plain; charset=utf-8"
                    } else {
                        "application/octet-stream"
                    };
                    let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\n{disposition}{length}Connection: close\r\n\r\n");
                    if stream.write_all(headers.as_bytes()).await.is_err() {
                        return;
                    }
                    for _ in 0..count {
                        if unknown
                            && stream
                                .write_all(format!("{:x}\r\n", data.len()).as_bytes())
                                .await
                                .is_err()
                        {
                            return;
                        }
                        if stream.write_all(&data).await.is_err() {
                            return;
                        }
                        if unknown && stream.write_all(b"\r\n").await.is_err() {
                            return;
                        }
                        sent.fetch_add(data.len() as u64, Ordering::Relaxed);
                        if matches!(path, "/slow" | "/endless") {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                    if unknown {
                        let _ = stream.write_all(b"0\r\n\r\n").await;
                    }
                });
            }
        });
        Self { url, task, sent }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Cli {
    session: String,
    directory: tempfile::TempDir,
    port: Option<u16>,
    idle_ms: Option<u64>,
}

impl Cli {
    fn new() -> Self {
        Self {
            session: format!(
                "download-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            directory: tempfile::tempdir().unwrap(),
            port: None,
            idle_ms: None,
        }
    }

    fn effective_name(&self) -> String {
        if self.port.is_some() {
            format!("{}-live", self.session)
        } else if std::env::var_os("EOKA_DOWNLOAD_TEST_HEADED").is_some() {
            format!("{}-headed", self.session)
        } else {
            self.session.clone()
        }
    }

    fn binary() -> std::ffi::OsString {
        std::env::var_os("EOKA_DOWNLOAD_TEST_BINARY")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_eoka").into())
    }

    fn command(&self, args: &[&str]) -> tokio::process::Command {
        self.command_mode(args, true)
    }

    fn command_mode(&self, args: &[&str], json: bool) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(Self::binary());
        for name in [
            "EOKA_CDP",
            "EOKA_AUTO_CONNECT",
            "EOKA_FROM_PROFILE",
            "EOKA_PROXY",
            "EOKA_PROXY_FILE",
            "EOKA_PERSIST",
            "EOKA_NO_JS",
            "EOKA_NO_STEALTH",
            "EOKA_IDLE_TIMEOUT",
        ] {
            command.env_remove(name);
        }
        if let Some(port) = self.port {
            command.args(["--cdp", &port.to_string()]);
        }
        if let Some(ms) = self.idle_ms {
            command.env("EOKA_IDLE_TIMEOUT", ms.to_string());
        }
        command.args(["--session", &self.session, "--no-geo-align"]);
        if json {
            command.arg("--json");
        }
        if std::env::var_os("EOKA_DOWNLOAD_TEST_HEADED").is_some() {
            command.arg("--headed");
        }
        if std::env::var_os("EOKA_DOWNLOAD_TEST_LIVE").is_some() {
            command.arg("--no-stealth");
        }
        command
            .args(args)
            .current_dir(self.directory.path())
            .kill_on_drop(true);
        command
    }

    async fn call(&self, args: &[&str]) -> Value {
        let output = tokio::time::timeout(Duration::from_secs(45), self.command(args).output())
            .await
            .unwrap()
            .unwrap();
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{args:?}: {error}: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }

    async fn ok(&self, args: &[&str]) -> Value {
        let result = self.call(args).await;
        assert_eq!(result["ok"], true, "{args:?}: {result}");
        result["data"].clone()
    }

    async fn open(&self, fixture: &Fixture) {
        self.ok(&["open", &fixture.url]).await;
        self.ok(&["wait", "--text", "Download"]).await;
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::process::Command::new(Self::binary())
            .args(["--session", &self.effective_name(), "kill"])
            .output();
    }
}

#[tokio::test]
#[ignore = "requires Chrome; page evidence and preflight dispatch guards"]
async fn unavailable_page_and_targets_report_evidence_without_clicking() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    cli.ok(&["exec", r#"document.title='Transfer fixture';document.body.innerHTML='<nav><button>Features</button><button>Get started</button></nav><main><h1>Unavailable transfer</h1><p role="alert">This transfer was deleted</p><input value="INPUT_SECRET"><textarea>TEXTAREA_SECRET</textarea><div hidden>HIDDEN_SECRET</div><button id="disabled" disabled>Disabled</button><button id="hidden" style="display:none">Hidden</button><button id="old">Old target</button></main>';globalThis.clicks=0;document.addEventListener('click',()=>++globalThis.clicks);history.replaceState(null,'','/?secret=TOKEN')"#]).await;
    let observed = cli.ok(&["observe", "--structured"]).await;
    let page = &observed["page"];
    assert_eq!(page["title"], "Transfer fixture");
    let filtered = cli
        .ok(&[
            "observe",
            "--structured",
            "--filter",
            "buttons",
            "--max",
            "1",
        ])
        .await;
    assert_eq!(filtered["elements"], json!([observed["elements"][0]]));
    let empty = cli.ok(&["observe", "--structured", "--max", "0"]).await;
    assert_eq!(empty["elements"], json!([]));
    assert!(empty["text"]
        .as_str()
        .unwrap()
        .contains("No interactive elements found."));
    let tack = cli
        .ok(&[
            "tack",
            "--raw-json",
            "return await tools.invoke({path:'eoka.observe',input:{structured:true,max:1}})",
        ])
        .await;
    assert!(
        tack.to_string().contains("This transfer was deleted"),
        "{tack}"
    );
    assert_eq!(page["messages"][0], "This transfer was deleted");
    for secret in ["INPUT_SECRET", "TEXTAREA_SECRET", "HIDDEN_SECRET"] {
        assert!(!page.to_string().contains(secret), "{page}");
    }
    let legacy = cli.ok(&["observe"]).await;
    assert!(legacy.is_string());
    assert!(legacy
        .as_str()
        .unwrap()
        .contains("This transfer was deleted"));
    let human = cli
        .command_mode(&["observe"], false)
        .output()
        .await
        .unwrap();
    assert!(human.status.success());
    assert!(
        String::from_utf8_lossy(&human.stdout).contains("Page message: This transfer was deleted")
    );
    let snapshot = cli.ok(&["snapshot", "-i"]).await;
    let old_ref = snapshot
        .as_str()
        .unwrap()
        .lines()
        .find(|line| line.contains("Old target"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    cli.ok(&["exec", "document.getElementById('old').remove()"])
        .await;
    for (target, code) in [
        ("text:Download all", "download_target_not_found"),
        ("css:#disabled", "download_target_disabled"),
        ("css:#hidden", "download_target_hidden"),
        (old_ref.as_str(), "download_target_stale"),
        ("@e999999", "download_target_stale"),
        ("css:[", "download_target_resolution_failed"),
    ] {
        let result = cli
            .call(&["download", "--click", target, "--out", "absent.bin"])
            .await;
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["error_detail"]["code"], code, "{result}");
        let context = &result["error_detail"]["context"];
        assert_eq!(context["action_status"], "not_attempted");
        assert!(context["page"]["url"].as_str().unwrap().ends_with('/'));
        assert_eq!(context["page"]["messages"][0], "This transfer was deleted");
        assert!(result.get("data").is_none());
    }
    assert_eq!(cli.ok(&["eval", "globalThis.clicks"]).await, "0");
    assert_eq!(cli.ok(&["download", "list"]).await, serde_json::json!([]));
    assert!(!cli.directory.path().join("absent.bin").exists());
    let error = cli
        .command_mode(
            &[
                "download",
                "--click",
                "text:Download all",
                "--out",
                "absent.bin",
            ],
            false,
        )
        .output()
        .await
        .unwrap();
    assert!(!error.status.success());
    let stderr = String::from_utf8_lossy(&error.stderr);
    assert!(stderr.contains("download_target_not_found"));
    assert!(stderr.contains("This transfer was deleted"));
    assert!(stderr.contains("Action was not attempted"));
}

#[tokio::test]
#[ignore = "requires Chrome; bounded context and application readiness"]
async fn observation_bounds_context_and_preserves_element_targeting() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    let before = cli.ok(&["observe", "--structured"]).await;
    cli.ok(&["exec", "const main=document.createElement('main');main.innerHTML=Array.from({length:8},(_,i)=>'<h1>Heading '+i+'</h1>').join('')+Array.from({length:8},(_,i)=>'<p role=alert>Message '+i+'</p>').join('')+'<p>'+ '🙂word '.repeat(1000)+'</p>';document.body.append(main)"]).await;
    let after = cli.ok(&["--agent", "observe", "--max", "1"]).await;
    assert_eq!(before["elements"][0], after["elements"][0]);
    let page = &after["page"];
    assert_eq!(page["headings"].as_array().unwrap().len(), 5);
    assert_eq!(page["messages"].as_array().unwrap().len(), 5);
    assert!(page["text_excerpt"].as_str().unwrap().chars().count() <= 2000);
    assert_eq!(page["truncated"], true);
    let completed = cli
        .ok(&["download", "--click", "0", "--out", "indexed.bin", "--wait"])
        .await;
    assert_eq!(completed["state"], "completed");
    cli.ok(&["exec", "document.body.innerHTML='<main><p role=status>Loading transfer</p></main>';setTimeout(()=>{document.querySelector('main').innerHTML='<h1>Transfer ready</h1><button>Download ready</button>'},1500)"]).await;
    let loading = cli.ok(&["observe", "--structured"]).await;
    assert_eq!(loading["page"]["ready_state"], "complete");
    assert_eq!(loading["page"]["messages"][0], "Loading transfer");
    cli.ok(&["wait", "--text", "Download ready"]).await;
    let ready = cli.ok(&["observe", "--structured"]).await;
    assert_eq!(ready["page"]["headings"][0], "Transfer ready");
}

#[tokio::test]
#[ignore = "requires Chrome; diagnosis failure preserves the operation result"]
async fn unavailable_context_does_not_mask_observation_or_download_errors() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    cli.ok(&[
        "exec",
        "document.createTreeWalker=()=>{throw new Error('fixture context failure')}",
    ])
    .await;
    let observed = cli.ok(&["observe", "--structured"]).await;
    assert_eq!(observed["page"], Value::Null);
    assert!(!observed["elements"].as_array().unwrap().is_empty());
    let result = cli
        .call(&["download", "--click", "css:#absent", "--out", "absent.bin"])
        .await;
    assert_eq!(result["error_detail"]["code"], "download_target_not_found");
    assert_eq!(
        result["error_detail"]["context"]["action_status"],
        "not_attempted"
    );
    assert!(result["error_detail"]["context"].get("page").is_none());
}

#[tokio::test]
#[ignore = "requires Chrome; temporary page context and resumed waiting"]
async fn html_attachment_diagnostics_and_wait_timeout_keep_job_identity() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    let before = cli.ok(&["info"]).await;
    let html = cli
        .call(&[
            "download",
            "--url",
            &format!("{}/html#secret", fixture.url),
            "--out",
            "html.bin",
            "--start-timeout-ms",
            "1000",
            "--wait",
        ])
        .await;
    assert_eq!(html["ok"], false, "{html}");
    let context = &html["error_detail"]["context"];
    assert_eq!(context["download_id"], html["data"]["id"]);
    assert_eq!(context["page"]["url"], format!("{}/html", fixture.url));
    assert!(context["page"]["text_excerpt"]
        .as_str()
        .unwrap()
        .contains("login"));
    assert_eq!(cli.ok(&["info"]).await, before);
    let waiting = cli
        .call(&[
            "download",
            "--click",
            "css:#slow",
            "--out",
            "slow.bin",
            "--wait",
            "--timeout-ms",
            "10",
        ])
        .await;
    assert_eq!(waiting["error_detail"]["code"], "download_wait_timeout");
    let id = waiting["data"]["id"].as_str().unwrap();
    assert_eq!(waiting["error_detail"]["context"]["download_id"], id);
    let human = cli
        .command_mode(&["download", "wait", id, "--timeout-ms", "10"], false)
        .output()
        .await
        .unwrap();
    assert!(!human.status.success());
    let stderr = String::from_utf8_lossy(&human.stderr);
    assert!(stderr.contains(id));
    assert!(stderr.contains("state:"));
    assert!(stderr.contains("download status"));
    let completed = cli.ok(&["download", "wait", id]).await;
    assert_eq!(completed["state"], "completed");
    assert_eq!(
        cli.ok(&["download", "list"])
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
#[ignore = "requires Chrome; local authenticated attachment fixture"]
async fn native_download_click_url_blob_and_file_guards() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    let job = cli
        .ok(&[
            "download",
            "--click",
            "css:#download",
            "--out",
            "click.bin",
            "--wait",
        ])
        .await;
    assert_eq!(job["state"], "completed");
    let expected: Vec<u8> = (0..65536)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>()
        .repeat(2);
    assert_eq!(
        std::fs::read(cli.directory.path().join("click.bin")).unwrap(),
        expected
    );
    assert_eq!(job["received_bytes"], expected.len());
    let conflict = cli
        .call(&["download", "--click", "css:#download", "--out", "click.bin"])
        .await;
    assert_eq!(
        conflict["error_detail"]["code"],
        "download_destination_exists"
    );
    let blob = cli
        .ok(&[
            "download",
            "--click",
            "css:#blob",
            "--out",
            "blob.bin",
            "--wait",
        ])
        .await;
    assert_eq!(blob["state"], "completed");
    assert_eq!(
        std::fs::read(cli.directory.path().join("blob.bin")).unwrap(),
        b"fixture blob bytes"
    );
    for path in ["redirect", "unknown", "empty"] {
        let name = format!("{path}.bin");
        let job = cli
            .ok(&[
                "download",
                "--url",
                &format!("{}/{path}", fixture.url),
                "--out",
                &name,
                "--wait",
            ])
            .await;
        assert_eq!(job["state"], "completed");
        assert_eq!(
            std::fs::metadata(cli.directory.path().join(name))
                .unwrap()
                .len(),
            if path == "empty" {
                0
            } else {
                expected.len() as u64
            }
        );
    }
    cli.ok(&[
        "download",
        "--click",
        "css:#popup",
        "--out",
        "popup.bin",
        "--wait",
    ])
    .await;
    assert_eq!(
        std::fs::read(cli.directory.path().join("popup.bin")).unwrap(),
        expected
    );
    let info = cli.ok(&["info"]).await;
    assert!(info.to_string().contains(&fixture.url));
    let html = cli
        .call(&[
            "download",
            "--url",
            &format!("{}/html", fixture.url),
            "--out",
            "html.zip",
            "--start-timeout-ms",
            "500",
            "--wait",
        ])
        .await;
    assert_eq!(html["ok"], false, "{html}");
    assert!(!cli.directory.path().join("html.zip").exists());
    let mismatch = cli
        .call(&[
            "download",
            "--url",
            &format!("{}/file", fixture.url),
            "--out",
            "wrong.bin",
            "--expect-bytes",
            "3",
            "--wait",
        ])
        .await;
    assert_eq!(
        mismatch["error_detail"]["code"], "download_size_mismatch",
        "{mismatch}"
    );
    assert!(!cli.directory.path().join("wrong.bin").exists());
    cli.ok(&["close"]).await;
    assert!(cli.directory.path().join("click.bin").exists());
}

#[tokio::test]
#[ignore = "requires Chrome; verifies separate clients and nonblocking waiting"]
async fn background_wait_cancel_and_destination_race() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    let job = cli
        .ok(&["download", "--click", "css:#slow", "--out", "slow.bin"])
        .await;
    let id = job["id"].as_str().unwrap();
    assert_eq!(
        cli.call(&["download", "--click", "css:#download", "--out", "other.bin"])
            .await["error_detail"]["code"],
        "download_busy"
    );
    assert_eq!(
        cli.call(&["download", "wait", id, "--timeout-ms", "20"])
            .await["error_detail"]["code"],
        "download_wait_timeout"
    );
    let mut waiter = cli
        .command(&["download", "wait", id])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let status = tokio::time::timeout(Duration::from_secs(2), cli.ok(&["download", "status", id]))
        .await
        .unwrap();
    assert!(!matches!(
        status["state"].as_str(),
        Some("failed" | "completed")
    ));
    cli.ok(&["download", "cancel", id]).await;
    assert!(
        !tokio::time::timeout(Duration::from_secs(10), waiter.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert!(!cli.directory.path().join("slow.bin").exists());
    let job = cli
        .ok(&["download", "--click", "css:#slow", "--out", "race.bin"])
        .await;
    std::fs::write(cli.directory.path().join("race.bin"), b"keep me").unwrap();
    let result = cli
        .call(&["download", "wait", job["id"].as_str().unwrap()])
        .await;
    assert_eq!(
        result["error_detail"]["code"], "download_destination_exists",
        "{result}"
    );
    assert_eq!(
        std::fs::read(cli.directory.path().join("race.bin")).unwrap(),
        b"keep me"
    );
}

#[tokio::test]
#[ignore = "requires Chrome; verifies pending trigger timeout and cancellation"]
async fn pending_click_is_not_replayed_and_releases_download_ownership() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    cli.ok(&[
        "eval",
        "globalThis.downloadClicks = 0; const button = document.createElement('button'); button.id = 'noop'; button.textContent = 'No download'; button.onclick = () => ++globalThis.downloadClicks; document.body.append(button)",
    ])
    .await;
    let result = cli
        .call(&[
            "download",
            "--click",
            "css:#noop",
            "--out",
            "timeout.bin",
            "--start-timeout-ms",
            "2000",
            "--wait",
        ])
        .await;
    assert_eq!(
        result["error_detail"]["code"], "download_not_started",
        "{result}"
    );
    assert_eq!(
        result["error_detail"]["context"]["action_status"],
        "may_have_executed"
    );
    assert_eq!(
        result["error_detail"]["context"]["download_id"],
        result["data"]["id"]
    );
    assert!(result["error_detail"]["context"]["page"]["url"]
        .as_str()
        .unwrap()
        .contains(&fixture.url));
    assert_eq!(result["data"]["staging_path"], Value::Null);
    assert!(!cli.directory.path().join("timeout.bin").exists());
    assert_eq!(cli.ok(&["eval", "globalThis.downloadClicks"]).await, "1");

    let job = cli
        .ok(&["download", "--click", "css:#noop", "--out", "canceled.bin"])
        .await;
    assert_eq!(job["state"], "pending");
    let id = job["id"].as_str().unwrap();
    cli.ok(&["download", "cancel", id]).await;
    let result = cli.call(&["download", "wait", id]).await;
    assert_eq!(
        result["error_detail"]["code"], "download_canceled",
        "{result}"
    );
    assert_eq!(result["data"]["staging_path"], Value::Null);
    assert!(!cli.directory.path().join("canceled.bin").exists());
    assert_eq!(cli.ok(&["eval", "globalThis.downloadClicks"]).await, "2");

    let completed = cli
        .ok(&[
            "download",
            "--click",
            "css:#download",
            "--out",
            "after.bin",
            "--wait",
        ])
        .await;
    assert_eq!(completed["state"], "completed");
}

#[tokio::test]
#[ignore = "requires Chrome; verifies bounded browser text reads"]
async fn bounded_fetch_limits_binary_and_endless_responses() {
    let fixture = Fixture::new().await;
    let cli = Cli::new();
    cli.open(&fixture).await;
    let text = cli.ok(&["fetch", &format!("{}/text", fixture.url)]).await;
    assert_eq!(text["bytesRead"], 8192);
    assert_eq!(text["truncated"], true);
    assert!(!text["body"].as_str().unwrap().contains('\u{fffd}'));
    let empty = cli
        .ok(&["fetch", &format!("{}/text", fixture.url), "--max-body", "0"])
        .await;
    assert_eq!(empty["bodyOmitted"], "headers_only");
    let binary = cli.ok(&["fetch", &format!("{}/file", fixture.url)]).await;
    assert_eq!(binary["bodyOmitted"], "binary");
    assert_eq!(binary["body"], "");
    assert_eq!(
        cli.call(&["fetch", &format!("{}/file", fixture.url), "--body-only"])
            .await["error_detail"]["code"],
        "fetch_binary_body"
    );
    assert_eq!(
        cli.call(&["fetch", &format!("{}/text", fixture.url), "--body-only"])
            .await["error_detail"]["code"],
        "fetch_body_truncated"
    );
    assert_eq!(
        cli.call(&[
            "fetch",
            &format!("{}/text", fixture.url),
            "--max-body",
            "16777217"
        ])
        .await["ok"],
        false
    );
    let encoded = cli
        .call(&[
            "fetch",
            &format!("{}/controls", fixture.url),
            "--max-body",
            "16777216",
        ])
        .await;
    assert_eq!(encoded["ok"], false);
    assert!(encoded["error"]
        .as_str()
        .unwrap()
        .contains("fetch_response_too_large"));
    let before = fixture.sent.load(Ordering::Relaxed);
    let endless = cli
        .ok(&[
            "fetch",
            &format!("{}/endless", fixture.url),
            "--max-body",
            "4096",
        ])
        .await;
    assert_eq!(endless["bytesRead"], 4096);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(fixture.sent.load(Ordering::Relaxed) - before < 2 * 1024 * 1024);
}

#[tokio::test]
#[ignore = "requires Chrome; validates client disconnection, idle suppression, and large native transfer"]
async fn disconnected_clients_do_not_cancel_and_large_files_stay_out_of_daemon_memory() {
    let fixture = Fixture::new().await;
    let mut cli = Cli::new();
    cli.idle_ms = Some(2500);
    cli.open(&fixture).await;
    let pid = std::fs::read_to_string(eoka_sdk::session::pid_path(&cli.effective_name())).unwrap();
    let socket = eoka_sdk::session::socket_path(&cli.effective_name());
    let mut stream = tokio::net::UnixStream::connect(&socket).await.unwrap();
    eoka_sdk::write_msg(
        &mut stream,
        &eoka_sdk::Request::DownloadStart(eoka_protocol::DownloadStartArgs {
            click: Some("css:#slow".into()),
            url: None,
            output: cli
                .directory
                .path()
                .join("disconnected.bin")
                .to_str()
                .unwrap()
                .into(),
            expect_bytes: None,
            start_timeout_ms: 1000,
        }),
    )
    .await
    .unwrap();
    drop(stream);
    tokio::time::sleep(Duration::from_millis(3000)).await;
    let jobs = cli.ok(&["download", "list"]).await;
    assert_eq!(jobs.as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(eoka_sdk::session::pid_path(&cli.effective_name())).unwrap(),
        pid
    );
    let id = jobs[0]["id"].as_str().unwrap();
    let mut waiter = cli.command(&["download", "wait", id]).spawn().unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    waiter.kill().await.unwrap();
    cli.ok(&["download", "wait", id]).await;
    assert_eq!(
        std::fs::metadata(cli.directory.path().join("disconnected.bin"))
            .unwrap()
            .len(),
        80 * 65536
    );
    fn memory_kib(pid: &str) -> u64 {
        std::fs::read_to_string(format!("/proc/{}/status", pid.trim()))
            .unwrap()
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .and_then(|text| text.split_whitespace().next())
                    .and_then(|n| n.parse().ok())
            })
            .unwrap()
    }
    let before = memory_kib(&pid);
    let result = cli
        .ok(&[
            "download",
            "--url",
            &format!("{}/large", fixture.url),
            "--out",
            "large.bin",
            "--expect-bytes",
            "67108864",
            "--wait",
        ])
        .await;
    assert_eq!(result["received_bytes"], 64 * 1024 * 1024);
    assert_eq!(
        std::fs::metadata(cli.directory.path().join("large.bin"))
            .unwrap()
            .len(),
        64 * 1024 * 1024
    );
    assert!(memory_kib(&pid).saturating_sub(before) < 32 * 1024);
    cli.ok(&["download", "--click", "css:#slow", "--out", "shutdown.bin"])
        .await;
    cli.ok(&["close"]).await;
    assert!(!cli.directory.path().join("shutdown.bin").exists());
    assert!(cli.directory.path().join("large.bin").exists());
}

#[tokio::test]
#[ignore = "requires Chrome; borrowed sessions cannot redirect browser downloads"]
async fn borrowed_browser_download_is_rejected_without_clicking() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let browser = eoka::Browser::launch_with(|config| {
        config.patch_binary = false;
        config
            .extra_args
            .push(format!("--remote-debugging-port={port}"));
    })
    .await
    .unwrap();
    let fixture = Fixture::new().await;
    let page = browser.new_page(&fixture.url).await.unwrap();
    page.execute("window.clicks=0;document.addEventListener('click',()=>window.clicks++)")
        .await
        .unwrap();
    let mut cli = Cli::new();
    cli.port = Some(port);
    cli.ok(&["tab", "attach", page.target_id()]).await;
    let response = cli
        .call(&[
            "download",
            "--click",
            "css:#download",
            "--out",
            "borrowed.bin",
        ])
        .await;
    assert_eq!(response["error_detail"]["code"], "download_unsupported");
    assert_eq!(page.evaluate::<u64>("window.clicks").await.unwrap(), 0);
    assert!(cli.directory.path().read_dir().unwrap().next().is_none());
    cli.ok(&["close"]).await;
    page.execute("window.alive=true").await.unwrap();
    browser.close().await.unwrap();
}
