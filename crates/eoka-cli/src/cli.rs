use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const TARGET: &str =
    "text:Submit (or bare text), placeholder:Email, css:#id, id:login, role:button, or a target from view";
const ADDR: &str = "Address, decimal or 0x-prefixed hex";
const MEMORY: &str = "JavaScript expression for the WebAssembly.Memory [default: auto-detect]";
const TAB_ID: &str = "Tab ID from tab list";
const OPERATION_ID: &str = "Operation ID returned by the action";
const CODE: &str = "JavaScript source; omit to use --file";

const DEADLINES: &str = "Deadlines:\n  Every command returns within a bounded time and never replays an action.\n  A failure carries error_detail.code and a hint naming the next command:\n    dialog_open          a JavaScript dialog blocks the tab: dialog accept | dialog dismiss\n    navigation_pending   the tab waits for a server: wait --url, or reload | open URL to abandon it\n    navigation_timeout   no response before the deadline; the load was stopped and the tab kept its page\n    daemon_busy          another command is running; this one was not started (EOKA_QUEUE_WAIT_MS, default 5000)\n    page_unresponsive    the page's main thread is frozen: retry, or tab new URL\n    reply_timeout        the daemon did not reply in time (EOKA_REPLY_TIMEOUT_MS overrides the default)\n    daemon_unresponsive  the daemon did not answer its handshake within 5 s\n  Page reads answer within 15 s, view within --timeout-ms (3000), open and actions\n  within --action-timeout-ms (30000). status shows what a session is running.";

#[derive(Parser)]
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

#[derive(Subcommand)]
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
        long_about = "Execute a JavaScript function and return its JSON result.\n\nCODE is a function, also when loaded with --file. It runs once in a fresh scope, promises are awaited and exceptions fail the command. The result must be JSON: undefined, cycles, non-finite numbers and non-JSON objects are rejected. Source, arguments and result are each bounded to 1 MiB. --raw instead evaluates source in global scope and returns display text, not typed JSON."
    )]
    Eval {
        #[arg(
            long,
            default_value = "[]",
            help = "JSON array of arguments passed to the function"
        )]
        args: String,
        #[arg(long, help = "Treat CODE as an expression, not a function")]
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
    #[command(about = "Execute JavaScript without returning a value")]
    Exec {
        #[arg(
            long,
            default_value = "[]",
            help = "JSON array of arguments passed to the function"
        )]
        args: String,
        #[arg(long, help = "Treat CODE as an expression, not a function")]
        raw: bool,
        #[arg(help = CODE)]
        code: Option<String>,
        #[arg(short, long, help = "Read JavaScript from a file")]
        file: Option<PathBuf>,
        #[arg(long, help = "Do not await a returned promise; requires --raw")]
        no_await: bool,
    },
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
        about = "Update this eoka binary from the latest GitHub release",
        long_about = "Update this eoka binary from the latest GitHub release.\n\nDownloads the release archive for this platform, verifies it against the release SHA256SUMS and replaces the running executable. Session daemons keep running their old version until restarted; they are listed, and stopped only with --kill-daemons."
    )]
    Update(UpdateArgs),
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

#[derive(Subcommand)]
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

#[derive(Subcommand)]
pub enum ToolsAction {
    #[command(about = "Print the tool manifest generated from eoka-protocol")]
    Manifest {
        #[arg(long, help = "Include tools hidden from the default agent exposure")]
        all: bool,
        #[arg(long, help = "Print raw {\"tools\": [...]} JSON")]
        json: bool,
    },
}

#[derive(Subcommand)]
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

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum CaptchaAction {
    #[command(about = "Detect an hCaptcha or reCAPTCHA widget and report its type and site key")]
    Detect,
    #[command(
        about = "Attempt local DataDome clearance on the current tab; keep the session open",
        long_about = "Attempt local DataDome clearance on the current tab. No API key needed.\n\nRequires an existing session; foregrounds the tab and leaves it open. challenge_cleared is not proof of login; not_present is a no-op; every other outcome exits nonzero. An old daemon is not restarted automatically."
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

#[derive(Args)]
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

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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

#[derive(Subcommand)]
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

/// Parse `X,Y,WIDTH,HEIGHT` for `screenshot --clip`.
fn parse_clip(value: &str) -> Result<eoka_protocol::sidecar::ScreenshotClip, String> {
    let numbers: Vec<f64> = value
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| "expected four numbers: X,Y,WIDTH,HEIGHT".to_owned())?;
    match numbers[..] {
        [x, y, width, height]
            if numbers.iter().all(|n| n.is_finite())
                && x >= 0.0
                && y >= 0.0
                && width > 0.0
                && height > 0.0 =>
        {
            Ok(eoka_protocol::sidecar::ScreenshotClip {
                x,
                y,
                width,
                height,
            })
        }
        [_, _, _, _] => Err("X and Y must be at least 0, WIDTH and HEIGHT greater than 0".into()),
        _ => Err("expected four numbers: X,Y,WIDTH,HEIGHT".into()),
    }
}

#[derive(Subcommand)]
pub enum DialogAction {
    #[command(about = "Accept the open dialog")]
    Accept {
        #[arg(long, help = "Text to enter for a prompt() dialog")]
        prompt_text: Option<String>,
    },
    #[command(about = "Dismiss the open dialog")]
    Dismiss,
}

#[derive(Subcommand)]
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

#[derive(Args)]
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
}
