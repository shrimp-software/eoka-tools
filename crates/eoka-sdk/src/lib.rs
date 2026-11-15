pub mod session;

mod client;

pub use client::{
    is_daemon_running, kill_daemon, send_command, serve_daemons_from, session_status, EokaClient,
};
pub use eoka_protocol::*;
