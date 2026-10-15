# eoka-server wire protocol

`eoka-server` is a private subprocess wrapping the `eoka` Rust browser library.
It accepts newline-delimited JSON on stdin and returns newline-delimited JSON on
stdout. It is not the CLI daemon and does not expose every CLI/MCP/Tack tool.

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
- At most 128 requests may remain in flight. Respect advertised limits and
  stdout backpressure; an unread pipe is not permission to buffer indefinitely.
- Request IDs are positive integers through 9,007,199,254,740,991, unique among
  in-flight requests. Responses echo the ID. A malformed message without a
  recoverable valid ID may receive an error with `id: null`.
- Ordinary page requests and action admission retain serialized dispatch.
  Operation waiters and list snapshots run independently. Get/cancel for an
  already-admitted operation also bypass a blocked ordinary request; unknown IDs
  fall back behind earlier queued admissions. List returns the current admitted
  snapshot, not a barrier for queued mutations. Controls still consume the
  advertised in-flight budget; shutdown retains its separate admission.
  Responses can arrive out of order. Always correlate by ID.
- Local request timeout or connection loss does not prove a browser action was
  cancelled. Durable operation IDs remain the recovery mechanism.

## Negotiation

Before launching Chrome, clients send:

```json
{"id":1,"method":"server.hello","params":{"protocol":{"major":1,"minor":3}}}
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

Protocol major 1/minor 3 is supported, including compatible 1.0/1.1/1.2 requests. A
different major or a requested minor newer than the server supports returns
`ProtocolMismatch` without launching a browser. Existing low-level clients can
still call compatible methods directly; the Go client requires protocol 1.1
before launch or attachment and fails explicitly against older servers.
The Node SDK requires protocol 1.3, including bounded browser fetch and capture contracts.

`durableActions`, `operationCancellation` and `gracefulShutdown` describe server
behavior. `explicitExecutablePath`, `browserAttachment`, `runtimeIsolation`,
`sandboxControl` and `viewportEmulation` advertise the managed session methods
below. `boundedFetch` advertises streaming response-body inspection with a
sidecar-enforced byte budget. `screenshotOptions`, `screenshotFile` and
`nativeRecording` advertise the capture methods below, not a guarantee that an
attached Chrome build supports a format or its native encoder. Browser support
is checked at runtime; no capability implies managed Chrome installation.

## Message shapes

Request:

```json
{"id":2,"method":"page.title","params":{"pageId":"ABCD1234"}}
```

Success:

```json
{"id":2,"result":{"title":"Example"}}
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
protocol error or canonical error detail. Branch on codes and fields, not text.
An absent receipt is not proof that nothing was dispatched.

Invalid request envelopes/parameters, unknown methods, incompatible protocols,
message-size violations, overload and shutdown are explicit protocol errors.
Core errors retain their established element, timeout and CDP classifications;
structured action evidence must not be discarded into a message-only error.

## Method families

Use the generated schema bundle for exact fields, requiredness, bounds and
method-specific results:

```sh
cargo run --quiet --locked -p eoka-protocol --bin export-sidecar-schema
```

This is the sidecar registry, not the tool catalog used by the CLI daemon.
Wire names and existing result shapes remain stable across canonicalization.

### Server lifecycle

| Method | Parameters | Result |
| --- | --- | --- |
| `server.hello` | `{protocol: {major, minor}}` | Version/capability/limit metadata |
| `server.shutdown` | `{}` | `{}` after bounded cleanup, then process exit |

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

| Method | Parameters | Result |
| --- | --- | --- |
| `browser.launch` | Headless flag; optional executable/runtime paths, user agent, timezone, paired CSS viewport dimensions, DPR, user-data directory, sandbox policy and proxy | `{ownership, browserVersion}` |
| `browser.connect` | `{wsEndpoint}` (browser DevTools WebSocket endpoint) | `{ownership: "borrowed", browserVersion}` |
| `browser.new_page` | Optional `url` | `{pageId}` |
| `browser.tabs` | `{}` | `{tabs: [{id, title, url}]}` |
| `browser.close_tab` | `{pageId}` | `{}` or `{cleanupError}` after a tab closed but input cleanup failed |
| `browser.close` | `{}` | Browser cleanup response, then process exit |

Launch uses camelCase fields: `headless`, `userAgent`, `timezone`,
`viewportWidth`, `viewportHeight`, `deviceScaleFactor`, `executablePath`,
`runtimeDir`, `sandbox`, `userDataDir`, and `proxy`. Width and height must be
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

The low-level sidecar and existing Go SDK default `sandbox` to false for
compatibility; the Node SDK requests true by default. A requested sandbox
failure is an error, not an automatic unsandboxed retry. Explicit
executable/runtime/sandbox policy cannot silently reuse a
running profile whose identity or policy cannot be guaranteed.

`browser.connect` accepts only `ws://host[:port]/devtools/browser/<id>` without
credentials, query, fragment or trailing path components. HTTP discovery URLs,
page endpoints and `wss://` are rejected with `InvalidParams`; this build does
not enable WebSocket TLS. Attachment is borrowed and leaves existing page
metrics untouched. Existing page target IDs can be used immediately;
`browser.tabs` reconciles handles for later-created or externally closed tabs.
Closing the session, failed initialization, EOF and startup shutdown disconnect
without closing borrowed Chrome or its tabs. Explicit `page.close` and
`browser.close_tab` still intentionally close the requested tab.

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

- Reads: `page.text`, `page.content`, `page.title`, `page.url`, `page.get_text`,
  `page.get_attribute`, `page.exists`.
- Waits: `page.wait_for`, `page.wait_for_visible`, `page.wait_for_text`.
- Evaluation: `page.evaluate` returns `{result: <JSON value>}`;
  `page.execute` discards the result. These are raw JavaScript primitives,
  not action-receipt-producing operations and not closure serialization.
- Fetch: `page.fetch` performs browser-context fetch with page credentials and
  preserves its existing result shape. The protocol response limit does not
  turn this legacy method's `response.text()` into a bounded streaming download.
- Bounded inspection: `page.fetch_bounded` accepts the fetch fields plus
  `maxBytes` (default 8192, maximum 1,048,576 raw body bytes). It streams through
  core bounded fetch, cancels at the retention limit, skips binary MIME bodies,
  and treats zero as headers-only. The sidecar's smaller cap leaves headroom for
  JSON escaping within CDP frames; it does not change the core library's limit.
  Results include `url`, `status`, `statusText`, `type`, `redirected`, `headers`,
  `body`, `truncated`, `bytesRead`, and nullable `bodyOmitted`
  (`binary`, `headers_only`, or `opaque`). Incomplete trailing UTF-8 is omitted;
  `bytesRead` counts retained raw bytes, not encoded output length. Reaching the
  limit before observing EOF sets `truncated`, even if the body ends there.
- State: `page.capture_state` and `page.restore_state` carry cookies, storage,
  URL and user-agent state. Treat this data as private authentication material.
- Screenshots: `page.screenshot` preserves `{dataBase64}` and defaults to PNG.
  It accepts `{pageId, format?, quality?, clip?, fullPage?}`. Formats are `png`,
  `jpeg` and `webp`; quality is an integer from 0 through 100 for JPEG/WebP
  only. Clip `{x,y,width,height}` uses finite CSS pixels, nonnegative origins
  and positive sizes. Clip and full-page capture are mutually exclusive.
  Viewport DPR is the sole render-scale control. Full-page capture uses CSS
  layout dimensions and remains subject to Chrome image-size limits; dynamic
  layout can change between captures. Raw in-memory output is capped at 16 MiB
  and must fit the negotiated JSON message limit. `ScreenshotTooLarge` directs
  callers to `page.screenshot_to_file`, which accepts the same capture fields
  plus an absolute `path` and returns artifact metadata without base64.
- Viewport: `page.set_viewport` takes `{pageId, width, height, deviceScaleFactor?}`
  with positive CSS dimensions up to 2,147,483,647 and finite positive DPR
  (default 1), returning `{}`. `page.reset_viewport` takes `{pageId}` and returns
  `{}` after clearing the override, restoring native window content dimensions
  and DPR rather than the configured launch viewport. A 640×360 CSS viewport at
  DPR 2 captures a 1280×720 PNG. Explicit changes also work on borrowed tabs and
  remain in effect until reset; disconnect does not silently alter page metrics.
  Fractional host display scaling can quantize native CSS dimensions and DPR;
  measured DOM layout remains authoritative. Eoka coordinates viewport changes
  and captures across handles for the same browser endpoint within this process,
  not independent CDP clients or other processes. Set the desired viewport
  explicitly before capturing externally emulated tabs. A local timeout is not
  native command cancellation: subsequent viewport work waits for the outstanding
  native response or transport/session teardown, subject to its own deadline.
  Native capture/reset can refresh Chrome's hidden-tab `outerWidth`/`outerHeight`
  readings without resizing the browser window; Eoka does not mask those values.
- Held input: `page.mouse_down`, `page.mouse_move`, `page.mouse_up`,
  `page.key_down`, `page.key_up`, `page.release_all_inputs` are primitives,
  distinct from verified high-level actions.
- Other implemented methods include `page.solve_captcha` and `page.close`.

JSON results are dynamic data, not statically known application types. Type
consumers must narrow them before use. Evaluation awaits promises and accepts
null, booleans, strings, finite numbers, arrays and plain objects. Integer-valued
numbers must be exactly representable in JavaScript. Undefined, BigInt,
functions, symbols, cycles, array holes and non-JSON object types are rejected
instead of silently fabricated or dropped. `page.execute` discards its return
value and is the appropriate primitive for effects returning undefined.
The serialized evaluation value is additionally limited to 16,773,120 UTF-8
bytes (16 MiB minus 4 KiB), leaving room for the CDP response envelope below
the core transport's frame limit. Evaluation checks this before returning
the value over CDP and reports `MessageTooLarge` without disconnecting Chrome.
This per-method ceiling is lower than the sidecar's 32 MiB message ceiling.

### File artifacts and native recording

`page.screenshot_to_file` and recordings share non-overwrite file publication.
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

| Method | Parameters | Result |
| --- | --- | --- |
| `recording.start` | `{pageId,recordingId,path,maxWidth?,maxHeight?,fps?}` | Recording record |
| `recording.get` | `{recordingId}` | Recording record |
| `recording.stop` | `{recordingId}` | Terminal recording record |
| `recording.cancel` | `{recordingId}` | Terminal recording record |

IDs are caller-known UUIDs, allocated before dispatch. Dimensions are positive
integers through 2,147,483,647; `fps` is an integer from 1 through 120, a native
frame-rate ceiling rather than a deterministic playback rate. Capture requires a
ready active page and allows one recorder per target. Unsupported native
commands or encoders return `RecordingUnsupported`, never another recorder.

Every record contains `{recordingId,pageId,state,path,artifact?,failure?}`.
States are `running`, `finalizing`, `completed`, `cancelled` and `failed`.
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

The sidecar also routes `page.goto`, `page.click`, `page.click_text`,
`page.human_click`, `page.human_click_text`, `page.fill`, `page.human_fill`,
`page.type_into`, `page.select`, `page.hover` and `page.press_key` through the
same operation manager. These methods require an operation ID and return an
operation record, not the empty objects documented by older protocol examples.

| Method | Parameters | Result |
| --- | --- | --- |
| `operations.get` | `{id}` | Operation record |
| `operations.list` | Optional `before_ms`, `limit` | Array of operation records |
| `operations.wait` | `{id, timeout_ms?}` | Latest operation record; may still be running at timeout |
| `operations.cancel` | `{id}` | Cancellation request result/operation record; inspect subsequent state |

Clicks, typing and key actions require explicit completion expectations where
the core action contract requires them. Fill/select/navigation retain intrinsic
verification. Check `outcome == "verified_complete"` before treating an action
as complete. A record can instead report `not_dispatched` or `outcome_unknown`.

No uncertain action is automatically replayed. Local waiter cancellation,
transport loss and shutdown remain separate from the site's business outcome.
See [OPERATIONS.md](OPERATIONS.md) for deduplication and durable recovery.

### Agent operations

`agent.observe` and `agent.step` use canonical agent schemas plus `pageId`.
Their presence does not grant access to unrelated CLI-only tools. Agent steps
preserve the same durable operation and outcome evidence as the other adapters.

## Type generation and clients

The Rust schema exporter provides canonical request/result maps, including
dynamic JSON and operation arrays. Consumers must not flatten every method into
an object-only result contract.

Go errors preserve structured server data and caller-known operation IDs.
Go launch negotiates compatibility before opening Chrome, and close requests
`server.shutdown`. A previously downloaded server without the new handshake is
rejected with compatible-binary guidance; do not assume an unreleased server
artifact exists simply because client source changed.
