# eoka Go client

A Go client for [`eoka`](https://github.com/cbxss/eoka), a Rust stealth
Chrome-automation library, via its `eoka-server` sidecar process. This
package spawns `eoka-server` as a child process and drives it over
newline-delimited JSON on stdin/stdout, per the contract in
[`PROTOCOL.md`](../../PROTOCOL.md).

## Install

```
go get github.com/shrimp-software/eoka-tools/clients/go
```

Import it as `eoka`:

```go
import eoka "github.com/shrimp-software/eoka-tools/clients/go"
```

## Prerequisites

You need a Chrome/Chromium install and an `eoka-server` binary implementing
sidecar protocol **1.1** or a newer compatible 1.x minor. The client locates the binary in this order:

1. `eoka.WithServerPath("/path/to/eoka-server")` passed to `Launch` or `Connect`, if given.
2. The `EOKA_SERVER_BIN` environment variable, if set.
3. `eoka-server` on `PATH`.
4. A prebuilt binary downloaded from this repo's GitHub Releases, still pinned to
   [`eoka-server-v0.3.0`](https://github.com/shrimp-software/eoka-tools/releases/tag/eoka-server-v0.3.0).
   Assets are available for linux/amd64, linux/arm64, darwin/amd64, darwin/arm64
   and windows/amd64. The binary is cached under `os.UserCacheDir()/eoka/` and
   verified against the release's published SHA-256 checksums before installation.
   Asset availability is not a cross-platform browser compatibility guarantee.
   Pass `eoka.WithNoAutoDownload()` to disable this and get
   `eoka.ErrServerBinaryNotFound` if steps 1–3 do not resolve.

The pinned 0.3.0 release predates the 1.1 handshake and is **not compatible**
with this client. It fails with `ProtocolMismatch` before browser creation,
including guidance to provide a compatible binary. The resolver does not invent
a newer release or fall back to launching an unverified protocol.
For this development checkout, run `cargo build -p eoka-server --release`
from this repo and use `eoka.WithServerPath` or `EOKA_SERVER_BIN` to select
the resulting server.

## Action migration

`Launch` and `Connect` send `server.hello` with protocol 1.1 before creating
a browser session. The client requires major 1 and minor >= 1; server/core
package versions are diagnostic information, not substitutes for protocol
compatibility. `Browser.ServerInfo()` returns the negotiated versions, supported
methods, capabilities and server limits. Missing required launch or connect support,
shutdown support or valid session metadata fails with cleanup of the private sidecar.
Every convenience API requires its method to be advertised before transmission.
Optional capabilities are interpreted: durable actions/recovery require
`durableActions`; explicit operation cancellation requires `operationCancellation`.

Protocol 1.3 servers remain compatible with these 1.1 conveniences:
`Page.Screenshot(ctx)` still requests default PNG and decodes the unchanged
`{dataBase64}` result. The server bounds raw screenshots at 16 MiB and returns
`ErrCodeScreenshotTooLarge` with direct-file guidance. Typed screenshot options,
direct file capture and recording handles are exposed by the Node SDK, not new
Go conveniences. Go errors preserve raw `Error.Data`, including recording ID,
terminal record, failure stage, retained partial path and cleanup uncertainty;
stable recording/file error constants are available for classification.

Click/type/key methods accept `ActionOptions{Expect: ...}` and require a fresh
observable completion condition. Use `NewOperationID` plus `Page.Perform` to retain
the ID before transmission; `Operation` and `WaitOperation` recover the record after
a lost response. Actions are never replayed automatically.
See [OPERATIONS.md](../../OPERATIONS.md) for JSON expectations and durable recovery.

## Usage

```go
ctx := context.Background()

b, err := eoka.Launch(ctx, eoka.WithProxy("socks5://user:password@host:1080"))
if err != nil {
    log.Fatal(err)
}
defer b.Close(ctx)

page, err := b.NewPage(ctx, "https://your-app.example/form")
if err != nil {
    log.Fatal(err)
}

if err := page.Fill(ctx, "#user", "bob"); err != nil {
    log.Fatal(err)
}
if err := page.Click(ctx, "#submit", eoka.ActionOptions{Expect: map[string]any{
    "kind": "text",
    "target": map[string]any{"query": map[string]any{"kind": "css", "selector": "#status"}},
    "value": "Saved",
}}); err != nil {
    log.Fatal(err)
}

text, err := page.Text(ctx)
png, err := page.Screenshot(ctx)
```

`CaptureState` and `RestoreState` preserve a complete authenticated browser
session. They include CDP cookies (including HttpOnly cookies), local storage,
session storage, the user agent, and the current URL. The SDK returns the
typed state; applications should persist it with owner-only permissions.

`WithProxy` accepts authenticated `socks5://` and `http://` URLs. Credentials
are split from the browser proxy endpoint before the JSON-RPC request and are
never included in client validation errors.

See [`example/main.go`](example/main.go) for a full runnable program
(`go run ./example`) — it requires a real `eoka-server` binary and Chrome,
so it is not part of `go test`.

## Managed launch options

`Launch` remains headless by default. `WithVisible()` or `WithHeadless(false)`
requests a visible browser. Existing user-agent, timezone, proxy, profile and
viewport options remain available.

| Option | Behavior |
| --- | --- |
| `WithExecutablePath(path)` | Select an absolute Chrome executable path; an invalid explicit executable fails without discovery or fallback. |
| `WithRuntimeDir(path)` | Use an absolute writable runtime root for private per-browser temporary files. The root remains caller-owned; only the owned unique child is removed. |
| `WithSandbox(enabled)` | Explicit sandbox policy. The legacy Go/Rust/sidecar default remains `false`; pass `true` to request sandboxing. Unsupported sandbox environments fail without retrying unsandboxed. The planned managed Node SDK will request `true` explicitly. |
| `WithViewport(width, height)` | CSS viewport dimensions, both positive and at most 2147483647. |
| `WithDeviceScaleFactor(scale)` | Finite positive DPR, effective default 1. It may be set without supplying dimensions. |
| `WithAutoSolveCloudflare(interval, timeout)` | Have the server watch every tab and solve full-page Cloudflare challenges. Zero durations use 500ms and 30s. Also accepted by `Connect`; requires the `cloudflareAutoSolve` capability. |

Executable/runtime paths, dimensions and DPR are validated before downloading or
starting the sidecar. Runtime storage is separate from the installed-browser
cache and durable operation store. An explicit `WithUserDataDir` profile is
caller-owned and is not deleted on shutdown. Explicit executable/runtime/sandbox
requests that cannot be honored by reusing an active profile fail rather than
silently ignoring the requested policy.

```go
b, err := eoka.Launch(ctx,
    eoka.WithServerPath("/absolute/path/to/eoka-server"),
    eoka.WithExecutablePath("/absolute/path/to/chrome"),
    eoka.WithRuntimeDir("/absolute/path/to/private-runtime"),
    eoka.WithSandbox(true),
    eoka.WithViewport(640, 360),
    eoka.WithDeviceScaleFactor(2),
)
```

`Page.SolveTurnstile(ctx, timeout)` clicks a Cloudflare Turnstile checkbox
until the page leaves the challenge and returns `TurnstileResult{Clicks, ElapsedMs}`;
a normal page returns zero clicks. A zero timeout uses 30s.

`Browser.SessionInfo()` returns `BrowserSessionInfo{Ownership, BrowserVersion}`.
Ownership is `BrowserOwnershipOwned` (`"owned"`) or
`BrowserOwnershipBorrowed` (`"borrowed"`), as reported by the actual session.
Launch can be borrowed when a legacy launch reuses an existing profile.
`BrowserVersion` is Chrome's actual CDP product/version, not a spoofed user agent.

## Attach to existing Chrome

`Connect` starts a private sidecar and attaches to a separately running Chrome.
It requires a `ws://host:port/devtools/browser/<id>` browser endpoint; HTTP URLs,
page endpoints and TLS `wss://` endpoints are not supported. Obtain the browser
endpoint from Chrome's DevTools discovery output.

```go
b, err := eoka.Connect(ctx, browserWSEndpoint,
    eoka.WithServerPath("/absolute/path/to/eoka-server"),
)
if err != nil {
    log.Fatal(err)
}
defer b.Close(ctx)

tabs, err := b.Tabs(ctx)
if err != nil {
    log.Fatal(err)
}
if len(tabs) != 0 {
    page := b.Page(tabs[0].ID)
    text, err := page.Text(ctx)
    fmt.Println(text, err)
}
```

Existing page IDs come from `Tabs`. `Browser.Page(pageID)` constructs a local
handle without opening a tab or changing its metrics; the server validates the
ID when a page method is called. Attachment does not alter existing viewports.
`Connect` always reports borrowed ownership. Closing the Go session disconnects
and reaps its sidecar without closing external Chrome or deleting the user profile.
`CloseTab` and `Page.Close` remain deliberate destructive tab operations.

Only `WithServerPath`, `WithStderr` and `WithNoAutoDownload` are accepted by
`Connect`. All launch-only options are rejected before starting a sidecar, even
when an option supplies the default value.

## Page viewport control

`Page.SetViewport(ctx, width, height, scale)` explicitly changes CSS dimensions
and DPR on either a launched or borrowed page. Width/height must be positive and
at most 2147483647; scale must be finite and positive. Invalid values are rejected
locally before sending a request. Both page viewport methods require advertised
server support.

```go
if err := page.SetViewport(ctx, 640, 360, 2); err != nil {
    log.Fatal(err)
}
png, err := page.Screenshot(ctx)
if err := page.ResetViewport(ctx); err != nil {
    log.Fatal(err)
}
```

A 640×360 CSS viewport at DPR 2 produces a 1280×720 viewport PNG.
`ResetViewport` clears emulation and restores native window content dimensions
and DPR, not the configured launch override.

## Errors

Protocol errors from `eoka-server` are `*eoka.Error`, with `Code`, `Message`
and optional `Data` (`json.RawMessage`). `Data` retains canonical error details,
operation IDs, action receipts and nested CDP errors without flattening them.
Codes include `ProtocolMismatch`, `MessageTooLarge`, `ServerBusy`, `ShuttingDown`,
`Cancelled`, `Action`, `Operation` and `Evaluation` in addition to the existing
page errors. Local capability rejection uses `CapabilityUnavailable`:

```go
_, err := page.Text(ctx)
var eErr *eoka.Error
if errors.As(err, &eErr) {
    fmt.Println(eErr.Code, eErr.Message, string(eErr.Data))
}
```

Transport-level failures (the server process died, its pipes closed, an invalid
response, unknown/duplicate response ID or mid-message EOF) fail the transport and
reject pending and future calls. Message-limit errors retain `MessageTooLarge`.
The transport correlates replies by safe positive integer IDs, including out-of-order
operation controls, and accepts JSON results including arrays, scalars and null.
Messages are bounded to the smaller of the server's limit and 32 MiB, excluding the
newline. Outstanding requests are bounded to the smaller of the server's limit and
128; cancelling a sent request does not free its remote slot until its reply arrives.

Every method takes a `context.Context`. Cancellation abandons the local waiter;
it does **not** send `operations.cancel` or stop a remote action. A late reply for
that abandoned request is discarded without poisoning unrelated calls.
`*eoka.OperationError` retains `OperationID`, `Record`, `Receipt` when available and
an `Uncertain` flag, and unwraps the original context/protocol/transport error.
Inspect the operation/receipt before another action; a lost response is not proof
that nothing happened. Use `CancelOperation` explicitly for remote cancellation.
`Browser.CloseTab` returns an error when held-input cleanup fails after the tab has
already closed; treat the tab as closed in that case.

## Shutdown

`Browser.Close` sends `server.shutdown`, rather than treating `browser.close` as
process teardown. It is idempotent: concurrent/repeated calls share one shutdown
and one process reap. Failed or cancelled launch/attachment also requests shutdown
before returning an error.

Cleanup has a 45-second hard grace, allowing the server's protected launch and
owned-Chrome cleanup to finish before forced sidecar termination. An earlier
`Close` caller-context cancellation returns `ctx.Err()` **locally**, while owned
cleanup continues in the background until completion or the hard grace; call
`Close` again with a live context to wait for that same cleanup. Launch/attachment
failure waits for this bounded cleanup even if its context was cancelled.
After the sidecar exits, inherited stderr-pipe draining has a separate one-second
bound so a descendant holding that pipe cannot make process reaping wait forever.
Borrowed Chrome remains external to sidecar process ownership. Graceful shutdown,
EOF and normal failure can run cleanup; forced sidecar termination or an abrupt
SIGKILL cannot guarantee that Rust destructors run or owned temporary files are
removed. Do not infer cleanup success after a hard termination.

## Testing

`go test ./...` requires neither Chrome nor a real `eoka-server`. Most tests use
an in-memory `io.Pipe` server; lifecycle tests launch the Go test executable as a
protocol fixture to exercise actual process shutdown/reaping. Coverage includes
handshake rejection, capability gating, out-of-order waits, split UTF-8 and partial
lines, malformed/unknown/duplicate IDs, message bounds, safe-ID exhaustion,
local cancellation and operation recovery without replay, preserved structured
errors, managed option validation, launch/attachment ownership metadata, existing
tab access, viewport request validation and method gating, cleanup after
failed/cancelled launch or attachment and in-flight waits, and forced
termination/reaping of an unresponsive sidecar. These fixtures do not prove real
Chrome ownership or screenshot dimensions; those require separate browser smoke
coverage.
Use `go test -race ./...` for concurrent transport/lifecycle coverage.
