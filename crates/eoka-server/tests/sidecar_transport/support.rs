pub(crate) use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub(crate) use eoka_protocol::sidecar::{
    method_names, MAX_IN_FLIGHT, MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
pub(crate) use serde_json::{json, Value};
pub(crate) use tempfile::TempDir;

pub(crate) struct Sidecar {
    pub(crate) child: Child,
    pub(crate) input: Option<ChildStdin>,
    pub(crate) replies: Receiver<Value>,
}

impl Sidecar {
    pub(crate) fn start(state: &Path, scratch: &Path) -> Self {
        Self::start_with_temp(state, scratch, scratch)
    }

    pub(crate) fn start_with_temp(state: &Path, scratch: &Path, temp: &Path) -> Self {
        std::fs::create_dir_all(scratch).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_eoka-server"))
            .env("EOKA_STATE_DIR", state)
            .env("TMPDIR", temp)
            .env("TMP", temp)
            .env("TEMP", temp)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (send, replies) = mpsc::sync_channel(256);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let line = line.unwrap();
                assert!(line.len() <= MAX_MESSAGE_BYTES);
                let value =
                    serde_json::from_str(&line).expect("stdout must contain only JSON responses");
                if send.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input: Some(input),
            replies,
        }
    }

    pub(crate) fn send(&mut self, value: Value) {
        let input = self.input.as_mut().unwrap();
        serde_json::to_writer(&mut *input, &value).unwrap();
        input.write_all(b"\n").unwrap();
        input.flush().unwrap();
    }

    pub(crate) fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"id":id,"method":method,"params":params}));
        let response = self.reply();
        assert_eq!(response["id"], id);
        response
    }

    pub(crate) fn reply(&self) -> Value {
        let response = self
            .replies
            .recv_timeout(Duration::from_secs(45))
            .expect("sidecar response deadline");
        assert_ne!(
            response.get("result").is_some(),
            response.get("error").is_some()
        );
        response
    }

    pub(crate) fn exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "sidecar exited with {status}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "sidecar did not exit by shutdown deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn shutdown(&mut self, id: u64) {
        assert_eq!(
            self.request(id, "server.shutdown", json!({}))["result"],
            json!({})
        );
        self.exit();
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_some() {
            return;
        }
        if let Some(mut input) = self.input.take() {
            let _ = input.write_all(
                b"{\"id\":9007199254740991,\"method\":\"server.shutdown\",\"params\":{}}\n",
            );
        }
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn directories() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::Builder::new().prefix("ep1-").tempdir().unwrap();
    let state = root.path().join("state");
    let scratch = root.path().join("tmp");
    (root, state, scratch)
}

pub(crate) fn launch(server: &mut Sidecar, profile: &Path) -> String {
    let mut params = json!({"headless":true,"userDataDir":profile});
    if let Ok(executable) = std::env::var("EOKA_TEST_CHROME_PATH") {
        params["executablePath"] = json!(executable);
    }
    let response = server.request(1, "browser.launch", params);
    assert_eq!(response["result"]["ownership"], "owned", "{response}");
    assert!(!response["result"]["browserVersion"]
        .as_str()
        .unwrap()
        .is_empty());
    let response = server.request(2, "browser.new_page", json!({}));
    response["result"]["pageId"].as_str().unwrap().to_owned()
}

#[cfg(target_os = "linux")]
pub(crate) fn browser_pid(profile: &Path) -> i32 {
    let link = std::fs::read_link(profile.join("SingletonLock")).unwrap();
    link.to_string_lossy()
        .rsplit('-')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[cfg(target_os = "linux")]
pub(crate) fn assert_process_gone(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "owned Chrome {pid} survived sidecar cleanup"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn fixture_server() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><h1>Heading</h1>\
        <input id=name aria-label=Name><select id=pick><option value=a>A<option value=b>B</select>\
        <button id=go onclick=\"document.getElementById('status').textContent='clicked'\">Go</button>\
        <p id=status></p><a id=link href=/next>Next</a><iframe id=inner srcdoc='<p>inner</p>'></iframe>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..read]).to_string();
                let (kind, body) = if head.starts_with("GET /data") {
                    ("text/plain", "ok")
                } else {
                    ("text/html", PAGE)
                };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

#[cfg(target_os = "linux")]
pub(crate) fn process_running(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat
            .rsplit_once(") ")
            .is_some_and(|(_, state)| state.starts_with('Z')),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("cannot inspect process {pid}: {error}"),
    }
}

pub(crate) fn page_metrics(server: &mut Sidecar, id: u64, page: &str) -> Value {
    let response = server.request(
        id,
        "page.evaluate_raw",
        json!({
            "pageId":page,"js":"({width:innerWidth,height:innerHeight,scale:devicePixelRatio})"
        }),
    );
    assert!(response.get("result").is_some(), "{response}");
    response["result"]["result"].clone()
}

pub(crate) fn assert_png_dimensions(
    server: &mut Sidecar,
    id: u64,
    page: &str,
    expected: (u32, u32),
) {
    use base64::Engine;
    let response = server.request(id, "page.screenshot", json!({"pageId":page}));
    let png = base64::engine::general_purpose::STANDARD
        .decode(response["result"]["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(
        (
            u32::from_be_bytes(png[16..20].try_into().unwrap()),
            u32::from_be_bytes(png[20..24].try_into().unwrap()),
        ),
        expected,
    );
}

pub(crate) fn stalling_server() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><input id=name>\
        <button id=go>Go</button><p id=status></p>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                if String::from_utf8_lossy(&request[..read]).starts_with("GET /hang") {
                    std::thread::sleep(Duration::from_secs(600));
                    return;
                }
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{PAGE}",
                        PAGE.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

#[derive(Default)]
pub(crate) struct Inbox {
    pub(crate) held: std::collections::HashMap<u64, Value>,
}

impl Inbox {
    pub(crate) fn take(&mut self, server: &Sidecar, id: u64, within: Duration) -> Option<Value> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(reply) = self.held.remove(&id) {
                return Some(reply);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let reply = server.replies.recv_timeout(remaining).ok()?;
            self.held.insert(reply["id"].as_u64().unwrap(), reply);
        }
    }

    pub(crate) fn timed(
        &mut self,
        server: &mut Sidecar,
        id: u64,
        method: &str,
        params: Value,
    ) -> (Value, Duration) {
        let started = Instant::now();
        server.send(json!({"id":id,"method":method,"params":params}));
        let reply = self
            .take(server, id, Duration::from_secs(45))
            .unwrap_or_else(|| panic!("{method} did not answer"));
        (reply, started.elapsed())
    }
}

pub(crate) fn two_fixture_pages(
    server: &mut Sidecar,
    profile: &Path,
    origin: &str,
) -> (String, String) {
    launch(server, profile);
    let mut open = |id: u64| {
        let response = server.request(id, "browser.new_page", json!({"url":format!("{origin}/")}));
        response["result"]["pageId"]
            .as_str()
            .unwrap_or_else(|| panic!("{response}"))
            .to_owned()
    };
    (open(3), open(4))
}

pub(crate) fn server_wait(
    inbox: &mut Inbox,
    server: &mut Sidecar,
    id: u64,
    operation: &str,
) -> Value {
    inbox
        .timed(
            server,
            id,
            "operations.wait",
            json!({"id":operation,"timeout_ms":30_000}),
        )
        .0
}

pub(crate) const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

pub(crate) fn png(reply: &Value) -> Vec<u8> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            reply["result"]["dataBase64"]
                .as_str()
                .unwrap_or_else(|| panic!("{reply}")),
        )
        .unwrap();
    assert_eq!(&bytes[..8], PNG_SIGNATURE);
    bytes
}

pub(crate) fn visibility(server: &mut Sidecar, id: u64, page: &str) -> Value {
    let seen = server.request(
        id,
        "page.evaluate",
        json!({"pageId":page,"function":"()=>({now:document.visibilityState,changes:window.visibilityChanges})"}),
    );
    seen["result"]["result"].clone()
}

pub(crate) const QUICK: Duration = Duration::from_millis(2000);
pub(crate) const UNAFFECTED: Duration = Duration::from_millis(1000);

pub(crate) fn blocked_fixture() -> std::net::SocketAddr {
    const PAGE: &str = "<!doctype html><title>fixture</title><h1>Blocked tabs</h1>\
        <button id=danger onclick=\"window.clicks=(window.clicks||0)+1;alert('are you sure?');\
        document.getElementById('s').textContent='Deleted'\">Delete</button>\
        <button id=plain onclick=\"document.getElementById('s').textContent='Plain'\">Plain</button>\
        <p id=s></p>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                if String::from_utf8_lossy(&request[..read]).starts_with("GET /hang") {
                    std::thread::sleep(Duration::from_secs(600));
                    return;
                }
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{PAGE}",
                        PAGE.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

pub(crate) struct Blocked {
    pub(crate) server: Sidecar,
    pub(crate) inbox: Inbox,
    pub(crate) origin: String,
    pub(crate) next: u64,
    pub(crate) _root: TempDir,
}

impl Blocked {
    pub(crate) fn start(hello: Option<Value>) -> Self {
        let (root, state, scratch) = directories();
        let mut server = Sidecar::start(&state, &scratch);
        if let Some(hello) = hello {
            let greeting = server.request(900, "server.hello", hello);
            assert_eq!(
                greeting["result"]["protocol"],
                json!(PROTOCOL_VERSION),
                "{greeting}"
            );
            assert!(greeting["result"]["capabilities"]
                .as_array()
                .unwrap()
                .contains(&json!("pageBlockers")));
        }
        launch(&mut server, &root.path().join("profile"));
        Self {
            server,
            inbox: Inbox::default(),
            origin: format!("http://{}", blocked_fixture()),
            next: 1000,
            _root: root,
        }
    }

    pub(crate) fn current() -> Self {
        Self::start(Some(json!({"protocol":PROTOCOL_VERSION})))
    }

    pub(crate) fn serving(fixture: std::net::SocketAddr) -> Self {
        let mut tabs = Self::current();
        tabs.origin = format!("http://{fixture}");
        tabs
    }

    pub(crate) fn page_at(&mut self, path: &str) -> String {
        let url = format!("{}{path}", self.origin);
        self.ok("browser.new_page", json!({"url":url}))["pageId"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub(crate) fn admit(
        &mut self,
        page: &str,
        action: Value,
        expect: Option<Value>,
        limit: u64,
    ) -> String {
        let operation = uuid::Uuid::new_v4().to_string();
        let mut options = json!({"timeout_ms":limit});
        if let Some(expect) = expect {
            options["expect"] = expect;
        }
        let record = self.ok(
            "actions.perform",
            json!({"pageId":page,"operation_id":operation,"action":action,"options":options}),
        );
        assert_eq!(record["id"], operation, "{record}");
        operation
    }

    pub(crate) fn act(
        &mut self,
        page: &str,
        action: Value,
        expect: Option<Value>,
        limit: u64,
    ) -> (Value, Duration) {
        let started = Instant::now();
        let operation = self.admit(page, action, expect, limit);
        let record = self.ok(
            "operations.wait",
            json!({"id":operation,"timeout_ms":limit + 5000}),
        );
        (record, started.elapsed())
    }

    pub(crate) fn evaluate(&mut self, page: &str, function: &str) -> Value {
        self.ok("page.evaluate", json!({"pageId":page,"function":function}))["result"].clone()
    }

    pub(crate) fn painted(&mut self, page: &str) -> u64 {
        self.evaluate(page, "()=>window.painted").as_u64().unwrap()
    }

    pub(crate) fn painting(&mut self, page: &str) -> bool {
        let before = self.painted(page);
        std::thread::sleep(Duration::from_millis(500));
        self.painted(page) > before + 5
    }

    pub(crate) fn stops_painting(&mut self, page: &str) {
        let deadline = Instant::now() + Duration::from_secs(4);
        while self.painting(page) {
            assert!(
                Instant::now() < deadline,
                "the hidden tab kept running animation frames"
            );
        }
    }

    pub(crate) fn seen(&mut self, page: &str) -> Value {
        self.evaluate(
            page,
            "()=>({now:document.visibilityState,changes:window.changes})",
        )
    }

    pub(crate) fn clear(&mut self, pages: &[&str]) {
        for page in pages {
            self.ok(
                "page.execute",
                json!({"pageId":page,"function":"()=>{for(const id of ['outcome','twice','dragged'])document.getElementById(id).textContent='';window.scrollTo(0,0)}"}),
            );
        }
    }

    pub(crate) fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub(crate) fn call(&mut self, method: &str, params: Value) -> (Value, Duration) {
        let id = self.id();
        self.inbox.timed(&mut self.server, id, method, params)
    }

    pub(crate) fn ok(&mut self, method: &str, params: Value) -> Value {
        let (reply, _) = self.call(method, params);
        assert!(reply.get("result").is_some(), "{method}: {reply}");
        reply["result"].clone()
    }

    pub(crate) fn send(&mut self, method: &str, params: Value) -> u64 {
        let id = self.id();
        self.server
            .send(json!({"id":id,"method":method,"params":params}));
        id
    }

    pub(crate) fn page(&mut self) -> String {
        let url = format!("{}/", self.origin);
        self.ok("browser.new_page", json!({"url":url}))["pageId"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub(crate) fn open_alert(&mut self, page: &str) {
        self.ok(
            "page.execute",
            json!({"pageId":page,"function":"()=>{setTimeout(()=>alert('are you sure?'),0)}"}),
        );
        std::thread::sleep(Duration::from_millis(300));
    }

    pub(crate) fn stall_navigation(&mut self, page: &str) {
        self.ok(
            "page.execute",
            json!({"pageId":page,"function":"()=>{setTimeout(()=>{location.href='/hang'},0)}"}),
        );
    }

    pub(crate) fn title(&mut self, page: &str, within: Duration) {
        let (reply, elapsed) = self.call("info", json!({"pageId":page}));
        assert_eq!(reply["result"]["title"], "fixture", "{reply}");
        assert!(elapsed < within, "{elapsed:?}");
    }

    pub(crate) fn finish(mut self) {
        let id = self.id();
        self.server.shutdown(id);
    }
}

pub(crate) fn assert_dialog_open(reply: &Value, method: &str) {
    let error = &reply["error"];
    assert_eq!(error["code"], "DialogOpen", "{method}: {reply}");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("dialog is open"), "{message}");
    assert!(message.contains("are you sure?"), "{message}");
    assert_eq!(error["data"]["detail"]["code"], "dialog_open", "{reply}");
    assert!(
        error["data"]["detail"]["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("dialog.accept") && hint.contains("dialog.dismiss")),
        "{reply}"
    );
    let blocker = &error["data"]["blocker"];
    assert_eq!(blocker["kind"], "dialog", "{reply}");
    assert_eq!(blocker["dialogType"], "alert", "{reply}");
    assert_eq!(blocker["message"], "are you sure?", "{reply}");
    assert!(blocker["elapsedMs"].is_u64(), "{reply}");
}

pub(crate) fn assert_navigation_pending(reply: &Value) {
    let error = &reply["error"];
    assert_eq!(error["code"], "NavigationPending", "{reply}");
    assert!(
        error["message"].as_str().unwrap().contains("/hang"),
        "{reply}"
    );
    assert_eq!(
        error["data"]["detail"]["code"], "navigation_pending",
        "{reply}"
    );
    assert_eq!(error["data"]["blocker"]["kind"], "navigation", "{reply}");
    assert!(
        error["data"]["blocker"]["url"]
            .as_str()
            .is_some_and(|url| url.ends_with("/hang")),
        "{reply}"
    );
    assert!(error["data"]["blocker"]["elapsedMs"].is_u64(), "{reply}");
}

pub(crate) fn pointer_fixture() -> std::net::SocketAddr {
    const PAGE: &str = r#"<!doctype html><title>fixture</title>
<button id=go onclick="outcome.textContent='clicked'" ondblclick="twice.textContent='twice'">Go</button>
<button id=ask onclick="setTimeout(()=>confirm('Proceed?'),0)">Ask</button>
<div id=handle style="width:80px;height:40px;background:#888;user-select:none">drag</div>
<div id=late style="height:30px"></div>
<p id=outcome></p><p id=twice></p><p id=dragged></p>
<div style="height:3000px"></div><button id=far onclick="outcome.textContent='far'">Far</button>
<script>STILL
window.changes=[];document.addEventListener('visibilitychange',()=>window.changes.push(document.visibilityState));
window.painted=0;(function paint(){window.painted++;requestAnimationFrame(paint)})();
let from=null;handle.addEventListener('mousedown',event=>{from=event.clientX});
document.addEventListener('mouseup',event=>{if(from!==null&&Math.abs(event.clientX-from)>20)dragged.textContent='dragged';from=null});
window.later=after=>setTimeout(()=>{late.innerHTML='<button id=arrived onclick="outcome.textContent=`arrived`">Arrived</button>'},after);
</script>"#;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                let still = String::from_utf8_lossy(&request[..read]).starts_with("GET /still");
                let body = PAGE.replace(
                    "STILL",
                    if still {
                        "window.requestAnimationFrame=()=>0;"
                    } else {
                        ""
                    },
                );
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}

pub(crate) fn css(selector: &str) -> Value {
    json!({"query":{"kind":"css","selector":selector}})
}

pub(crate) fn shows(selector: &str, value: &str) -> Value {
    json!({"kind":"text","target":css(selector),"value":value})
}

pub(crate) fn behind_and_front(tabs: &mut Blocked, path: &str) -> (String, String) {
    let (behind, front) = (tabs.page_at(path), tabs.page_at(path));
    std::thread::sleep(Duration::from_secs(3));
    for page in [&behind, &front] {
        tabs.ok(
            "page.execute",
            json!({"pageId":page,"function":"()=>{window.changes=[]}"}),
        );
    }
    assert_eq!(tabs.seen(&behind)["now"], "hidden");
    assert_eq!(tabs.seen(&front)["now"], "visible");
    (behind, front)
}

pub(crate) fn assert_stayed_behind(tabs: &mut Blocked, behind: &str, front: &str) {
    assert_eq!(tabs.seen(behind), json!({"now":"hidden","changes":[]}));
    assert_eq!(tabs.seen(front), json!({"now":"visible","changes":[]}));
}

pub(crate) fn masked(value: &Value, page: &str, root: &str, origin: &str) -> Value {
    match value {
        Value::String(text) => {
            let text = text.split("\n    at ").next().unwrap_or_default();
            let text = text
                .replace(page, "<page>")
                .replace(root, "<root>")
                .replace(origin, "<origin>");
            let hex = text.len() == 32 && text.chars().all(|c| c.is_ascii_hexdigit());
            if hex {
                json!("<id>")
            } else if text.len() > 256 && !text.contains(' ') {
                json!("<bytes>")
            } else {
                json!(text)
            }
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| masked(item, page, root, origin))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let volatile = key.ends_with("Ms")
                        || key.ends_with("_ms")
                        || matches!(
                            key.as_str(),
                            "bytes"
                                | "sha256"
                                | "size"
                                | "width"
                                | "height"
                                | "loaderId"
                                | "id"
                                | "userAgent"
                        );
                    let item = if volatile && (item.is_number() || item.is_string()) {
                        json!("<volatile>")
                    } else {
                        masked(item, page, root, origin)
                    };
                    (key.clone(), item)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(crate) fn leave_fixture() -> std::net::SocketAddr {
    const LEAVE: &str = "<!doctype html><title>leave</title><textarea id=note></textarea>\
        <a id=summary href=/summary>Summary</a><script>note.addEventListener('input',()=>{\
        window.onbeforeunload=e=>{e.preventDefault();e.returnValue=''}})</script>";
    const SUMMARY: &str = "<!doctype html><title>summary</title><h1>Summary</h1>";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = [0u8; 4096];
                let read = std::io::Read::read(&mut stream, &mut request).unwrap_or(0);
                let page = if String::from_utf8_lossy(&request[..read]).starts_with("GET /summary")
                {
                    SUMMARY
                } else {
                    LEAVE
                };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
                        page.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    address
}
