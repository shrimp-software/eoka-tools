# Node SDK architecture and integration contract

The Node SDK, managed Chrome, capture, and native recording are implemented in
`clients/node`. This document preserves architectural decisions and Topology's
application integration requirements, not the completed implementation journal.

Core [eoka#31](https://github.com/shrimp-software/eoka/pull/31) is merged and
`eoka 0.7.0` is published with registry checksum
`447a498ec28c34b2020be583617c70f89dc718228bb02db5750f72e4da333a1e`.
Normal production builds must consume registry core with its real checksum;
paired-checkout patches and metadata overrides are no longer a release path.
The SDK and native npm packages remain private by default. Core publication
neither satisfies npm prerequisites nor authorizes merges, tags, or publication.
Historical native runs below do not qualify newly changed source.

## Documentation ownership

- [Node SDK README](clients/node/README.md): public API, options, platforms
  and examples.
- [Node SDK releasing](clients/node/RELEASING.md): versions, staging,
  qualification and the npm release procedure.
- [PROTOCOL.md](PROTOCOL.md): wire negotiation, methods, schemas, numeric/size
  limits, recording states, and lifecycle semantics.
- [OPERATIONS.md](OPERATIONS.md): consequential-action evidence, operation
  identity/storage, recovery, and file-publication behavior across adapters.
- This document: component boundaries, non-negotiable safety decisions,
  Topology-owned timing/export responsibilities, and historical evidence.

Canonical tool and sidecar definitions live in `eoka-protocol`. Generate schemas,
TypeScript contracts, and downstream descriptors from that source; do not define
parallel DTOs or edit generated files independently. Preserve wire/public
signatures and generated JSON/TypeScript during simplification. The sidecar
registry is not the complete CLI/MCP/Tack tool catalog.

## Architecture and product boundaries

| Component | Responsibility |
| --- | --- |
| Core Rust library | Chrome launch/CDP, browser ownership, viewport/capture primitives, target-bound raw recording streams |
| `eoka-server` | Private NDJSON sidecar, negotiated capabilities/limits, operation management, recording history, file policy, bounded MP4 finalization |
| Node SDK | One owned sidecar per `Browser`, installation/resolution, typed Browser/Page/Operation/Recording APIs, local cancellation and bounded disposal |
| Topology exporter | Scenario/render clock, application readiness, camera/captions/crop, deterministic frame sampling, encoding presets, final export policy |

Use stdin/stdout, not a CLI daemon, socket service, native Node addon, or WebAssembly
port. Rust controls Chrome; JavaScript installs it and supplies its path, not a
second Puppeteer automation stack. Keep npm packaging to compiled ESM and
TypeScript declarations with explicit export/file allowlists; no bundler or native
addon toolchain is required. CommonJS callers use dynamic import.

Support Node 24 LTS starting at 24.21.0 (`^24.21.0`), pinned for development/CI
through `mise.toml`. Earlier Node 22/24.18 results are historical only. Do not
claim browser-runtime JavaScript, Bun, Deno, or drop-in Puppeteer compatibility.
Evaluation exchanges JSON-compatible data, not serialized closures or JSHandles.

SDK and native npm packages share an exact npm version. Rust server/core versions
are pinned separately in release metadata; wire compatibility depends on protocol
negotiation and actual methods/capabilities, never package-version equality.
Server capability presence is not proof that an attached Chrome supports an
encoder or command. Preserve typed access to every implemented sidecar method
without promising daemon-only functionality.

### Installation and runtime isolation

- Import and npm installation are silent and have no process/download/cache side
  effects or lifecycle scripts. Explicit preparation and first managed launch
  install the pinned full Chrome distribution using `@puppeteer/browsers`.
- Never select runtime `latest`, silently discover a system browser, substitute a
  build/mirror, compile Rust, or download an unpinned server. Explicit paths and
  environment overrides remain deliberate escape hatches and must negotiate.
- Pin reviewed archive hashes from controlled preparation; they are integrity
  pins, not vendor signatures. Verify archive and installed companion files,
  coordinate concurrent installs, and atomically publish complete cache entries.
  Interrupted installs cannot appear usable. TLS/hash failures fail closed.
- Preserve complete distributions, resources, libraries, and signed macOS bundle
  metadata. Managed downloads remain immutable; patch only owned copies, reject
  escaping links, and verify signing where required. Do not duplicate core patching
  in Node or mutate the source executable.
- Separate persistent browser cache, private per-browser runtime/profile/temp
  state, and operation state. Keep child-only temporary environment changes out
  of the Node/global server environment. Surface quota/space errors with location.
- Verified offline/read-only cache reuse must not require network or writes.
  Browser close never removes cached builds or caller-owned roots/profiles.
  Cache removal is an explicit administrator action, not automatic retention.
- Do not install privileged OS dependencies or host sandbox policies. The SDK
  requests sandboxing by default; `sandbox: false` is an explicit opt-out, never
  an automatic retry. Existing low-level Rust/server/Go defaults remain separate.

### Evidence, cancellation, and ownership

Preserve bounded incremental NDJSON framing, safe request IDs, negotiated message
and in-flight limits, stdout backpressure, out-of-order correlation, and redacted
bounded stderr. Transport corruption/exit settles outstanding calls rather than
leaving promises pending. Dynamic JSON, operation arrays, and successful null
results retain their actual shapes.

Allocate consequential-action IDs before transmission. Admission is not completion:
strict conveniences require verified receipts and applicable explicit expectations.
Errors retain identity, dispatch evidence, receipts, and the primary failure even
when cleanup also fails. Never automatically replay an uncertain action or restart
a browser to conceal uncertainty. Local abort/deadline is not remote rollback;
request cancellation, operation cancellation, and recording cancellation differ.
Preserve existing durable operation identity/history guarantees without implying
that sidecar-session recovery is persistent crash recovery.

Keep ordinary page/action ordering while admitted-operation controls, recording
status/cancel, EOF, and shutdown remain responsive. Shared close/finalization work
must outlive abandoned waiters. Cleanup is bounded and removes only owned
resources after termination is confirmed; uncertain cleanup retains evidence and
runtime paths. Borrowed Chrome is detached, not killed. Explicit tab close remains
an intentional destructive request. Forced termination cannot promise destructor
cleanup or recovery from SIGKILL.

## Capture and recording safety

Use viewport DPR as the single scale control and CSS pixels for screenshot crops.
Preserve bounded screenshot Buffers and explicit direct-file output; file output
does not remove Chrome/CDP limits. Resolve paths once in the Node caller. Stage
privately beside the destination, verify/flush/synchronize/close, and publish
atomically without overwriting. Failures retain partial-file evidence rather than
claiming a success-named artifact.

### Raw core versus finalized SDK output

Core's native recording primitive yields Chrome's raw stream. It does not promise
valid finalized MP4 timing or apply sidecar file policy. The server/SDK save path
adds bounded validation and narrowly scoped lossless timing repair before
publication. Do not collapse these contracts or promise raw-byte identity for
repaired output.

- Keep one active recorder per target, caller-known recording IDs, same-ID recovery
  and conflict detection, shared stop/finalization, truthful terminal states, and
  bounded non-evicting recording history. Recovery is live-session-only.
- Require a ready active page and supported native encoder. Unsupported recording
  fails explicitly; no alternate engine, browser build, or algorithm is selected.
- Stream bounded chunks to staging; never buffer the whole video or return giant
  base64 JSON. A waiter abort does not cancel finalization. Explicit cancellation
  and publication retain their defined arbitration and late-outcome evidence.
- Validate metadata before correction on the original staging descriptor, with
  bounded storage/work and the repair deadline. Unsupported/malformed containers,
  insufficient correction room, and unresolved timing collisions fail safely.
- Repair only the documented native zero-duration/tail defects, preserving encoded
  packets, sample count, file length, and final presentation endpoint. Clean files
  remain byte-identical. This private parser is not a generic muxer/transcoder.
- Only a completed, published artifact is saved output. Close cancels unfinished
  recording by default; explicitly stop to save. Partial/uncertain cleanup cannot
  become success merely because a late file appears.

Native output is silent MP4 with Chrome-selected codec, not promised H.264 or
fixed-fps playback. Native `fps` is a ceiling. Audio, screenshot-style recording
crop/quality, and deterministic live timing are not implied. FFmpeg is not an SDK
runtime dependency; it belongs to explicit exporter postprocessing and independent
release validation.

## Topology application integration

Topology's source has not been inspected by this work. Inspect its actual README
recorder, scenario API, and rendering clock before implementation. Replace or
generalize that recorder rather than maintaining a parallel implementation.
No Topology migration is implied by SDK completion or publication.

Users author scenario specifications, not Puppeteer scripts. Topology imports the
SDK and owns scenario validation, playback, camera keyframes, captions, crop,
quality presets, duration/fps/scale, and output naming. A single application export
command should offer deterministic frames and live modes.

### Deterministic frames mode

1. Start/reuse the local app server, launch managed Chrome, and set fixed
   viewport/DPR. Seed randomness and await explicit fonts/images/assets readiness;
   navigation commit or generic network-idle is not application readiness.
2. Expose application `prepare`, `renderAt(timeSeconds)`, and completion metadata
   in the main world. Pause autonomous simulation/rAF and drive physics, layout,
   randomness, CSS/Web Animations, and scenario media from the explicit clock.
3. For integer frame `i`, await completed rendering at `i / fps`, then capture.
   Never approximate stepping with real-time sleeps. Set frame count to
   `ceil(duration * fps)`, sample from zero, and report requested duration plus
   encoded duration `frameCount / fps`.
4. Render camera/captions before capture. Use DPR for scale and one documented
   CSS-to-output-pixel crop conversion. Keep output dimensions fixed; H.264
   yuv420p requires even dimensions, using explicit padding rather than stretching.
5. Feed ordered frames to one encoder with backpressure and a bounded queue.
   Use screenshot files when frames exceed transport limits; do not accumulate
   all frames or launch the Eoka CLI per frame.
6. Select H.264/pixel format/quality explicitly for MP4; use palette generation/use
   for GIF. GIF centisecond timing cannot exactly represent every fps: choose and
   report the closest supported timing rather than promise exact 30 fps.
7. Cancellation closes the encoder and owned capture/browser resources. Publish
   only after encoding and validation succeed, with no success-named partial.

Topology must preflight an explicit `ffmpegPath` or installed FFmpeg and required
codecs before playback. Missing FFmpeg is an actionable prerequisite, not a fake
fallback. Bundling/managing FFmpeg requires a separate licensing/build/platform
decision and is outside the SDK.

Acceptance requires deterministic sampled simulation states and frame counts,
not merely a playable file. Cover delayed assets/fonts, duration/fps boundaries,
padding/GIF timing, missing/failing encoder, cancellation, and visible
camera/captions/crop. Cross-OS/GPU/font bit-identical rendering is not promised;
pin browser, fonts, platform, and rendering settings when repeatability matters.

### Live mode

Start native recording before playback, await explicit application completion,
then stop/finalize. Camera/captions remain app-owned. Different codecs, crop, or
GIF require an explicit postprocessing stage. Live timing is not deterministic.

## Qualification and maintenance contract

Retain all five native targets: Linux x64/arm64 GNU, macOS x64/arm64, and Windows
x64. GNU releases enforce glibc 2.35 compatibility using baseline-built artifacts.
Configured builds/downloads are not runtime guarantees; do not silently drop a
promised target. musl/Alpine and Windows arm64 are outside managed support.
Explicit paths are expert overrides, not additional support claims.

Require clean, outside-repository packed consumers without Cargo/CLI dependencies:
fresh/cache/offline installation, verified actions and JSON evaluation, actual
image inspection, native recording validation, headed launch where applicable,
owned cleanup, and borrowed-browser survival. Exercise minimum/current supported
Node and the supported Linux baseline. Keep transport/installer/ownership,
uncertainty, no-overwrite, quota/disconnect, and repair/security regressions; do
not replace behavioral coverage with source-text assertions or artifact counts.

The SDK README owns the exact native schedule and publication procedure. Every
Chrome pin change requires the complete recording suite and fresh five-platform,
single-attempt native evidence. Collection and publication independently validate
exact source/provenance, actual running Chrome identity, complete artifacts,
strictly increasing packet DTS, and full decoding. A passing summary or decoder
exit alone is insufficient. Only the documented, semantically proven VFR
estimation advisory is allowable; preserve raw diagnostics and independently
recompute classification. Unknown/inexact diagnostics fail; no retry-to-pass,
hidden filtering, or retiming to satisfy a frame-rate estimator.

Use the same controlled server artifacts for npm and native releases, retaining
real digests/provenance. Host-only staging remains development evidence, never
production qualification. PR-triggered workflows use GitHub-hosted runners.
Registry-backed qualification of the changed source remains required; historical
paired-source previews are not a substitute.

npm scope/package bootstrap, trusted publishers, reviewer-protected environment,
registry-install/provenance smoke, and explicit publication approval remain
separate prerequisites. Preserve private source/ordinary stages, native-first
SDK-last prerelease publication, and exact same-run artifact verification for
partial-publish recovery. No automatic overwrite, retry, or retagging. Rust
publishing workflows trigger on main, so merging version changes is a release
action, not routine PR preparation.

Change owning API/protocol/operations docs and affected clients together; public
core additions also require core API docs/changelog. Regenerate canonical outputs
and run integrated conformance/release gates after integration. Do not mark
qualification complete based on compilation, skipped native tests, or older runs.

## Historical evidence

These links explain decisions, not qualification of the current revision:

- [Tools review #58](https://github.com/shrimp-software/eoka-tools/pull/58): original
  implementation/review and attached exact-source evidence; the completed phase
  checklists and run-by-run journal are superseded by this contract.
- [Five-target preview 38534451688](https://github.com/shrimp-software/eoka-tools/actions/runs/38534451688):
  packed native/runtime coverage; jobs remained blocked by raw recording timing.
- [macOS isolated cache probe](https://github.com/shrimp-software/eoka-tools/actions/runs/38536488877):
  demonstrated signature-invalid cache acceptance before the fix and rejection
  after it on both Mac architectures without changing the source executable.
- Raw Chrome reproduced duplicate terminal DTS outside Eoka. Chromium's
  [tail-frame insertion](https://github.com/chromium/chromium/blob/155.0.8059.39/content/services/devtools_media_encoding_service/devtools_media_encoding_service_impl.cc)
  and [timescale conversion](https://github.com/chromium/chromium/blob/155.0.8059.39/media/muxers/mp4_type_conversion.cc)
  explain why passing live samples alone could not resolve the defect.
- [First repair matrix 38608946086](https://github.com/shrimp-software/eoka-tools/actions/runs/38608946086)
  exposed intermediate/consecutive zero durations. Its retained defective
  originals and clean files supplied lossless repair/preservation regressions.
- [Refreshed repair matrix 38612635401](https://github.com/shrimp-software/eoka-tools/actions/runs/38612635401):
  50 one-attempt finalized recordings and 28 deterministic repair regressions per
  architecture passed at tools `70b47e0ff27b271407c3073f5c1a389d87a5fd0a` and core
  `9bae96b9cefa02794a4058ae54b67273ad453490`. Jobs still ended source-readiness
  blocked; later evidence requirements cannot be backfilled from that summary.
- [Native run 38636518503](https://github.com/shrimp-software/eoka-tools/actions/runs/38636518503)
  produced all 50 scheduled files but failed Windows/macOS Intel qualification.
  It is failure evidence, not a successful qualification. The Mac VFR case and
  FFmpeg's [input frame-rate estimator](https://github.com/FFmpeg/FFmpeg/blob/n9.0.1/libavformat/demux.c#L2834-L2871)
  motivated semantic advisory validation, not decoder-warning suppression or a
  media timing change.

External contracts: [browser installer](https://pptr.dev/browsers-api),
[expected-hash installation options](https://pptr.dev/browsers-api/browsers.installoptions),
[Chrome for Testing downloads](https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json),
[native recording protocol](https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-startScreenRecording),
and [npm trusted publishers](https://docs.npmjs.com/trusted-publishers/).
