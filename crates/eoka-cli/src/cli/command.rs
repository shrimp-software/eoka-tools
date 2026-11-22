use super::*;

#[derive(Clone, Subcommand)]
pub enum Command {
    #[command(
        about = "Compact agent page state with scoped control references",
        long_about = "Compact agent page state with scoped control references.\n\nAnswers within --timeout-ms. When the tab cannot answer, it fails at once with the reason and the next command: dialog_open, navigation_pending, page_unresponsive, or daemon_busy while another command is running."
    )]
    View {
        #[arg(long, help = "Continue from a cursor returned by a previous view")]
        cursor: Option<String>,
        #[arg(long, help = "Only controls with this role")]
        role: Option<String>,
        #[arg(long, help = "Only controls whose accessible name contains this text")]
        name: Option<String>,
        #[arg(
            long,
            default_value_t = 40,
            help = "Maximum controls to return, 1 through 200"
        )]
        max_controls: usize,
        #[arg(
            long,
            default_value_t = 3000,
            help = "Observation deadline, 50 through 60000 ms"
        )]
        timeout_ms: u64,
    },
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
        #[arg(help = "URL, or a path relative to the current page")]
        url: String,
        #[arg(long, help = "Extra HTTP headers as a JSON object")]
        headers: Option<String>,
        #[arg(long, help = "User-Agent header for this navigation")]
        user_agent: Option<String>,
        #[arg(long, help = "Disable the page's Content-Security-Policy")]
        bypass_csp: bool,
        #[arg(
            long,
            value_name = "FILE",
            help = "JavaScript source, or a .js file, to run on each new document before page scripts"
        )]
        inject_js: Option<String>,
        #[arg(
            long,
            value_name = "FILE",
            help = "Restore a save-state file before navigating"
        )]
        load_state: Option<PathBuf>,
    },
    #[command(about = "Go back in history")]
    Back,
    #[command(about = "Go forward in history")]
    Forward,
    #[command(about = "Reload current page")]
    Reload,
    #[command(about = "Accessibility tree with @eN refs")]
    Snapshot {
        #[arg(short, long, help = "Only interactive nodes")]
        interactive: bool,
        #[arg(long, help = "Keep decorative and empty text nodes")]
        all: bool,
    },
    #[command(about = "Inspect page context and interactive elements with indices")]
    Observe {
        #[arg(long, help = "Filter elements: inputs, buttons, or all")]
        filter: Option<String>,
        #[arg(long, help = "Maximum elements to return")]
        max: Option<usize>,
        #[arg(long, help = "Return structured element objects instead of text")]
        structured: bool,
    },
    #[command(about = "Take screenshot")]
    Screenshot {
        #[arg(
            short,
            long,
            help = "PNG path [default: eoka-screenshot-<ms>.png in the temp dir]"
        )]
        output: Option<PathBuf>,
        #[arg(long, help = "Overlay numbered labels on interactive elements")]
        annotate: bool,
        #[arg(long, value_parser = ["png", "jpeg", "webp"], help = "Image format [default: png]")]
        format: Option<String>,
        #[arg(long, help = "JPEG or WebP quality, 0 to 100")]
        quality: Option<u8>,
        #[arg(
            long,
            conflicts_with = "clip",
            help = "Capture the whole scrollable page"
        )]
        full_page: bool,
        #[arg(
            long,
            value_name = "X,Y,WIDTH,HEIGHT",
            value_parser = parse_clip,
            help = "Capture a document-relative region in CSS pixels"
        )]
        clip: Option<eoka_protocol::sidecar::ScreenshotClip>,
    },
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
    Emulate {
        #[arg(long, default_value = "390", help = "Viewport width in CSS pixels")]
        width: u32,
        #[arg(long, default_value = "844", help = "Viewport height in CSS pixels")]
        height: u32,
        #[arg(long, default_value = "2", help = "Device pixel ratio")]
        dpr: f64,
        #[arg(long, help = "Emulate a desktop instead of a touch mobile device")]
        desktop: bool,
        #[arg(long, help = "Clear emulation and ignore the other flags")]
        reset: bool,
    },
    #[command(about = "Get page URL and title")]
    Info,
    #[command(about = "Get all visible text on page")]
    Text,
    #[command(about = "Find elements by text substring")]
    Find {
        #[arg(help = "Text to search for")]
        text: String,
    },
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
    GetText {
        #[arg(help = "CSS selector")]
        selector: String,
    },
    #[command(about = "Get an attribute of the first element matching a CSS selector")]
    GetAttribute {
        #[arg(help = "CSS selector")]
        selector: String,
        #[arg(help = "Attribute name, for example href")]
        name: String,
    },
    #[command(about = "Check whether a CSS selector matches an element")]
    Exists {
        #[arg(help = "CSS selector")]
        selector: String,
    },
    #[command(
        about = "Click an element inside open or closed shadow roots",
        long_about = "Click an element inside open or closed shadow roots with human-like movement.\n\nThe result proves the click was dispatched, not what it did; check the page afterwards."
    )]
    ClickShadow {
        #[arg(help = "CSS selector, matched through shadow roots")]
        selector: String,
        #[arg(long, help = "Frame ID from `eoka frames`; the top frame when omitted")]
        frame: Option<String>,
        #[arg(long, default_value = "10000", help = "Deadline in milliseconds")]
        timeout_ms: u64,
    },
    #[command(
        about = "Click element",
        long_about = "Click element. Requires --expect with the result that proves the click worked; without it the command fails before any input."
    )]
    Click {
        #[arg(help = TARGET)]
        target: String,
    },
    #[command(name = "dblclick", about = "Double-click element")]
    DoubleClick {
        #[arg(help = TARGET)]
        target: String,
    },
    #[command(about = "Clear and fill input")]
    Fill {
        #[arg(help = TARGET)]
        target: String,
        #[arg(help = "Text to enter")]
        text: String,
        #[arg(
            long,
            default_value = "sequential",
            value_parser = ["sequential", "bulk"],
            help = "sequential types native keystrokes; bulk sets the value at once"
        )]
        input_mode: String,
    },
    #[command(about = "Select dropdown option")]
    Select {
        #[arg(help = TARGET)]
        target: String,
        #[arg(help = "Option value or visible label")]
        value: String,
    },
    #[command(about = "Hover over element")]
    Hover {
        #[arg(help = TARGET)]
        target: String,
    },
    #[command(
        about = "Press keyboard key",
        long_about = "Press a key on the current focus, including a focused frame.\n\n`o` sends a native printable key; `Shift+o` sends `O`. Shift uses US-keyboard ASCII mappings; Ctrl/Alt/Meta shortcuts do not insert text. Requires --expect: choose an expectation that proves the intended effect, because read-only controls and prevented events can reject edits."
    )]
    Key {
        #[arg(help = "Key name or chord such as Enter, a, Shift+o, Control+a")]
        key: String,
    },
    #[command(about = "Press and hold a mouse button at viewport coordinates")]
    MouseDown {
        #[arg(help = "Viewport X in CSS pixels")]
        x: f64,
        #[arg(help = "Viewport Y in CSS pixels")]
        y: f64,
        #[arg(
            long,
            default_value = "left",
            help = "left, middle, right, back or forward"
        )]
        button: String,
    },
    #[command(about = "Move the mouse to viewport coordinates")]
    MouseMove {
        #[arg(help = "Viewport X in CSS pixels")]
        x: f64,
        #[arg(help = "Viewport Y in CSS pixels")]
        y: f64,
    },
    #[command(about = "Release a held mouse button at viewport coordinates")]
    MouseUp {
        #[arg(help = "Viewport X in CSS pixels")]
        x: f64,
        #[arg(help = "Viewport Y in CSS pixels")]
        y: f64,
        #[arg(
            long,
            default_value = "left",
            help = "left, middle, right, back or forward"
        )]
        button: String,
    },
    #[command(about = "Press and hold a keyboard key")]
    KeyDown {
        #[arg(help = "Key name such as Shift, Control or a")]
        key: String,
    },
    #[command(about = "Release a held keyboard key")]
    KeyUp {
        #[arg(help = "Key name such as Shift, Control or a")]
        key: String,
    },
    #[command(about = "Release every held mouse button and key")]
    ReleaseAllInputs,
    #[command(about = "Scroll page or element into view")]
    Scroll {
        #[arg(help = TARGET)]
        target: String,
    },
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
        #[arg(help = CODE)]
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
    FrameEval {
        #[arg(
            help = "CSS selector matching one frame, or id:<frame-id> from frames; no bare URL/index"
        )]
        frame: String,
        #[arg(help = CODE)]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
    },
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
        #[arg(long, help = "Run CODE unchanged in global scope")]
        raw: bool,
        #[arg(help = CODE)]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Do not await a returned promise; requires --raw")]
        no_await: bool,
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
        #[arg(help = "URL to fetch from the page's context")]
        url: String,
        #[arg(short, long, help = "HTTP method [default: GET]")]
        method: Option<String>,
        #[arg(long, help = "Request headers as a JSON object")]
        headers: Option<String>,
        #[arg(short, long, help = "Request body")]
        body: Option<String>,
        #[arg(long, help = "Fetch redirect mode: follow, error or manual")]
        redirect: Option<String>,
        #[arg(
            long,
            help = "Print only the body; fails on binary or truncated bodies"
        )]
        body_only: bool,
        #[arg(
            long,
            help = "Raw text bytes to retain: default 8192, maximum 16777216; 0 means headers only"
        )]
        max_body: Option<usize>,
    },

    #[command(about = "Network recording, inspection, export, and interception")]
    Network {
        #[command(subcommand)]
        action: NetworkAction,
    },
    #[command(about = "Get all cookies")]
    Cookies,
    #[command(about = "Set a cookie")]
    SetCookie {
        #[arg(help = "Cookie name")]
        name: String,
        #[arg(help = "Cookie value")]
        value: String,
        #[arg(long, help = "Cookie domain [default: current page]")]
        domain: Option<String>,
        #[arg(long, help = "Cookie path")]
        path: Option<String>,
    },
    #[command(about = "Delete a cookie by name")]
    DeleteCookie {
        #[arg(help = "Cookie name")]
        name: String,
        #[arg(long, help = "Cookie domain [default: current page hostname]")]
        domain: Option<String>,
    },
    #[command(about = "Clear all cookies")]
    ClearCookies,
    #[command(about = "Get localStorage")]
    Storage {
        #[arg(help = "Key to read; omit for every key")]
        key: Option<String>,
        #[arg(long, help = "Use sessionStorage instead of localStorage")]
        session_storage: bool,
    },
    #[command(about = "Set storage value")]
    SetStorage {
        #[arg(help = "Storage key")]
        key: String,
        #[arg(help = "Value to store")]
        value: String,
        #[arg(long, help = "Use sessionStorage instead of localStorage")]
        session_storage: bool,
    },
    #[command(about = "Dump localStorage and sessionStorage")]
    DumpStorage,
    #[command(
        about = "Save cookies and storage to JSON",
        long_about = "Save the current page's cookies and storage to JSON.\n\nThe path must end in .json. The file is written atomically, owner-only, and can only replace an empty file or an earlier state file."
    )]
    SaveState {
        #[arg(help = "State file to write; must end in .json")]
        path: PathBuf,
    },
    #[command(
        about = "Load cookies and storage from JSON",
        long_about = "Load cookies and storage from a save-state file.\n\nCookies are replaced only on the domains the file covers, including their parent domains and subdomains; every other site's cookies are kept. Storage is restored only on the saved origin, and skipped keys are reported."
    )]
    LoadState {
        #[arg(help = "State file written by save-state")]
        path: PathBuf,
        #[arg(long, help = "Do not navigate to the saved URL first")]
        no_navigate: bool,
    },
    #[command(about = "Set persistent extra HTTP headers")]
    Headers {
        #[arg(help = "Headers as a JSON object")]
        headers_json: String,
    },
    #[command(about = "Read browser console output")]
    Console {
        #[arg(long, help = "Clear captured output after reading")]
        clear: bool,
        #[arg(long, help = "Only this level: log, warn, error, info or debug")]
        level: Option<String>,
    },
    #[command(about = "Read JavaScript errors")]
    Errors {
        #[arg(long, help = "Clear captured errors after reading")]
        clear: bool,
    },
    #[command(about = "Tab management")]
    Tab {
        #[command(subcommand)]
        action: TabAction,
    },
    #[command(
        about = "Wait for time, text, URL, selector, or load state",
        long_about = "Wait for time, text, URL, selector, or load state.\n\nA condition wait returns by --timeout (default 10000 ms). It keeps waiting through a navigation that has not answered yet and reports navigation_pending if that is why it timed out."
    )]
    Wait {
        #[arg(help = "Fixed delay in milliseconds")]
        ms: Option<u64>,
        #[arg(long, help = "Wait until this text is visible")]
        text: Option<String>,
        #[arg(long, help = "Wait until the URL contains this string")]
        url: Option<String>,
        #[arg(long, help = "Wait for network idle")]
        load: Option<String>,
        #[arg(long, help = "Wait until this CSS selector reaches --state")]
        selector: Option<String>,
        #[arg(
            long,
            requires = "selector",
            value_parser = ["attached", "visible", "hidden"],
            help = "Selector condition [default: visible]"
        )]
        state: Option<String>,
        #[arg(short, long, help = "Timeout in milliseconds")]
        timeout: Option<u64>,
    },
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
    FakeCamera {
        #[arg(help = "Video file to play as the camera")]
        file: PathBuf,
        #[arg(long, help = "Loop the video instead of stopping at the end")]
        loop_video: bool,
    },
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
    SpaNavigate {
        #[arg(help = "Route path such as /dashboard")]
        path: String,
    },
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
    CloneFrom {
        #[arg(help = "DevTools port or WebSocket URL of the running Chrome")]
        source: String,
        #[arg(long, help = "Write the snapshot to this state file")]
        to: Option<PathBuf>,
    },
}
