#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variable {
    pub name: &'static str,
    pub read_by: &'static [&'static str],
    pub description: &'static str,
}

macro_rules! variables {
    ($($constant:ident = $name:literal, [$($reader:literal),+], $description:literal;)*) => {
        $(pub const $constant: &str = $name;)*

        pub const ALL: &[Variable] = &[$(Variable {
            name: $constant,
            read_by: &[$($reader),+],
            description: $description,
        }),*];
    };
}

variables! {
    CDP = "EOKA_CDP", ["eoka-cli"], "Connect to Chrome by DevTools port or WebSocket URL (`--cdp`)";
    AUTO_CONNECT = "EOKA_AUTO_CONNECT", ["eoka-cli"], "Auto-discover a running Chrome on ports 9222-9229 (`--auto-connect`)";
    FROM_PROFILE = "EOKA_FROM_PROFILE", ["eoka-cli"], "Launch with a copy of an existing Chrome profile directory, `auto` or a path (`--from-profile`)";
    NO_STEALTH = "EOKA_NO_STEALTH", ["eoka-cli"], "Disable stealth CDP filtering and evasion script injection (`--no-stealth`)";
    PERSIST = "EOKA_PERSIST", ["eoka-cli"], "Keep a durable Chrome profile for the session across restarts (`--persist`)";
    NO_GEO_ALIGN = "EOKA_NO_GEO_ALIGN", ["eoka-cli"], "Disable IP-geolocation timezone and language alignment (`--no-geo-align`)";
    NO_SANDBOX = "EOKA_NO_SANDBOX", ["eoka-cli"], "Disable Chrome's sandbox, only for containers and CI that cannot run it (`--no-sandbox`)";
    NO_JS = "EOKA_NO_JS", ["eoka-cli"], "Start with JavaScript blocked by default (`--no-js`)";
    AUTO_SOLVE_CLOUDFLARE = "EOKA_AUTO_SOLVE_CLOUDFLARE", ["eoka-cli", "eoka-mcp"], "Solve full-page Cloudflare challenges in every tab (`--auto-solve-cloudflare`; the MCP server takes `1` or `true`)";
    PROXY = "EOKA_PROXY", ["eoka-cli", "eoka-mcp"], "Proxy URL for a launched browser (`--proxy`)";
    PROXY_FILE = "EOKA_PROXY_FILE", ["eoka-cli", "eoka-mcp"], "File of proxy URLs; each launch picks one of its non-empty, non-`#` lines (`--proxy-file`). The MCP server prefers it over `EOKA_PROXY`";
    IDLE_TIMEOUT = "EOKA_IDLE_TIMEOUT", ["eoka-cli"], "Milliseconds a session daemon with no clients stays up (default 1800000)";
    QUEUE_WAIT_MS = "EOKA_QUEUE_WAIT_MS", ["eoka-cli"], "Milliseconds a command waits for a busy session before `daemon_busy` (default 5000; read when the daemon starts)";
    REPLY_TIMEOUT_MS = "EOKA_REPLY_TIMEOUT_MS", ["eoka-sdk"], "Reply deadline in milliseconds for daemon commands without their own timeout (CLI and Rust SDK)";
    BIN = "EOKA_BIN", ["eoka-sdk"], "`eoka` binary the Rust SDK starts as the session daemon when the host program is not the CLI (default `eoka` on `PATH`)";
    MCP_ALL_TOOLS = "EOKA_MCP_ALL_TOOLS", ["eoka-mcp"], "List the catalog's opt-in tools too (any value but `0` or `false`)";
    HEADLESS = "EOKA_HEADLESS", ["eoka-mcp"], "Launch the MCP browser headless (default); `0` or `false` launches it headed";
    PROFILE_DIR = "EOKA_PROFILE_DIR", ["eoka-protocol"], "Base directory for `--persist` profiles (default `$XDG_STATE_HOME/eoka/profiles`)";
    STATE_DIR = "EOKA_STATE_DIR", ["eoka-server"], "Directory for operation and download history (default `$XDG_STATE_HOME/eoka` or the platform's user state directory)";
    ALLOW_FILE_URLS = "EOKA_ALLOW_FILE_URLS", ["eoka-server"], "Allow `file:` URLs (`1` or `true`); the CLI session daemon reads it when it starts";
    PATCH_BINARY = "EOKA_PATCH_BINARY", ["eoka-server"], "Patch the Chrome binary when the launch does not decide (`1` or `true`; default off)";
    CHROME_ARGS = "EOKA_CHROME_ARGS", ["eoka-server"], "Extra Chrome command-line arguments, separated by `:`";
}

#[cfg(test)]
mod tests;
