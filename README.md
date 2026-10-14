# eoka-tools

Companion tools for [eoka](https://github.com/shrimp-software/eoka), the low-level CDP browser automation library.

For existing AI agents, use `eoka view` and `eoka step` (MCP `agent_observe` /
`agent_step`, Tack `eoka.agent.observe` / `eoka.agent.step`). They combine compact
page state, stable control references, one durable action and fresh observations.
Exploratory input reports uncertainty without claiming task completion.
See [AGENT-BROWSING.md](AGENT-BROWSING.md) for the contract and examples.

## Durable actions (core 0.7 / CLI 0.5)

High-level actions now return durable operation records. Click and key actions
require an explicit expected outcome. Generate an operation UUID before sending,
then inspect `operations get/wait` after a lost response. See [OPERATIONS.md](OPERATIONS.md)
for CLI, MCP, Rust/Go and runner migration, retention and download recovery.

## Components

| Component | Purpose |
|---|---|
| [**eoka-mcp**](crates/eoka-mcp) | Stdio MCP server for browser tools |
| [**eoka-cli**](crates/eoka-cli) | Interactive shell CLI for browser automation and debugging |
| [**eoka-server**](crates/eoka-server) | Shared browser runtime, `Session` API, and browser helper primitives |
| [**eoka-protocol**](crates/eoka-protocol) | Shared daemon protocol types and tool catalog metadata |
| [**eoka-sdk**](crates/eoka-sdk) | Rust SDK for typed Eoka browser sessions |
| [**eoka-tack**](crates/eoka-tack) | Tack `ToolSet` adapter generated from the Eoka protocol catalog |
| [**@cbxss/eoka**](clients/node) | Node/TypeScript SDK for private sidecar sessions; source-only, not published |
| [**eoka-runner**](crates/eoka-runner) | Declarative YAML automation runner |
| [**eoka-captcha**](crates/captcha) | Optional Anti-Captcha integrations |
| [**eoka-datadome**](crates/datadome) | Experimental local DataDome slider solver |
| [**eoka-email**](crates/eoka-email) | IMAP helpers for OTP and verification-link flows |
| [**eoka-proxy**](crates/eoka-proxy) | Shared proxy parsing and configuration |

## Choose an interface

- Use `eoka-mcp` when an MCP client needs browser tools.
- Use `eoka-cli` for interactive exploration and debugging.
- Use `eoka tack` when an agent needs Tack TypeScript execution
  against the active browser session.
- Use `eoka-runner` for versioned, repeatable YAML workflows.
- Use the Go client and `eoka-server` when embedding browser automation in a Go service.
- Use the Node SDK for typed JavaScript/TypeScript automation with explicit
  server and Chrome paths.

## Local DataDome on the current CLI tab

Build/install with `mise run install-cli`. On this unreleased SDK branch, run
build commands through the paired-core wrapper described under Development.
On a challenged page in an existing session:

```sh
eoka --session demo captcha datadome
eoka --session demo snapshot
```

The command uses the existing session and selected tab, brings that tab to the
foreground, and leaves the browser open. No API key, model, separate browser or
cookie transfer is used. Default: one attempt and a 60-second solve budget;
`--max-attempts 1..4` and `--timeout-ms 5000..120000` are optional. Allow up to five
additional seconds for cleanup. JSON output is available through `--json`.

`challenge_cleared` is widget clearance, not login proof. `not_present` is a
successful no-op. Blocks, bans, failed clearance and errors exit nonzero. No
session is launched/restarted and no preceding application action is replayed.
Use the same session/connection flags as before; an older daemon must be upgraded
explicitly. See [the DataDome guide](crates/datadome/README.md) for details.

## Page evidence and download diagnostics

`observe` now includes the selected page URL/title, document readiness, visible
headings and status/alert messages, and up to 2,000 characters of visible text.
Document readiness does not establish that an application has finished loading.
Context is best effort, limited to the top document, and excludes hidden text and
form values; a context collection failure leaves element observation usable.

`--agent` and `observe --structured` retain `text` and `elements` and add `page`
(`null` when unavailable). Unstructured JSON retains string-valued `data`. MCP
uses the same collector and honors `structured`. Control indices retain their
existing meaning; re-observe or take a fresh snapshot after page changes.

Download failures can include `error_detail.context`: optional `page`,
`download_id`, and `action_status` (`not_attempted` or `may_have_executed`). Missing,
hidden, disabled and stale controls are rejected before clicking. Resolution
failures retain their own error code instead of becoming missing-target errors.
No native start includes page evidence without asserting that the link expired.
Download diagnostic URLs omit credentials, query strings and fragments.

Normal terminal errors show the code, page evidence, known job state/paths and a
follow-up command. JSON retains its envelope and existing job fields. A lost wait
response retains the known job ID without claiming a current state. Wait timeouts
leave jobs running; inspect or wait for the same job instead of starting again.
Old daemons need an explicit restart to provide the new evidence.

## Browser downloads and bounded fetch

Use an existing Eoka-owned session to save files with Chrome's cookies, proxy and
browser network stack:

```sh
eoka --session work open https://example.com/files
eoka --session work download --click 'text:Download' --out ./archive.zip
eoka --session work download list
eoka --session work download status JOB_ID
eoka --session work download wait JOB_ID --timeout-ms 120000
eoka --session work download cancel JOB_ID
eoka --session work download --url https://example.com/attachment --out ./file.zip --wait
```

Starts return a job ID; `--wait` adds a separate wait. Wait timeout or client
exit leaves the transfer running, and active jobs prevent idle shutdown. One file
may run per session. Closing the session cancels unfinished work. Jobs remain in
durable SQLite storage; daemon restart preserves job history. Recovery can reconcile
provably completed files. There is no
resume, queue or fallback HTTP downloader. Borrowed/remote `--cdp` browsers are
rejected. Existing old daemons need an explicit restart to gain these operations;
requests never upgrade or replay automatically.

`--click` accepts normal observation refs, text and CSS targets. `--url` accepts an
HTTP(S) attachment and uses a temporary tab in the same browser context, preserving
the selected tab. HTML pages do not count as successful file downloads. Defaults:
30 seconds to observe a native start (`--start-timeout-ms`, maximum 300000) and
30 seconds per wait (`--timeout-ms`, maximum 86400000). Neither limits an active
background transfer. `--expect-bytes` checks an independently known size.

The output's parent must exist. Relative CLI paths resolve against the caller's
working directory; SDK/MCP/Tack inputs require absolute filenames. A private
staging directory is created beside the destination. Completion checks the actual
regular file and byte counts, syncs it, then publishes through an atomic hard link
that cannot replace an existing file. Filesystems must support hard links.
Successful files survive session closure. Failed jobs report retained
`staging_path`, and `published_path` if publication occurred before a later cleanup
failure. These paths are local to the daemon/MCP host. Suggested filenames are
untrusted display metadata. Check status before retrying an ambiguous action.

`fetch` is text inspection: it streams at most 8192 raw bytes by default, cancels
at the limit, and reports `bytesRead`, `truncated` and `bodyOmitted`. `--max-body`
accepts 0 (headers only) through 16777216. Known binary MIME bodies are skipped;
use `download` to save them. `--body-only` still obeys the limit and fails if the
body is binary or truncated. An exact-limit body is conservatively truncated until
EOF is observed; incomplete trailing UTF-8 is omitted. Responses that expand beyond
the protocol budget after JSON escaping fail with guidance to lower the limit.
These bounds cover retained text, not Chrome's internal network buffering.

The canonical operations are `download.start`, `download.list`, `download.get`,
`download.wait` and `download.cancel`. SDK helpers use typed protocol arguments;
Tack exposes them as `eoka.download.*`, and native MCP uses the canonical dotted
names. Job/file policy lives in `eoka-server`; the sibling `eoka` library supplies
native event watching and bounded browser fetch primitives.

Local browser regression (also runs with `EOKA_DOWNLOAD_TEST_HEADED=1 xvfb-run -d`
and optionally `EOKA_DOWNLOAD_TEST_LIVE=1`):

```sh
cargo test -p eoka-cli --test download_cli -- --ignored --test-threads=1
```

Concurrent headed runs need separate displays. Use `xvfb-run -d` where supported;
this asks Xvfb to allocate the display. If the wrapper only supports `-a`, run the
headed suites sequentially: some wrappers race when choosing a free display, then
continue running tests even when their X server failed to start.

## MCP quick start

```sh
cargo install eoka-mcp
claude mcp add eoka -- eoka-mcp
```

The MCP server communicates over standard input and output. It supports MCP `2026-07-28` through stateless `server/discover` requests. It creates and closes its browser through the shared eoka-server runtime for the lifetime of the MCP connection.

## Frame evaluation

```sh
eoka --session demo frames
eoka --session demo frame-eval 'iframe#login' 'document.title'
eoka --session demo frame-eval 'id:FRAME_ID_FROM_FRAMES' 'document.title'
```

Frame operations require an existing session and selected tab. CSS selectors must
match exactly one frame in the top document; use `id:` for explicit or nested
selection. Bare URLs and numeric indices are not accepted. Cross-origin/OOPIF
execution uses CDP isolated worlds: DOM access works, but page-owned JavaScript
globals are unavailable. Reacquire frame IDs after navigation. These commands do
not open the iframe in a new tab, restart the browser, or replay uncertain requests.
Upgrade an older running daemon explicitly to use the corrected evaluation path.

## Held input

The CLI, MCP server, Tack tools, Rust SDK, and JSON-RPC server support `mouse_down`, `mouse_move`, `mouse_up`, `key_down`, `key_up`, and `release_all_inputs`. Pointer coordinates are viewport CSS pixels and mouse buttons are `left`, `middle`, `right`, `back`, or `forward`. Always release held input explicitly when a drag or chord is complete; tab and session shutdown also attempt cleanup.

```sh
eoka mouse-down 100 120
eoka mouse-move 220 120
eoka mouse-up 220 120
eoka key-down Shift
eoka key-up Shift
```

## Rust session API

```rust
use eoka_server::{Session, eoka::{ActionOptions, Expectation}};

let mut session = Session::launch().await?;
session.goto("https://example.com").await?;
session.observe().await?;
session.set_action_options(ActionOptions::expecting(Expectation::Url {
    value: "https://www.iana.org/help/example-domains".into(),
    contains: false,
}));
session.click(0).await?;
session.close().await?;
```

## YAML runner quick start

```yaml
name: Example
target:
  url: https://example.com
actions:
  - click:
      text: More information
      expect:
        kind: url
        value: https://www.iana.org/help/example-domains
  - screenshot:
      path: result.png
```

```sh
cargo install eoka-runner
eoka-runner automation.yaml
```

## Go client

```sh
go get github.com/shrimp-software/eoka-tools/clients/go
```

The client negotiates the `eoka-server` protocol before launching Chrome and
preserves durable operation IDs and structured errors. For this source checkout,
build the matching server and pass `WithServerPath`; previously published servers
without `server.hello` are rejected before browser work. See the
[Go client README](clients/go/README.md) for binary resolution and lifecycle rules.

## Node / TypeScript SDK

[`clients/node`](clients/node) provides the unpublished `@cbxss/eoka` SDK:
compiled ESM/declarations for Node 24 LTS (`^24.21.0`), one private sidecar per Browser,
owned launch or borrowed attachment, verified actions, durable operation recovery,
page reads, bounded text fetch, PNG/JPEG/WebP screenshots and native MP4 recording.

`Browser.launch()` resolves an exact optional server package and installs pinned
full Chrome for Testing 155.0.8059.39 on demand. `installBrowser()` supports explicit
preparation, progress, cancellation and verified offline/read-only cache reuse.
Explicit `serverPath`/`executablePath` and environment overrides remain available;
there is no PATH, Cargo, server-download or system-browser fallback.
Import and npm installation have no browser/process side effects. Sandboxing is
enabled by default, without an unsandboxed retry. Local managed consumption requires
staging the SDK and native package together from actual release artifacts; the
source manifest deliberately leaves unbuilt server hashes unset. Phase 5 capture
and recording are implemented; no npm package has been published.

`page.screenshot()` returns a bounded Buffer; `page.screenshotToFile()` publishes
without replacing existing files. Capture supports CSS-pixel crops, full-page
output and viewport DPR. `page.record()` returns a recoverable recording handle;
`stop()` resolves only after publishing the complete native MP4 artifact.
Cancellation and local waiter deadlines are distinct. Chrome selects the codec;
H.264, audio, deterministic replay and transcoding are not promised. See the
[capture contract](clients/node/README.md#screenshots-and-native-recording) and
[phase 5 evidence](SDK-PLAN.md#phase-5-implementation-and-verification).

```sh
npm --prefix clients/node ci --ignore-scripts --omit=optional
export EOKA_CORE_PATH=/absolute/path/to/matching/eoka-checkout
bash scripts/with-local-core.sh mise run node-generate
bash scripts/with-local-core.sh cargo build --locked -p eoka-server
mise run node-build
bash scripts/with-local-core.sh mise run sidecar-check
```

The sidecar registry is separate from the CLI tool catalog. See
[PROTOCOL.md](PROTOCOL.md) for negotiation, message bounds, shutdown and action
outcome guarantees.

See the package README for release staging, managed/explicit launch, offline and
container examples, cache/runtime paths, proxy settings, deadlines, caller-owned
profiles, bounded disposal and uncertain action outcomes.
Protocol schemas remain canonical in `eoka-protocol::sidecar`; the SDK's typed
`invoke` covers that actual sidecar registry, not all CLI/MCP/Tack operations.

## Development

This unreleased branch requires its matching core checkout for the new runtime,
sandbox and viewport APIs. `scripts/with-local-core.sh` applies a Cargo patch in
an isolated temporary `CARGO_HOME`, reuses the local registry cache, and removes
that configuration afterward. It does not modify the installed Chrome, the
global Cargo configuration, or checked-in manifests. Set `EOKA_CORE_PATH` to the
core worktree being developed alongside this repository:

```sh
export EOKA_CORE_PATH=/absolute/path/to/eoka-npm-recording
bash scripts/with-local-core.sh cargo build --locked -p eoka-server
```

Core `0.7.0` must be published before this tools branch can be released. The
paired-development lockfile contains no invented registry checksum: regenerate
it against the published core without overrides before production packaging.
A development override is not release verification against crates.io.

```sh
bash scripts/with-local-core.sh mise run conformance
bash scripts/with-local-core.sh mise run release-check
cd clients/go && go test ./...
```

Without mise:

```sh
bash scripts/with-local-core.sh cargo test --workspace
bash scripts/with-local-core.sh bash scripts/release-check.sh
```

Use the local Tack override when testing unpublished Tack changes from a sibling
checkout:

```sh
bash scripts/release-check.sh --with-local-tack
```

Install the local CLI from this checkout:

```sh
mise run install-cli
```

Inspect the generated protocol-backed tool catalog:

```sh
eoka tools manifest --json
```

Run Tack TypeScript against the active browser session:

```sh
eoka tack 'return await tools.invoke({ path: "eoka.info", input: {} });'
```

## Documentation

- [eoka-mcp](crates/eoka-mcp/README.md)
- [eoka CLI skill](crates/eoka-cli/SKILL.md)
- [eoka-runner](crates/eoka-runner/README.md)
- [Go client](clients/go/README.md)
- [server protocol](PROTOCOL.md)

Install the Codex skill with the open skills CLI:

```sh
npx skills add https://github.com/shrimp-software/eoka-tools/tree/main/crates/eoka-cli --skill eoka -a codex -y
```

## License

MIT
