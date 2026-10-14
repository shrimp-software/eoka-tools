# Node SDK, managed Chrome, and recording implementation plan

Status: phases 1–5 are implemented in the paired `investigate/npm-recording` worktrees. Nothing is published. Package name: `@cbxss/eoka`, subject to scope ownership. npm is the distribution channel for a Node/TypeScript SDK, not the CLI. This plan covers the SDK, managed Chrome, native recording, and the integration contract for Topology's deterministic exporter. `clients/node` builds a locally packable, private Node SDK with compiled ESM, declarations, managed Chrome and native capture; native server packages must be staged with the SDK from actual artifacts. Broader platform qualification and publication remain phase 6.

## 1. Product decisions

- Implement the package in `eoka-tools/clients/node`. Ship compiled JavaScript and TypeScript declarations.
- Use one private `eoka-server` subprocess per `Browser` instance, communicating over stdin/stdout. No CLI daemon, Unix socket, native Node addon, or WebAssembly port.
- Eoka's Rust library launches and controls Chrome through CDP. JavaScript manages installation and passes the executable path; it does not run a second Puppeteer automation stack.
- Default to a tested, pinned Chrome for Testing build downloaded on first `Browser.launch()`. Import and npm installation have no browser downloads or process side effects.
- Support explicit installation for CI/offline preparation, explicit Chrome executable paths, and connection to existing Chrome. Never silently fall back from managed Chrome to a system browser.
- Use `@puppeteer/browsers` as the installer/cache primitive. Pin a tested dependency version and include its required ZIP/proxy dependencies rather than relying on accidental transitive installations.
- Support the Node 24 LTS line starting at 24.21.0 (`^24.21.0`); pin development and CI through `mise.toml`. Publish ESM and declarations; CommonJS callers use dynamic `import()`. Node 22 and Current/non-LTS lines are no longer support targets. Earlier multi-version results below are historical evidence. Do not claim browser, edge-runtime, Bun, or Deno support without runtime gates.
- SDK and platform packages share an npm version. Rust server/core retain their own versions, pinned in an explicit release manifest. Wire compatibility uses a separate protocol version and actual capabilities, not package-version equality.
- Native recording produces Chrome's MP4, not a promised codec. Deterministic GIF/H.264 encoding belongs to Topology, not the basic browser SDK.

## 2. Evidence and existing gaps

Inspected worktrees: `~/Code/eoka-npm-recording` and `~/Code/eoka-tools-npm-recording`, both on `investigate/npm-recording`.

| Area | Current evidence | Work required |
| --- | --- | --- |
| Sidecar transport | Node runtime consumes bounded NDJSON, negotiated limits, correlated responses, cancellation evidence and bounded teardown | Platform execution gates before publication |
| Handshake | Protocol 1.3 reports actual versions, methods, capabilities and limits; Node checks support before dispatch | Keep generated contracts synchronized with the native registry |
| Launch | Managed or explicit executable/runtime paths, sandbox policy and DPR reach core launch; results report actual browser version and ownership | Remaining native-platform qualification |
| Connection | Node `Browser.connect` attaches through a validated `ws://` endpoint and preserves borrowed Chrome | TLS WebSocket attachment is not supported; remaining platform qualification |
| Schemas | Canonical DTOs/registry in `eoka-protocol::sidecar`; generated schemas, runtime exports and declarations consumed by the SDK | Maintain synchronized capture and recording contracts |
| Results | SDK preserves operation arrays, dynamic JSON, null success, typed errors, receipts and dispatch uncertainty | Maintain these contracts as methods are added |
| Actions | Caller-known UUIDs, durable handles, strict verified conveniences, explicit expectations and lost-response recovery | No automatic replay |
| Screenshots | Typed PNG/JPEG/WebP, CSS crops/full-page, DPR, bounded Buffer and non-overwriting file output verified with Chrome | Native image/CDP bounds still apply |
| Recording | Target-bound native MP4, bounded streaming finalization, recoverable IDs and truthful terminal states | Remaining platform/encoder qualification; deterministic export stays in Topology |
| Distribution | SDK and verified native package stage/pack/install locally without lifecycle scripts; five targets mapped | Remaining platform execution gates and trusted publication |
| Chrome | Pinned full distribution, archive/tree integrity, atomic concurrent installation, offline/read-only reuse and private runtime roots | Remaining native-platform qualification and deliberate pin updates |
| Sandbox | Existing Rust/server/Go defaults remain unsandboxed; Node explicitly requests sandboxing by default without fallback | Native platform sandbox qualification before publication |

Initial investigation smoke evidence, before phase 1 implementation:

- Installed `eoka 0.3.6`, attached to cached Chrome 155.0.8059.39, captured three PNG frames at 1280x720 from a 640x360 viewport with DPR 2. FFmpeg produced one-second, three-frame H.264 MP4 and GIF files.
- Direct CDP on that Chrome produced an approximately two-second MP4 containing AV1 video. Native recording required an initialized page and usable temporary storage.
- System Chrome 152 crashed before DevTools startup. A quota-limited `/tmp` caused native stream reading to fail; a writable per-run temporary directory resolved the probe.
- The existing built sidecar rejected missing handshake/connect methods and rejected a zero-width viewport. EOF without `browser.close` exited nonzero.
- These probes did not exercise the future Node SDK, other operating systems, Topology, audio recording, or release-built npm artifacts.

## 3. Public SDK contract

Implemented API shape:

```ts
import { Browser, installBrowser } from '@cbxss/eoka';

await installBrowser({ browserCacheDir: '/var/cache/eoka' });

const browser = await Browser.launch({
  browserCacheDir: '/var/cache/eoka',
  download: 'never',
  headless: true,
  viewport: { width: 1280, height: 720, deviceScaleFactor: 2 },
});

try {
  const page = await browser.newPage();
  await page.goto('http://127.0.0.1:3000/demo');
  const png = await page.screenshot({ format: 'png' });
  const recording = await page.record({ path: './demo.mp4', fps: 30 });
  await page.execute('window.demo.play()');
  const video = await recording.stop();
} finally {
  await browser.close();
}
```

The example illustrates API shape, not reliable scenario completion: a real exporter must await an explicit application completion condition before stopping the recording.

### Browser management

- `installBrowser({cacheDir?, signal?, onProgress?})` installs the SDK-pinned build and returns verified installation metadata. It does not launch Chrome or the server. No unconstrained `latest` selector.
- `Browser.launch(options)` uses managed Chrome by default; `download: 'never'` makes a missing installation an actionable error. `executablePath` explicitly bypasses managed installation.
- `Browser.connect({browserWSEndpoint, serverPath?, signal?, timeoutMs?})` starts a sidecar and attaches without installing or launching Chrome. Validate endpoint schemes and redact sensitive endpoint data in errors.
- Resolve server binary from explicit `serverPath`, then `EOKA_SERVER_BIN`, then the matching npm platform package. Do not search PATH or fetch arbitrary GitHub releases implicitly. Every override must pass the handshake.
- Browser executable precedence: explicit `executablePath`, explicit `EOKA_CHROME_PATH`, then managed pin. No automatic system-browser search in the SDK default path.
- `newPage`, `pages`/`tabs`, page close, browser close, and `Symbol.asyncDispose` have documented ownership. `newPage(url)` delegates to the same verified navigation path as `page.goto`, not a weaker hidden navigation.
- Return actual browser version, ownership and usable capabilities in connection metadata. Preserve the distinction between server support and browser-session support.

### Browser/page automation

- Cover every currently implemented sidecar operation with generated request/result types and a documented typed invocation API. Add ergonomic Browser/Page methods for lifecycle, navigation, reads, waits, actions, evaluation, screenshots, state, bounded fetch and existing agent operations.
- Do not claim every CLI/MCP/Tack tool is a sidecar method. Advertise only the generated sidecar support set. CLI-only functionality is outside this SDK release unless deliberately added to that set.
- Expose canonical actions/locators/expectations/receipts through generated types. High-level consequential actions use `actions.perform` and the existing operation manager.
- `page.perform(action, options)` returns an admitted operation handle with its caller-known UUID. Its `wait()` returns the operation record, including an unknown outcome if that is the evidence available. `cancel()` requests cancellation; inspect the resulting state.
- Convenience `click`, `type`, `pressKey`, etc. admit and wait, resolving with a receipt only for verified completion. Preserve required explicit expectations; missing expectations fail before dispatch. Fill/select/navigation retain their intrinsic verification behavior.
- Operation IDs are generated before transmission or supplied by the caller. Transport/action errors carry the ID, dispatch evidence and any receipt. No automatic action retry or browser restart after uncertainty.
- `evaluate(expression, options?)` returns JSON-compatible data (`unknown` by default); `execute` discards it. Do not advertise closure serialization or Puppeteer JSHandle/ElementHandle compatibility. Characterize current undefined/non-JSON return behavior, define it in the protocol, and reject unsupported serialization rather than inventing values. Add JSON arguments through a canonical optional field if needed by Topology's frame stepping.
- `AbortSignal` cancels local waiting, not proof of remote rollback. Drop an unsent request when possible. Once dispatched, keep the distinction between request cancellation and durable operation cancellation; preserve late-response correlation until completion or transport teardown.
- Local transport deadlines and server action deadlines are distinct and documented. Never resolve an action as successful solely because an RPC returned without an error.

### Error model

Use stable typed errors for installation, unsupported platform, browser dependency/launch failure, protocol incompatibility, transport closure/timeout, and server operation errors. Preserve canonical codes and structured action receipts instead of parsing error strings. Attach bounded, redacted stderr diagnostics. A cleanup failure after a tab closed must still report that the tab is closed.

## 4. Canonical protocol and sidecar changes

### Types and negotiation

1. Put sidecar envelopes, method input/result schemas and lifecycle metadata in `eoka-protocol`. Reuse existing action/agent/operation types; relocate duplicate DTOs rather than define parallel copies.
2. Add a typed sidecar method registry alongside the canonical operation catalog, with explicit mappings where surfaces differ. Generate TypeScript request/result unions and method metadata from Rust JSON Schemas. Keep the generator deterministic and checked-in output reviewable.
3. Add `server.hello`, valid before launch: protocol major/minor, server/core version, supported method/capability set, and transport limits. SDK rejects missing or incompatible handshake before opening Chrome.
4. Keep existing wire names and result shapes for current consumers unless a specific migration is required. Do not introduce legacy/new aliases. Update Go and documentation where behavior changes; do not force unrelated consumers to use the Node installer.
5. Preserve dynamic JSON results as dynamic JSON, not falsely precise generated types. Define numeric bounds for cross-language request IDs and integer fields; do not silently round u64 values above JavaScript's safe range.

### Transport and lifecycle

- Frame input/output by newlines with incremental UTF-8 decoding; support split characters, split lines, multiple replies per read and out-of-order replies.
- Match positive safe-integer IDs, handle stdout backpressure, bound pending work and message size, and fail all outstanding calls on EOF/process exit/protocol corruption. Retain a bounded stderr tail without allowing logs to contaminate stdout.
- Negotiate an initial 32 MiB JSON message ceiling, enforced during reads rather than after allocating arbitrary input. Large screenshots use file output; video is never one base64 response. Oversized evaluation/state output returns a structured size-limit error.
- Ordinary page work remains serialized per target where required by the action contract. Do not make all commands concurrent to solve shutdown latency.
- Separate stdin intake/control handling from long-running command dispatch sufficiently for EOF and `server.shutdown` to trigger bounded cleanup. Preserve operation-manager ownership and unknown outcomes when work is interrupted.
- Add shutdown that works before launch, after failed launch, with an attached browser, and during an active wait/recording. Browser close is idempotent at the SDK boundary. Drain/terminate waiter tasks explicitly; never leave promises pending on child exit.
- Graceful close releases owned input and recording resources. After a documented grace deadline, terminate only the SDK-owned sidecar and owned browser process tree. Test abrupt parent death as well as normal close. Do not claim output recovery after SIGKILL without evidence.
- Borrowed Chrome is detached, never terminated by SDK cleanup. Explicit page-close commands remain intentional destructive actions on the specified tab.

### Managed runtime launch

Add executable-path and configuration types to `src/methods/browser.rs`, passing the path to `StealthConfig.chrome_path`. Preserve current validation for viewport and user-owned profiles. Add sidecar connect and explicit ownership metadata. Pass viewport DPR through the appropriate core/CDP path rather than overloading CSS dimensions.

Extend the existing launch/profile/patcher code only where necessary to honor a dedicated runtime directory. Do not introduce another browser launcher in TypeScript.

## 5. Chrome installation, cache, profiles and temporary files

### Installation

- Commit a release manifest containing SDK version, server version/hash per platform, protocol requirement, Chrome build/platform URL/hash, and installer dependency version. Start validation from the already exercised 155.0.8059.39; choose the final pin at release only after all platform gates pass.
- Use `@puppeteer/browsers` install with the expected SHA-256. Chrome's version-index URL is not an integrity signature: pin reviewed hashes obtained in a controlled release preparation step; do not fetch an unchecked hash from the same mutable runtime URL.
- Install the complete Chrome distribution, including resources, libraries and macOS bundle contents, not just one executable. Do not include ChromeDriver; Eoka uses CDP directly.
- Use an OS-appropriate per-user cache, keyed by product/platform/build. Coordinate concurrent processes with a per-build lock. Download/extract into a staging location, verify, then publish the complete cache entry atomically. Interrupted downloads must not look installed.
- Progress is exposed through a callback; default first-launch diagnostics go to stderr, not stdout. Import is silent. Do not modify a caller's global process environment.
- Support `browserCacheDir`, `EOKA_BROWSER_CACHE_DIR`, and the explicit offline setting. Resolve relative explicit paths once in the Node caller and pass absolute paths over the wire.
- Honor proxy settings through the installer dependency. Fail closed on TLS/hash errors; do not disable certificate verification or silently substitute a different mirror/build.
- `download: 'never'` works with a preseeded readable cache, including a read-only container image. Do not require mutating a verified cache on every launch.

### Runtime state

- Keep persistent browser cache separate from ephemeral profile/temp state and existing durable operation state.
- Allocate a private per-browser runtime directory under configurable `runtimeDir`/`EOKA_RUNTIME_DIR`, with restrictive permissions where supported. Child-only temp environment directs Chrome recording scratch and core temporary work there. Surface ENOSPC/EDQUOT with the affected location.
- Preserve caller-owned profile directories; never delete them or silently attach to a user's default Chrome profile. If core detects an already-running explicit profile and borrows it, report borrowed ownership honestly.
- The core's patcher already copies rather than mutates the source executable and patches only its owned ephemeral-profile path. Keep managed downloads immutable and preserve companion-file/macOS-bundle behavior. Gate concurrent patch creation, cache reuse and startup cost; add a narrowly scoped core cache/root option if needed, rather than duplicating patch logic in Node.
- Close removes only owned temporary resources. Do not automatically delete cached Chrome builds still used by another instance. Cache removal is explicit and separate from Browser.close.
- Never automatically install privileged OS packages. Core/server now expose explicit sandbox control; existing Rust/server/Go defaults remain `sandbox: false` for compatibility. The Node SDK must explicitly request sandboxing by default, with a clearly named `sandbox: false` opt-out for caller-controlled environments. Never retry a failed launch by silently disabling the sandbox. Document Linux dependencies/container setup and return useful errors for unsupported environments.

## 6. Packaging, platforms and publication

Package layout (new paths):

```text
clients/node/
  package.json
  src/
    index.ts
    browser.ts
    page.ts
    operations.ts
    recording.ts
    transport.ts
    browser-install.ts
    server-binary.ts
    errors.ts
    generated/
  scripts/
  test/
  platforms/
  release-manifest.json
```

Use npm with a committed lockfile and TypeScript's compiler. Do not introduce a bundler or native addon toolchain merely to package this SDK. `files`/`exports` allowlists include only compiled runtime, declarations, manifest, license and package documentation.

Platform packages: `@cbxss/eoka-server-linux-x64-gnu`, `-linux-arm64-gnu`, `-darwin-x64`, `-darwin-arm64`, `-win32-x64`. Each has exact `os`/`cpu` selectors and GNU packages declare `libc: glibc`. Each contains its matching server binary, release metadata and license; preserve executable mode. The main package uses exact-version optional dependencies, not floating ranges.

Target all five existing server build targets. The pinned Chrome manifest currently lists downloads for all five corresponding browser platforms, including Linux arm64. Configured build targets and available Chrome ZIPs are not runtime support guarantees.

- Define and verify a glibc baseline. Use Ubuntu 22.04/glibc 2.35 as the intended minimum unless measured Chrome/server dependencies require a higher floor; build release binaries against that baseline rather than `ubuntu-latest` alone. Resolve this before advertising Linux compatibility.
- Do not advertise musl/Alpine or Windows arm64 support. Reject unsupported combinations before downloading Chrome. An explicit server/browser path is an expert override, not a supported-platform claim.
- Windows needs real attach/cleanup verification: core discovery has Unix-assuming paths, and the server's existing download subsystem explicitly rejects some non-Unix operations. Browser automation/recording support must pass independently; expose unsupported optional capabilities honestly rather than claiming complete CLI feature parity.
- Handle missing optional platform packages with an actionable install error, including the case of `--omit=optional`. Do not fall back to compiling Rust or an unpinned server download.
- Verify `npm ci --ignore-scripts` can install and import the SDK; first launch/explicit install still manages Chrome. Check npm, pnpm and Yarn consumption of packed artifacts before claiming support. No install-time script is required.
- Configure trusted npm publishing/OIDC with provenance on protected GitHub-hosted release jobs. Confirm scope ownership, package bootstrap requirements and publisher configuration for every package. First publication requires explicit approval and credentials/setup; none is authorized by this plan.
- Publish validated platform packages first, then SDK under `next`; run clean registry-install smoke on every platform before promoting the SDK's `latest` tag. If a publish partially fails, do not point `latest` at an incomplete release. Verify any existing same-version artifact before resuming.
- Publish core and tools Rust crates in existing dependency order where changed. Consume the same server artifacts used by GitHub releases; don't build subtly different binaries for npm. Retain hashes and provenance.
- Chrome updates are deliberate SDK releases with browser smoke gates, not runtime auto-updates. Monitor upstream security releases and advance the pin promptly; users can opt into a tested explicit browser path without silently changing existing installs.

## 7. Capture and native recording

### Screenshots

Core owns typed `Page.captureScreenshot` options; server exposes them through canonical schemas. Provide PNG/JPEG/WebP where Chrome supports them, appropriate quality validation, CSS-pixel crop rectangles and full-page capture. Reject conflicting clip/full-page options. Set render scale through viewport DPR, not a second independent scale multiplier.

- `page.screenshot(options)` returns a Buffer for bounded in-memory use.
- `page.screenshotToFile({path, ...options})` avoids sending the image through Node as base64, returning metadata after verified file publication.
- Paths resolve in the caller; the server receives absolute paths. Do not overwrite existing files by default. Verify full-page dimensions and document Chrome limits. For deterministic exports use a fixed viewport/crop, not full-page layout that changes dimensions frame to frame.

### Recording

Core adds a target-bound owned recording handle around `Page.startScreenRecording`, `Page.stopScreenRecording`, `IO.read` and `IO.close`. Server owns file policy and exposes start/get/stop/cancel through the protocol; Node provides a `Recording` object.

- One active recording per target. Serialize state transitions; duplicate stop shares finalization/result instead of stopping twice. A failed start leaves no usable handle.
- Require a ready, active page. Detect unsupported commands and report `RecordingUnsupported`; do not silently switch algorithms, browser versions or quality modes. A protocol method being listed alone is not proof its encoder works.
- Start has a caller-known request/recording ID so a lost response cannot justify blindly starting another recorder. Recovery is scoped to the live sidecar; no crash-resume claim.
- Initial options: output path, max frame dimensions, maximum frame rate. Silent video is the baseline; audio is not promised until separately supported and verified across targets. Native frameRate is a ceiling, not deterministic fixed-fps playback.
- `stop()` completes only after Chrome stops, all IO chunks reach a bounded-buffer staging file, the stream closes, fragmented MP4 timing is validated/corrected losslessly, and the artifact is atomically published without overwriting another file. Encoded packets, sample count, file length and the final presentation endpoint are preserved; already-valid native files remain byte-identical. No whole-video buffering or giant base64 JSON response.
- Finalization runs as bounded background work so status/cancel/shutdown remain responsive. A cancelled waiter is not cancellation of finalization. An explicit cancellation stops capture and releases the stream; report any uncertain cleanup and never label a partial file complete.
- If finalization fails, return stage/error information and any retained partial-file path. Browser/tab close cancels unfinished recording by default; callers explicitly stop to save. Disconnects invalidate recording handles and preserve honest failure outcomes.
- Native output is MP4 with Chrome-selected codec; the observed build used AV1. Do not require FFmpeg for this path or label the output H.264. Verify container/decodability with ffprobe during release smoke, not as an implicit SDK runtime dependency.
- Crop/captions in live exports are scene composition or explicit exporter postprocessing. Do not pretend the native recording API supports screenshot clip options.

## 8. Topology exporter integration

Topology's source was not inspected in this task. The reported README recorder is the migration starting point, not assumed proven reusable code. Inspect that repository before editing and replace/generalize its implementation rather than maintain a second recorder.

Proposed application command:

```text
node scripts/export.mjs my.spec.mjs --mode frames --format mp4 --fps 30 --scale 2
node scripts/export.mjs my.spec.mjs --mode live --format mp4
```

Users author scenario specifications, not Puppeteer scripts. Topology imports `@cbxss/eoka`. Its exporter owns scenario validation, playback, camera keyframes, captions, crop and quality presets, duration/fps/scale and output naming.

### Deterministic frames mode

1. Start/reuse the local application server, launch managed Chrome and set fixed viewport/DPR.
2. Load the scenario, seed randomness, await explicit asset/font/image readiness and enter the application's export mode.
3. Expose `prepare`, `renderAt(timeSeconds)` and completion metadata in the page's main world. Pause autonomous simulation/rAF loops; drive physics/layout/randomness from the same explicit timeline. Control CSS/Web Animations and media used by the scenario too.
4. For each integer frame i, await the application's completed render at i/fps, then capture. Do not approximate simulation stepping with real-time sleeps or generic network-idle waits.
5. Define duration rounding once: frame count is ceil(duration * fps), samples start at zero, encoded duration is frame count / fps. Report both requested and encoded duration.
6. Render camera/captions before capture. Scale uses DPR; crop coordinates are CSS pixels with a single documented conversion to output pixels. Keep final dimensions fixed. H.264 yuv420p requires even dimensions; apply a documented padding policy, not silent stretching.
7. Feed frames in order to one encoder with backpressure. Start with SDK Buffers and a bounded queue; use file output for frames exceeding the negotiated message ceiling. Do not accumulate every frame in memory or spawn the Eoka CLI per frame.
8. MP4 preset explicitly selects H.264/pixel format and quality; GIF uses palette generation/use. GIF centisecond timing cannot represent every requested fps exactly: choose and report the closest supported timing rather than promise exact 30 fps GIF playback.
9. Cancellation closes the encoder, stops capture, closes owned resources and leaves no success-named partial output. Publish output only after encoding and validation succeed.

### Live mode

Start native recording before playback, await application completion, then stop/finalize. Scene camera/captions remain application-owned. If output needs a different codec, crop or GIF, run the explicit encoding/postprocessing stage; live timing is not deterministic.

### FFmpeg policy and limits

Topology uses an explicit `ffmpegPath` or an installed FFmpeg and preflights required codecs before playback. Do not bundle FFmpeg into the Eoka SDK. An install-managed FFmpeg distribution would require a separate licensing/build/platform decision; until then missing FFmpeg is an actionable prerequisite for frame exports/postprocessing, not a fake fallback.

Deterministic simulation and frame count are acceptance criteria. Bit-identical rendering across OS/GPU/font stacks is not promised; pin fonts/browser/platform/software-rendering settings for repeatability-sensitive presentations. Example tests must validate sampled simulation state, not just successful encoding.

## 9. Implementation milestones and acceptance gates

These are the full sequence, not permission to publish an incomplete SDK. Each milestone must leave existing consumers usable. Implementation status and completed acceptance evidence are tracked in the phase backlog below.

| Milestone | Implementation targets | Acceptance gate |
| --- | --- | --- |
| 1. Canonical sidecar contract | `eoka-protocol`, server protocol/dispatch/main, Go client, `PROTOCOL.md`, `OPERATIONS.md` | Handshake rejects incompatible server before launch; actual schemas cover sidecar results; out-of-order waits correlate; shutdown works before/after launch and on EOF |
| 2. Managed launch and connection | Server browser handlers/state; core launch/profile/patcher only where needed | Explicit Chrome path is used; attached Chrome survives close; owned Chrome is cleaned up; private runtime location works with an unusable system temp directory |
| 3. Node transport and SDK | New `clients/node`, generated schemas/types, ergonomic API | Actual sidecar/browser round trip; verified action receipts; lost-response errors retain operation ID; abort does not replay; no orphaned processes after success/failure |
| 4. Browser installer and binary resolution | Node installer/resolver, release manifest, platform package staging | Fresh/cache-hit/offline installs; checksum/corruption/interruption/concurrent-install handling; explicit-path and missing-optional-package behavior |
| 5. Capture and native recording | Core CDP types/connection/Page; protocol/server recording and file handling; Node capture API | Real cropped/scaled screenshots; complete decodable recording; duplicate stop/cancel/disconnect/disk-full coverage; existing outputs not replaced |
| 6. Cross-platform packaging | Server release workflow, new npm workflow, `mise.toml`, Node package docs | Packed SDK installed outside repo without Cargo/CLI; real managed Chrome launch/capture/close on every advertised target; artifact ABI/ownership checks |
| 7. Topology exporter | Existing Topology recorder and scenario/render-clock integration after inspection | One command runs live and frame modes; camera/caption/crop/quality paths exercised; precise frame count/state samples; playable MP4 and GIF |
| 8. Release | Rust release order, platform npm packages, SDK prerelease and promotion | All gates green; clean registry-install smoke; documented platform/features and install prerequisites; explicit publication approval |

After milestone 1 establishes types, transport/client work and installer/package work can proceed independently. Recording's core implementation can proceed independently of Node packaging. Integrate only against the agreed protocol and lifecycle contract. Topology work depends on working capture/SDK contracts, not final publication.

## 10. Verification plan

### Behavioral regression coverage

- Transport: UTF-8 split across chunks, replies out of order, simultaneous waits, mid-message EOF, malformed response, duplicate response ID, size limits and backpressure. Assert resolved/rejected requests and cleanup, not copied source text or descriptor counts.
- Actions: required expectation rejected before dispatch; a timed-out local waiter followed by operation recovery; uncertain action never replayed; cancellation reports actual dispatch/outcome; page-close cleanup failure retains closed state.
- Installer: cache hit without network, offline miss, invalid hash, interrupted publication, two concurrent installers, read-only verified cache, path with spaces, no supported platform and explicit override behavior.
- Lifecycle: explicit path, managed path, failed launch, attach/detach, close before launch completes, in-flight wait, Node parent exit and forced timeout cleanup. No user profile deletion or borrowed-browser termination.
- Capture/recording: DPR and crop pixels, full-page boundaries, oversized response redirected by caller to file mode, existing-file protection, start/stop twice, target close/navigation, stream failure, quota failure, cancellation during finalization and completed MP4 decoding.
- Exporter: deterministic sample states and frame count; delayed font/asset readiness; fps/duration boundaries; even-dimension policy; GIF timing; encoder unavailable/failure; cancellation; captions/camera/crop visually inspected.

### Required runtime smoke

Use actual packed npm artifacts in a clean temporary application, without Cargo, installed `eoka` CLI or repository-relative paths. Install the browser, launch the sidecar, load a local fixture, perform and verify a consequential action, evaluate JSON, capture a screenshot, save a recording, close, and assert process/profile cleanup. Repeat with a cached browser and networking disabled. Attach to a separately launched Chrome and prove it remains alive after SDK close.

Run on Linux x64/arm64 GNU, macOS x64/arm64 and Windows x64 before promising support. Include a oldest-supported-glibc image and both minimum and current supported Node versions. Run a headed case where applicable; headless-only success is not headed coverage. Decode recorded outputs and inspect actual screenshots; tests alone are not recording proof.

Existing repository gates remain: `mise run conformance`, `mise run release-check` in eoka-tools and documented core Rust/MSRV/browser gates when core changes. Extend mise with Node build/typecheck/behavioral and packed-artifact/browser workflows. During implementation, run integrated build/lint/test gates once per completed work batch, not mid-flight in each delegated slice.

## 11. Release documentation and maintenance

Update package README/API examples, root README, PROTOCOL, OPERATIONS, affected existing clients and release instructions in the same changes as their contracts. Core public API additions require core README/context API reference and changelog updates. Generated files are regenerated from canonical definitions, never edited independently.

Document download size/cache location, Chrome/system dependency requirements, ownership and shutdown, Node/platform support, privacy of session state, offline/container usage, code-vs-receipt success, native codec limitations and FFmpeg prerequisites. Never advertise the SDK as browser-side JavaScript or a drop-in Puppeteer replacement.

## 12. Prerequisites and remaining decisions

Defaults are chosen above; these are release gates, not reasons to block design:

- Confirm npm scope ownership and create trusted-publisher configuration, including any first-publish bootstrap. No publication until explicitly authorized.
- Run the release-built server/Chrome matrix; settle actual Linux ABI and Windows ownership limitations before naming supported targets. Fix a required browser/recording path or explicitly obtain agreement before dropping a promised target.
- Linux extraction, authenticated HTTPS proxy behavior and all five archive hashes are verified for the pinned installer/build. Repeat native installation and launch gates on the remaining release platforms; hashes are reviewed HTTPS-download integrity pins, not vendor signatures.
- Inspect Topology's actual recorder, scenario API and rendering clock before implementation. This repository does not establish those application contracts.
- Audio, musl, Windows arm64, remote Eoka servers, browser-runtime JS, arbitrary Puppeteer compatibility and bundled FFmpeg are not part of this plan. Existing sidecar capabilities are covered; adding all daemon-only tools is separate product scope.

## 13. Execution backlog: phases 1–5

Phases 1–5 below are **implemented and locally verified**. Phase 6 release qualification and linked PR preparation are now in progress; no publication is authorized. IDs are stable handoff references. Checked implementation tasks do not imply that every advertised platform has passed native release qualification.

No npm publication, release promotion or Topology edits are included in this backlog; those remain phases 6–8. Existing contract tests and documentation change with their owning task. Run integrated build/lint/test gates after a completed work batch, and require the real runtime proof in each phase gate.

### Phase 1 — Canonical sidecar protocol

- [x] **1.1 — Define canonical sidecar request/result types and registry.**
  - Targets: `crates/eoka-protocol/src/`, server `protocol.rs`, `dispatch.rs`, `methods/`, `operations/sidecar.rs`.
  - Work: inventory actual accepted methods; move shared DTOs into the protocol crate; reuse action/agent/receipt definitions; specify real result shapes, error payloads, JSON evaluation semantics and safe numeric ranges.
  - Depends on: none.
  - Acceptance: every currently accepted sidecar method has an explicit typed input/result contract; existing Go wire shapes remain usable; no assertion that the sidecar accepts the entire CLI catalog.

- [x] **1.2 — Implement protocol handshake and actual capability reporting.**
  - Targets: protocol registry, server `dispatch.rs` and state.
  - Work: add `server.hello` before launch, protocol/server/core versions, supported methods and transport limits; distinguish static server support from browser-session features.
  - Depends on: 1.1.
  - Acceptance: a real server responds without launching Chrome; unsupported protocol versions fail before browser work; unimplemented browser features are not advertised.

- [x] **1.3 — Generate TypeScript protocol types from Rust schemas.**
  - Targets: protocol schema export, new Node generation script and `clients/node/src/generated/`.
  - Work: generate envelopes, method maps, inputs, results, errors and action/receipt types; preserve dynamic JSON rather than casting it to a fabricated static shape.
  - Depends on: 1.1, 1.2.
  - Acceptance: a TypeScript consumer compiles valid calls and rejects invalid arguments; real operation-array and evaluation results decode using the generated contract.

- [x] **1.4 — Bound protocol framing and preserve structured errors.**
  - Targets: server `main.rs`, `protocol.rs`, dispatch error conversion.
  - Work: incremental bounded input; 32 MiB message ceiling; bounded responses; retain canonical action evidence; stdout exclusively carries protocol data.
  - Depends on: 1.1, 1.2.
  - Acceptance: malformed/oversized traffic fails predictably without unbounded allocation; large valid messages work; operation errors retain IDs and receipts where available.

- [x] **1.5 — Implement responsive, bounded sidecar shutdown.**
  - Targets: server `main.rs`, `state.rs`, lifecycle handlers and operation-manager integration.
  - Work: separate input/control intake from blocking page work; add `server.shutdown`; handle EOF, pending waiters, failed launch and shutdown before launch; preserve uncertain operation outcomes.
  - Depends on: 1.2, 1.4.
  - Acceptance: shutdown/EOF can terminate an in-flight wait without hanging; responses remain correlated by ID; owned resources are cleaned up and no uncertain action is replayed.

- [x] **1.6 — Migrate affected Go contracts and protocol documentation.**
  - Targets: `clients/go/`, `PROTOCOL.md`, `OPERATIONS.md`, affected existing consumers.
  - Work: add handshake/capability handling where required, retain durable IDs, document out-of-order waits and actual result shapes, and update lifecycle behavior without adding legacy aliases.
  - Depends on: 1.1–1.5.
  - Acceptance: existing Go launch/action/state flows retain their observable behavior; incompatible servers fail clearly; documented EOF/shutdown semantics match the process.

- [x] **1.7 — Prove the protocol phase against a real sidecar.**
  - Targets: existing protocol/server/Go behavioral suites and local sidecar smoke.
  - Work: cover concurrent waits, split framing, size boundaries, transport loss and shutdown; run integrated conformance/relevant regression gates.
  - Depends on: 1.1–1.6.
  - Acceptance: a real sidecar completes handshake and operation queries, correlates out-of-order responses and exits cleanly through the new lifecycle; record observed results.

#### Phase 1 verification

Implementation used four independent owners for canonical contracts, server
lifecycle/framing, TypeScript generation, and Go migration, followed by two
read-only reviews and parent-owned integration. Only the tools worktree changed;
the core worktree remains unchanged.

Observed on Linux with local Chrome:

- The real sidecar negotiated protocol 1.0 before launch, reported server 0.4.0
  and core 0.6.1, and rejected an incompatible protocol. Unimplemented
  `browser.connect` and recording features are not advertised.
- A local HTTP scenario performed exactly one click with a durable operation ID,
  reached `verified_complete`, and returned its receipt. A later hello request
  completed before the pending operation wait; IDs remained correctly correlated.
- Generated Node schemas decoded the actual operation-array and Unicode
  evaluation responses. The screenshot was valid PNG and matched the browser's
  measured 640×273 content viewport.
- Shutdown during a browser-confirmed pending evaluation returned the interrupted
  request and shutdown responses, cleaned up and exited 0 in 0.21 seconds.
  EOF before launch also exited 0.
- A real Go consumer launched, performed a verified fill, evaluated its value,
  captured and restored cookies/local storage, observed a missing-operation
  error, and closed twice successfully.
- Chrome-backed regressions exercised owned-browser cleanup, a caller-provided
  profile, shutdown during launch, abrupt parent termination and durable unknown
  outcomes after EOF. Oversized ASCII, escaped-string and Unicode evaluations
  returned `MessageTooLarge`; a 15 MiB value subsequently succeeded without
  reconnecting Chrome. Non-JSON evaluation values were rejected.

The oversized evaluation probe exposed the core's 16 MiB CDP frame limit before
the 32 MiB sidecar output limit. Evaluation now checks serialized UTF-8 size in
Chrome and reserves 4 KiB for its CDP envelope; the per-method ceiling is
documented in `PROTOCOL.md`.

Completed checks:

- Workspace formatting, all-feature check/tests, and all-target/all-feature
  Clippy with warnings denied.
- Protocol/server suite: 77 passed; seven relevant Chrome-backed tests were
  also run explicitly. Other ignored browser suites were not run.
- `mise run conformance` and the server release-trigger check.
- `mise run go-check` with the race detector, including accepted-operation
  uncertainty and terminal-receipt regressions.
- Generated-file drift check, valid TypeScript consumers, ten rejected invalid
  consumers, eight Node tests, and the schema-consumer example.
- Verified `eoka-protocol` package build and workspace packaging with
  `--allow-dirty --no-verify`; no publishing.

No macOS/Windows, managed-Chrome, public Node runtime or npm-release claims are
made by this phase. The historical Go binary pin remains unchanged; source
consumers must select the matching newly built sidecar as documented.

### Phase 2 — Managed launch and ownership

- [x] **2.1 — Add explicit Chrome executable-path launch support.**
  - Targets: canonical launch DTO, server `methods/browser.rs`, Go launch options.
  - Work: pass an absolute executable path to `StealthConfig.chrome_path`; retain current proxy/profile/viewport validation; report actual launched browser version.
  - Depends on: 1.1, 1.2.
  - Acceptance: a supplied Chrome binary is used even when another system Chrome exists; an invalid path fails without discovering or launching a substitute.

- [x] **2.2 — Add attachment and explicit browser ownership.**
  - Targets: protocol connect types, server browser handlers/state, core connection ownership where needed.
  - Work: expose `browser.connect`; identify launched versus borrowed browsers; initialize page enumeration without assuming ownership.
  - Depends on: 1.1, 1.2, 1.5.
  - Acceptance: attach to separately launched Chrome, operate on a page, then close the SDK session; external Chrome remains usable and its profile remains intact.

- [x] **2.3 — Implement private runtime and temporary-directory policy.**
  - Targets: core launch/profile/patcher paths as necessary; server runtime configuration.
  - Work: isolate per-browser runtime files from browser cache and durable operation state; honor the configured runtime root; keep source Chrome immutable and patch-copy dependencies intact.
  - Depends on: 2.1.
  - Acceptance: launch and temporary-file work succeed with a writable explicit runtime root despite an unusable default temp directory; cleanup removes only owned state; simultaneous launches do not corrupt patch artifacts.

- [x] **2.4 — Make sandbox behavior an explicit setting.**
  - Targets: core launch configuration/arguments, canonical launch DTO, server handler, affected caller documentation.
  - Work: replace the unconditional launch-policy assumption with explicit sandbox control; SDK-managed launch will request sandboxing by default; provide a deliberate opt-out without automatic fallback.
  - Depends on: 2.1.
  - Acceptance: real launch behavior matches the selected policy; an unsupported sandbox environment errors rather than silently retrying unsandboxed; document existing-caller behavior.

- [x] **2.5 — Expose viewport dimensions and device scale factor consistently.**
  - Targets: core/CDP emulation path, sidecar launch/page viewport schema and handlers.
  - Work: distinguish CSS dimensions from physical capture pixels; validate positive finite scale and dimensions; apply/reset overrides through one path.
  - Depends on: 1.1, 2.1.
  - Acceptance: a 640x360 viewport at DPR 2 renders/captures as 1280x720; changing/resetting the viewport gives the documented layout and capture dimensions.

- [x] **2.6 — Complete owned/borrowed cleanup across launch failures.**
  - Targets: sidecar shutdown/state, core lifecycle and Node-facing ownership results.
  - Work: handle failed launch, caller-owned profiles, close during startup, EOF and forced termination; release held inputs; preserve borrowed browser processes.
  - Depends on: 1.5, 2.1–2.4.
  - Acceptance: normal close and abrupt parent loss leave no owned browser/profile behind where cleanup is possible; borrowed Chrome and user profiles survive; cleanup uncertainty is reported, not hidden.

- [x] **2.7 — Prove launch, attachment and runtime isolation.**
  - Targets: real-browser lifecycle fixtures, Go launch coverage and core browser regressions.
  - Work: exercise explicit path, attachment, sandbox policy, DPR, runtime placement and failure cleanup; update affected API docs/changelog.
  - Depends on: 2.1–2.6, 1.7.
  - Acceptance: observe actual browser version, screenshot dimensions, ownership and process cleanup; distinguish Linux-only proof from the later all-platform release gates.

#### Phase 2 verification

Four implementation owners covered core runtime/ownership, viewport/CDP,
sidecar/protocol and Go, followed by read-only reviews and parent-owned
integration. Both worktrees changed. Protocol 1.1 now reports 52 methods and
eight capabilities; generated Node contracts remain private tooling, not the
phase 3 SDK runtime.

Observed on Linux with explicit Chrome for Testing 155.0.8059.39 and local
system-Chrome regressions:

- Explicit executable selection returned the actual Chrome product/version.
  An invalid path failed without fallback. Source Chrome's SHA-256 remained
  `ed05e73a2cdf875326fcfa438b31c71a6a885a353610731dd52090dfc476f778`.
- A 640×360 CSS viewport at DPR 2 produced a visually inspected 1280×720 PNG.
  Changing to 400×240 at DPR 1.5 produced 600×360; reset restored measured native
  640×273 at DPR 1 and a matching PNG.
- Two simultaneous patched launches succeeded with an unusable default TMPDIR
  and a writable explicit runtime root. Child-only temp placement was observed;
  close/EOF removed owned runtime children while preserving the caller root
  and caller-profile sentinel.
- Sandboxed launch omitted disable-sandbox flags. Chrome's `chrome://sandbox`
  reported namespace isolation and Seccomp-BPF active. No unsandboxed retry was
  introduced; legacy Rust/server/Go defaults remain false.
- Borrowed attachment exposed existing and subsequently created external tabs,
  supported page work, released held input and left Chrome/profile usable after
  disconnect or EOF. Owned close/drop, retained Page handles, interrupted
  startup and abrupt parent loss exercised cleanup without replaying uncertain
  actions.
- Viewport regressions exercised independent attachments, closed-controller
  replacement, fresh-session reset, scrolled capture/reset and hidden-tab reset
  without foregrounding or resizing the actual browser window. Completion
  owners retain serialization through late native replies without retaining
  transport ownership.
- A real Go consumer exercised launch options, actual session metadata, verified
  fill, DPR screenshots, reset, idempotent close and borrowed existing-tab access.

Completed checks:

- Core formatting, 178 non-browser tests, Clippy with warnings denied, examples
  build and Rust 1.85 all-feature library check.
- Twelve targeted core browser regressions: eight viewport, two runtime-isolation
  and two ownership/shutdown cases, including headed lifecycle coverage.
- Protocol/server suite: 80 non-browser tests; eight sidecar Chrome regressions
  plus the JSON/non-JSON evaluation browser check.
- Paired-core tools `mise run release-check`, conformance, and final scoped
  server build/Clippy/browser gates after viewport refinements.
- Go race tests and vet; ten Node tests, fourteen rejected invalid TypeScript
  consumers, valid consumers and generated-file drift verification.

Build paired source checkouts through `scripts/with-local-core.sh` as documented
in the root README. The new core API is not in the existing registry release;
release versions/pins must advance before publication. No release was made.

Scope boundaries: Linux proof is not a macOS/Windows release matrix. Attachment
currently supports `ws://`, not `wss://`. Direct SIGKILL of the core/sidecar cannot
promise destructor cleanup. Fractional host DPI and capture boundaries retain
Chrome's native rounding/rasterization; hidden-tab outer-dimension readings can
change during native capture without a window resize. Raw CDP probes reproduced
these browser behaviors, and Eoka does not mask them. Independent external CDP
clients/processes are outside the in-process viewport coordinator.

### Phase 3 — TypeScript SDK

- [x] **3.1 — Create the Node package and type-generation integration.**
  - Targets: new `clients/node/package.json`, lockfile, TypeScript configuration, public exports and mise tasks.
  - Work: Node 24 LTS (`^24.21.0`, superseding the initial Node 22 floor), compiled ESM/declarations, export allowlist and generated protocol imports; no install/import side effects.
  - Depends on: 1.3.
  - Acceptance: a separate Node application can import compiled exports and typecheck; import launches no process, downloads nothing and writes no cache files.

- [x] **3.2 — Implement the correlated NDJSON subprocess transport.**
  - Targets: `clients/node/src/transport.ts`, protocol/error types.
  - Work: incremental UTF-8 framing, safe request IDs, backpressure, pending-work bounds, handshake, stderr tail, exit/EOF handling and negotiated message limits.
  - Depends on: 3.1, 1.2, 1.4.
  - Acceptance: split and out-of-order responses settle the correct calls; a crashed/malformed/oversized peer rejects affected promises without hanging or leaking listeners.

- [x] **3.3 — Implement typed error and local cancellation semantics.**
  - Targets: Node `errors.ts`, transport and operation error conversion.
  - Work: stable installation/transport/protocol/server errors; carry IDs and receipts; distinguish abort before send from abort after dispatch; handle late responses without replay.
  - Depends on: 3.2.
  - Acceptance: abort removes an unsent request when possible; dispatched actions remain recoverable by operation ID; no timeout is misreported as rollback or verified completion.

- [x] **3.4 — Implement Browser lifecycle APIs with explicit paths first.**
  - Targets: Node `browser.ts`, transport lifecycle and canonical browser requests.
  - Work: `launch`, `connect`, page enumeration/creation, idempotent close and async disposal; initially exercise explicit server/Chrome paths while installer work proceeds independently.
  - Depends on: 3.2, 3.3, 2.1, 2.2, 2.6.
  - Acceptance: actual launch/connect/new-page/close work with the sidecar; `newPage(url)` uses the same verified navigation contract as `goto`; failures close only owned resources.

- [x] **3.5 — Implement typed sidecar invocation and Page read APIs.**
  - Targets: Node `page.ts`, generated method map and relevant API groups.
  - Work: typed access to every existing sidecar operation; ergonomic reads, waits, evaluation/execution, state, bounded fetch and agent calls; capability checks reflect actual sidecar support.
  - Depends on: 3.1–3.4.
  - Acceptance: real page data and JSON evaluation round-trip correctly; unsupported/non-JSON results follow the documented error contract; unsupported methods are not advertised as working APIs.

- [x] **3.6 — Implement durable operations and strict action conveniences.**
  - Targets: Node `operations.ts`, Page actions and generated core action types.
  - Work: caller-known UUIDs, `perform` admission, get/list/wait/cancel, explicit expectations and verified-only convenience methods; preserve canonical locator behavior.
  - Depends on: 3.3–3.5.
  - Acceptance: missing expectations fail before dispatch; admission alone does not resolve a convenience action; lost-response recovery works by ID; unknown outcomes never trigger automatic replay.

- [x] **3.7 — Integrate bounded disposal and documentation.**
  - Targets: Browser/transport disposal, package README/examples, root README and mise workflows.
  - Work: stop pending local calls on disposal, use graceful shutdown then owned-process termination, document Node/platform limits and caller-owned profiles; use explicit-path examples until phase 4 completes.
  - Depends on: 3.4–3.6, 1.5.
  - Acceptance: close during active waits/failure settles all promises; borrowed Chrome survives; examples report actual receipts and do not imply installation already exists.

- [x] **3.8 — Prove the SDK with a real browser and consumer program.**
  - Targets: Node behavioral tests and throwaway external consumer smoke.
  - Work: typecheck, build and exercise launch, verified action, evaluation, operation recovery and close through Node rather than CLI commands.
  - Depends on: 3.1–3.7, 2.7.
  - Acceptance: consumer-visible results and cleanup are observed using actual server/Chrome processes; retain regression coverage for action uncertainty and transport boundaries.

#### Phase 3 implementation and verification

`clients/node` now exports `Browser`, `Page`, `Operation`, typed errors and the
correlated subprocess transport, alongside generated protocol types. The package
was private, version `0.0.0`, at phase 3 completion; its Node 22.14.0 floor has since been superseded by the Node 24 LTS policy above. Its packed payload
contains compiled ESM/declarations, the canonical schema bundle, license,
documentation and an explicit-path example—not Rust sources or executables.
There is no install hook, CLI binary, browser download or implicit PATH lookup.

The canonical `page.fetch_bounded` method advances the protocol to 1.2 and
advertises `boundedFetch`. It uses core streaming capture with an 8192-byte
default and a 1 MiB sidecar limit, leaving room for JSON encoding overhead.
Legacy `page.fetch` retains its original unbounded behavior. Node requires 1.2;
the existing Go 1.1 contract remains compatible.

| Verification | Observed result |
| --- | --- |
| Node 22.14.0 | 133 tests passed; valid consumers typechecked and 17 invalid consumers rejected |
| Node 24.18.0 | 133 tests passed; valid consumers typechecked and 17 invalid consumers rejected |
| Packed external consumer | Installed with `--ignore-scripts`; silent import, no SDK cache/process side effects, compiled exports and declarations usable outside the repo |
| Canonical/native gates | Schema generation check, protocol/server tests and Clippy passed; workspace conformance and release-check passed |
| Go compatibility | Race tests and `go vet ./...` passed |
| Real SDK on both Node versions | Chrome 152.0.7977.13 launched with sandbox requested; verified navigation/fill/click, JSON evaluation, state restore and cleanup succeeded |
| Actual screenshot | 640×360 CSS at DPR 2 produced 1280×720 PNG; 400×240 at DPR 1.5 produced 600×360; reset matched native viewport dimensions |
| Streaming reads | 8192-byte stream canceled early; partial UTF-8 tail omitted; binary/headers-only skips, session cookies and over-limit rejection exercised |
| Durable actions | Missing expectations rejected before admission; unknown outcome remained unknown; deliberately dropped admission reply recovered by the same ID with exactly one click |
| Agent and cancellation APIs | Node 22.14 observed a named control, completed a verified agent fill, and canceled an already-clicked durable action; terminal outcome stayed unknown and click count stayed one |
| Lifecycle | Active local waits settled; owned Chrome/runtime removed; borrowed Chrome/profile preserved; direct terminal invokes followed by close/async disposal succeeded |
| Forced termination | A SIGSTOPed real sidecar was boundedly terminated with its owned Chrome; a separate Chrome survived; runtime-file cleanup uncertainty remained an error |
| Documented example | Explicit-path example produced a verified fill receipt and read back `Ada` from the actual page |

Regression coverage includes late page creation after abort/disposal, exact-tab
cleanup, unknown creation outcomes, mismatched operation IDs/targets, malformed
framing, backpressure, late replies, credential-safe stderr and terminal-method
shutdown races. Cleanup errors preserve primary action evidence rather than
replacing it.

Scope boundaries: the browser/process evidence above is Linux x64, not a
macOS/Windows release matrix. Forced termination cannot promise runtime-file
cleanup or termination of descendants that leave the owned process boundary.
POSIX runtime roots must be short enough for Chrome's singleton socket path.
The independent Chrome fixture required `--password-store=basic` to avoid its
desktop password-store startup stall; this was reproduced before SDK attachment,
and no core interception-policy change was made. No managed installation,
recording or npm publication is implied by completion of this phase.

### Phase 4 — Managed installation

- [x] **4.1 — Define release-manifest and platform resolution contracts.**
  - Targets: Node release manifest/types and platform selector.
  - Work: pin SDK/platform-package version, server version/hash, protocol requirement, Chrome build/platform URL/hash and tested installer version; define unsupported-platform and glibc behavior.
  - Depends on: 1.2, 3.1.
  - Acceptance: supported OS/architecture/libc combinations map unambiguously; musl/unsupported combinations fail before browser download; exact-version pins never resolve to latest implicitly.

- [x] **4.2 — Implement npm server-binary resolution.**
  - Targets: Node `server-binary.ts`, local platform-package staging metadata.
  - Work: explicit path, environment override, then matching optional package; verify executable metadata/integrity as appropriate; handshake every override; no implicit PATH/GitHub/Cargo fallback.
  - Depends on: 4.1, 3.2.
  - Acceptance: staged platform packages resolve outside the repository; missing optional dependencies and incompatible overrides give actionable errors instead of selecting another server.

- [x] **4.3 — Implement verified Chrome installation and cache publication.**
  - Targets: Node `browser-install.ts`, installer dependency and release manifest.
  - Work: full Chrome distribution, pinned hash verification, cross-process per-build lock, staging/extraction and atomic cache publication; validate paths/executable after install.
  - Depends on: 4.1.
  - Acceptance: fresh install works; bad hashes/corruption fail closed; interruption never leaves a usable-looking incomplete install; two concurrent installers converge on one complete installation.

- [x] **4.4 — Add explicit installation, offline/cache and progress controls.**
  - Targets: public `installBrowser`, browser-install configuration and typed errors.
  - Work: cache path precedence, proxy support, abort/progress callbacks, read-only cache reuse and `download: never`; no privileged dependency installation or TLS bypass.
  - Depends on: 4.3, 3.3.
  - Acceptance: preinstalled Chrome launches without network; offline miss fails clearly; progress never corrupts stdout; verified read-only cache reuse does not require writes.

- [x] **4.5 — Connect installation to managed Browser.launch.**
  - Targets: Node Browser lifecycle, binary resolver and runtime-directory creation.
  - Work: make managed pin the default; preserve explicit-path/connect bypasses; create child-only runtime environment; report browser/server versions and ownership.
  - Depends on: 3.4, 4.2–4.4, 2.3, 2.4.
  - Acceptance: one `Browser.launch()` on a clean machine installs and starts the expected browser; explicit connect performs no download; failure unwinds owned runtime resources without deleting caches/user files.

- [x] **4.6 — Add installation documentation and offline/container examples.**
  - Targets: Node package README, root README, mise tasks and release preparation instructions.
  - Work: document cache/runtime paths, system libraries, sandbox behavior, Chrome hash provenance, proxy/offline setup and separate server/browser versioning.
  - Depends on: 4.1–4.5.
  - Acceptance: a consumer can preinstall during image/setup time and run with networking disabled; instructions do not assume Cargo, CLI installation or hidden postinstall scripts.

- [x] **4.7 — Prove managed installation from locally packed artifacts.**
  - Targets: local npm pack/install harness, real installer/browser smoke and fault fixtures.
  - Work: install staged main/platform packages into a clean consumer; exercise fresh download, cache hit, offline, spaces in paths, interrupted install and unsupported platform; run integrated Node/server gates.
  - Depends on: 4.1–4.6, 3.8.
  - Acceptance: managed browser flow works without repository-relative binaries, Cargo or Eoka CLI; `--ignore-scripts` install/import works; no npm publication is needed for this gate.

#### Phase 4 implementation and verification

Implemented in `clients/node`: validated release/platform metadata, exact optional
server-package resolution, controlled release staging, full Chrome installation,
worker cancellation, full-tree cache validation, cross-process locking, atomic
publication, offline/progress controls and managed Browser lifecycle integration.
Four implementation agents owned those independent slices; two read-only reviewers
checked release integrity and installer/lifecycle safety. No core source changes
were needed in this phase.

At phase 4 completion, release pins were SDK/platform packages `0.0.0`, server `0.4.0`, core `0.6.1`,
protocol `1.2` and Chrome for Testing `155.0.8059.39`. Installer dependencies are
`@puppeteer/browsers` `3.2.4`, `yauzl` `3.4.0` and `proxy-agent` `8.0.1`.
All five full Chrome archives were fetched over HTTPS and SHA-256 measured.
Unbuilt server hashes remain unset in source; staging embeds actual binary
hashes in both SDK and platform metadata. Runtime rejects unprepared pins.

The Linux x64 server used for the gate was release-built inside Ubuntu 22.04,
not taken from the host debug build. Its measured maximum GLIBC requirement is
2.34; managed Linux selection remains conservatively >=2.35 with OpenSSL 3.
Linux release workflow builds now use Ubuntu 22.04 containers. Other native
platforms are configured, not claimed as runtime-qualified.

Observed acceptance evidence:

- Complete Node 22.14 suite: **213 passed, zero failed/skipped**, including actual
  server staging and the external packed managed-consumer gate. Node 24.18
  regression and packed-consumer gates also passed. Valid TypeScript consumers
  compiled; all 17 invalid consumers were rejected.
- Locally packed main/native tarballs installed into a separate application with
  `--ignore-scripts --omit=optional`, the native tarball supplied directly.
  Import remained silent. Managed launch used neither repository binary paths,
  Cargo nor Eoka CLI; its application PATH was empty.
- No-argument `Browser.launch()` downloaded the pinned full Chrome distribution
  through an authenticated HTTPS CONNECT proxy, reported Chrome
  `155.0.8059.39` and server `0.4.0`, and used the default enabled sandbox.
  Actual fill/click returned verified receipts, DOM state showed one click,
  and a visually inspected 1280x720 PNG matched a 640x360 viewport at DPR 2.
- A second installation was a cache hit without another proxy connection.
  Two independent installers converged on one complete distribution. Aborting
  a real download left no usable incomplete installation; a later install
  recovered. Modifying a real `.pak` resource caused integrity rejection.
  Unsupported-platform selection failed before cache creation.
- The same packed application ran inside Ubuntu 22.04 with `--network=none`,
  a read-only root and read-only browser cache, Node 22.14 and no repository,
  Cargo or CLI mounts. It verified actions and the same screenshot dimensions,
  reused the cache and removed owned runtime children. This isolated container
  explicitly used `sandbox: false`; it does not qualify container sandboxing.
- Shipped preparation and offline examples ran successfully from the installed
  package. Cache paths containing spaces worked. Failure fixtures cover bad
  hashes/archives, escaping links, dead owners, parent death, cancellation,
  deadlines, proxy credential redaction and TLS-disable rejection.
- Runtime cleanup now requires confirmed owned-process termination. Denied
  termination regressions retain runtime files and report
  `RuntimeCleanupUncertain`, preserving the primary failure.
- The real download gate caught a URL reconstruction error that removed the
  Google bucket prefix. The fix retains that prefix; the installer fixture
  now exercises a nested download origin.
- Canonical conformance, workspace format/check/tests/clippy/package gates,
  Go race tests and vet passed. No publication or recording was performed.

The package README documents source staging, cache/runtime ownership, proxy and
offline setup, system libraries, sandbox policy, archive size/provenance and
separate npm/server/browser versions. Five-platform native qualification,
registry publication and phase 5 capture/recording remain separate gates.

### Phase 5 — Capture and native recording

- [x] **5.1 — Add typed screenshot options to the core.**
  - Targets: core CDP page types/connection and Page diagnostics API.
  - Work: PNG/JPEG/WebP, format-specific quality validation, CSS-pixel clipping and full-page behavior; use viewport DPR as the single render-scale control.
  - Depends on: 2.5.
  - Acceptance: actual image dimensions/content match crop and scale; invalid clip/full-page/quality combinations fail clearly; existing screenshot consumers remain usable.

- [x] **5.2 — Add bounded screenshot bytes and safe file output to the sidecar.**
  - Targets: canonical screenshot types, server page/file-output handlers and error metadata.
  - Work: preserve bounded in-memory base64 for SDK Buffer results; add direct file output with caller-resolved absolute paths, staging, non-overwrite publication and artifact metadata.
  - Depends on: 1.1, 1.4, 5.1.
  - Acceptance: byte and file paths produce equivalent images; oversized in-memory results fail with file-output guidance; existing destination files are not replaced; failed writes are not reported complete.

- [x] **5.3 — Implement the core native recording handle and lifecycle.**
  - Targets: core CDP recording/IO types and connection helpers, Page recording API.
  - Work: initialized/active-page preconditions, start/stop/read/close, one recorder per target, explicit unsupported errors and bounded chunk consumption; keep file policy outside core.
  - Depends on: 2.3.
  - Acceptance: real Chrome yields readable MP4 data; duplicate/concurrent state transitions are defined; unsupported encoders/commands and disconnection release resources or report uncertain cleanup honestly.

- [x] **5.4 — Define canonical recording operations and result states.**
  - Targets: `eoka-protocol` recording DTOs/registry and generated Node contracts.
  - Work: caller-known recording ID; start/get/stop/cancel; running/finalizing/completed/cancelled/failed states; output metadata, failure stage and retained partial path.
  - Depends on: 1.1–1.3; coordinate the core API with 5.3.
  - Acceptance: state transitions and lost-response behavior are explicit; native MP4 is not mislabeled H.264; session-scoped recovery does not imply crash resume or automatic restart.

- [x] **5.5 — Implement sidecar recording management and file finalization.**
  - Targets: server recording manager/state/dispatch and file policy.
  - Work: start ownership, background finalization, bounded IO-to-file writes, non-overwrite publication, retained failure information and shared/idempotent stop result.
  - Depends on: 5.3, 5.4, 1.5; reuse file-publication behavior established by 5.2.
  - Acceptance: `stop` reports success only after a complete published artifact; lost responses can be inspected by ID; disk/quota/stream errors do not expose a success-named partial file.

- [x] **5.6 — Integrate recording cancellation and shutdown.**
  - Targets: recording manager, tab/browser close, transport disconnect and sidecar shutdown.
  - Work: distinguish waiter abort from recording cancel; cancel unfinished recording on close; stop/read/close resources within bounded cleanup; keep status/control responsive while finalizing.
  - Depends on: 5.5, 1.5, 2.6.
  - Acceptance: cancel during capture/finalization, page close and browser disconnect produce truthful terminal states; repeated stop does not double-stop Chrome; no cleanup terminates borrowed Chrome.

- [x] **5.7 — Expose Node screenshot and Recording APIs.**
  - Targets: Node `page.ts`, `recording.ts`, generated types, examples and API docs.
  - Work: `screenshot`, `screenshotToFile`, `record`, recording status/stop/cancel; path resolution, promises, error details and explicit native-codec limitations.
  - Depends on: 3.5, 3.7, 5.2, 5.4–5.6.
  - Acceptance: Node consumers obtain real image Buffers/files and completed video artifacts; no whole-video Buffer/base64 response; unsupported browser recording fails rather than silently selecting another engine.

- [x] **5.8 — Prove capture and recording end to end.**
  - Targets: core/server/Node behavioral coverage, actual browser/media smoke, core docs/changelog and SDK examples.
  - Work: capture a local animated fixture; inspect DPR/crop output; record, stop and decode MP4; exercise repeated stop, lost-response inspection, existing-file protection and quota/disconnect failures.
  - Depends on: 5.1–5.7, 4.7.
  - Acceptance: locally packed SDK with managed Chrome produces visibly correct screenshots and a playable MP4, with verified cleanup; report actual codec/duration and observed platform coverage. Broader release matrix remains phase 6.

#### Phase 5 implementation and verification

Five implementation agents owned core screenshots, core recording, canonical
contracts, server file/lifecycle handling, and the Node API. Three read-only
reviewers checked lifecycle races, publication safety, and public contracts.
The parent integrated changes and ran the packed browser/media gate.

Protocol 1.3 adds typed screenshots, direct screenshot file output, and
`recording.start/get/stop/cancel`. Core owns CDP recording streams and target
admission; the sidecar owns file staging, bounded 64 KiB reads, finalization and
non-overwriting publication. Node exposes Buffer/file screenshots and recoverable
`Recording` handles, not a whole-video Buffer. Native MP4 does not promise H.264,
audio, deterministic frame timing or transcoding.

Observed Linux x64 acceptance evidence with Node 22.14.0 and managed Chrome for
Testing 155.0.8059.39:

- Release-built the server inside Ubuntu 22.04, staged and packed the SDK/native
  packages, and installed them into a separate consumer with `--ignore-scripts`.
  Normal capture used the packaged server, an empty application PATH, the managed
  Chrome cache and the default enabled sandbox; no Cargo or Eoka CLI was needed.
- PNG/JPEG/WebP byte and file results matched. A 640×360 viewport at DPR 2
  produced 1280×720; a 100×80 CSS crop produced 200×160; an offscreen 100×100
  crop produced 200×200. Full-page output was 1280×1800. Visual inspection
  confirmed text, colored regions, offscreen clipping and full-page layout.
- A deterministic 2500×2500 noise image produced an 18,810,473-byte PNG:
  Buffer output rejected it with `ScreenshotTooLarge` and file-output guidance;
  direct file output succeeded without losing the browser connection.
  This gate exposed and fixed the WebSocket frame limit: both frame and message
  budgets are now 64 MiB, including base64/JSON overhead, not unbounded.
- The animated native artifact was a silent AV1 MP4, 640×360, 1.5568 seconds,
  25 frames and 36,281 bytes. FFprobe inspected it; FFmpeg decoded it without
  errors, and sampled frames visibly showed changing text, color and position.
  These are observed properties, not a cross-platform codec or timing guarantee.
- Repeated/concurrent stop returned the same completed artifact. Equivalent
  caller-ID starts recovered rather than restarting; duplicate target admission
  failed. Existing destinations and a destination created during recording
  were preserved. Failed publication retained its partial path.
- A relay dropped actual native start and stop responses. Local deadlines
  retained the recording ID and dispatch uncertainty; status and equivalent
  retries recovered a completed 27,940-byte AV1 MP4 (1.2646 seconds, 20 frames),
  which also decoded successfully. Recovery remains live-session-only.
- A read-only Ubuntu 22.04 container with networking disabled, a read-only
  browser cache and a filled 4 KiB output tmpfs produced real `ENOSPC` at the
  write stage. Status was failed, the partial path remained explicit, and no
  destination artifact was published. This container explicitly disabled the
  sandbox; it does not qualify container sandboxing.
- Cancellation during capture/finalization and page close produced truthful
  terminal states. Closing a borrowed connection preserved external Chrome;
  reattachment could record again. Killing that externally owned test Chrome
  produced a disconnect failure with explicit cleanup uncertainty. Connect
  installed nothing; owned runtime directories were cleaned after shutdown.
- Review fixes cover cancelled read delivery, one outstanding stream read,
  connection-specific teardown ownership, teardown-waiter cancellation,
  responsive publication arbitration, Windows file identity and late retained
  partial-file evidence. Windows identity code is not a Windows runtime claim.
- Core all-feature tests passed: 193 reported passes and 77 browser tests
  intentionally ignored. Core formatting/clippy and Rust 1.85.0 all-feature
  library checks passed. The browser evidence above is separate from those
  pure-logic checks.
- The complete Node 22.14 suite passed: 222 tests, zero failures or skips,
  including staging and the external packed capture/recording consumer. Valid
  TypeScript consumers compiled; all 22 invalid consumers were rejected.
- Canonical conformance, workspace format/check/all-feature tests and clippy
  passed against the matching local core. Protocol package verification and
  workspace packaging (`--no-verify`) passed; Go race tests and vet passed.
- The installed capture example produced all three image formats and a native
  MP4, then removed its owned runtime children. As documented, POSIX runtime
  roots must be short enough for Chrome's singleton Unix socket; the gate used
  a short root after a longer test root caused Chrome to abort at startup.

Core README/changelog, SDK examples/docs, canonical protocol docs, server
operations and Go contracts are updated. No npm publication, Topology migration,
other-platform media qualification or full ignored-browser suite was performed.

#### Phase 5 refinement pass

Five agents inspected core recording, core capture, server recording, file
handling and Node capture. Public APIs, protocol 1.3, file protection and
ownership/cancellation contracts remain unchanged. Capture geometry was left
unchanged because inspection found no worthwhile safe simplification.

- Core recording removes an unnecessary response channel, borrows read-delivery
  guards instead of cloning their three owning references, retains failed stream
  handles in place, and avoids cleanup tasks for already-finished recordings.
  A regression covers either of two stream closes failing, with cleanup
  continuing and retry closing only the unacknowledged handle.
- Server control paths inspect terminal state without cloning complete records.
  Shutdown copies only the selected uncertain failure's evidence; reservation
  and publication helpers no longer return unused data.
- Screenshot responses move their base64 string into JSON without another
  full-size copy. File capture releases its raw buffer before flush/publication.
  Unix identity verification reuses descriptor metadata, removing two redundant
  metadata calls per successful publication without changing Windows handling.
- Node capture wrappers avoid three conditional-spread temporary objects per
  call and reuse the recording UUID regex. Terminal evidence handling is unchanged.

These are identified allocation, copying and syscall reductions, not benchmarked
throughput or latency claims.

Verification after integration:

- Core all-feature tests reported 194 passes; formatting, clippy and Rust 1.85.0
  all-feature library checks passed. The 77 ignored browser tests remain separate.
- Workspace conformance and the full release-check workflow passed.
- Node 24.18 passed all 222 tests with zero skips, including the actual packed
  managed capture consumer and its oversized-image regression. Valid TypeScript
  consumers compiled and all 22 invalid consumers were rejected.
- A fresh host-development server/SDK package ran the installed capture example
  against managed Chrome 155.0.8059.39. PNG/JPEG/WebP output succeeded; the
  native AV1 MP4 was 800×600, 2.017733 seconds and 61 frames. FFmpeg decoded it
  without errors; visual inspection confirmed readable text and moving geometry.
  Owned runtime children were removed after close.

This pass qualifies Linux x64 host-development artifacts, not a new release ABI
or other-platform runtime matrix. No package was published.

#### Broad SDK refinement pass

Ten agents covered native recording, browser ownership, CDP transport, sidecar
recording, file publication, sidecar scheduling, Node lifecycle, Node transport,
browser installation and release integrity. Public signatures, protocol 1.3,
release pins and safety limits remain unchanged.

Two behavioral bugs were demonstrated against preserved pre-pass builds:

- Error-carried recording data could replace a handle's known output path.
  The same identity/path/artifact validation now applies to success and error
  records. Invalid evidence raises `Protocol` without retargeting the handle,
  while preserving the original wire error and valid late failure enrichment.
- Get/list/cancel could time out behind an unrelated page wait. Admitted-ID
  controls now execute independently; unknown IDs fall back behind earlier
  queued admissions. Lists are current snapshots. Lookup runs in bounded,
  shutdown-cancellable tasks rather than on stdin; successful get/cancel reuse
  the fetched record. A held-store-lock regression protects reader responsiveness.

Concrete internal improvements:

- Borrowed CDP envelopes, borrowed binary-frame text and moved result/event
  payloads remove redundant deep serialization and copies. Interception reserves
  queue capacity before constructing delivered pauses; notifications leave locks
  before waking consumers.
- Native teardown eliminates a forwarding channel and releases startup stderr
  storage after connection. Rejected recording admission avoids unused state.
  Recording notifications/map lookups hold locks for less work.
- Both NDJSON readers parse complete incoming frames without an accumulator copy.
  Node transports lazily share compiled canonical schema validators and collect
  stderr evidence on failure rather than successful calls.
- Browser/action wrappers remove duplicate sets and temporary option objects;
  tab reconciliation indexes IDs instead of repeatedly scanning live tabs.
- Download publication skips a discarded second digest but retains the complete
  bounded read, length/identity verification and synchronization. Installer lock
  scans, redaction and platform metadata/ABI checks avoid unnecessary allocations
  and repeated diagnostic/hostname work without skipping integrity checks.

Verification:

- Core all-feature tests reported 195 passes; formatting, clippy and Rust 1.85.0
  checks passed. The full ignored-browser suite was not run.
- Workspace conformance, final release-check and Go race tests passed.
- All 239 SDK tests passed with zero skips on Node 22.14.0 and 24.18.0.
  TypeScript valid consumers passed and all 22 invalid consumers were rejected.
- Both saved bug reproducers failed before and passed after. With the operation
  store exclusively locked, `server.hello` still replied; after lock release,
  the operation lookup and cleanup completed. The two targeted Chrome tests
  passed for blocked-worker controls/pending limits and queued admission ordering.
- Final host-development packages ran managed Chrome 155.0.8059.39. The capture
  example produced PNG/JPEG/WebP and an 800×600 AV1 MP4 of 2.0172 seconds and
  61 frames, with visual playback and owned-runtime cleanup verified.
- An attached recorder survived another attached connection closing. Explicit
  stop produced a second playable artifact; closing a borrowed owner cancelled
  unfinished capture without terminating external Chrome. Both videos decoded
  with native timebases and strictly increasing source DTS. Default null-output
  timebase conversion produced a timestamp warning, so decoding was
  verified without resampling rather than treating that conversion as source data.

Gains are identified eliminated work, not benchmarked speedup claims. Runtime
coverage remains Linux x64; this is not new cross-platform or release-ABI
qualification. No package was published.

#### Node LTS consolidation

Node 24.21.0 (Krypton) is the latest LTS on the [official release table](https://nodejs.org/en/about/previous-releases) at this pass; Node 26 is Current. Current support is restricted to `^24.21.0`, replacing the previous Node 22 minimum and broad newer-major claim.

- `mise.toml` supplies the exact development/CI runtime. The new Node CI job installs through the pinned mise action, checks the committed canonical schema projection, compiles valid/rejected consumers, and exercises runtime/package tests. It does not claim a native Chrome gate; the full local check still compares generated contracts with the Rust exporter.
- Package metadata and lockfile use the Node 24 engine policy and `@types/node` 24.19.2. SDK, schema typechecks, and external consumer compilation target ES2024. Native packages inherit the SDK engine field instead of duplicating its minimum.
- Clean `npm ci` exposed unresolved unpublished native packages in the source lockfile. Source metadata now omits those dependencies; staging derives all five exact optional dependencies from the validated release manifest, preserving the staged consumer contract without duplicate version pins. The same clean-install command failed before and passed after this change; CI retains that gate.
- Stable `import.meta.dirname`/`filename` replace script path-conversion helpers. Native `Promise.withResolvers` replaces executor-assigned transport callbacks and definite-assignment fields. Explicit-close and process-closed promises remain separate, preserving settlement order and cleanup-error propagation. No cancellation, timeout, integrity, ownership, or protocol safeguards were removed.
- Node 24.21.0 full SDK gate: **239 passed, zero failed/skipped**. Valid TypeScript consumers compiled and all **22** invalid consumers were rejected. Canonical Rust schema generation and the CI's committed-schema projection check passed. The exact browser-independent CI check commands also passed locally: 237 tests passed and the two explicitly gated native checks were skipped; GitHub-hosted execution was not run.
- Separately packed/installed Linux x64 host-development consumers used managed Chrome 155.0.8059.39, produced PNG/JPEG/WebP and 800×600 AV1 MP4, and removed owned runtime children. SDK/native engine declarations and staged optional dependencies matched their authoritative metadata. Screenshots and recording frames were visually inspected. The initial 2.017833-second recording decoded cleanly; the final staged sample played but ffmpeg reported a duplicate terminal timestamp, confirmed in its source packets by ffprobe (DTS 59659 at time base 1/30000). This remains a native-recording release-qualification caveat; no transcoding or timestamp rewriting was added to the SDK.

This pass does not qualify additional platforms or the host binary's release ABI, rerun the earlier container qualification, or publish packages. Earlier Node 22/24.18 execution evidence remains unchanged as history.

#### Strict Node SDK cleanup pass

Four parallel implementation slices covered transport, browser/page APIs, installation, and release/schema tooling. Public interfaces, protocol 1.3, the Node 24 baseline, and cancellation/ownership/integrity policies remain unchanged.

- Transport snapshots redacted stderr once per synchronous rejection batch and final teardown, and avoids recomputing normal line byte lengths. Promise settlement order and per-request evidence remain intact.
- Schema generation removes a Map/object/entries roundtrip, uses Set membership for required fields, and materializes strict-union property keys once. Canonical generated outputs remain unchanged.
- Installer cleanup removes redundant recursive directory creation and unsafe non-null assertions around stable lock ownership/tickets; worker path normalization no longer allocates a split array. Browser startup failure handling inlines a single-use async cleanup wrapper without losing combined failure evidence.
- SDK builds enforce `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch`. Incidental version-pin assertions and duplicate consumer-result assertions were removed; native recording checks now inspect the published file's size and MP4 signature.
- Node 24.21.0 verification: **239 tests passed, zero failed/skipped**, valid consumers compiled, all **22** invalid consumers rejected, and protocol projection conformance passed with the paired core worktree.
- The updated installer freshly downloaded managed Chrome 155.0.8059.39. A separately packed/installed Linux x64 consumer reused that cache, captured PNG/JPEG/WebP and native MP4, preserved an acknowledged real connection failure, completed a verified form fill, correlated 12 concurrent requests, and shared explicit/async disposal with no owned runtime children or remaining child-process resources. The 800×600 AV1 recording was visually inspected and decoded cleanly: 61 packets, strictly increasing DTS, 2.013833 seconds.

This sample does not resolve the earlier intermittent native timestamp caveat. No benchmark speedup, new platform/ABI qualification, publication, or recording format change is claimed.

### Phase 6 — Release workflows and draft review

Core review: [eoka#31](https://github.com/shrimp-software/eoka/pull/31), revision
`9bae96b9cefa02794a4058ae54b67273ad453490`. Reserved versions are core `0.7.0`,
server/protocol crates `0.5.0`, Rust SDK `0.6.0`, runner/datadome `0.3.0`, and npm
SDK/native packages `0.1.0-beta.1`. Wire protocol remains `1.3`.
Tools review: [eoka-tools#58](https://github.com/shrimp-software/eoka-tools/pull/58).

Implemented release infrastructure:

- `qualify-node-release.yml` builds all five native targets, enforces the GNU
  glibc 2.35 baseline, installs packed consumers outside the repository, and
  retains original recording/decoder evidence on failure. An exact `core_ref`
  permits paired-core previews; publication forbids that override.
- `publish-node.yml` permits only a manually selected exact protected-main
  revision, an existing reviewer-protected `npm-prerelease` environment, and
  same-run qualified artifacts. OIDC/provenance publication is native-first,
  SDK-last, under `next`; token fallback and implicit environment bootstrap are
  not supported. A separate registry consumer checks installation/provenance.
- Source and ordinary stages remain private. Publication staging and workflow
  qualification both enforce `publication-readiness.json`.

Fresh local evidence:

- Core formatting, all-feature clippy/tests, examples, Rust 1.85.0, and all
  **77** ignored Chrome integration tests passed with Chrome 155.0.8059.39.
- Full tools release-check and protocol conformance passed with the paired
  local core override. Go race tests passed. These do not supply a crates.io
  checksum for unpublished core 0.7.0.
- Node 24.21.0 clean installation, canonical generation, build, valid consumers,
  all **22** rejected consumers, and **264** SDK tests passed, with zero skips.
- The new qualifier initially rejected mise's npm shell wrapper; POSIX now
  executes npm normally while Windows invokes its JavaScript CLI without shell
  interpolation. The corrected qualifier completed every runtime check outside
  the repository with sandbox enabled: fresh/cache/offline installation,
  verified fill, six PNG/JPEG/WebP captures, headed launch, owned cleanup, and
  borrowed-browser survival. Three original AV1 MP4s each had 61 strictly
  increasing packet timestamps and no decoder stderr.
- That local run correctly ended **blocked**, not passed: it used a host-only
  development binary requiring glibc 2.39, and the known Chrome recorder defect
  remains unresolved. Publication smoke rejected both host-only staging and the
  recorded readiness blocker before creating output. No publication command ran.

The first hosted paired-core preview built both Linux and both macOS targets.
Windows exposed an unconditional Unix-only `libc::EDQUOT` reference in download
error classification. The classifier now uses portable `StorageFull` and
`QuotaExceeded` error kinds, preserving `download_no_space` without depending on
raw OS errno values. Its regression failed before and passed after the fix;
all four filesystem regressions, server clippy, formatting, and a Rust 1.85
capacity-error smoke passed locally. Those build results alone did not qualify
the browser runtime.

The next preview built and staged all five native targets. Linux x64 completed
the runtime checks and remained blocked by recording readiness.
The Windows run reached verified interaction and the first screenshot, exposing
an assertion against path spelling rather than filesystem identity. Screenshot
qualification now compares real paths. Recording identity validation accepts
equivalent absolute Windows namespace paths while still rejecting different or
relative destinations; a Windows-path smoke failed before and passed after, and
all 15 capture regressions passed. macOS and ARM Linux launch failures prompted
native diagnostics rather than removing either platform.

Hosted paired-source evidence:

- [Run 38534451688](https://github.com/shrimp-software/eoka-tools/actions/runs/38534451688),
  tools `3cf6a75881958a3fed47650b4d3309b44a8c27f7` with the core revision above,
  built/staged all five native targets and completed every runtime check:
  fresh/cache/offline installation, artifact rejection, verified input, six
  screenshots per platform, three original AV1 MP4s per platform, headed launch,
  owned cleanup, and borrowed-browser survival. All 15 recordings had strictly
  increasing packet DTS and empty decoder stderr. Native frames were inspected.
  The macOS samples had 27–38 frames over roughly two seconds; live capture does
  not promise fixed cadence.
- ARM's original failure also occurred with unmodified Chrome. Full stderr and
  kernel audit logs identified AppArmor user-namespace denial, not patching or
  missing CPU-frequency files. CI now permits only Chrome in a private per-job
  directory and removes the policy afterward. The global restriction remained
  enabled, and both Linux runs observed active PID/network namespaces and seccomp.
- The verified ARM Chrome archive's eight ELF images were inspected separately;
  their highest GLIBC requirement was 2.25. This is static ABI evidence, not an
  Ubuntu 22.04 ARM runtime claim. GNU server artifacts passed the 2.35 ABI ceiling.
- macOS now copies bundle files into owned storage, rejects escaping symlinks,
  preserves signing metadata, and requires strict signature verification.
  Four native regressions passed on each Mac architecture. The
  [isolated cache probe](https://github.com/shrimp-software/eoka-tools/actions/runs/38536488877)
  demonstrated failed-signature cache acceptance before the fix and rejection
  afterward on both architectures, without changing the source file. An earlier
  shared-target diagnostic produced contradictory results; independently built
  probes resolved them.
- [Core CI](https://github.com/shrimp-software/eoka/actions/runs/38533767068)
  passed all five jobs, including the full Chrome integration gate and Rust 1.85.
  All five packed runtime runs still correctly ended **blocked** by the recorded
  Chrome timestamp defect. No readiness flag was cleared and nothing was published.

The upstream defect reproduced in raw CDP (**2/40** short captures) and the SDK
(**1/40**). Across 224 investigative captures, four defective originals were
retained. A traced 21µs final-frame gap becomes 0.63 ticks at time base 1/30000
and truncates to zero, producing duplicate terminal DTS. FFmpeg can exit zero
while warning. This is not an Eoka stream-copy error; passing captures alone
do not resolve it. The approved downstream resolution is lossless container
timing repair, described below; Chromium's raw output remains affected. See
[tail-frame insertion](https://github.com/chromium/chromium/blob/155.0.8059.39/content/services/devtools_media_encoding_service/devtools_media_encoding_service_impl.cc)
and [timescale conversion](https://github.com/chromium/chromium/blob/155.0.8059.39/media/muxers/mp4_type_conversion.cc).

Remaining release prerequisites: actual core publication and registry-backed
lockfile regeneration; npm scope/package bootstrap with trusted publishers; and
the reviewer-protected environment (currently absent). Recording correctness is
resolved by the lossless finalization evidence below. Main is protected, but
local npm authentication is absent. No tags, main merges, or packages were published.
Existing Rust publishing workflows trigger on main, so merging the version
changes is a release action, not merely PR cleanup.

### Lossless recording finalization

The user approved MP4 timing metadata correction instead of maintaining a
Chrome fork or preserving defective output bytes. The official browser pin,
encoded video and public protocol remain unchanged.

- `crates/eoka-server/src/recordings/mp4.rs` validates single-track fragmented
  native containers using bounded metadata storage and seeks past encoded
  payloads. It validates all metadata before any correction.
- Within-fragment runs of `k` explicit zero durations become `k` one-tick
  durations, borrowing `k` ticks from the next duration only when it remains
  positive. Each run endpoint and subsequent timestamps stay unchanged. A
  duplicate terminal sample starting a new keyframe fragment instead advances
  its decode origin and matching index by one tick, reducing its duration by
  one tick. Corrected DTS is validated across fragment boundaries before writes.
  Encoded packets, sample count, file length and the final presentation endpoint
  remain unchanged. Already-valid files receive no writes.
- The original staging descriptor is retained through validation and repair.
  The five-second repair deadline and existing cancellation path precede
  flush/fsync/close and atomic non-overwrite publication. Invalid or unsupported
  corrections retain failure/partial evidence rather than claiming completion.
- Independent FFprobe/FFmpeg smoke repaired all four retained defective Chrome
  originals: packet hashes and exact durations stayed unchanged, DTS became
  strictly increasing, decoding emitted no warnings, and a second repair made
  no changes. All 15 retained clean five-platform originals remained identical.
- Four reconstructed cross-fragment cases use genuine native keyframe packets,
  with and without indexes and preceding samples. Independent FFprobe/FFmpeg
  checks confirmed strictly increasing DTS, unchanged packet hashes/duration
  and no decoder warnings. These are reconstructed cases, not claimed captures.
- A security review found an unbounded default-only sample loop. A 256 MiB
  sparse malformed input exceeded a two-second process limit before correction.
  Checked aggregate processing now rejects it immediately. Default-only range,
  timestamp-overflow and unchanged-valid-file regressions cover that path.
- Local recording/parser tests passed (31), as did artifact tests (5) and the
  complete paired-core `mise run release-check`. Actual managed SDK smoke
  completed eight recordings spanning short clips, 1/30/120 fps and a 6.5-second
  clip, with strict DTS, empty decoder diagnostics and owned-runtime cleanup.
  Linux namespace and seccomp sandbox checks passed. Initial launches with long
  runtime paths aborted; a short private `/tmp` runtime root worked without
  disabling the sandbox or changing the system Chrome installation.
- The full Node gate passed 264 tests with zero skips and both native gates
  enabled; all 22 invalid TypeScript consumers were rejected. Actionlint and
  qualification-script syntax checks passed.
- The first repair matrix, [38608946086](https://github.com/shrimp-software/eoka-tools/actions/runs/38608946086),
  exposed nine additional native failures: intermediate/consecutive zero
  durations on Linux x64 and both macOS targets. All 50 scheduled cases ran
  once; 41 completed, and all lifecycle checks completed. Linux ARM and Windows
  completed every runtime check but remained blocked by source readiness.
- Those nine unchanged originals are now regression fixtures. The generalized
  two-pass repair passed independent FFprobe/FFmpeg checks on all 13 retained
  defective files, preserving packet hashes, length and exact duration, with
  strict DTS, no decoder diagnostics and idempotence. All 41 already-valid
  finalized files from that matrix stayed byte-identical. The recording/parser
  suite now passes 41 tests, including late-invalid metadata, insufficient room,
  multiple runs, cross-trun runs and corrected cross-fragment collisions.
- After this extension, the complete paired-core release check and all 264 Node
  tests passed again with zero skips and both native gates enabled. Independent
  read-only timing and parser-security reviews reported no findings.
- The refreshed [native matrix 38612635401](https://github.com/shrimp-software/eoka-tools/actions/runs/38612635401)
  used tools `70b47e0ff27b271407c3073f5c1a389d87a5fd0a` and core
  `9bae96b9cefa02794a4058ae54b67273ad453490`. All 28 deterministic repair
  regressions passed on each of the five architectures. All 50 scheduled live
  recordings passed strict DTS and empty decoder diagnostics, without retries.
  Every target completed image, headed-launch, owned-cleanup and borrowed-browser
  checks; Linux sandbox checks remained enabled.
- Those jobs ended `blocked` only because source readiness had not yet been
  cleared. The recording blocker is now resolved by the causal repair,
  deterministic retained-file preservation proof and fresh native matrix.
  A ready-state preview must still complete artifact collection; publication
  additionally requires the registry-backed core and npm prerequisites above.
  No publication, tags or main merges are authorized.

### Review hardening: bounded history and verified publication recovery

- Partial npm publication now preflights all six packages and resumes only exact
  same-version artifacts from the original workflow run. Existing uploads require
  matching SHA-512, registry signatures, keyless provenance bound to the protected
  main source/workflow/event/run, and the existing `next` tag. Missing native
  packages precede the SDK; no automatic retry, overwrite, or retag is allowed.
  `sigstore@4.1.1` is a pinned release-tooling development dependency; verification
  uses its public API and standard isolated `npm audit signatures`, not npm
  internal modules. No registry publication was performed.
- Recording history retains at most 1,024 distinct IDs and 16 MiB of UTF-8 start
  parameters per sidecar. Admission checks both limits atomically with O(1)
  accounting. Terminal entries are not evicted; recovery/conflict semantics take
  precedence over capacity. New IDs fail with `ServerBusy` before dispatch or
  file staging. Only a new sidecar session resets capacity.
- The MP4 parser remains private and narrowly scoped. Artifact collection and
  publication now validate actual running Chrome identity and all ten native
  recording cases, published artifacts, exact strictly increasing DTS and empty
  decoder diagnostics instead of trusting a passing summary. Native builds run
  the entire recording suite on every architecture, including admission tests
  with platform-native paths. Chrome upgrades require fresh five-platform proof.
- Local checks passed: 47 recording tests; Node schema generation/build/types
  and 289 passing tests, with two opt-in native tests skipped; Rust formatting
  and server Clippy with warnings denied. The 27 new release-tooling tests cover
  partial/lost-response recovery, trust failures, the public Sigstore verifier
  and recording-evidence rejection.
- Actual sidecar smoke compared the prior qualified binary with the new binary:
  the old process admitted past both limits; the new process rejected overflow
  without reserving IDs or writing files, preserved existing-ID recovery and
  conflicts, and reset capacity in a fresh session. A real read-only npm smoke
  installed and signature-verified a foreign macOS ARM package on Linux with
  lifecycle scripts disabled. Prior passing native evidence was rejected because
  it lacked actual running Chrome identity; it is not reused as new qualification.
  Exact-source native results and artifacts are attached to draft PR #58.

### Execution order and handoff rules

- Phases 1–5 are implemented. Next: phase 6 native-platform/release qualification; publication remains separately authorized.
- Preserve the paired core/server release pins and generated protocol contracts when staging artifacts.
- Keep deterministic scenario playback, camera/caption policy and GIF/H.264 encoding in Topology rather than silently expanding native recording.
- Each task owns its corresponding contract tests and documentation. Phase-ending tasks provide integration/runtime proof, not a substitute for completing earlier work.
- Shared protocol/server files need one integration owner. Independent workers must not redefine DTOs, recording states or browser ownership independently.
- Do not mark a parent phase complete until its runtime gate passes. Do not publish or start Topology migration merely because phases 1–5 compile.

## References

- [Puppeteer browser installer](https://pptr.dev/browsers-api)
- [Installer options, including expectedHash](https://pptr.dev/browsers-api/browsers.installoptions)
- [Chrome for Testing version/download manifest](https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json)
- [Native Chrome recording protocol](https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-startScreenRecording)
- [Puppeteer page.record](https://pptr.dev/api/puppeteer.page.record)
- [npm package platform selectors](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)
- [npm trusted publishers](https://docs.npmjs.com/trusted-publishers/)
