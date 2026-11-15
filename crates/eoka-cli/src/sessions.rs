use crate::{client, launch_spec::LaunchSpec, output, protocol, session};
use serde_json::json;

pub(crate) async fn print_sessions(json_mode: bool) {
    let mut rows = Vec::new();
    for name in session::list_sessions() {
        let hello =
            client::send_command(&name, protocol::Request::SessionHello, LaunchSpec::Reuse).await;
        let details = hello
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| r.data)
            .unwrap_or_default();
        rows.push(json!({"session":name,"running":client::is_daemon_running(&name),"socket":session::socket_path(&name).to_string_lossy(),"mode":details["mode"],"daemon_version":details["daemon_version"],"protocol_major":details["protocol_major"],"legacy_name":name.ends_with("-headed") || name.ends_with("-live")}));
    }
    if json_mode {
        output::print_response(&protocol::Response::ok(json!({"sessions":rows})), true);
    } else if rows.is_empty() {
        println!("No sessions found");
    } else {
        println!("SESSION  MODE  VERSION  RUNNING");
        for row in rows {
            println!(
                "{}  {}  {}  {}",
                row["session"].as_str().unwrap_or_default(),
                row["mode"].as_str().unwrap_or("unknown"),
                row["daemon_version"].as_str().unwrap_or("unknown"),
                row["running"]
            );
        }
    }
}

pub(crate) async fn status(session_name: &str) -> serde_json::Value {
    let running = client::is_daemon_running(session_name);
    let hello = if running {
        client::session_status(session_name).await
    } else {
        None
    };
    let activity = hello.as_ref().map(|hello| &hello["activity"]);
    json!({
        "running": running,
        "responsive": hello.is_some(),
        "session": session_name,
        "socket": session::socket_path(session_name).to_string_lossy(),
        "log": session::socket_path(session_name).with_extension("log").to_string_lossy(),
        "busy": activity.map(|activity| activity["busy"].clone()),
        "queued": activity.map(|activity| activity["queued"].clone()),
        "blocker": activity.map(|activity| activity["blocker"].clone()),
    })
}

pub(crate) fn status_text(status: &serde_json::Value) -> String {
    use std::fmt::Write;
    let session = status["session"].as_str().unwrap_or_default();
    if !status["running"].as_bool().unwrap_or(false) {
        return format!("No daemon running (session={session})\n");
    }
    let mut text = format!(
        "Daemon running (session={session})\nSocket: {}\n",
        status["socket"].as_str().unwrap_or_default()
    );
    if !status["responsive"].as_bool().unwrap_or(false) {
        let _ = writeln!(
            text,
            "State: not answering; the daemon process is suspended or overloaded. Log: {}",
            status["log"].as_str().unwrap_or_default()
        );
        return text;
    }
    let queued = status["queued"].as_u64().unwrap_or_default();
    match status["busy"]["cmd"].as_str() {
        Some(command) => {
            let _ = writeln!(
                text,
                "State: busy; `{command}` has been running for {:.1} s, {queued} queued",
                status["busy"]["for_ms"].as_u64().unwrap_or_default() as f64 / 1000.0
            );
        }
        None => text.push_str("State: idle\n"),
    }
    if let Some(description) = status["blocker"]["description"].as_str() {
        let _ = writeln!(text, "Blocked: {description}.");
        if let Some(hint) = status["blocker"]["hint"].as_str() {
            let _ = writeln!(text, "Hint: {hint}");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_text_names_the_running_command_and_the_blocker() {
        let idle = json!({"running":true,"responsive":true,"session":"work","socket":"/run/eoka-work.sock","busy":null,"queued":0,"blocker":null});
        assert!(status_text(&idle).contains("State: idle"));
        let busy = json!({"running":true,"responsive":true,"session":"work","socket":"/run/eoka-work.sock",
            "busy":{"cmd":"eval","for_ms":12300},"queued":2,
            "blocker":{"kind":"dialog","description":"A JavaScript alert dialog is open on this tab (\"hi\")","hint":"Run `eoka --session work dialog accept`"}});
        let text = status_text(&busy);
        assert!(text.contains("State: busy; `eval` has been running for 12.3 s, 2 queued"));
        assert!(text.contains("Blocked: A JavaScript alert dialog is open"));
        assert!(text.contains("Hint: Run `eoka --session work dialog accept`"));
        let silent = json!({"running":true,"responsive":false,"session":"work","socket":"s","log":"/run/eoka-work.log"});
        assert!(status_text(&silent).contains("State: not answering"));
        assert!(
            status_text(&json!({"running":false,"session":"work"})).contains("No daemon running")
        );
    }
}
