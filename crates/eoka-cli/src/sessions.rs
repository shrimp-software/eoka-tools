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
