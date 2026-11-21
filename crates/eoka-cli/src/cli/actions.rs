use super::*;

#[derive(Clone, Subcommand)]
pub enum NetworkAction {
    #[command(
        about = "Start, stop or inspect network recording",
        long_about = "Start, stop or inspect network recording.\n\nBodies are captured unredacted by default; treat logs and exports as sensitive."
    )]
    Record {
        #[command(subcommand)]
        action: NetworkRecordAction,
    },
    #[command(about = "List recorded requests")]
    Log {
        #[arg(long, help = "Maximum entries to return")]
        limit: Option<usize>,
        #[arg(long, help = "URL glob such as */api/*")]
        pattern: Option<String>,
        #[arg(long, help = "Only this HTTP method")]
        method: Option<String>,
        #[arg(long, help = "Only this response status")]
        status: Option<u16>,
        #[arg(long, help = "Only entries after this request ID")]
        since: Option<u64>,
        #[arg(long, help = "One line per request")]
        compact: bool,
    },
    #[command(about = "Show one recorded request")]
    Show {
        #[arg(help = "Request ID from network log")]
        id: u64,
        #[arg(long, help = "Include the response body")]
        body: bool,
        #[arg(long, help = "Maximum body bytes to print")]
        max_body: Option<usize>,
    },
    #[command(about = "Write recorded requests as a HAR file")]
    Har {
        #[arg(help = "HAR file to write")]
        path: PathBuf,
        #[arg(long, help = "Wait this long for in-flight requests before writing")]
        settle_ms: Option<u64>,
    },
    #[command(about = "Export recorded requests as HAR or JSON")]
    Export {
        #[arg(help = "File to write")]
        path: PathBuf,
        #[arg(long, default_value = "har", help = "har or json")]
        format: String,
        #[arg(long, help = "Wait this long for in-flight requests before writing")]
        settle_ms: Option<u64>,
    },
    #[command(about = "Wait for a matching request")]
    Wait {
        #[arg(long, help = "URL glob such as */api/*")]
        pattern: Option<String>,
        #[arg(long, help = "Only this HTTP method")]
        method: Option<String>,
        #[arg(long, help = "Only this response status")]
        status: Option<u16>,
        #[arg(long, help = "Timeout in milliseconds")]
        timeout: Option<u64>,
        #[arg(long, help = "Only match requests after this request ID")]
        since: Option<u64>,
        #[arg(long, help = "Also match requests recorded before the wait started")]
        include_existing: bool,
    },
    #[command(about = "Discard recorded requests")]
    Clear,
    #[command(
        about = "Capture or mock requests matching a URL pattern",
        long_about = "Capture or mock requests matching a URL pattern.\n\nThe log keeps the latest 1000 entries. Response and capture files must be regular files of at most 8 MiB; FIFOs and devices are rejected, and each file operation has a 500 ms deadline. A failed capture, read or fulfillment invalidates DataDome clearance. Requests that may already have been dispatched are never replayed."
    )]
    Intercept {
        #[command(subcommand)]
        action: InterceptAction,
    },
}

#[derive(Clone, Subcommand)]
pub enum ToolsAction {
    #[command(about = "Print the tool manifest generated from eoka-protocol")]
    Manifest {
        #[arg(long, help = "Include tools hidden from the default agent exposure")]
        all: bool,
        #[arg(long, help = "Print raw {\"tools\": [...]} JSON")]
        json: bool,
    },
}

#[derive(Clone, Subcommand)]
pub enum NetworkRecordAction {
    #[command(about = "Start recording")]
    Start {
        #[arg(long = "pattern", help = "URL glob to record; repeat for several")]
        patterns: Vec<String>,
        #[arg(long, help = "Record metadata only")]
        no_bodies: bool,
        #[arg(long, default_value = "10485760", help = "Per-response body cap")]
        max_body_bytes: usize,
        #[arg(long, help = "Discard earlier entries first")]
        clear: bool,
    },
    #[command(about = "Stop recording")]
    Stop,
    #[command(about = "Show recording state")]
    Status,
}

#[derive(Clone, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum CaptchaAction {
    #[command(about = "Detect an hCaptcha or reCAPTCHA widget and report its type and site key")]
    Detect,
    #[command(
        about = "Attempt local DataDome clearance on the current tab; keep the session open",
        long_about = "Attempt local DataDome clearance on the current tab. No API key needed.\n\nRequires an existing session; foregrounds the tab and leaves it open. challenge_cleared is not proof of login; not_present is a no-op; every other outcome exits nonzero."
    )]
    Datadome {
        #[arg(long, default_value_t = eoka_protocol::DATADOME_DEFAULT_ATTEMPTS, value_parser = clap::value_parser!(u32).range(1..=i64::from(eoka_protocol::DATADOME_MAX_ATTEMPTS)), help = "Clearance attempts, 1 through 4")]
        max_attempts: u32,
        #[arg(long, default_value_t = eoka_protocol::DATADOME_DEFAULT_TIMEOUT_MS, value_parser = clap::value_parser!(u64).range(eoka_protocol::DATADOME_MIN_TIMEOUT_MS..=eoka_protocol::DATADOME_MAX_TIMEOUT_MS), help = "Total budget, 5000 through 120000 ms")]
        timeout_ms: u64,
    },
    #[command(
        about = "Click the Cloudflare Turnstile checkbox on the current tab until the challenge clears",
        long_about = "Click the Cloudflare Turnstile checkbox on the current tab until it leaves the challenge page.\n\nReports clicks and elapsed_ms; a normal page returns zero clicks. --auto-solve-cloudflare does this for every tab of a browser eoka launches."
    )]
    Turnstile {
        #[arg(long, default_value_t = eoka_protocol::TURNSTILE_DEFAULT_TIMEOUT_MS, value_parser = clap::value_parser!(u64).range(1..=eoka_protocol::TURNSTILE_MAX_TIMEOUT_MS), help = "Total budget, at most 120000 ms")]
        timeout_ms: u64,
    },
    #[command(about = "Solve a CAPTCHA through the Anti-Captcha provider")]
    Solve(Box<SolveArgs>),
    #[command(about = "Inject a solved token into the current page")]
    Inject {
        #[arg(help = "Token returned by captcha solve")]
        token: String,
        #[arg(long, default_value = "auto", help = "auto, recaptcha or hcaptcha")]
        captcha_type: String,
        #[arg(long, help = "Page callback to call with the token")]
        callback: Option<String>,
        #[arg(long, help = "Target to click after injecting")]
        click_after: Option<String>,
    },
}

#[derive(Clone, Args)]
pub struct SolveArgs {
    #[arg(
        long,
        help = "recaptcha_v2, recaptcha_v2_enterprise, recaptcha_v3, hcaptcha or amazon_waf"
    )]
    pub captcha_type: String,
    #[arg(long, help = "URL of the page showing the CAPTCHA")]
    pub website_url: String,
    #[arg(
        long,
        help = "Site key from the page (amazon_waf: key from window.gokuProps)"
    )]
    pub website_key: String,
    #[arg(long, env = "ANTI_CAPTCHA_KEY", help = "Anti-Captcha API key")]
    pub api_key: Option<String>,
    #[arg(long, help = "recaptcha_v3 page action [default: submit]")]
    pub page_action: Option<String>,
    #[arg(long, help = "recaptcha_v3 minimum score [default: 0.3]")]
    pub min_score: Option<f32>,
    #[arg(
        long,
        help = "recaptcha_v2_enterprise extra render parameters as JSON, such as {\"s\": ...}"
    )]
    pub enterprise_payload: Option<String>,
    #[arg(
        long,
        help = "recaptcha_v2_enterprise widget domain, such as www.google.com"
    )]
    pub api_domain: Option<String>,
    #[arg(long, help = "amazon_waf iv from window.gokuProps (required)")]
    pub iv: Option<String>,
    #[arg(long, help = "amazon_waf context from window.gokuProps (required)")]
    pub context: Option<String>,
    #[arg(long, help = "amazon_waf captcha script URL")]
    pub captcha_script: Option<String>,
    #[arg(long, help = "amazon_waf challenge script URL")]
    pub challenge_script: Option<String>,
    #[arg(long, help = "Inject the token into the current page after solving")]
    pub inject: bool,
    #[arg(long, help = "Page callback to call with the injected token")]
    pub inject_callback: Option<String>,
    #[arg(long, help = "Target to click after injecting")]
    pub click_after: Option<String>,
}

#[derive(Clone, Subcommand)]
pub enum WasmAction {
    #[command(about = "Read LEN bytes at ADDR as hex")]
    Read {
        #[arg(help = ADDR)]
        addr: String,
        #[arg(help = "Number of bytes")]
        len: usize,
        #[arg(long, help = MEMORY)]
        memory: Option<String>,
    },
    #[command(about = "Write hex bytes at ADDR")]
    Write {
        #[arg(help = ADDR)]
        addr: String,
        #[arg(help = "Bytes as hex, such as deadbeef")]
        hex: String,
        #[arg(long, help = MEMORY)]
        memory: Option<String>,
    },
    #[command(about = "Search memory for a hex byte pattern")]
    Find {
        #[arg(help = "Hex bytes to find; spaces allowed, such as \"ff d8 ff e0\"")]
        pattern: String,
        #[arg(long, help = "Start address [default: 0]")]
        start: Option<String>,
        #[arg(long, help = "End address [default: end of memory]")]
        end: Option<String>,
        #[arg(long, default_value = "20", help = "Maximum matches")]
        max: usize,
        #[arg(long, help = MEMORY)]
        memory: Option<String>,
    },
    #[command(about = "List WASM memories and their sizes")]
    Info,
}

#[derive(Clone, Subcommand)]
pub enum InterceptAction {
    #[command(about = "Add an interception rule")]
    Add {
        #[arg(help = "URL glob such as */api/data*")]
        url_pattern: String,
        #[arg(long, help = "Write matching requests to this file")]
        capture: Option<PathBuf>,
        #[arg(long, help = "Serve this file's exact bytes as the response")]
        respond: Option<PathBuf>,
        #[arg(long, default_value = "200", help = "Status code for --respond")]
        status: u16,
    },
    #[command(about = "List interception rules")]
    List,
    #[command(about = "Remove an interception rule")]
    Remove {
        #[arg(help = "Rule ID from intercept list")]
        id: String,
    },
    #[command(about = "Read the interception log (latest 1000 entries)")]
    Log {
        #[arg(long, help = "Clear the log after reading")]
        clear: bool,
    },
}

#[derive(Clone, Subcommand)]
pub enum JsAction {
    #[command(about = "Set the default policy")]
    Mode {
        #[arg(help = "block-all or allow-all")]
        mode: String,
    },
    #[command(about = "Always run JavaScript on a domain")]
    Allow {
        #[arg(help = "Domain such as example.com")]
        domain: String,
    },
    #[command(about = "Always block JavaScript on a domain")]
    Block {
        #[arg(help = "Domain such as example.com")]
        domain: String,
    },
    #[command(about = "Remove a domain rule")]
    Remove {
        #[arg(help = "Domain such as example.com")]
        domain: String,
    },
    #[command(about = "List the policy")]
    List,
}

#[derive(Clone, Subcommand)]
pub enum TabAction {
    #[command(about = "List tabs")]
    List,
    #[command(about = "Open a tab, optionally at a URL")]
    New {
        #[arg(help = "URL to open [default: blank page]")]
        url: Option<String>,
    },
    #[command(about = "Make a tab current")]
    Switch {
        #[arg(help = TAB_ID)]
        tab_id: String,
    },
    #[command(about = "Close a tab")]
    Close {
        #[arg(help = TAB_ID)]
        tab_id: String,
    },
    #[command(about = "Adopt a tab eoka did not open")]
    Attach {
        #[arg(help = TAB_ID)]
        tab_id: String,
    },
}

#[derive(Clone, Subcommand)]
pub enum OperationCommand {
    #[command(about = "Show one operation record")]
    Get {
        #[arg(help = OPERATION_ID)]
        id: String,
    },
    #[command(about = "List recent operation records")]
    List {
        #[arg(long, default_value_t = 100, help = "Maximum records to return")]
        limit: u32,
    },
    #[command(about = "Wait for an operation to finish")]
    Wait {
        #[arg(help = OPERATION_ID)]
        id: String,
        #[arg(
            long,
            default_value_t = 30000,
            help = "How long to wait; the operation keeps running after it"
        )]
        timeout_ms: u64,
    },
    #[command(about = "Cancel a running operation")]
    Cancel {
        #[arg(help = OPERATION_ID)]
        id: String,
    },
}

#[derive(Clone, Subcommand)]
pub enum DialogAction {
    #[command(about = "Accept the open dialog")]
    Accept {
        #[arg(long, help = "Text to enter for a prompt() dialog")]
        prompt_text: Option<String>,
    },
    #[command(about = "Dismiss the open dialog")]
    Dismiss,
}

#[derive(Clone, Subcommand)]
pub enum RecordingAction {
    #[command(about = "Start recording the current tab; prints the recording ID")]
    Start {
        #[arg(help = "MP4 file to write")]
        path: PathBuf,
        #[arg(long, help = "UUID naming this recording [default: generated]")]
        id: Option<String>,
        #[arg(long, help = "Frames per second, 1 to 120")]
        fps: Option<u32>,
        #[arg(long, help = "Scale frames down to at most this width")]
        max_width: Option<u32>,
        #[arg(long, help = "Scale frames down to at most this height")]
        max_height: Option<u32>,
    },
    #[command(about = "Show a recording's state")]
    Status {
        #[arg(help = "Recording ID from start")]
        id: String,
    },
    #[command(about = "Stop a recording and finalize its file")]
    Stop {
        #[arg(help = "Recording ID from start")]
        id: String,
    },
    #[command(about = "Cancel a recording and discard its file")]
    Cancel {
        #[arg(help = "Recording ID from start")]
        id: String,
    },
}

#[derive(Clone, Args)]
pub struct UpdateArgs {
    #[arg(
        long,
        conflicts_with_all = ["force", "kill_daemons"],
        help = "Report the installed and latest versions without installing"
    )]
    pub check: bool,
    #[arg(long, help = "Reinstall even when already on the latest release")]
    pub force: bool,
    #[arg(
        long,
        help = "Stop session daemons still running another version (closes their browsers)"
    )]
    pub kill_daemons: bool,
    #[arg(long, help = "Leave installed copies of the agent skill as they are")]
    pub keep_skill: bool,
}

#[derive(Clone, Subcommand)]
pub enum SkillAction {
    #[command(
        about = "Write this binary's agent skill into a skill directory",
        long_about = "Write this binary's agent skill into a skill directory.\n\nWrites SKILL.md and its references, creating the directory when needed. Other files there are left alone. A directory that holds a different skill is refused."
    )]
    Install {
        #[arg(
            value_name = "DIR",
            help = "Skill directory to write [default: ~/.agents/skills/eoka, and ~/.claude/skills/eoka when ~/.claude exists]"
        )]
        dir: Option<PathBuf>,
    },
    #[command(
        about = "Rewrite installed copies of the agent skill to match this binary",
        long_about = "Rewrite installed copies of the agent skill to match this binary.\n\nLooks for an eoka skill under ~/.agents/skills, ~/.claude/skills, ~/.codex/skills, ~/.pi/agent/skills, ~/.config/opencode/skills, ~/.cursor/skills and ~/.gemini/skills, and under $CODEX_HOME/skills and $CLAUDE_CONFIG_DIR/skills when those are set. Only files that differ are rewritten; other files in the directory are left alone. Project-local copies are not searched: name one with `eoka skill install DIR`. `eoka update` runs this after installing a release."
    )]
    Refresh,
}
