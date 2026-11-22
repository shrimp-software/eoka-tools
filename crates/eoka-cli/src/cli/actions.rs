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
    Log(eoka_protocol::NetworkLogArgs),
    #[command(about = "Show one recorded request")]
    Show(eoka_protocol::NetworkShowArgs),
    #[command(about = "Write recorded requests as a HAR file")]
    Har {
        #[arg(help = "HAR file to write")]
        path: PathBuf,
        #[arg(long, help = "Wait this long for in-flight requests before writing")]
        settle_ms: Option<u64>,
    },
    #[command(about = "Export recorded requests as HAR or JSON")]
    Export(eoka_protocol::NetworkExportArgs),
    #[command(about = "Wait for a matching request")]
    Wait(eoka_protocol::NetworkWaitArgs),
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
    Start(eoka_protocol::NetworkRecordStartArgs),
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
    Datadome(eoka_protocol::CaptchaDatadomeArgs),
    #[command(
        about = "Click the Cloudflare Turnstile checkbox on the current tab until the challenge clears",
        long_about = "Click the Cloudflare Turnstile checkbox on the current tab until it leaves the challenge page.\n\nReports clicks and elapsed_ms; a normal page returns zero clicks. --auto-solve-cloudflare does this for every tab of a browser eoka launches."
    )]
    Turnstile(eoka_protocol::CaptchaTurnstileArgs),
    #[command(about = "Solve a CAPTCHA through the Anti-Captcha provider")]
    Solve(Box<SolveArgs>),
    #[command(about = "Inject a solved token into the current page")]
    Inject(eoka_protocol::CaptchaInjectArgs),
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
    Read(eoka_protocol::WasmReadArgs),
    #[command(about = "Write hex bytes at ADDR")]
    Write(eoka_protocol::WasmWriteArgs),
    #[command(about = "Search memory for a hex byte pattern")]
    Find(eoka_protocol::WasmFindArgs),
    #[command(about = "List WASM memories and their sizes")]
    Info,
}

#[derive(Clone, Subcommand)]
pub enum InterceptAction {
    #[command(about = "Add an interception rule")]
    Add(eoka_protocol::InterceptAddArgs),
    #[command(about = "List interception rules")]
    List,
    #[command(about = "Remove an interception rule")]
    Remove(eoka_protocol::IdArgs),
    #[command(
        about = "Read the interception log (latest 1000 entries)",
        mut_arg("clear", |arg| arg.help("Clear the log after reading"))
    )]
    Log(eoka_protocol::ClearFlagArgs),
}

#[derive(Clone, Subcommand)]
pub enum JsAction {
    #[command(about = "Set the default policy")]
    Mode(eoka_protocol::ModeArgs),
    #[command(about = "Always run JavaScript on a domain")]
    Allow(eoka_protocol::DomainArgs),
    #[command(about = "Always block JavaScript on a domain")]
    Block(eoka_protocol::DomainArgs),
    #[command(about = "Remove a domain rule")]
    Remove(eoka_protocol::DomainArgs),
    #[command(about = "List the policy")]
    List,
}

#[derive(Clone, Subcommand)]
pub enum TabAction {
    #[command(about = "List tabs")]
    List,
    #[command(about = "Open a tab, optionally at a URL")]
    New(eoka_protocol::TabNewArgs),
    #[command(about = "Make a tab current")]
    Switch(eoka_protocol::TabIdArgs),
    #[command(about = "Close a tab")]
    Close(eoka_protocol::TabIdArgs),
    #[command(about = "Adopt a tab eoka did not open")]
    Attach(eoka_protocol::TabIdArgs),
}

#[derive(Clone, Subcommand)]
pub enum OperationCommand {
    #[command(about = "Show one operation record")]
    Get(eoka_protocol::OperationIdArgs),
    #[command(about = "List recent operation records")]
    List(eoka_protocol::OperationListArgs),
    #[command(about = "Wait for an operation to finish")]
    Wait(eoka_protocol::OperationWaitArgs),
    #[command(about = "Cancel a running operation")]
    Cancel(eoka_protocol::OperationIdArgs),
}

#[derive(Clone, Subcommand)]
pub enum DialogAction {
    #[command(about = "Accept the open dialog")]
    Accept(eoka_protocol::DialogAcceptArgs),
    #[command(about = "Dismiss the open dialog")]
    Dismiss,
}

#[derive(Clone, Subcommand)]
pub enum RecordingAction {
    #[command(about = "Start recording the current tab; prints the recording ID")]
    Start(eoka_protocol::RecordingStartArgs),
    #[command(about = "Show a recording's state")]
    Status(eoka_protocol::RecordingIdArgs),
    #[command(about = "Stop a recording and finalize its file")]
    Stop(eoka_protocol::RecordingIdArgs),
    #[command(about = "Cancel a recording and discard its file")]
    Cancel(eoka_protocol::RecordingIdArgs),
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
