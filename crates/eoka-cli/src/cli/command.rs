use super::*;

#[derive(Clone, Subcommand)]
pub enum Command {
    #[command(
        about = "Compact agent page state with scoped control references",
        long_about = "Compact agent page state with scoped control references.\n\nAnswers within --timeout-ms. When the tab cannot answer, it fails at once with the reason and the next command: dialog_open, navigation_pending, page_unresponsive, or daemon_busy while another command is running."
    )]
    View(eoka_protocol::AgentObserveArgs),
    #[command(
        about = "Perform one agent action and return its outcome and fresh page state",
        long_about = "Perform one agent action and return its outcome and fresh page state.\n\nREQUEST is JSON. Steps verify by default: give clicks and keys an explicit expected result. Exploratory input needs \"mode\": \"explore\" and exits 4 when unverified. Fill is sequential native input unless \"input_mode\": \"bulk\"."
    )]
    Step {
        #[arg(help = "Step request as JSON")]
        request: String,
    },
    #[command(about = "Submit a typed action with an operation ID")]
    Action {
        #[arg(help = "Typed action request as JSON")]
        request: String,
    },
    #[command(about = "Inspect durable action records")]
    Operations {
        #[command(subcommand)]
        command: OperationCommand,
    },
    #[command(
        about = "Navigate to URL",
        long_about = "Navigate to URL. file: URLs are blocked unless EOKA_ALLOW_FILE_URLS=1 was set when the daemon started.\n\nA load still pending from an earlier command is replaced. If the server sends no response before --action-timeout-ms (default 30000), the load is stopped, the tab keeps the page it had, and the command fails with navigation_timeout."
    )]
    Open {
        #[arg(long, help = "Extra HTTP headers as a JSON object")]
        headers: Option<String>,
        #[command(flatten)]
        args: eoka_protocol::OpenArgs,
    },
    #[command(about = "Go back in history")]
    Back,
    #[command(about = "Go forward in history")]
    Forward,
    #[command(about = "Reload current page")]
    Reload,
    #[command(about = "Accessibility tree with @eN refs")]
    Snapshot(eoka_protocol::SnapshotArgs),
    #[command(about = "Inspect page context and interactive elements with indices")]
    Observe(eoka_protocol::ObserveArgs),
    #[command(about = "Take screenshot")]
    Screenshot(eoka_protocol::ScreenshotArgs),
    #[command(about = "Record the current tab to an MP4 file")]
    Recording {
        #[command(subcommand)]
        action: RecordingAction,
    },
    #[command(about = "Handle CAPTCHAs locally or solve/inject provider tokens")]
    Captcha {
        #[command(subcommand)]
        action: CaptchaAction,
    },
    #[command(about = "Emulate a device viewport")]
    Emulate(eoka_protocol::EmulateArgs),
    #[command(about = "Get page URL and title")]
    Info,
    #[command(about = "Get all visible text on page")]
    Text,
    #[command(about = "Find elements by text substring")]
    Find(eoka_protocol::TextArgs),
    #[command(about = "Get the page HTML")]
    Content,
    #[command(
        about = "Accept or dismiss an open alert, confirm or prompt dialog",
        long_about = "Accept or dismiss an open alert, confirm, prompt or beforeunload dialog.\n\nChrome answers no page command while a dialog is open, so other commands fail with dialog_open until it is handled. These two commands also work while another command is still running."
    )]
    Dialog {
        #[command(subcommand)]
        action: DialogAction,
    },
    #[command(about = "Get the text of the first element matching a CSS selector")]
    GetText(eoka_protocol::SelectorArgs),
    #[command(about = "Get an attribute of the first element matching a CSS selector")]
    GetAttribute(eoka_protocol::AttributeArgs),
    #[command(about = "Check whether a CSS selector matches an element")]
    Exists(eoka_protocol::SelectorArgs),
    #[command(
        about = "Click an element inside open or closed shadow roots",
        long_about = "Click an element inside open or closed shadow roots with human-like movement.\n\nThe result proves the click was dispatched, not what it did; check the page afterwards."
    )]
    ClickShadow(eoka_protocol::ClickShadowArgs),
    #[command(
        about = "Click element",
        long_about = "Click element. Requires --expect with the result that proves the click worked; without it the command fails before any input."
    )]
    Click(eoka_protocol::ClickArgs),
    #[command(name = "dblclick", about = "Double-click element")]
    DoubleClick(eoka_protocol::TargetArgs),
    #[command(about = "Clear and fill input")]
    Fill(eoka_protocol::FillArgs),
    #[command(about = "Select dropdown option")]
    Select(eoka_protocol::SelectArgs),
    #[command(about = "Hover over element")]
    Hover(eoka_protocol::TargetArgs),
    #[command(
        about = "Press keyboard key",
        long_about = "Press a key on the current focus, including a focused frame.\n\n`o` sends a native printable key; `Shift+o` sends `O`. Shift uses US-keyboard ASCII mappings; Ctrl/Alt/Meta shortcuts do not insert text. Requires --expect: choose an expectation that proves the intended effect, because read-only controls and prevented events can reject edits."
    )]
    Key(eoka_protocol::KeyArgs),
    #[command(about = "Press and hold a mouse button at viewport coordinates")]
    MouseDown {
        #[command(flatten)]
        position: eoka_protocol::MouseButtonArgs,
        #[arg(
            long,
            default_value = "left",
            help = "left, middle, right, back or forward"
        )]
        button: String,
    },
    #[command(about = "Move the mouse to viewport coordinates")]
    MouseMove(eoka_protocol::MouseMoveArgs),
    #[command(about = "Release a held mouse button at viewport coordinates")]
    MouseUp {
        #[command(flatten)]
        position: eoka_protocol::MouseButtonArgs,
        #[arg(
            long,
            default_value = "left",
            help = "left, middle, right, back or forward"
        )]
        button: String,
    },
    #[command(about = "Press and hold a keyboard key")]
    KeyDown(eoka_protocol::HeldKeyArgs),
    #[command(about = "Release a held keyboard key")]
    KeyUp(eoka_protocol::HeldKeyArgs),
    #[command(about = "Release every held mouse button and key")]
    ReleaseAllInputs,
    #[command(about = "Scroll page or element into view")]
    Scroll(eoka_protocol::TargetArgs),
    #[command(
        about = "Execute JavaScript and return result",
        long_about = "Execute JavaScript and return its JSON result.\n\nCODE, also when loaded with --file, is a function called with --args, an expression such as document.title, or statements run as an async function body where return gives the value. Syntax is checked before anything runs, so the code runs exactly once, in a fresh scope; promises are awaited and exceptions fail the command. The result must be JSON: undefined, cycles, non-finite numbers and non-JSON objects are rejected. Source, arguments and result are each bounded to 1 MiB. --raw instead evaluates source in global scope and returns display text, not typed JSON."
    )]
    Eval {
        #[arg(
            long,
            default_value = "[]",
            help = "JSON array of arguments passed to the function"
        )]
        args: String,
        #[arg(
            long,
            help = "Evaluate CODE unchanged in global scope and return display text"
        )]
        raw: bool,
        #[arg(help = eoka_protocol::CODE_HELP)]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Run without returning a result")]
        no_return: bool,
        #[arg(
            long,
            help = "Fail when the JSON result exceeds this many bytes (at most 1 MiB)"
        )]
        max_size: Option<usize>,
        #[arg(long, help = "Do not await a returned promise; requires --raw")]
        no_await: bool,
    },
    #[command(about = "List frame IDs, URLs and names in the active page")]
    Frames,
    #[command(
        about = "Execute JavaScript in an isolated frame world",
        long_about = "Execute JavaScript in an isolated frame world, including cross-origin OOPIFs.\n\nThe code has DOM access but not the page's own JavaScript globals. Requires an existing session; never launches, restarts or replays. Frame IDs change on navigation: list them again with `frames`."
    )]
    FrameEval(eoka_protocol::FrameEvalArgs),
    #[command(
        about = "Execute JavaScript without returning a value",
        long_about = "Execute JavaScript without returning a value.\n\nCODE, also when loaded with --file, is a function called with --args, an expression such as an IIFE, or statements run as an async function body. Syntax is checked before anything runs, so the code runs exactly once; promises are awaited and exceptions fail the command. --raw runs the source unchanged in global scope."
    )]
    Exec {
        #[arg(
            long,
            default_value = "[]",
            help = "JSON array of arguments passed to the function"
        )]
        args: String,
        #[command(flatten)]
        script: eoka_protocol::ScriptArgs,
    },
    #[command(
        about = "Serve the eoka browser tools over MCP on stdio",
        long_about = "Serve the eoka browser tools over MCP on stdio. Register it with an MCP client, for example: claude mcp add eoka -- eoka mcp. The server owns its own browser; set EOKA_MCP_ALL_TOOLS=1 to list opt-in tools."
    )]
    Mcp,
    #[command(about = "Run Tack TypeScript against the active eoka browser session")]
    Tack {
        #[arg(help = "Tack TypeScript source; omit to use --file")]
        code: Option<String>,
        #[arg(short, long, help = "Read Tack TypeScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Execution timeout in milliseconds")]
        timeout_ms: Option<u64>,
        #[arg(long, help = "Print raw Tack execution JSON")]
        raw_json: bool,
        #[arg(long, help = "Expose all non-lifecycle eoka tools to Tack")]
        all_tools: bool,
        #[arg(long = "capability", help = "Expose opt-in tools for a capability")]
        capabilities: Vec<String>,
    },
    #[command(about = "Inspect the generated eoka tool catalog")]
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
    },
    #[command(
        about = "Download files through the existing browser session",
        long_about = "Download files through the existing eoka-owned browser session; borrowed and remote browsers are rejected.\n\nStarting returns a job ID. One file runs per session. Start actions are never replayed. Closing the session cancels unfinished jobs; published files survive. History persists across daemon restarts under EOKA_STATE_DIR; partial transfers are not resumed and clicks are never replayed. Completion means a verified regular file was atomically published without replacing anything; on failure inspect published_path and staging_path. Relative --out paths resolve in the calling shell's directory."
    )]
    Download(crate::download_cmd::DownloadCli),
    #[command(
        about = "Fetch bounded text from browser context; use download for files",
        long_about = "Fetch from inside the page with its cookies and fingerprint; use download for files.\n\nJSON output reports bytesRead, truncated and bodyOmitted. Known binary bodies are skipped."
    )]
    Fetch {
        #[arg(long, display_order = 1, help = "Request headers as a JSON object")]
        headers: Option<String>,
        #[command(flatten)]
        args: eoka_protocol::FetchArgs,
    },

    #[command(about = "Network recording, inspection, export, and interception")]
    Network {
        #[command(subcommand)]
        action: NetworkAction,
    },
    #[command(about = "Get all cookies")]
    Cookies,
    #[command(about = "Set a cookie")]
    SetCookie(eoka_protocol::SetCookieArgs),
    #[command(about = "Delete a cookie by name")]
    DeleteCookie(eoka_protocol::DeleteCookieArgs),
    #[command(about = "Clear all cookies")]
    ClearCookies,
    #[command(about = "Get localStorage")]
    Storage(eoka_protocol::StorageArgs),
    #[command(about = "Set storage value")]
    SetStorage(eoka_protocol::SetStorageArgs),
    #[command(about = "Dump localStorage and sessionStorage")]
    DumpStorage,
    #[command(
        about = "Save cookies and storage to JSON",
        long_about = "Save the current page's cookies and storage to JSON.\n\nThe path must end in .json. The file is written atomically, owner-only, and can only replace an empty file or an earlier state file."
    )]
    SaveState(eoka_protocol::PathArgs),
    #[command(
        about = "Load cookies and storage from JSON",
        long_about = "Load cookies and storage from a save-state file.\n\nCookies are replaced only on the domains the file covers, including their parent domains and subdomains; every other site's cookies are kept. Storage is restored only on the saved origin, and skipped keys are reported."
    )]
    LoadState(eoka_protocol::LoadStateArgs),
    #[command(about = "Set persistent extra HTTP headers")]
    Headers(eoka_protocol::HeadersArgs),
    #[command(about = "Read browser console output")]
    Console(eoka_protocol::ConsoleArgs),
    #[command(about = "Read JavaScript errors")]
    Errors(eoka_protocol::ClearFlagArgs),
    #[command(about = "Tab management")]
    Tab {
        #[command(subcommand)]
        action: TabAction,
    },
    #[command(
        about = "Wait for time, text, URL, selector, or load state",
        long_about = "Wait for time, text, URL, selector, or load state.\n\nA condition wait returns by --timeout (default 10000 ms). It keeps waiting through a navigation that has not answered yet and reports navigation_pending if that is why it timed out."
    )]
    Wait(eoka_protocol::WaitArgs),
    #[command(about = "Execute multiple commands in sequence")]
    Batch {
        #[arg(help = "JSON array of {\"cmd\": ..., \"args\": {...}} objects")]
        input: Option<String>,
        #[arg(short, long, help = "Read the batch from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Stop at the first failed command")]
        bail: bool,
    },
    #[command(about = "Inject fake camera from a video file")]
    FakeCamera(eoka_protocol::FakeCameraArgs),
    #[command(about = "WASM linear memory operations")]
    Wasm {
        #[command(subcommand)]
        action: WasmAction,
    },
    #[command(about = "Manage the per-domain JavaScript policy")]
    Js {
        #[command(subcommand)]
        action: JsAction,
    },
    #[command(about = "Detect SPA router type")]
    SpaInfo,
    #[command(about = "Navigate SPA without page reload")]
    SpaNavigate(eoka_protocol::PathStringArgs),
    #[command(about = "List all sessions")]
    Sessions,
    #[command(
        about = "Show whether the session is running, what it is running and what blocks its tab",
        long_about = "Show whether the session is running, what it is running and what blocks its tab.\n\nAnswers even while another command runs. Reports the running command and its age (busy), queued commands, and an open dialog or pending navigation (blocker) with the command that clears it. A daemon that accepts connections but does not answer is reported as not responsive."
    )]
    Status,
    #[command(about = "Diagnose local eoka runtime state")]
    Doctor,
    #[command(
        about = "Force-kill daemon",
        long_about = "Force-kill the session daemon and its browser. Page state is lost.\n\nA last resort for a daemon that status reports as not responsive. A busy or blocked session is recovered with the command its error names instead."
    )]
    Kill,
    #[command(
        visible_alias = "upgrade",
        about = "Update this eoka binary and the installed agent skill from the latest GitHub release",
        long_about = "Update this eoka binary and the installed agent skill from the latest GitHub release.\n\nDownloads the release archive for this platform, verifies it against the release SHA256SUMS and replaces the running executable. Installed copies of the agent skill are then rewritten to match the installed binary, also when the binary was already current; --keep-skill leaves them alone. Session daemons keep running their old version until restarted; they are listed, and stopped only with --kill-daemons."
    )]
    Update(UpdateArgs),
    #[command(about = "Install or refresh the agent skill this binary ships")]
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    #[command(about = "Close browser and daemon")]
    Close,
    #[command(
        name = "cdp-url",
        about = "Print the discovered DevTools WebSocket URL"
    )]
    CdpUrl {
        #[arg(long, help = "DevTools port [default: discover 9222-9229]")]
        port: Option<u16>,
    },
    #[command(
        name = "clone-from",
        about = "Snapshot cookies and storage from a running Chrome"
    )]
    CloneFrom(eoka_protocol::CloneFromArgs),
}
