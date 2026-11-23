# eoka wire protocol

One wire protocol, newline-delimited JSON with `{id, method, params}` requests
and `{id, result}` or `{id, error}` replies, has two transports:

- **stdio:** `eoka-server` is a private subprocess wrapping the `eoka` Rust
  browser library. It reads requests on stdin and writes replies on stdout. The
  Node and Go clients use it. Most of this document describes this transport.
- **Session socket:** the CLI's per-session daemon listens on a Unix socket
  (`$XDG_RUNTIME_DIR/eoka/eoka-<session>.sock`). The CLI and `eoka-sdk` use
  it. See [Session socket](#session-socket).

The canonical sidecar envelopes, method parameters, results and schemas live in
`eoka-protocol::sidecar`. Server handlers and generated TypeScript definitions
consume those contracts. The Go client implements the same wire protocol.

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
- Requests are dispatched per page. Requests that name the same `pageId`,
  including action admission, catalog operations and `browser.close_tab`, run
  one at a time in the order received. Requests for different pages run
  concurrently, so a page whose request is slow or never answers delays only
  its own later requests. `browser.new_page` and `browser.tabs` belong to no
  page and are never queued behind one. `browser.launch`, `browser.connect` and
  `browser.close` are barriers: each waits for every request received before it
  and holds back every request received after it until it finishes.
- Requests that clear a blocked page do not wait for that page's queue:
  `dialog.accept` and `dialog.dismiss` always start at once, and
  `browser.close_tab` starts at once while the page has an open dialog or a
  pending navigation. Requests received after them for the same page still
  wait for them.
- Catalog operations that act on one page alone run on that page: the reads
  and waits, held input, `click_shadow`, `captcha.turnstile`, `eval`, `exec`,
  `fetch`, `screenshot` without `annotate`, and `emulate` with `desktop` or
  `reset`. The other catalog operations share one engine and run one at a time
  across pages, so a slow one delays only those operations on other pages.
  `agent.observe` and `agent.step` likewise share one agent session.
- Operation waiters and list snapshots run independently. Get/cancel for an
  already-admitted operation also bypass a blocked page; an unknown ID waits
  only for an earlier queued request carrying the same operation ID. List
  returns the current admitted snapshot, not a barrier for queued mutations.
  Controls still consume the advertised in-flight budget; shutdown retains its
  separate admission. Responses can arrive out of order. Always correlate by ID.
- Local request timeout or connection loss does not prove a browser action was
  cancelled. Durable operation IDs remain the recovery mechanism.

## Negotiation

Before launching Chrome, clients send:

```json
{"id":1,"method":"server.hello","params":{"protocol":{"major":4,"minor":0}}}
```

The result contains:

| Field | Meaning |
| --- | --- |
| `protocol` | Supported `{major, minor}` wire version |
| `serverVersion` | Version of the running server build |
| `coreVersion` | Core Eoka dependency resolved for that build |
| `methods` | Actually implemented sidecar method names |
| `capabilities` | Implemented sidecar capabilities, not inferred Chrome capabilities |
| `limits.maxMessageBytes` | Maximum serialized message size |
| `limits.maxInFlight` | Maximum in-flight requests |

The server speaks protocol 4.0. A hello naming another major, or a minor newer
than the server's, returns `ProtocolMismatch` without launching a browser. There
is no negotiation and no fallback: the Node and Go SDKs ship pinned to the
matching server and send exactly `{major: 4, minor: 0}`. Protocol 4.0 removed
the `page.*` methods that duplicated catalog operations; their callers use the
catalog operations described below.

`durableActions`, `operationCancellation` and `gracefulShutdown` describe server
behavior. `explicitExecutablePath`, `browserAttachment`, `runtimeIsolation`,
`sandboxControl` and `viewportEmulation` advertise the managed session methods
below. `boundedFetch` advertises streaming response-body inspection with a
sidecar-enforced byte budget. `screenshotOptions`, `screenshotFile` and
`nativeRecording` advertise the capture methods below, not a guarantee that an
attached Chrome build supports a format or its native encoder. `cloudflareAutoSolve`
advertises the `autoSolveCloudflare` option on `browser.launch` and
`browser.connect`: `{intervalMs?, timeoutMs?}` (defaults 500 and 30000) starts a
server-side watcher that solves full-page Cloudflare challenges in every tab until
the browser is closed. Browser support
is checked at runtime; no capability implies managed Chrome installation.

## Message shapes

Request:

```json
{"id":2,"method":"info","params":{"pageId":"ABCD1234"}}
```

Success:

```json
{"id":2,"result":{"url":"https://example.com/","title":"Example"}}
```

Error:

```json
{"id":2,"error":{"code":"InvalidPage","message":"unknown pageId"}}
```

Exactly one of `result` or `error` is present. A successful null result is still
`"result": null`; it must not disappear during serialization. Result types are
method-specific: `operations.list`, for example, returns an array directly.

Errors retain stable `code` and human-readable `message`. Optional `data`
contains structured evidence such as an operation ID, action receipt, original
protocol error or canonical error detail. A failed catalog operation always
carries its canonical detail in `data.detail`, including the `hint` naming
the next step. Branch on codes and fields, not text.
An absent receipt is not proof that nothing was dispatched.

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

`data.detail` carries the catalog `ErrorDetail` with code `dialog_open`,
`navigation_pending` or `page_unresponsive`, the same message, a `hint` naming
the remedy and, for a request that was not started, `context.action_status`.
`elapsedMs` is how long the dialog has been open or the navigation has waited.

Registered page methods and catalog operations report these states the same
way:

- A request for a page with an open dialog is refused before it starts. So is
  a request for a page whose navigation has waited five seconds, except waits
  and captures, which run, and navigations, which first stop the pending load
  and then proceed; that is how a fresh navigate action recovers
  a stalled tab.
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
  while any other request is blocked. `release_all_inputs`,
  `emulate`, `page.restore_state`,
  `captcha.turnstile`, `recording.*` and tab close are never refused.
- An operation that is running when a dialog opens on its page ends: its record
  is `finished` with `error_code` `dialog_open`, `outcome` `outcome_unknown`
  when input had been dispatched (`not_dispatched` otherwise) and a receipt
  whose `dispatch` is `may_have_been_sent` or `not_sent`. Nothing is replayed.
  Input the interrupted action still held is released before the next action
  on that page.
- A dialog that was already open before the server attached to the page is not
  known to it; such a page reports `PageUnresponsive`, and `dialog.dismiss`
  clears it.

Invalid request envelopes/parameters, unknown methods, incompatible protocols,
message-size violations, overload and shutdown are explicit protocol errors.
Core errors retain their established element, timeout and CDP classifications;
structured action evidence must not be discarded into a message-only error.

## Methods

The registry has two parts: the methods registered in the
`define_sidecar_methods!` table in `eoka-protocol::sidecar`, and the catalog
operations. The schema bundle has every method's exact fields, bounds and
result:

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
named by its catalog path (`console`, `cookies`, `network.record.start`,
`intercept.add`, `download.start`, ...), executed by the same engine as the CLI
daemon. The schema bundle lists them under `catalogMethods`.

- Params are the operation's catalog input plus a required string `pageId`.
  Catalog fields are snake_case. An unknown field is `InvalidParams`, not ignored.
- The result is the operation's `data` value. Every operation declares its result
  type in the schema bundle. Only `eval` is untyped, because its result is the
  value the page's JavaScript returned. Some results take more than one shape:
  `emulate`, `observe`, `scroll`, `screenshot`, `storage` and `fetch` are
  unions, and action results are the operation record plus its `status`.
- Page reads, waits, held input, `click_shadow` and `captcha.turnstile` run on
  their page alone: a slow request on one page never delays them on another.
- Failures return an `Operation` error carrying the catalog `ErrorDetail`.
- Actions (`click`, `double_click`, `fill`, `select`, `hover`, `key`, `scroll`,
  `back`, `forward`, `reload`) are durable operations. `operation_id` is
  optional and generated when omitted. An admitted action returns its operation
  record as the result even when the outcome is unverified or failed, like
  `actions.perform`; read `outcome` and `error` on the record.
- Target references from `agent.observe` resolve in catalog actions.
- `open` navigates the addressed page, on launched and connected browsers.
- Session lifecycle and tab management are not projected: use `server.shutdown`,
  `browser.close`, `browser.new_page`, `browser.tabs` and `browser.close_tab`.
- A catalog path that already names a registered method keeps that method's
  contract.
- Errors raised by the core library keep their kind: a catalog operation that
  times out returns `Timeout`, a missing element returns `ElementNotFound`, the
  same codes a registered method returns.

Some catalog state belongs to the browser session rather than one page: the
network recorder and its log, the download slot, and the JavaScript policy
(`js.*`, which applies to navigations made through catalog `open` and `reload`). Interception
rules are re-applied to whichever page a catalog call addresses. Catalog actions
reach a page behind another tab the same way `actions.perform` does (see
[Durable actions](#durable-actions-and-operation-queries)).

The catalog includes operations that read or write local files chosen by the
caller (`save_state`, `load_state`, `network.export`, `intercept.add` with
`capture` or `respond`, `fake_camera`, `download.start`, `screenshot`,
`recording.start`) and `clone_from`, which copies state from another local
Chrome. A sidecar client has the same local reach as the `eoka` CLI. Do not pass
untrusted input through as method names or file paths.

Every registered method declares a role in the `define_sidecar_methods!` table in
`eoka-protocol::sidecar`:
lifecycle, shared with a catalog operation of the same name, or distinct from
the catalog for a stated reason. No registered method duplicates a catalog
operation.

The JavaScript policy is applied by catalog `open`, `reload` and `tab.new`. It is
not applied to `back`, `forward` or a link that leaves the host, because the
destination is not known before the navigation starts.

A JavaScript dialog is reported from the event Chrome sends when it opens.
Chrome does not send that event again to a session that attaches afterwards,
and `Page.enable` on such a tab does not answer until the dialog closes. So
attaching to a tab whose dialog is already open (a browser reached with
`connect`, or a popup that alerted before anyone switched to it) times out
instead of naming the dialog.

```json
{"id":7,"method":"console","params":{"pageId":"<id>","clear":false}}
```

### Server lifecycle

Shutdown works before browser launch, after failed launch and while an ordinary
request or operation waiter is pending. The server stops admitting normal work,
settles/cancels pending requests, stops owned operations, releases held inputs
and closes owned browser resources. A cancelled waiter does not establish a
verified action outcome; inspect the durable record.

EOF on stdin follows the same cleanup path. Successful cleanup exits zero;
cleanup failure is not reported as success. Shutdown is bounded, including
response flushing. During Chrome launch or attachment, allow up to 35 seconds
for startup/admission and its child guard, then up to 5 seconds of browser cleanup and
2 seconds of output flushing. The Go client's hard teardown grace is 45 seconds.
Ordinary shutdown does not spend the launch grace. Cancelling a Go close waiter
does not abandon the shared background teardown or immediately kill the sidecar.

### Browser

Launch fields are camelCase. `viewportWidth` and `viewportHeight` must be
supplied together, positive and no greater than 2,147,483,647 CSS pixels.
`deviceScaleFactor` must be finite and positive; it defaults to 1 and can be
supplied without dimensions to scale the default viewport. Explicit executable,
runtime and user-data paths must be absolute. Missing, unreadable or
non-executable explicit Chrome paths fail without discovering a substitute.
`browserVersion` is the actual CDP `Browser.getVersion` product, not a spoofed
user-agent string. `ownership` is `"owned"` only when the handle owns Chrome;
legacy reuse of a running profile reports `"borrowed"` instead.

`runtimeDir` selects a writable root for a private per-browser child containing
owned temporary profiles, patch artifacts and Chrome temporary files. It is
independent of the installed-browser cache and durable operation store. Chrome
receives child-local `TMPDIR`, `TMP` and `TEMP`; the sidecar's global environment
is unchanged. Only the owned child is removed, never the caller's root or an
explicit user profile. Source browser executables and their dependency files are
not modified.

The sidecar, Go SDK, Node SDK and CLI all default `sandbox` to true; pass
false (`--no-sandbox` / `EOKA_NO_SANDBOX` in the CLI) only where the environment
cannot provide one. A requested sandbox failure is an error, not an automatic
unsandboxed retry. Explicit executable/runtime policy cannot silently reuse a
running profile, and a sandboxed launch reuses one only when that process is
verifiably running without sandbox-disabling switches.

`browser.connect` accepts only `ws://host[:port]/devtools/browser/<id>` without
credentials, query, fragment or trailing path components. HTTP discovery URLs,
page endpoints and `wss://` are rejected with `InvalidParams`; this build does
not enable WebSocket TLS. Attachment is borrowed and leaves existing page
metrics untouched. Existing page target IDs can be used immediately;
`browser.tabs` reconciles handles for later-created or externally closed tabs.
Closing the session, failed initialization, EOF and startup shutdown disconnect
without closing borrowed Chrome or its tabs. An explicit `browser.close_tab`
still intentionally closes the requested tab.

Proxy fields are `server`, optional `username` and `password`. Do not put
credentials in the server endpoint field.

`server.shutdown` is the process-lifecycle operation, including when no browser
was successfully launched. `browser.close` retains its browser-specific meaning.
A close-time input cleanup error must not cause a client to assume the tab is
still open or repeat the action that preceded closing it.

Held inputs are released on either owned close or borrowed disconnect. Cleanup
errors retain structured failure evidence; transport loss does not prove that a
dispatched action failed. Normal close and parent EOF permit owned process and
runtime cleanup. Forced sidecar termination cannot run Rust destructors:
OS-specific process death guarantees do not promise portable deletion of
runtime files after SIGKILL. Caller-owned profiles are never cleanup targets.

### Page reads and primitives

All page methods carry `pageId`, an opaque target ID returned by the server.
Element handles do not cross this wire.

- Reads and waits are catalog operations: `text`, `content`, `info`, `get_text`,
  `get_attribute`, `exists`, `frames` and `wait` (selector `state`, text, URL,
  load or a fixed `ms`).
- Evaluation: `eval` returns the JSON value itself and `exec` discards it.
  With `function: true`, `code` is function source called once with the JSON
  `arguments`, without expression or statement wrapping. Without it, `code` is
  an expression, a function or a statement body, as in the CLI. `raw: true` with
  `json: true` evaluates `code` as a raw expression and returns its strict JSON
  value; `raw` alone returns the CLI's text rendering. `function` conflicts with
  `raw`, and `json` requires it. Script failures keep the `Cdp` code and
  `data.cdp`. These are JavaScript primitives, not action-receipt-producing
  operations and not closure serialization.
- Fetch: `fetch` performs browser-context fetch with page credentials. It
  accepts `url`, `method`, `headers`, `body`, `redirect` and `max_body`
  (default 8192, maximum 16,777,216 raw body bytes). It streams through core
  bounded fetch, cancels at the retention limit, skips binary MIME bodies, and
  treats zero as headers-only. Results include `url`, `status`, `statusText`,
  `type`, `redirected`, `headers`, `body`, `truncated`, `bytesRead`, and
  nullable `bodyOmitted` (`binary`, `headers_only`, or `opaque`). Incomplete
  trailing UTF-8 is omitted; `bytesRead` counts retained raw bytes, not encoded
  output length. Reaching the limit before observing EOF sets `truncated`, even
  if the body ends there. `file:` URLs are refused unless
  `EOKA_ALLOW_FILE_URLS=1`.
- State: `page.capture_state` and `page.restore_state` carry cookies, storage,
  URL and user-agent state. Treat this data as private authentication material.
- Screenshots: `screenshot` defaults to PNG and accepts `format`, `quality`,
  `clip` and `full_page`. Formats are `png`, `jpeg` and `webp`; quality is an
  integer from 0 through 100 for JPEG/WebP only. Clip `{x,y,width,height}` uses
  finite CSS pixels, nonnegative origins and positive sizes. Clip and full-page
  capture are mutually exclusive. Viewport DPR is the sole render-scale control.
  Full-page capture uses CSS layout dimensions and remains subject to Chrome
  image-size limits; dynamic layout can change between captures. `inline: true`
  returns `{dataBase64}`, capped at 16 MiB of image bytes; a larger capture fails
  with `ScreenshotTooLarge`. Otherwise the image is published to `output`, an
  absolute path on the sidecar, or to a new temporary file, and the result is
  `{path,size,mimeType,bytes}`. `inline` conflicts with `output` and
  `annotate`. A page behind another tab is captured in place: Chrome paints no
  frames for a hidden tab, so the server holds a throwaway screencast on a second
  session of that page for the duration of the capture. The page is not brought
  to the front, no page's `visibilityState` changes, and pages can be captured
  concurrently.
- Viewport: `emulate` with `desktop: true` takes `width`, `height` and `dpr`
  (defaults 390, 844 and 2) with positive CSS dimensions up to 2,147,483,647 and
  finite positive DPR, and returns `{width,height,dpr,mobile:false}`.
  `emulate` with `reset: true` clears the override, restoring native window
  content dimensions and DPR rather than the configured launch viewport. Without
  either flag it emulates a touch device. A 640×360 CSS viewport at DPR 2
  captures a 1280×720 PNG. Explicit changes also work on borrowed tabs and remain
  in effect until reset; disconnect does not silently alter page metrics.
  Fractional host display scaling can quantize native CSS dimensions and DPR;
  measured DOM layout remains authoritative. Eoka coordinates viewport changes
  and captures across handles for the same browser endpoint within this process,
  not independent CDP clients or other processes. Set the desired viewport
  explicitly before capturing externally emulated tabs. A local timeout is not
  native command cancellation: subsequent viewport work waits for the outstanding
  native response or transport/session teardown, subject to its own deadline.
  Native capture/reset can refresh Chrome's hidden-tab `outerWidth`/`outerHeight`
  readings without resizing the browser window; Eoka does not mask those values.
- Held input: the catalog operations `mouse.down`, `mouse.move`, `mouse.up`,
  `key.down`, `key.up` and `release_all_inputs` are primitives, distinct from
  verified high-level actions.
- Cloudflare: `captcha.turnstile` (`{pageId, timeout_ms?}`, default 30000, at
  most 120000) clicks the Turnstile checkbox through its closed shadow root until
  the page leaves the challenge and returns `{tab_id, clicks, elapsed_ms}`; a
  normal page returns zero clicks. It holds that page's request queue for up to
  `timeout_ms`.
- Other implemented methods include `page.solve_captcha`. Close a page with
  `browser.close_tab`.

JSON results are dynamic data, not statically known application types. Type
consumers must narrow them before use. Evaluation awaits promises and accepts
null, booleans, strings, finite numbers, arrays and plain objects. Integer-valued
numbers must be exactly representable in JavaScript. Undefined, BigInt,
functions, symbols, cycles, array holes and non-JSON object types are rejected
instead of silently fabricated or dropped. `exec` discards its return
value and is the appropriate primitive for effects returning undefined.
A function or expression result is limited to 1 MiB of JSON. A strict-JSON raw
result is limited to 16,773,120 UTF-8 bytes (16 MiB minus 4 KiB), leaving room for the CDP response envelope below
the core transport's frame limit. Evaluation checks this before returning
the value over CDP and reports `MessageTooLarge` without disconnecting Chrome.
This per-method ceiling is lower than the sidecar's 32 MiB message ceiling.

### File artifacts and native recording

`screenshot` file output and recordings share non-overwrite file publication.
The server receives absolute paths resolved once by the caller, stages into an
exclusive private file in the destination directory, flushes and closes it, then
atomically publishes without replacing an existing destination. An artifact is
`{path,bytes,mimeType,container?}`; `bytes` is JavaScript-safe. Screenshot
artifacts omit `container`; recording artifacts use `container:"mp4"` and
`mimeType:"video/mp4"`. Codec is Chrome-selected and is not fabricated as H.264.
No audio, FFmpeg runtime dependency, whole-video Buffer or video base64 response
is promised.

Before sealing a recording, the server validates its fragmented MP4 timing on
the original staging descriptor. For supported native AV1 zero-duration runs,
`k` explicit zeros within one fragment become `k` one-tick durations, borrowing
those ticks from the following duration only if it remains positive. Each run's
endpoint and later timestamps stay unchanged. A duplicate terminal sample in a
new fragment advances by one tick, reducing its duration and correcting the
matching fragment/index times.
Encoded packets, sample count, file length and the final presentation endpoint
are preserved. Already-valid native recordings remain byte-identical; this is
container metadata repair, not re-encoding or a raw-byte identity guarantee.
Unsupported/malformed containers, insufficient donor duration or unresolved
cross-fragment collisions fail without publication. Validation/repair has a five-second deadline;
an interrupted correction can leave an incomplete retained partial, never a
completed artifact. The subsequent flush/fsync/close and publication contract is
unchanged.

IDs are caller-known UUIDs, allocated before dispatch. Dimensions are positive
integers through 2,147,483,647; `fps` is an integer from 1 through 120, a native
frame-rate ceiling rather than a deterministic playback rate. Capture requires a
ready active page and allows one recorder per target. Unsupported native
commands or encoders return `RecordingUnsupported`, never another recorder.

Every record contains `{recordingId,pageId,state,path,artifact?,failure?,frames?,durationMs?}`.
`frames` and `durationMs` appear only on completed records and come from the
validated MP4 timeline: the sample count and the span from the first decode
timestamp to the end of the last sample, in whole milliseconds. States are `running`, `finalizing`, `completed`, `cancelled` and `failed`.
Successful start is returned only after native start; failed native admission
retains a failed record. Same-ID retries return the existing record only for
equivalent start parameters; a mismatch returns `RecordingConflict` without
restarting. Unknown IDs return `RecordingNotFound`. Recovery by `recording.get`
is session-scoped, not persistent crash recovery or automatic restart.

One sidecar admits at most 1,024 distinct recording IDs and 16 MiB of cumulative
UTF-8 bytes across admitted `recordingId`, `pageId`, and `path` values. This is a
retained-parameter budget, not an exact process-memory ceiling. Completed,
cancelled, and failed entries remain recoverable and keep their capacity.
Admission atomically checks both limits before reserving a new ID, starting a
worker, or staging a file; rejection returns `ServerBusy` with no new record.
Invalid parameters and capacity rejections consume no history capacity. Existing-ID recovery and
`RecordingConflict` take precedence over either limit. Nothing is evicted or
recycled; a new sidecar session provides fresh capacity.

Stop shares one background finalization across requests and waits for a terminal
record while get/cancel/shutdown remain responsive. `completed` requires the
published destination MP4 artifact and no failure; other states cannot contain
an artifact. Only completed stop is successful in the Node API. A lost response
is recovered by querying the same ID. Aborting or timing out a waiter does not
cancel the recorder or finalizer. Explicit cancel stops/releases owned resources.
Cancellation and publication are linearized at a short publication claim. Cancel
before the claim prevents publication; cancel after the claim waits for the
commit outcome and returns completed only if publication succeeds. Filesystem
commit runs outside the status/control lock. Cancel otherwise returns truthful
cancelled/failed evidence without a success-named partial file.

Failures contain `{stage,code,message,partialPath?,cleanupUncertain?}`. Stages are
`start`, `stop`, `read`, `write`, `flush`, `close`, `publish`, `cancel` and
`disconnect`. Failed records require failure evidence. Retained partial paths
are absolute private staging paths, never the destination success path. Disk
space/quota, stream, close and publication failures do not report success.
If bounded shutdown expires during an already-claimed commit, the record remains
failed at `publish` with `cleanupUncertain:true`: destination output may still
appear. A late commit does not upgrade that reported failure to success or attach
an artifact; retained path/message evidence reports the uncertain commit outcome.
Errors preserve optional `data.recordingId`, `data.recording`, `data.path` and
`data.partialPath`; `RecordingFailed`, `RecordingUnsupported`,
`RecordingAlreadyActive`, `RecordingInvalidState`, `RecordingConflict`,
`RecordingNotFound`, `ServerBusy`, `FileOutput` and `ScreenshotTooLarge` are stable codes.
Page/browser close cancels unfinished recordings by default; disconnect and
target loss retain honest failure and uncertain-cleanup evidence. Owned recording
cleanup never terminates a borrowed browser.

### Durable actions and operation queries

`actions.perform` takes `pageId`, a caller-known canonical UUID in
`operation_id`, a canonical `action`, and `options`. It returns an admitted
operation record, not an unconditional success receipt.

Clicks, typing and key actions require explicit completion expectations where
the core action contract requires them. Fill/select/navigation retain intrinsic
verification. Check `outcome == "verified_complete"` before treating an action
as complete. A record can instead report `not_dispatched` or `outcome_unknown`.

No uncertain action is automatically replayed. Local waiter cancellation,
transport loss and shutdown remain separate from the site's business outcome.
See [operations.md](operations.md) for deduplication and durable recovery.

Chrome answers no page-level request (`info`, evaluation,
element reads) for a page whose main-frame navigation is still waiting for the
site's response; such a request waits until the navigation commits, fails or is
stopped, or until its own command deadline (30 seconds, 90 behind a proxy). To keep that from
outliving the operation, a navigation operation that ends while its navigation
command is still unanswered (its deadline passes, or it is cancelled after
dispatch) stops the pending load. The page stays on its current document, the
record keeps `outcome_unknown`, and requests queued for the page then answer.
A navigation that committed before its deadline is left to finish loading.
`browser.new_page` with a `url` creates the page, then navigates it; when that
navigation command gets no answer the load is stopped the same way and the
error is returned.

`browser.close_tab` cancels the running operations of that page
only. Operations on other pages keep running.

An action on a page behind another tab runs in place. Chrome produces no frames
for a hidden tab, while a click, hover, drag or scroll into view waits two
animation frames to confirm its target has stopped moving, and Chrome delivers
pointer movement with a frame. For as long as an action is in flight on a hidden
page, the server therefore keeps one screenshot request open on a second session
of that page, which makes Chrome produce frames for it, and completes that
request when the action ends. The page is not brought to the front, and no
page's `visibilityState` changes or receives `visibilitychange`. The hidden
page's animation frames, and whatever its scripts and styles drive from them,
run for the duration of the action and stop within about a second of its end; a
page can observe that. A visible page is only asked for its visibility. The
typed action methods, `agent.step`, catalog actions, `mouse.move`,
`click_shadow` and `captcha.turnstile` behave the same, and a page that
becomes hidden while its action waits is covered from then on. When a click,
hover, drag or scroll into view times out in preparation on a page that ran no
animation frame, `error.message` says so and says whether the tab was hidden.

### Agent operations

`agent.observe` and `agent.step` use canonical agent schemas plus `pageId`.
Their presence does not grant access to unrelated CLI-only tools. Agent steps
preserve the same durable operation and outcome evidence as the other adapters.

## Type generation and clients

`clients/node/scripts/generate-protocol.mjs` turns the schema bundle into the
Node SDK's TypeScript types (`clients/node/src/generated`), the Go catalog
constants and the method table above; CI fails when any of them is stale.

- **Node** (`@cbxss/eoka`) runs one private `eoka-server` from its platform
  package. Local aborts and deadlines don't roll back dispatched work, and
  strict action helpers require verified receipts. When both work and cleanup
  fail, the `AggregateError` keeps the primary error, with its operation
  identity, as `cause`. See [the Node SDK README](../clients/node/README.md).
- **Go** downloads the matching `eoka-server` from the GitHub release and checks
  it against `SHA256SUMS`. It negotiates `server.hello` before launching
  Chrome, closes with `server.shutdown`, and keeps structured server errors and
  caller-known operation IDs.

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
- **Admission.** One command runs at a time. Another waits up to
  `EOKA_QUEUE_WAIT_MS` (default 5000) and is then refused with `daemon_busy`;
  `dialog.accept` and `dialog.dismiss` skip the queue while a dialog blocks
  the tab.

`crates/eoka-protocol/tests/locks/session-wire.jsonl` locks the frames.
