# eoka wire protocol

One wire protocol, newline-delimited JSON with `{id, method, params}` requests
and `{id, result}` or `{id, error}` replies, has two transports:

- **stdio:** `eoka-server` is a private subprocess wrapping the `eoka` Rust
  browser library. It reads requests on stdin and writes replies on stdout. The
  Node and Go clients use it. Most of this document describes this transport.
- **Session socket:** the CLI's per-session daemon listens on a Unix socket
  (`$XDG_RUNTIME_DIR/eoka/eoka-<session>.sock`). The CLI and `eoka-sdk` use
  it. See [Session socket](#session-socket).

The canonical envelopes, method parameters, results and schemas live in
`eoka-protocol::sidecar`. Server handlers and the generated client types
consume those contracts.

## Transport

- Each message is one UTF-8 JSON value followed by a newline. Embedded newlines
  in JSON strings must be escaped. Readers must handle split UTF-8 characters,
  partial lines and several responses in one read.
- stdout contains protocol responses only. Diagnostics go to stderr. Do not
  parse stderr as a protocol stream or include credentials in diagnostic logs.
- A serialized message is limited to 33,554,432 bytes (32 MiB), excluding its
  line terminator. Input is bounded while reading. Oversized output becomes a
  structured error rather than an unlimited JSON message.
- At most 128 requests may remain in flight. A request stops counting before its
  response is written, so a client that keeps at most `maxInFlight` unanswered
  requests is never refused with `ServerBusy`. Respect advertised limits and
  stdout backpressure; an unread pipe is not permission to buffer indefinitely.
- Request IDs are positive integers through 9,007,199,254,740,991, unique among
  in-flight requests. Responses echo the ID. A malformed message without a
  recoverable valid ID may receive an error with `id: null`.
- `pageId` is an opaque target ID returned by the server. Element handles do
  not cross this wire.
- Requests are dispatched per page. Requests that name the same `pageId`,
  including action admission, catalog operations and `browser.close_tab`, run
  one at a time in the order received. Requests for different pages run
  concurrently, so a page whose request is slow or never answers delays only
  its own later requests. `browser.new_page` and `browser.tabs` belong to no
  page and are never queued behind one. `browser.launch`, `browser.connect` and
  `browser.close` are barriers: each waits for every request received before it
  and holds back every request received after it until it finishes.
- Requests that clear a blocked page skip its queue: `dialog.accept` and
  `dialog.dismiss` always, and `browser.close_tab` while the page has an open
  dialog or a pending navigation. Later requests for the page still wait for them.
- Catalog operations that act on one page alone run on that page: the reads
  and waits, held input, `click_shadow`, `captcha.turnstile`, `eval`, `exec`,
  `fetch`, `screenshot` without `annotate`, and `emulate` with `desktop` or
  `reset`. The other catalog operations share one engine and run one at a time
  across pages, so a slow one delays only those operations on other pages.
  `agent.observe` and `agent.step` likewise share one agent session.
- `operations.wait` and `operations.list` run outside the page queues, and so do
  `operations.get` and `operations.cancel` for an admitted ID; an unknown ID
  waits only for an earlier queued request carrying it. They still count toward
  `maxInFlight`; `server.shutdown` has its own admission. Responses can arrive
  out of order, so always correlate by ID.
- A local timeout or connection loss does not prove a browser action was
  cancelled. Durable operation IDs are the recovery mechanism.

## Negotiation

Before launching Chrome, clients send:

```json
{"id":1,"method":"server.hello","params":{"protocol":{"major":4,"minor":0}}}
```

The result (`HelloResult`) names the server's protocol version, build, the
methods and capabilities it implements, and its transport limits. A hello
naming another major, or a minor newer than the server's, returns
`ProtocolMismatch` without launching a browser. There is no fallback: the Node
and Go SDKs ship pinned to the matching server.

Capabilities describe this server, not the attached Chrome, and none implies
managed Chrome installation:

- `durableActions`, `operationCancellation` and `gracefulShutdown`: server
  behavior.
- `explicitExecutablePath`, `browserAttachment`, `runtimeIsolation`,
  `sandboxControl` and `viewportEmulation`: the managed session methods.
- `boundedFetch`: streaming response-body inspection under a byte budget.
- `screenshotOptions`, `screenshotFile` and `nativeRecording`: the capture
  methods, not that a Chrome build supports a format or has the encoder.
- `cloudflareAutoSolve`: the `autoSolveCloudflare` option on `browser.launch`
  and `browser.connect`, a watcher that solves full-page Cloudflare challenges
  in every tab until the browser closes.
- `operationCatalog` and `pageBlockers`: described below.

## Message shapes

A request, a success and an error:

```json
{"id":2,"method":"info","params":{"pageId":"ABCD1234"}}
{"id":2,"result":{"url":"https://example.com/","title":"Example"}}
{"id":2,"error":{"code":"InvalidPage","message":"unknown pageId"}}
```

Exactly one of `result` or `error` is present. A successful null result is still
`"result": null`; it must not disappear during serialization. Result types are
method-specific: `operations.list`, for example, returns an array directly.

Errors have a stable `code` and a human-readable `message`. Optional `data`
holds structured evidence such as an operation ID, an action receipt or the
canonical `ErrorDetail` in `data.detail`, which every failed catalog operation
carries with a `hint` naming the next step. Branch on codes and fields, not
text. An absent receipt is not proof that nothing was dispatched.

Invalid envelopes or parameters, unknown methods, incompatible protocols,
message-size violations, overload and shutdown are explicit protocol errors.
Core errors keep their element, timeout and CDP classifications; structured
action evidence is never reduced to a message-only error.

### Blocked pages

Chrome answers no page command while a JavaScript dialog is open on the page,
while the page's main frame waits for a navigation response, or while the
page's main thread is busy. The `pageBlockers` capability says the server
detects these states and fails the request promptly instead of at the 30-second
command deadline. The request fails with:

| Code | State | `data.blocker` |
| --- | --- | --- |
| `DialogOpen` | An `alert`, `confirm`, `prompt` or `beforeunload` dialog is open | `{kind:"dialog", dialogType, message, url, elapsedMs}` |
| `NavigationPending` | The main frame has waited for a navigation response | `{kind:"navigation", url?, elapsedMs}` |
| `PageUnresponsive` | The page answers nothing and neither state above is known | absent |

`data.detail` carries the `ErrorDetail` (`dialog_open`, `navigation_pending` or
`page_unresponsive`) with a `hint` and, for a request that was not started,
`context.action_status`. `elapsedMs` is how long the dialog has been open or
the navigation has waited. Registered methods and catalog operations behave
the same:

- A request for a page with an open dialog is refused before it starts. So is
  a request for a page whose navigation has waited five seconds, except waits
  and captures, which run, and navigations, which first stop the pending load
  and then proceed; that is how a fresh navigate action recovers a stalled tab.
- A request already in flight ends as soon as a dialog opens, or once a
  navigation that started under it has waited five seconds. Its message says it
  returned without a result; a script or input may already have taken effect.
  `agent.observe`, `agent.step` and action admission stop by themselves and
  return their own evidence: `agent.observe` fails with the same code, and
  `agent.step` and the operation record carry `dialog_open`.
- Reads (`info`, `text`, `content`, `get_text`, `get_attribute`, `exists`,
  `frames`, `page.capture_state`) that get no answer within 15 seconds end as
  `PageUnresponsive`. Other methods that reach the command deadline are checked
  the same way before their timeout is reported.
- `dialog.accept` and `dialog.dismiss` answer the dialog directly and work
  while any other request is blocked. `release_all_inputs`, `emulate`,
  `page.restore_state`, `captcha.turnstile`, `recording.*` and tab close are
  never refused.
- An operation running when a dialog opens on its page finishes with
  `error_code` `dialog_open` and `outcome_unknown` if input had been dispatched
  (`not_dispatched` otherwise). Nothing is replayed, and input it still held is
  released before the page's next action.
- A dialog is known from the event Chrome sends when it opens, and Chrome does
  not send that event again to a session that attaches later. Attaching to a
  tab whose dialog is already open does not answer until the dialog closes, so
  `browser.connect` to a browser with such a tab, or `browser.tabs` picking up
  a popup that alerted before it was attached, fails with a CDP timeout after
  30 seconds. The tab gets no `pageId`; close the dialog in Chrome first.

## Methods

The registry has two parts: the methods registered in the
`define_sidecar_methods!` table in `eoka-protocol::sidecar`, and the catalog
operations. The schema bundle has every method's exact fields, bounds, defaults
and result:

```sh
cargo run --quiet --locked -p eoka-protocol --bin export-sidecar-schema
```

<!-- generated:methods -->
<!-- Generated from the schema bundle by clients/node/scripts/generate-protocol.mjs. Do not edit. -->

| Method | Params (`?` optional) | Result |
| --- | --- | --- |
| `actions.perform` | `action`, `operation_id`, `options?`, `pageId` | `OperationRecord` |
| `agent.observe` | `cursor?`, `max_controls?`, `name?`, `pageId`, `role?`, `timeout_ms?` | `AgentObservation` |
| `agent.step` | `action`, `action_timeout_ms?`, `destination?`, `expect?`, `max_controls?`, `mode?`, `operation_id?`, `pageId`, `timeout_ms?` | `AgentStepResult` |
| `browser.close` | none | `Empty` |
| `browser.close_tab` | `pageId` | `CloseTabResult` |
| `browser.connect` | `autoSolveCloudflare?`, `wsEndpoint` | `BrowserSessionResult` |
| `browser.launch` | `aggressiveCdpEvasion?`, `autoSolveCloudflare?`, `deviceScaleFactor?`, `executablePath?`, `extraArgs?`, `geoAlign?`, `headless?`, `proxy?`, `runtimeDir?`, `sandbox?`, `timezone?`, `userAgent?`, `userDataDir?`, `viewportHeight?`, `viewportWidth?` | `BrowserSessionResult` |
| `browser.new_page` | `url?` | `NewPageResult` |
| `browser.tabs` | none | `TabsResult` |
| `operations.cancel` | `id` | `OperationRecord` |
| `operations.get` | `id` | `OperationRecord` |
| `operations.list` | `before_ms?`, `limit?` | `OperationRecord[]` |
| `operations.wait` | `id`, `timeout_ms?` | `OperationRecord` |
| `page.capture_state` | `pageId` | `CaptureStateResult` |
| `page.restore_state` | `pageId`, `state` | `Empty` |
| `page.solve_captcha` | `apiDomain?`, `apiKey`, `callback?`, `captchaMode`, `captchaType`, `enterprisePayload?`, `pageId`, `websiteKey`, `websiteURL` | `SolveCaptchaResult` |
| `recording.cancel` | `recordingId` | `RecordingRecord` |
| `recording.get` | `recordingId` | `RecordingRecord` |
| `recording.start` | `fps?`, `maxHeight?`, `maxWidth?`, `pageId`, `path`, `recordingId` | `RecordingRecord` |
| `recording.stop` | `recordingId` | `RecordingRecord` |
| `server.hello` | `protocol` | `HelloResult` |
| `server.shutdown` | none | `Empty` |

Catalog operations (83), each taking its catalog input plus `pageId`: `back`, `captcha.datadome`, `captcha.detect`, `captcha.inject`, `captcha.turnstile`, `clear_cookies`, `click`, `click_shadow`, `clone_from`, `console`, `content`, `cookies`, `delete_cookie`, `dialog.accept`, `dialog.dismiss`, `double_click`, `download.cancel`, `download.get`, `download.list`, `download.start`, `download.wait`, `dump_storage`, `emulate`, `errors`, `eval`, `exec`, `exists`, `fake_camera`, `fetch`, `fill`, `find`, `forward`, `frame_eval`, `frames`, `get_attribute`, `get_text`, `headers`, `hover`, `info`, `intercept.add`, `intercept.list`, `intercept.log`, `intercept.remove`, `js.allow`, `js.block`, `js.list`, `js.mode`, `js.remove`, `key`, `key.down`, `key.up`, `load_state`, `mouse.down`, `mouse.move`, `mouse.up`, `network.clear`, `network.export`, `network.log`, `network.record.start`, `network.record.status`, `network.record.stop`, `network.show`, `network.wait`, `observe`, `open`, `release_all_inputs`, `reload`, `save_state`, `screenshot`, `scroll`, `select`, `set_cookie`, `set_storage`, `snapshot`, `spa.info`, `spa.navigate`, `storage`, `text`, `wait`, `wasm.find`, `wasm.info`, `wasm.read`, `wasm.write`.
<!-- /generated:methods -->

The sections below describe behavior the schemas can't express.

### Catalog operations

Servers advertising the `operationCatalog` capability also serve the operation
catalog used by the CLI daemon, MCP and Tack. Each catalog operation is a method
named by its catalog path (`console`, `cookies`, `network.record.start`, ...),
executed by the same engine as the CLI daemon. The schema bundle lists them
under `catalogMethods`.

```json
{"id":7,"method":"console","params":{"pageId":"<id>","clear":false}}
```

- Params are the operation's catalog input plus a required string `pageId`.
  Catalog fields are snake_case. An unknown field is `InvalidParams`, not ignored.
- The result is the operation's `data` value, typed in the schema bundle. Only
  `eval` is untyped, because its result is the value the page's JavaScript
  returned. Action results are the operation record plus its `status`.
- Failures return an `Operation` error carrying the catalog `ErrorDetail`.
  Errors raised by the core library keep their kind: a catalog operation that
  times out returns `Timeout`, a missing element returns `ElementNotFound`.
- Actions (`click`, `double_click`, `fill`, `select`, `hover`, `key`, `scroll`,
  `back`, `forward`, `reload`) are durable operations. `operation_id` is
  optional and generated when omitted. An admitted action returns its operation
  record as the result even when the outcome is unverified or failed, like
  `actions.perform`; read `outcome` and `error` on the record.
- Target references from `agent.observe` resolve in catalog actions.
- `open` navigates the addressed page, on launched and connected browsers.
- Session lifecycle and tabs are not projected: use `server.shutdown`,
  `browser.close`, `browser.new_page`, `browser.tabs` and `browser.close_tab`.
- A catalog path that already names a registered method keeps that method's
  contract. Every registered method declares its role in
  `define_sidecar_methods!`: lifecycle, shared with the catalog operation of the
  same name, or distinct from the catalog for a stated reason.

Some catalog state belongs to the browser session rather than one page: the
network recorder and its log, the download slot, and the JavaScript policy
(`js.*`). The policy applies to navigations made by catalog `open` and
`reload`, not to `back`, `forward` or a link that leaves the host, because
their destination is not known before the navigation starts. Interception rules
are re-applied to whichever page a catalog call addresses. Catalog actions reach
a page behind another tab the same way `actions.perform` does (see
[Durable actions](#durable-actions-and-operation-queries)).

The catalog includes operations that read or write local files chosen by the
caller (`save_state`, `load_state`, `network.export`, `intercept.add` with
`capture` or `respond`, `fake_camera`, `download.start`, `screenshot`,
`recording.start`) and `clone_from`, which copies state from another local
Chrome. A sidecar client has the same local reach as the `eoka` CLI. Do not pass
untrusted input through as method names or file paths.

### Server lifecycle

Shutdown works before browser launch, after failed launch and while an ordinary
request or operation waiter is pending. The server stops admitting normal work,
settles/cancels pending requests, stops owned operations, releases held inputs
and closes owned browser resources. A cancelled waiter does not establish a
verified action outcome; inspect the durable record.

EOF on stdin follows the same path. Successful cleanup exits zero; failed
cleanup is not reported as success. Shutdown is bounded: during Chrome launch or
attachment allow up to 35 seconds for startup, then 5 seconds of browser cleanup
and 2 seconds of output flushing. Ordinary shutdown skips the launch grace.

### Browser

`viewportWidth` and `viewportHeight` go together; `deviceScaleFactor` alone
scales the default viewport. Explicit executable, runtime and user-data paths
must be absolute. Missing,
unreadable or non-executable explicit Chrome paths fail without discovering a
substitute. `browserVersion` is the actual CDP `Browser.getVersion` product,
not a spoofed user-agent string. `ownership` is `"owned"` only when the handle
owns Chrome; reuse of a running profile reports `"borrowed"`.

`runtimeDir` selects a writable root for a private per-browser child holding
owned temporary profiles, patch artifacts and Chrome temporary files; Chrome
gets child-local `TMPDIR`, `TMP` and `TEMP`. Only that child is removed, never
the caller's root or an explicit user profile.

`sandbox` defaults to true everywhere; turn it off only where the environment
cannot provide one. A sandbox failure is an error, never an unsandboxed retry.
Explicit executable or runtime paths never reuse a running profile, and a
sandboxed launch reuses one only when it verifiably runs with the sandbox on.

`browser.connect` accepts only `ws://host[:port]/devtools/browser/<id>` without
credentials, query, fragment or trailing path; HTTP discovery URLs, page
endpoints and `wss://` are `InvalidParams`. Attachment is borrowed and leaves
page metrics untouched. Existing tabs are usable at once by target ID;
`browser.tabs` reconciles later-created or externally closed tabs. Closing,
failed initialization and EOF disconnect without closing borrowed Chrome or its
tabs; only an explicit `browser.close_tab` closes one. Proxy credentials go in
`username` and `password`, never in the server endpoint.

Held inputs are released on owned close and on borrowed disconnect. A
close-time input cleanup error does not mean the tab is still open; don't repeat
the action that preceded closing it. Forced sidecar termination cannot run Rust
destructors, so runtime files may survive a SIGKILL. Caller-owned profiles are
never cleanup targets.

### Page reads and primitives

- Evaluation: `eval` returns the JSON value and `exec` discards it, so `exec`
  suits effects that return undefined. With `function: true`, `code` is
  function source called once with the JSON `arguments`; otherwise it is an
  expression, function or statement body, as in the CLI. `raw: true` with
  `json: true` returns a raw expression's strict JSON value; `raw` alone returns
  the CLI's text rendering; `function` conflicts with `raw`, and `json` requires
  `raw`. Script failures keep the `Cdp` code and `data.cdp`.
  These primitives produce no action receipt.
- Fetch: `fetch` runs in the page with its credentials. It streams the body
  under a byte budget, cancels at the limit, skips binary MIME bodies and treats
  a zero budget as headers-only. `bytesRead` counts retained raw bytes, not
  encoded output; incomplete trailing UTF-8 is omitted. Reaching the limit
  before EOF sets `truncated`, even if the body ends there. `file:` URLs are
  refused unless `EOKA_ALLOW_FILE_URLS=1`.
- State: `page.capture_state` and `page.restore_state` carry cookies, storage,
  URL and user-agent state. Treat this data as private authentication material.
- Screenshots: viewport DPR is the only render-scale control, and full-page
  capture is subject to Chrome's image-size limits. `clip` and `full_page` are
  exclusive, and `inline` conflicts with `output` and `annotate`. An inline
  capture over 16 MiB fails with `ScreenshotTooLarge`; otherwise the image is
  published to `output` or a new temporary file. A page behind another tab is captured in
  place through a throwaway screencast on a second session, without bringing it
  to the front or changing any page's `visibilityState`.
- Viewport: `emulate` with `desktop: true` sets a desktop viewport, `reset: true`
  restores the native window dimensions and DPR (not the launch viewport), and
  neither emulates a touch device. A 640×360 CSS viewport at DPR 2 captures a
  1280×720 PNG. Changes also work on borrowed tabs and last until reset.
  Viewport changes and captures are coordinated per browser endpoint within one
  process, not across other CDP clients, so set the viewport explicitly before
  capturing a tab someone else emulated. A local timeout does not cancel the
  native command; later viewport work waits for its response.
- Held input: `mouse.down`, `mouse.move`, `mouse.up`, `key.down`, `key.up` and
  `release_all_inputs` are primitives, distinct from verified high-level actions.
- Cloudflare: `captcha.turnstile` clicks the Turnstile checkbox through its
  closed shadow root until the page leaves the challenge; a normal page returns
  zero clicks. It holds that page's request queue for up to `timeout_ms`.

JSON results are dynamic data; narrow them before use. Evaluation awaits
promises and accepts null, booleans, strings, finite numbers, arrays and plain
objects. Integer-valued numbers must be exactly representable in JavaScript.
Undefined, BigInt, functions, symbols, cycles, array holes and non-JSON object
types are rejected, never silently fabricated or dropped. A function or
expression result is limited to 1 MiB of JSON, and a strict-JSON raw result to
16,773,120 UTF-8 bytes (16 MiB minus 4 KiB, leaving room for the CDP envelope).
Evaluation checks this before returning the value over CDP and reports
`MessageTooLarge` without disconnecting Chrome.

### File artifacts and native recording

`screenshot` file output and recordings share non-overwrite publication: the
server stages an exclusive private file in the destination directory, flushes
and closes it, then publishes it atomically without replacing an existing file.
Paths are absolute, resolved once by the caller. Recordings are silent MP4 with
a Chrome-selected codec. [operations.md](operations.md#sidecar-capture-and-recording)
covers recovery by ID, stop, cancel and shutdown.

Before sealing a recording, the server validates its fragmented MP4 timing and
repairs the zero-duration runs and duplicate terminal samples of Chrome's native
AV1 encoder by borrowing ticks from following samples. Packets, sample count,
file length and the final presentation endpoint are preserved, and valid
recordings stay byte-identical; this is metadata repair, not re-encoding.
Anything it can't repair within its five-second deadline fails without
publication, leaving at most a retained partial. `frames` and `durationMs` on a
completed record come from the validated timeline.

Recording IDs are caller-known UUIDs. `fps` is a native frame-rate ceiling, not
a playback rate. One recorder runs per target; unsupported native commands or
encoders return `RecordingUnsupported`, never another recorder. A same-ID start
with equivalent parameters returns the existing record; a mismatch returns
`RecordingConflict`.

One sidecar admits at most 1,024 recording IDs and 16 MiB of cumulative
`recordingId`, `pageId` and `path` bytes. Finished records keep their capacity
and nothing is evicted; a new sidecar starts fresh. Admission checks both limits
before reserving anything and refuses with `ServerBusy`; existing-ID recovery
and `RecordingConflict` take precedence.

`completed` requires the published artifact and no failure; no other state has
an artifact. Failed records carry their stage and any retained partial, an
absolute staging path, never the destination. If shutdown expires during a
claimed commit, the record stays failed at `publish` with
`cleanupUncertain:true`: the destination may still appear, and a late commit
does not turn the failure into success.

### Durable actions and operation queries

`actions.perform` returns an admitted operation record, not a success receipt.
Clicks, typing and key actions need explicit completion expectations where the
core action contract requires them; fill, select and navigation verify
themselves. Treat an action as complete only at `outcome == "verified_complete"`.
No uncertain action is replayed. See [operations.md](operations.md) for
deduplication and recovery.

While a page's main-frame navigation waits for the site's response, Chrome
answers no other request for that page until the navigation ends or the
request's own command deadline (30 seconds, 90 behind a proxy) passes. So a
navigation operation that ends while its navigation command is unanswered (its
deadline passes, or it is cancelled after dispatch) stops the pending load, and
the page's queued requests answer again.
The page stays on its current document and the record keeps `outcome_unknown`.
A navigation that committed in time is left to finish loading. `browser.new_page`
with a `url` stops an unanswered load the same way and returns the error.
`browser.close_tab` cancels the running operations of that page only.

An action on a page behind another tab runs in place. Chrome produces no frames
for a hidden tab, but a click, hover, drag or scroll into view waits two
animation frames for its target to settle, and pointer movement is delivered
with a frame. While an action is in flight on a hidden page, the server keeps a
screenshot request open on a second session of it, which makes Chrome produce
frames, and ends it with the action. The page is not brought to the front and
sees no `visibilitychange`, but its animation frames run for the action and
stop about a second after; a page can observe that. Typed actions, `agent.step`,
catalog actions, `mouse.move`, `click_shadow` and `captcha.turnstile` all do
this. A click, hover, drag or scroll that times out in preparation on a page
that ran no animation frame says so, and whether the tab was hidden.

### Agent operations

`agent.observe` and `agent.step` use canonical agent schemas plus `pageId`.
Their presence does not grant access to unrelated CLI-only tools. Agent steps
preserve the same durable operation and outcome evidence as the other adapters.

## Type generation and clients

`clients/node/scripts/generate-protocol.mjs` turns the schema bundle into the
Node SDK's TypeScript types (`clients/node/src/generated`), the Go catalog
constants and the method table above; CI fails when any of them is stale. For
client behavior see the [Node](../clients/node/README.md) and
[Go](../clients/go/README.md) READMEs.

## Session socket

A named session outlives the CLI invocation that started it. Its daemon speaks
the same framing, envelopes, error codes and `server.hello` as `eoka-server`,
with these differences:

- **One exchange per connection.** A client connects, sends `server.hello`,
  then at most one request, reads its reply and closes. Closing early tells the
  daemon the caller left; actions that already started are not cancelled.
- **Hello.** The reply carries `protocol`, `serverVersion`, `session`, `mode`
  (`headless`, `headed` or `connect`), `configurationFingerprint`,
  `commandLimitMs` and `activity` (the running command, the queue and any
  page blocker). A hello with another protocol major is refused with
  `protocol_mismatch`, and a connection that does not start with a JSON frame
  is closed at once, so clients from other versions fail fast.
- **Methods** are catalog paths with no `pageId`: they act on the session's
  current tab. The session surface adds `tab.*`, `close` and `shutdown`,
  which `eoka-server` does not serve.
- **Errors** keep the CLI's full detail: `error.data.detail` is the
  `ErrorDetail` (code, message, hint, retryable, context), and
  `error.data.result` carries any partial result, such as an operation record.
- **Admission.** One command runs at a time; a queued one is refused with
  `daemon_busy` after a bounded wait (see
  [the CLI guide](cli.md#blocked-and-busy-sessions)). `dialog.accept` and
  `dialog.dismiss` skip the queue while a dialog blocks the tab.

`crates/eoka-protocol/tests/locks/session-wire.jsonl` locks the frames.
