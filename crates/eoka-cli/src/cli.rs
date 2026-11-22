use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const DEADLINES: &str = "Deadlines:\n  Every command returns within a bounded time and never replays an action.\n  A failure carries error_detail.code and a hint naming the next command:\n    dialog_open          a JavaScript dialog blocks the tab: dialog accept | dialog dismiss\n    navigation_pending   the tab waits for a server: wait --url, or reload | open URL to abandon it\n    navigation_timeout   no response before the deadline; the load was stopped and the tab kept its page\n    daemon_busy          another command is running; this one was not started (EOKA_QUEUE_WAIT_MS, default 5000)\n    page_unresponsive    the page's main thread is frozen: retry, or tab new URL\n    reply_timeout        the daemon did not reply in time (EOKA_REPLY_TIMEOUT_MS overrides the default)\n    daemon_unresponsive  the daemon did not answer its handshake within 5 s\n  Page reads answer within 15 s, view within --timeout-ms (3000), open and actions\n  within --action-timeout-ms (30000). status shows what a session is running.";

#[derive(Clone, Parser)]
#[command(
    name = "eoka",
    about = "Browser automation CLI",
    version,
    after_long_help = DEADLINES
)]
pub struct Cli {
    #[arg(long, global = true, help = "Expected action result as typed JSON")]
    pub expect: Option<String>,
    #[arg(long, global = true, help = "Stable UUID for a consequential action")]
    pub operation_id: Option<String>,
    #[arg(long, global = true, help = "Total action deadline in milliseconds")]
    pub action_timeout_ms: Option<u64>,
    #[arg(
        long,
        default_value = "default",
        global = true,
        help = "Session name for isolated browser instances"
    )]
    pub session: String,
    #[arg(long, global = true, help = "JSON output mode")]
    pub json: bool,
    #[arg(
        long,
        global = true,
        help = "JSON output plus structured metadata for agent callers"
    )]
    pub agent: bool,
    #[arg(
        long,
        global = true,
        help = "Create a headed browser or require an existing headed session"
    )]
    pub headed: bool,
    #[arg(
        long,
        global = true,
        conflicts_with = "headed",
        help = "Create a headless browser or require an existing headless session"
    )]
    pub headless: bool,
    #[arg(
        long,
        value_name = "PORT|URL",
        global = true,
        env = "EOKA_CDP",
        help = "Connect to Chrome by DevTools port or WebSocket URL"
    )]
    pub cdp: Option<String>,
    #[arg(
        long,
        global = true,
        env = "EOKA_AUTO_CONNECT",
        help = "Auto-discover a running Chrome on ports 9222-9229"
    )]
    pub auto_connect: bool,
    #[arg(
        long,
        value_name = "PORT|URL",
        global = true,
        help = "Launch fresh Chrome and pre-load cookies/storage from a running Chrome"
    )]
    pub clone_state_from: Option<String>,
    #[arg(
        long,
        value_name = "auto|PATH",
        global = true,
        env = "EOKA_FROM_PROFILE",
        help = "Launch with a copy of an existing Chrome profile directory"
    )]
    pub from_profile: Option<String>,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_STEALTH",
        help = "Disable stealth CDP filtering and evasion script injection"
    )]
    pub no_stealth: bool,
    #[arg(
        long,
        global = true,
        env = "EOKA_PERSIST",
        help = "Keep a durable Chrome profile for this session across restarts"
    )]
    pub persist: bool,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_GEO_ALIGN",
        help = "Disable IP-geolocation timezone/language alignment (on by default)"
    )]
    pub no_geo_align: bool,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_SANDBOX",
        help = "Disable Chrome's sandbox (only for containers/CI that cannot run it)"
    )]
    pub no_sandbox: bool,
    #[arg(
        long,
        global = true,
        env = "EOKA_AUTO_SOLVE_CLOUDFLARE",
        help = "Solve full-page Cloudflare challenges in every tab automatically"
    )]
    pub auto_solve_cloudflare: bool,
    #[arg(
        long,
        global = true,
        value_name = "URL",
        env = "EOKA_PROXY",
        conflicts_with = "proxy_file",
        help = "Proxy URL for launched browser"
    )]
    pub proxy: Option<String>,
    #[arg(long, hide = true, requires = "daemon")]
    pub proxy_stdin: bool,
    #[arg(
        long,
        global = true,
        value_name = "FILE",
        env = "EOKA_PROXY_FILE",
        conflicts_with = "proxy",
        help = "Read proxy URL from a file"
    )]
    pub proxy_file: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        env = "EOKA_NO_JS",
        help = "Start with JavaScript blocked by default"
    )]
    pub no_js: bool,
    #[arg(
        long,
        global = true,
        value_name = "DOMAIN",
        help = "Domain to always run JavaScript on"
    )]
    pub js_allow: Vec<String>,
    #[arg(
        long,
        global = true,
        value_name = "DOMAIN",
        help = "Domain to always block JavaScript on"
    )]
    pub js_block: Vec<String>,
    #[arg(long, hide = true)]
    pub daemon: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

mod actions;
mod command;

pub use actions::*;
pub use command::Command;

#[cfg(test)]
pub(crate) mod test_support {
    use super::{Cli, Command};
    use clap::Parser;

    pub(crate) fn parsed_command(args: &[&str]) -> (Cli, Command) {
        let mut cli = Cli::try_parse_from(args).unwrap();
        let command = cli.command.take().unwrap();
        (cli, command)
    }
}
