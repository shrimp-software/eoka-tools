#![allow(dead_code)]

use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INHERITED: [&str; 10] = [
    "EOKA_CDP",
    "EOKA_AUTO_CONNECT",
    "EOKA_FROM_PROFILE",
    "EOKA_PROXY",
    "EOKA_PROXY_FILE",
    "EOKA_PERSIST",
    "EOKA_NO_JS",
    "EOKA_NO_STEALTH",
    "EOKA_IDLE_TIMEOUT",
    "EOKA_REPLY_TIMEOUT_MS",
];

pub fn eoka() -> Command {
    isolated(OsStr::new(env!("CARGO_BIN_EXE_eoka")))
}

fn isolated(binary: &OsStr) -> Command {
    let mut command = Command::new(binary);
    for name in INHERITED {
        command.env_remove(name);
    }
    command
}

pub struct Session {
    pub name: String,
    pub directory: tempfile::TempDir,
    binary: OsString,
    flags: Vec<String>,
    env: Vec<(&'static str, String)>,
}

impl Session {
    pub fn new(prefix: &str) -> Self {
        Self {
            name: format!("{prefix}-{}", uuid::Uuid::new_v4()),
            directory: tempfile::tempdir().unwrap(),
            binary: env!("CARGO_BIN_EXE_eoka").into(),
            flags: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn agent(prefix: &str) -> Self {
        Self::new(prefix)
            .flags(&["--agent"])
            .env("EOKA_IDLE_TIMEOUT", 60000)
    }

    pub fn binary(mut self, binary: OsString) -> Self {
        self.binary = binary;
        self
    }

    pub fn flags(mut self, flags: &[&str]) -> Self {
        self.flags.extend(flags.iter().map(|flag| flag.to_string()));
        self
    }

    pub fn env(mut self, name: &'static str, value: impl ToString) -> Self {
        self.env.push((name, value.to_string()));
        self
    }

    pub fn state_dir(&self) -> PathBuf {
        self.directory.path().join("state")
    }

    fn launch(&self, flags: &[String], args: &[&str]) -> Command {
        let mut command = isolated(&self.binary);
        command
            .env("EOKA_STATE_DIR", self.state_dir())
            .envs(self.env.iter().map(|(name, value)| (name, value)))
            .current_dir(self.directory.path())
            .args(flags)
            .args(["--session", &self.name])
            .args(args);
        command
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = self.launch(&self.flags, args);
        if args.contains(&"open") && !self.flags.iter().any(|flag| flag == "--no-geo-align") {
            command.arg("--no-geo-align");
        }
        command
    }

    pub fn tokio_command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::from(self.command(args));
        command.kill_on_drop(true);
        command
    }

    pub fn output(&self, args: &[&str]) -> (Output, Value, Duration) {
        let started = Instant::now();
        let output = self.command(args).output().unwrap();
        let elapsed = started.elapsed();
        let response = parse(args, &output);
        (output, response, elapsed)
    }

    pub fn run(&self, args: &[&str]) -> Value {
        let (output, response, _) = self.output(args);
        assert!(output.status.success(), "{args:?}: {response}");
        assert_eq!(response["ok"], true, "{args:?}: {response}");
        response["data"].clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.launch(&[], &["close"]).output();
        let _ = self.launch(&[], &["kill"]).output();
    }
}

pub fn parse(args: &[&str], output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?} printed no JSON ({error}): stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub struct Reply {
    pub status: &'static str,
    pub content_type: &'static str,
    pub body: &'static str,
}

impl Reply {
    pub fn html(body: &'static str) -> Option<Self> {
        Some(Self {
            status: "200 OK",
            content_type: "text/html",
            body,
        })
    }

    pub fn not_found() -> Option<Self> {
        Some(Self {
            status: "404 Not Found",
            content_type: "text/plain",
            body: "not found",
        })
    }
}

pub struct Site {
    origin: String,
}

impl Site {
    pub fn start(route: fn(&str) -> Option<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let silent: Arc<Mutex<Vec<TcpStream>>> = Default::default();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let silent = silent.clone();
                std::thread::spawn(move || answer(stream, route, &silent));
            }
        });
        Self { origin }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }
}

fn answer(mut stream: TcpStream, route: fn(&str) -> Option<Reply>, silent: &Mutex<Vec<TcpStream>>) {
    let mut request = String::new();
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    if BufReader::new(reader).read_line(&mut request).is_err() {
        return;
    }
    let Some(reply) = route(request.split_whitespace().nth(1).unwrap_or("/")) else {
        silent.lock().unwrap().push(stream);
        return;
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.content_type,
        reply.body.len(),
        reply.body
    );
}
