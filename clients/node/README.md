# @cbxss/eoka

The Node.js SDK for the eoka NDJSON browser sidecar. The source package remains private and unpublished; npm version `0.1.0-beta.1` is reserved for the qualified beta release and can be built and packed locally for integration testing. It supports **Node.js 24 LTS, starting at 24.21.0** (`^24.21.0`) and exports compiled ESM and TypeScript declarations. Node 22 and Current/non-LTS lines are outside the support policy. CommonJS `require` is not an export.

## Current scope and platform qualification

The SDK provides correlated subprocess transport, typed browser/page operations, durable operation handles, pinned full Chrome for Testing installation, verified native server resolution, PNG/JPEG/WebP capture, native MP4 recording, and bounded disposal. It does not provide deterministic scenario replay or video transcoding. Importing the package and installing with `--ignore-scripts` do not launch processes, download Chrome, or create a browser cache. There are no install lifecycle scripts, Cargo requirements, or package command-line binaries.

Managed targets are `linux-x64-gnu`, `linux-arm64-gnu`, `darwin-x64`, `darwin-arm64`, and `win32-x64`. Paired-core preview builds and runtime checks have completed on all five; **publication remains blocked**, as described below. Linux managed selection requires glibc >= 2.35 and rejects musl/Alpine; Windows arm64 and unknown combinations are rejected before downloads. GNU release artifacts are built on Ubuntu 22.04 and checked for GLIBC requirements no higher than 2.35. Arbitrary source-built or host-development binaries are not qualified by that result.

Both explicit server and Chrome paths are an expert override for managed platform checks, not a new support guarantee. Node.js support does not imply Bun, Deno, browser-side JavaScript, or drop-in Puppeteer compatibility. Packed npm consumption is verified; pnpm/Yarn and registry-backed publication remain separate gates.

## Managed launch and explicit preparation

```js
import { Browser, installBrowser } from "@cbxss/eoka";

await installBrowser({
  browserCacheDir: "/opt/eoka/cache",
  timeoutMs: 300000,
  onProgress: ({ downloadedBytes, totalBytes, buildId }) => {
    process.stderr.write(`${buildId}: ${downloadedBytes}/${totalBytes} bytes\n`);
  },
});

const browser = await Browser.launch({
  browserCacheDir: "/opt/eoka/cache",
  download: "never",
  sandbox: true,
  timeoutMs: 30000,
});
try {
  const page = await browser.newPage("data:text/html,%3Ctitle%3Eoffline%3C/title%3E");
  console.log(await page.invoke("page.evaluate", { pageId: page.id, js: "document.title" }));
} finally {
  await browser.close();
}
```

`Browser.launch()` resolves its server **before** installing Chrome. Server precedence is `serverPath` > `EOKA_SERVER_BIN` > exact optional platform package. It never searches PATH, builds Rust, or downloads a server. Browser precedence is `executablePath` > `EOKA_CHROME_PATH` > the managed pin. There is no fallback to a system browser or a different build. `Browser.connect()` resolves/starts the server but does not install or access the managed Chrome cache.

`installBrowser` prepares only Chrome; it does not install system libraries or native server packages. Its options are `browserCacheDir`, `download` (`"auto"` by default or `"never"`), `signal`, `timeoutMs` (300000 ms by default), and `onProgress`. It returns `{ executablePath, cacheDir, buildId, platform, source }`, with `source: "cache"` or `"download"`. Cancellation/deadlines terminate the owned installer worker before cleaning its own staging resources. One launch deadline covers server resolution, Chrome installation, handshake, and native launch.

`onProgress` receives download byte counts without writing to stdout. Without a callback, the first actual download chunk emits one diagnostic to stderr; verified cache hits and imports remain silent.

## Cache, runtime, and offline operation

Cache root precedence is `browserCacheDir` > `EOKA_BROWSER_CACHE_DIR` > the per-user OS default:

| OS | Default cache root |
| --- | --- |
| Linux | `${XDG_CACHE_HOME}/eoka`, or `~/.cache/eoka` |
| macOS | `~/Library/Caches/eoka` |
| Windows | `${LOCALAPPDATA}/eoka/Cache`, or `~/AppData/Local/eoka/Cache` |

The installer keeps the complete distribution, including resources, libraries, and macOS bundle contents. It hashes the archive against the release pin, inventories the full installed tree, stages publication atomically, and coordinates concurrent installs per build. Cache validation detects missing/corrupt companion files as well as executable changes. Local writable inventory metadata is a same-user trust boundary, not protection against a malicious account that can replace both data and metadata.

Preinstall using the same platform/build and cache root as the eventual runtime. Preserve the **whole** cache entry and its metadata; copying just Chrome's executable is insufficient. A verified cache hit requires no network or writes, including on a read-only image. `download: "never"` fails with `BrowserNotInstalled` on an absent entry and does not repair corruption by downloading. Cache integrity failures are explicit. Default cache retention is persistent; `Browser.close()` never removes installed Chrome. Remove unused cache roots only as an explicit administrator action when no session is using them.

Runtime root precedence is `runtimeDir` > `EOKA_RUNTIME_DIR` > the OS temporary directory. Every launch gets a private child beneath that root. Child-only `TMPDIR`/`TMP`/`TEMP` direct native scratch work there without changing the Node process's environment. Keep runtime writable even when the browser cache is read-only. Owned child directories are cleaned only after owned-process termination is confirmed; uncertain termination retains the directory and reports its path through `RuntimeCleanupUncertain`. Caller roots, persistent `userDataDir` profiles, and the cache are not deleted. On POSIX keep the root short enough for Chrome's singleton Unix socket. Do not share persistent profiles concurrently; they contain cookies and other private session data.

From an installed SDK application, run the shipped examples directly:

```sh
node node_modules/@cbxss/eoka/examples/install-browser.mjs /opt/eoka/cache
EOKA_RUNTIME_DIR=/tmp/eoka node node_modules/@cbxss/eoka/examples/offline.mjs /opt/eoka/cache
```

The offline example uses a data URL, so its page also requires no network. `download: "never"` governs installation, not web page requests; use an OS/container network policy to prevent application traffic.

## Download pins, size, and proxy configuration

This source pins full Chrome for Testing **155.0.8059.39**, not ChromeDriver or a runtime-selected latest version. `releaseManifest` and `@cbxss/eoka/release-manifest.json` expose the release metadata. The archive SHA-256 values were computed from full HTTPS archives during controlled release preparation; they are reviewed integrity pins, **not vendor signatures**. Runtime installation uses those embedded pins and never looks up a hash or substitutes a mirror/build. Browser updates require a deliberate new release and fresh native gates.

Measured compressed archive sizes for this pin:

| Chrome platform | Bytes |
| --- | ---: |
| linux64 | 198195769 |
| linux-arm64 | 198202003 |
| mac-x64 | 203101743 |
| mac-arm64 | 192634606 |
| win64 | 207197429 |

Budget additional space for the expanded distribution, temporary archive/staging, and concurrent runtime profiles. The SDK does not download browser drivers.

The installer honors `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, and `NO_PROXY` through its pinned proxy dependency. Configure them in the setup process environment, or in `serverEnv` for a launch-scoped child environment. Do not put proxy credentials in progress logs. Standard Node trust configuration such as `NODE_EXTRA_CA_CERTS` must be set before starting Node when an enterprise CA is required. TLS failures and hash mismatches fail closed; do not disable certificate verification.

```sh
HTTPS_PROXY=http://proxy.example:8080 NO_PROXY=localhost,127.0.0.1 \
  node node_modules/@cbxss/eoka/examples/install-browser.mjs /opt/eoka/cache
```

## Linux libraries, sandbox, and containers

The SDK does not install privileged OS packages. Chrome requires its host libraries and fonts even in headless mode: NSS/NSPR, ATK/AT-SPI, CUPS, DRM/GBM, X11/XCB/Xcomposite/Xdamage/Xfixes/Xrandr, xkbcommon, ALSA, GLib, Cairo/Pango, and fonts. The native server's dependencies are artifact-specific; current Linux staging also needs OpenSSL 3 (`libssl.so.3` and `libcrypto.so.3`). Inspect actual release binaries and qualify the chosen distribution rather than treating this list as an ABI certification.

Sandboxing is enabled by default. Run as an unprivileged user with Chrome's sandbox requirements satisfied (including the host/container user-namespace and seccomp policy). A sandbox failure is an error; the SDK never retries with sandbox disabled. `sandbox: false` is an explicit security-reducing expert choice for a separately isolated environment, not the default container recipe.

Ubuntu 23.10+ can deny unprivileged user namespaces through AppArmor even when
Chrome itself is sandbox-enabled. An administrator must provide a narrowly
scoped executable-path policy for the owned Chrome runtime, as described in
[Chromium's AppArmor guidance](https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md).
Any process able to replace an executable at an allowed path gains that
permission; use a private runtime directory, not a broad `/tmp/**/chrome` rule.
The SDK neither installs privileged policies nor disables the host restriction.
ARM release CI uses a private per-job path exception, removes it afterward,
and checks Chrome's actual PID/network namespace and seccomp status.

For example, prepare a Linux x64 image with Node 24.21.0 and a distribution new enough for the **actual staged binary**. The following Debian trixie setup is a recipe to qualify, not a statement of completed container support. Supply the two locally staged tarballs described below as build inputs:

```dockerfile
FROM node:24.21.0-trixie
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates fonts-liberation libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64 \
    libcairo2 libcups2t64 libdbus-1-3 libdrm2 libgbm1 libglib2.0-0t64 libnspr4 \
    libnss3 libpango-1.0-0 libssl3t64 libx11-6 libxcb1 libxcomposite1 libxdamage1 \
    libxext6 libxfixes3 libxkbcommon0 libxrandr2 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN chown node:node /app
COPY --chown=node:node cbxss-eoka-0.1.0-beta.1.tgz cbxss-eoka-server-linux-x64-gnu-0.1.0-beta.1.tgz ./
USER node
RUN npm init -y && npm install --ignore-scripts --omit=optional --no-audit --no-fund \
    ./cbxss-eoka-server-linux-x64-gnu-0.1.0-beta.1.tgz ./cbxss-eoka-0.1.0-beta.1.tgz
ENV EOKA_BROWSER_CACHE_DIR=/app/browser-cache
ENV EOKA_RUNTIME_DIR=/tmp/eoka
RUN node node_modules/@cbxss/eoka/examples/install-browser.mjs
CMD ["node", "node_modules/@cbxss/eoka/examples/offline.mjs"]
```

Network is needed during image preparation for npm dependencies and the pinned Chrome ZIP. Runtime can then be isolated with `docker run --init --network=none --read-only --tmpfs /tmp:rw,nosuid,size=1g IMAGE`, provided the host's sandbox policy permits Chrome. Use an init process to reap orphaned browser children. Preserve cache ownership/metadata in the image and keep `/tmp` writable. Adapt library names, resource limits, and sandbox policy to the deployment; never use an Alpine/musl base for managed mode.

## Build, source staging, and local consumption

This SDK and its five native packages share reserved npm version `0.1.0-beta.1`. The native server is pinned separately to `0.5.0`, paired core to `0.7.0`, and the protocol to `1.3`; npm version equality is not wire compatibility. Release manifests accept exact semantic versions, including prereleases, but reject floating ranges, tags, and malformed versions. Packaged servers are checked against their name/version/platform, staged release metadata, actual binary digest, and expected handshake version. Explicit servers still must pass the protocol handshake.

The source manifest intentionally has no fabricated hashes for unprepared native artifacts. The source package omits native optional dependencies so a clean `npm ci` does not need unpublished packages. Staging derives all five exact native dependencies from the validated release manifest and adds them to the staged SDK's package metadata. Packing the source SDK alone is useful for import/type gates, but is **not** a ready managed server distribution. Missing native packages (including after `--omit=optional`) raise `ServerPackageMissing`; a present package with an unstaged SDK pin raises `ReleaseNotPrepared`. There is no compile/download fallback.

Run `mise install` at the repository root to install the pinned Node 24.21.0 toolchain. With mise activated, prepare local artifacts from this package directory without publishing:

```sh
npm ci --ignore-scripts --omit=optional
npm run build
npm run stage:release -- --artifacts /absolute/path/to/release-server-artifacts \
  --output /absolute/path/to/new-stage --host-only
npm pack --ignore-scripts --pack-destination /absolute/path/to/tarballs \
  /absolute/path/to/new-stage/platforms/linux-x64-gnu
npm pack --ignore-scripts --pack-destination /absolute/path/to/tarballs \
  /absolute/path/to/new-stage/sdk
```

Use the detected host ID in the platform path. `--host-only` is a local gate, not qualification of unbuilt targets: it marks the stage as development and embeds the Linux artifact's measured minimum glibc, which may exceed 2.35. Without that flag, staging requires all five target artifacts and rejects Linux binaries whose `readelf` GLIBC requirements exceed 2.35; repeated `--platform ID` selects specific preparation targets. Staging consumes existing server release binaries, computes their real digest/size, and writes matching SDK/platform metadata; it does not invoke Cargo or build alternate binaries. Use the same controlled artifacts as the server release and retain their provenance. Unstaged targets remain explicitly unavailable.

The artifact directory uses the existing server release asset names:

| Platform ID | Required server artifact |
| --- | --- |
| linux-x64-gnu | `eoka-server-linux-amd64` |
| linux-arm64-gnu | `eoka-server-linux-arm64` |
| darwin-x64 | `eoka-server-darwin-amd64` |
| darwin-arm64 | `eoka-server-darwin-arm64` |
| win32-x64 | `eoka-server-windows-amd64.exe` |

In a separate ESM application, install the matching native tarball as a direct dependency together with the staged SDK:

```sh
npm install --ignore-scripts --omit=optional \
  /absolute/path/to/cbxss-eoka-server-linux-x64-gnu-0.1.0-beta.1.tgz \
  /absolute/path/to/cbxss-eoka-0.1.0-beta.1.tgz
npm install --save-dev typescript@5.9.3 @types/node@24.19.2
```

The local direct dependency deliberately avoids relying on unpublished optional packages being available in a registry. A future qualified published release will install the exact matching optional package normally; do not omit it unless supplying an explicit server path. No publishing is performed or authorized by source staging.

Use `"type": "module"`, TypeScript `target: "ES2024"`, and `module`/`moduleResolution: "NodeNext"`. The root export supplies runtime SDK classes, installation APIs/errors, release/platform metadata, options, and common protocol types. `@cbxss/eoka/protocol` exposes the complete wire map. CommonJS consumers use dynamic `import()`. Runtime dependencies pin AJV `8.17.1`, `@puppeteer/browsers` `3.2.4`, `yauzl` `3.4.0`, and `proxy-agent` `8.0.1`; their engine requirements are compatible with the SDK's Node 24 LTS baseline.

## Explicit-path launch and verified receipts

```js
import { randomUUID } from "node:crypto";
import { Browser } from "@cbxss/eoka";

const browser = await Browser.launch({
  serverPath: "/absolute/path/to/eoka-server",
  executablePath: "/absolute/path/to/chrome",
  headless: true,
  sandbox: true,
  viewport: { width: 1024, height: 768, deviceScaleFactor: 1 },
  timeoutMs: 15000,
});

try {
  const html = '<!doctype html><label>Name <input id="name"></label>';
  const page = await browser.newPage(`data:text/html,${encodeURIComponent(html)}`);
  const target = { query: { kind: "css", selector: "#name" } };
  const operationId = randomUUID();
  const receipt = await page.fill(target, "Ada", {
    operationId,
    timeoutMs: 10000,
    actionOptions: {
      expect: { kind: "value", target, value: "Ada" },
      timeout_ms: 5000,
    },
  });
  console.log({ operationId, receipt });
  console.log(await page.invoke("page.evaluate", {
    pageId: page.id,
    js: '({name: document.querySelector("#name").value})',
  }));
} finally {
  await browser.close();
}
```

`fill`, `goto`, `select`, `hover`, and the other action conveniences resolve with an actual canonical `ActionReceipt` only after terminal verified completion. Admission is not completion. `click`, `type`, `pressKey`, `submit`, `drag`, and consequential actions passed to `perform` require an explicit `actionOptions.expect` before dispatch. Locator strings mean CSS selectors; generated `LocatorSpec` objects retain the canonical query/frame/index semantics.

`newPage(url)` and `goto(url)` verify a navigation commit, not DOM parsing or application readiness. Before parser-dependent reads, use `evaluate` to await `DOMContentLoaded` only while `document.readyState === "loading"`; resolve the promise to a JSON-compatible value such as `true`. `goto` accepts an explicit `actionOptions.expect` when navigation must also establish application readiness.

Run the included example after building:

```sh
npm run example -- /absolute/path/to/eoka-server /absolute/path/to/chrome
```

Alternatively set `EOKA_SERVER_BIN` and `EOKA_CHROME_PATH`. Explicit options take precedence. Providing both paths bypasses managed installation/platform selection; no arbitrary system discovery is performed. `userDataDir` remains caller-owned. A temporary profile belongs to its owned session.

## Durable operations and uncertainty

```js
const operationId = randomUUID();
const operation = await page.perform(
  { kind: "fill", target, value: "Ada" },
  { operationId, actionOptions: { expect: { kind: "value", target, value: "Ada" } } },
);
const record = await operation.wait({ waitTimeoutMs: 1000, timeoutMs: 2000 });
console.log({ operationId, state: record.state, outcome: record.outcome, receipt: record.receipt });
```

`perform` returns an `Operation` after admission. `Operation.get`, `wait`, and `cancel` return canonical operation records. A bounded `wait` may return a still-running record; it does not assert success. `waitTimeoutMs` bounds the server wait, while `timeoutMs` and `signal` govern the local call. Cancellation is not rollback.

Get/cancel for an already-admitted operation remain responsive while an unrelated page request waits. Unknown IDs retain ordering behind earlier queued admissions. Operation lists are independent snapshots of currently admitted records, not barriers for queued mutations. These controls still obey the negotiated in-flight limit.

Persist a caller-known UUID before dispatch when recovery matters. After a lost response or local timeout, recover the same operation using `browser.operation(operationId).get()` or `page.operation(operationId).get()`. The operation must still exist in the same sidecar runtime; its ledger is not a restart-persistent store. Do not automatically replay an action with an unknown outcome.

```js
import { EokaError } from "@cbxss/eoka";

try {
  await page.fill(target, "Ada", { operationId, timeoutMs: 1000 });
} catch (error) {
  if (!(error instanceof EokaError)) throw error;
  console.error({
    code: error.code,
    requestId: error.requestId,
    operationId: error.operationId,
    requestState: error.requestState,
    receipt: error.receipt,
  });
  if (error.operationId) {
    console.log(await browser.operation(error.operationId).get({ timeoutMs: 5000 }));
  }
}
```

`EokaError` is the base for `ConfigurationError`, `InstallationError`, `TransportError`, `ProtocolError`, `ServerOperationError`, and `OperationError`. Installation errors expose stable codes including `UnsupportedPlatform`, `BrowserNotInstalled`, `IntegrityMismatch`, `InstallationFailed`, `InstallationAborted`, `InstallationTimeout`, `ServerPackageMissing`, `ServerPackageMismatch`, and `ReleaseNotPrepared`. Errors preserve available request/operation IDs, dispatch uncertainty, receipt, structured server data, stderr, and cause. `OperationError` also exposes the latest record. These fields are evidence, not a promise that dispatch was prevented. `ServerError` is a generated wire type, not the runtime exception class.

Stderr diagnostics retain bounded complete lines while redacting credentials, configured proxy secrets, and DevTools endpoint capability details. Truncated partial lines are discarded rather than exposed as potentially unredacted fragments.

When both an operation/lifecycle failure and its cleanup fail, the SDK reports an `AggregateError`. Its `cause` preserves the primary error (also `errors[0]`), including any `operationId`, request state, and receipt; inspect that cause when recovering an operation. The remaining `errors` preserve cleanup failures rather than replacing the original evidence.

## Typed reads and invocation

`Browser` and `Page` implement the generated `InvokeClient` contract:

```ts
import type { MethodResults } from "@cbxss/eoka";
import { protocolVersion } from "@cbxss/eoka/protocol";

const result: MethodResults["page.evaluate"] = await page.invoke(
  "page.evaluate",
  { pageId: page.id, js: "({answer: 42, items: [1, null]})" },
  { timeoutMs: 5000 },
);
console.log(protocolVersion, result.result);
```

The method map is generated from the canonical sidecar schemas, including dynamic JSON evaluation results. Use `browser.supports(method)` and `browser.hasCapability(name)` to inspect the actual handshake rather than assuming every generated method is available on an older sidecar. `hello`, `sessionInfo`, `ownership`, and `browserVersion` expose negotiated/session metadata.

Page conveniences cover text/content/title/URL, element reads/waits, evaluation/execution, screenshots, viewport, browser state, and agent observation/steps. `page.fetch` is the legacy unbounded response API. Prefer `page.fetchBounded` for response-body reads: its default body budget is 8192 bytes and maximum is 1048576 bytes; inspect `truncated`, `bytesRead`, and `bodyOmitted` along with the body/status/headers. The `boundedFetch` capability indicates sidecar support. A bounded read is not the same as a complete response.

## Screenshots and native recording

```js
const image = await page.screenshot({
  format: "webp",
  quality: 80,
  clip: { x: 0, y: 0, width: 640, height: 360 },
  timeoutMs: 10000,
});
const screenshot = await page.screenshotToFile({
  path: "./captures/page.png",
  fullPage: true,
});
const recording = await page.record({
  path: "./captures/session.mp4",
  maxWidth: 1280,
  maxHeight: 720,
  fps: 30,
});
console.log(recording.id, await recording.status());
const video = await recording.stop({ timeoutMs: 60000 });
console.log(screenshot.path, screenshot.bytes, video.path, video.container);
```

Create destination directories first. Relative paths resolve once in the Node caller, not in the sidecar's working directory. File output stages privately beside the destination, flushes/closes, and publishes without overwriting an existing file; there is no overwrite option. An `Artifact` contains `path`, `bytes`, and `mimeType`; completed videos also report `container: "mp4"`. No video Buffer/base64 API is offered.

Returned artifact paths are canonical and may use Windows' `\\?\` namespace
prefix. Compare filesystem identity, not the spelling of the requested path.

`screenshot()` returns a PNG Buffer by default and still accepts the original `{signal, timeoutMs}` call options. `format` supports `png`, `jpeg`, and `webp` where Chrome supports them. `quality` is an integer from 0 through 100 for JPEG/WebP and is invalid for PNG. `clip` uses nonnegative CSS-pixel positions and positive sizes. `fullPage` and `clip` are mutually exclusive. Viewport `deviceScaleFactor` is the only render-scale control. Full-page dimensions depend on current layout and Chrome's image limits; prefer a fixed viewport/crop for deterministic output. In-memory screenshots are bounded to 16 MiB raw bytes; `ScreenshotTooLarge` directs callers to `screenshotToFile`, with no automatic retry or fallback.

Direct file output bypasses the 16 MiB sidecar response limit, not Chrome's image limits or the core's 64 MiB CDP message/frame budget (which includes base64 encoding and JSON). It is not an unbounded streaming screenshot API.

Capture dispatch requires negotiated methods and capabilities: `screenshotOptions`, `screenshotFile`, and `nativeRecording`. Native recording additionally requires a ready active page and an encoder supported by the actual Chrome build. Unsupported recording raises `RecordingUnsupported`; the SDK never switches engines/browser versions. Video is silent native MP4 with Chrome-selected codec, not a promise of H.264. FFmpeg is not an SDK runtime dependency. `fps` is an integral ceiling from 1 through 120, not deterministic fixed-rate playback; optional `maxWidth`/`maxHeight` are positive integers no greater than 2147483647. Screenshot crops/quality are not recording options.

Chrome 155.0.8059.39 can emit duplicate MP4 decoding timestamps, including
zero-duration prefixes and terminal tails. The sidecar validates the fragmented
MP4 before publication and applies bounded, lossless timing corrections.
Within a fragment, a run of `k` explicit zero durations becomes `k` one-tick
durations; the following duration decreases by `k` and must remain positive.
The run endpoint and later timestamps remain unchanged. A duplicate final sample
starting a new fragment advances by one tick, with its duration reduced by one
tick and matching fragment decode/index times corrected.
Encoded packets, sample count, file length and the final presentation endpoint
are preserved; clean recordings stay byte-identical.

This changes container timing metadata, not the codec or encoded frames. It
requires neither a Chrome fork nor FFmpeg. Unsupported or malformed containers,
insufficient donor duration and unresolved cross-fragment timing collisions
fail finalization without publishing a success-named file. Validation/repair uses the original
staging descriptor under a five-second deadline; failure retains partial-file
evidence. The core Rust raw-stream primitive remains unmodified Chrome output.
Release checks require strictly increasing finalized packet DTS and semantically
verified diagnostics, not merely a successful decoder exit. The
[qualification policy](#prerelease-qualification-and-publication) distinguishes
proven frame-rate-estimation advisories from failures without retiming the video.

`page.record` generates a UUID before dispatch, or accepts a caller-provided `recordingId`. There is one active recorder per target. A repeated start with the same ID and equivalent parameters recovers the existing record; changed parameters conflict instead of restarting. After a lost response use the ID rather than starting another recorder:

```js
import { RecordingError } from "@cbxss/eoka";

try {
  const recording = await page.record({ path: "./captures/session.mp4" });
  await recording.stop();
} catch (error) {
  if (!(error instanceof RecordingError)) throw error;
  const recording = browser.recording(error.recordingId);
  const record = await recording.status();
  console.log(record.state, record.failure?.stage, record.failure?.partialPath);
}
```

Recovery is scoped to the live sidecar session, not crash resume. `Recording.status()` returns the current `RecordingRecord`; `.record` retains the latest observed evidence. States are `running`, `finalizing`, `completed`, `cancelled`, and `failed`. Unknown IDs fail explicitly. `stop()` shares server-side finalization and fulfills only with a completed, published native artifact. Repeated/concurrent stop calls do not stop Chrome twice. Each local waiter has its own signal/deadline; aborting or timing out a waiter does **not** cancel the recorder or finalizer. Recover status or wait again. `cancel()` explicitly releases capture/stream resources and returns a terminal record, including `completed` if publication won the race or `failed` if cleanup failed.

A sidecar retains at most 1,024 distinct recording IDs and 16 MiB of cumulative
UTF-8 `recordingId`, `pageId`, and resolved `path` bytes. The byte limit bounds
retained start parameters, not exact process memory. Completed, cancelled, and
failed records keep their capacity; IDs are never evicted or reused. At capacity,
new IDs fail with `ServerBusy` before recording or file staging starts. Existing
IDs still recover, and changed parameters still conflict. Start a new
`Browser`/sidecar session for fresh capacity; stopping or cancelling does not
reset history.

`RecordingError` preserves `recordingId`, `record`, `failure`, `path`, `partialPath`, and `cleanupUncertain`, plus the normal request state/ID, cause, server data, and stderr evidence. Failed/cancelled stop outcomes are errors, never completed artifacts. A retained partial file is not the destination success name. Closing a tab/browser cancels unfinished recordings by default; explicitly await stop to save first. Disconnect/target-loss outcomes preserve honest failure/cleanup uncertainty; closing a borrowed session never kills the borrowed browser.

Error-carried recording records receive the same identity, output-path and completed-artifact checks as successful responses. Inconsistent records raise `Protocol`, retain the rejected wire evidence, and do not replace a handle's known record. Valid later evidence for the same failed terminal recording may still add partial-file or publication details.

`examples/capture.mjs OUTPUT_DIRECTORY [BROWSER_CACHE_DIRECTORY]` captures a local animated scene using an installed managed Chrome. It writes fresh screenshots and a silent native MP4 without requiring network or an alternate recorder. Existing output names intentionally fail instead of being replaced.

## Connect and cleanup

```js
const browser = await Browser.connect({
  serverPath: "/absolute/path/to/eoka-server",
  browserWSEndpoint: "ws://127.0.0.1:9222/devtools/browser/your-browser-id",
});
try {
  console.log(browser.ownership, await browser.tabs());
} finally {
  await browser.close();
}
```

`connect` borrows Chrome; closing the SDK stops its owned sidecar and detaches without closing the borrowed browser. `launch` owns its Chrome session. `close()` is idempotent and must be awaited; async disposal is also supported. Pending local calls are rejected when closing starts. Teardown requests graceful shutdown, then waits within `shutdownTimeoutMs` (45000 ms by default) before terminating the SDK-owned process tree: a private process group on POSIX, or the owned PID tree on Windows. This escalation does not target a separately running borrowed Chrome process.

Cleanup failures remain errors. The normal owned-Chrome cleanup path is graceful shutdown/sidecar EOF. Forced termination reports cleanup uncertainty: it cannot guarantee removal of runtime files or termination of descendants that left the owned process group/tree. Caller-owned profiles are never an SDK installation cache.

If `newPage` was dispatched before a local abort/deadline, the SDK retains its native completion so it can close the exact late-created tab. If that response does not arrive within the bounded cleanup deadline, it closes the owned sidecar and reports `CleanupUncertain`; a borrowed Chrome may retain an unknown tab. That uncertainty is also retained in `Browser.close()` failures and is not evidence that the tab was removed.

## Development checks

```sh
npm run generate:check
npm run typecheck
npm test
npm run test:consumer
```

`build` emits JavaScript/declarations (including the installer worker), the pinned release manifest, canonical runtime schema bundle, and license; it does not regenerate protocol outputs. SDK source builds reject unused locals, unused parameters, and switch fallthrough. `typecheck` checks accepted/rejected fixtures and the public SDK consumer against compiled exports. The packed-consumer regression installs with `--ignore-scripts --omit=optional`, checks silent/no-side-effect imports and actionable missing-server errors, compiles the public API consumer, and runs it. The native capture fixture checks the published MP4's file size and container signature rather than accepting a nonempty artifact count.

For the real outside-repository managed/offline consumer gate, supply a prepared stage and an already verified host Chrome cache:

```sh
EOKA_RELEASE_STAGE_DIR=/absolute/path/to/new-stage \
EOKA_BROWSER_CACHE_DIR=/absolute/path/to/cache npm run test:consumer
```

Without both variables the staged native/browser test is skipped; import/type tests are not evidence of a native browser launch. The native test packs/installs only local staged SDK/platform tarballs and launches with `download: "never"`. It exercises PNG/JPEG/WebP, DPR/crop/full-page dimensions, equivalent Buffer/file images, existing-file protection, native MP4 stop/recovery/repeated stop, and explicit cancel with no success-named partial. A deterministic 2500×2500 RGB-noise scene verifies that screenshots larger than 16 MiB raise `ScreenshotTooLarge` with direct-file guidance, remain usable through `screenshotToFile`, and do not disconnect the page. Container decoding/observed codec and duration require the separate ffprobe/ffmpeg release smoke; those tools are not SDK runtime dependencies. Actual no-network enforcement, sandbox, container, and all-platform qualification remain separate release gates.

From the repository root use `mise run node-build`, `node-check`, or `node-consumer`; `node-stage-release -- --artifacts DIR --output NEWDIR --host-only`, `node-install-browser -- CACHE`, and `node-offline -- CACHE` expose the preparation/offline workflows. `npm run example:schema` retains the generated-schema example.

`mise.toml` pins the development and CI runtime to Node 24.21.0; CI reads that configuration rather than maintaining another Node version. Node types are pinned to `@types/node` 24.19.2. SDK and consumer fixtures target ES2024. Native-package staging inherits the SDK's `engines` field. The Node CI job checks the committed schema projection, accepted/rejected consumers, runtime regressions, and packed imports without requiring Rust or Chrome; the full local `node-check` additionally checks the schema against the Rust exporter. Native gates still require the explicit environment above. Earlier Node 22 verification in `SDK-PLAN.md` is historical, not a current support guarantee.

## Prerelease qualification and publication

Reserved versions are core `0.7.0`, server/protocol crates `0.5.0`, wire protocol
`1.3`, and SDK/native npm packages `0.1.0-beta.1`. Source and ordinary staged
packages remain private; `--publication` is a separate, fail-closed staging mode.

`.github/workflows/qualify-node-release.yml` builds all five native targets and
qualifies installed tarballs outside the repository. GNU builds use Ubuntu 22.04
and enforce glibc ≤2.35; runtime checks use non-root hosted runners with Chrome's
sandbox enabled. Evidence includes fresh/cache/offline installation, artifact
rejection, verified input, image dimensions, finalized MP4 packet timestamps and
decode diagnostics, headed launch, owned cleanup, and borrowed-browser survival.
Failures retain recordings and diagnostics as workflow artifacts. FFmpeg/ffprobe
are qualification tools, not consumer dependencies.

The [lossless-repair preview](https://github.com/shrimp-software/eoka-tools/actions/runs/38612635401)
at tools `70b47e0ff27b271407c3073f5c1a389d87a5fd0a` and core
`9bae96b9cefa02794a4058ae54b67273ad453490` completed every runtime check on
Ubuntu 22.04 x64, Ubuntu 24.04 ARM64, macOS 15 x64/ARM64, and Windows 2025 x64.
All 50 one-attempt finalized recordings passed strict packet DTS and empty
decoder diagnostics; all 28 deterministic repair regressions passed on each
architecture. Image, sandbox, headed-launch and ownership checks also passed.
Those jobs ended **blocked** solely by the then-closed source readiness record.
The recording blocker is now cleared using that native evidence plus retained
defective-file repair/preservation proof, not passing live samples alone.
Registry-backed qualification and npm publication prerequisites remain separate.
Live recording still does not promise fixed-cadence export.

For local host-development evidence, not release qualification:

```sh
mise run node-qualify-release -- --stage /absolute/stage \
  --cache /absolute/cache --output /absolute/evidence --preview
```

The workflow accepts `source_ref` and an exact 40-character `core_ref` for
paired-core previews. Publication forbids a core override. Ordinary PR CI remains
blocked until core `0.7.0` is published and `Cargo.lock` is regenerated against its
real registry checksum; a local path-patched check is not registry proof.

`scripts/publication-readiness.json` records unresolved release defects. Clearing
a recording blocker requires deterministic repair/preservation regressions and
fresh five-platform qualification, not passing samples alone. Every Chrome pin
update must pass the complete recording suite on each architecture and ten
single-attempt native captures: four short/default-rate clips, two short
120-fps clips, one 1-fps clip, two two-second clips, and one multi-fragment clip.
Both artifact collection and publication independently require the actual running
Chrome version to match the pin, the complete schedule, published MP4 records,
strictly increasing packet DTS, and successful full decoding. A `passed` summary
alone is insufficient. One known FFmpeg MOV input-rate-estimation diagnostic may
be retained as a VFR advisory only when its adjacent packet indices and exact
timestamps match the probe and the declared positive packet duration exactly
reaches the next timestamp. Missing or inexact timing evidence, unknown
diagnostics, and nonzero decoder exits remain fatal. Raw probe/decoder logs are
preserved, and collection/publication independently recompute advisory
classifications; labels alone cannot authorize release. Probe logging includes
warnings. There is no retry-to-pass, hidden warning filtering, or retiming to
satisfy a frame-rate heuristic. The private MP4 correction remains limited to
the documented native timing defects; changing Chrome's container layout requires
renewed qualification, not an implicit generic muxer or transcoder.

`publish-node.yml` is manual, restricted to an exact current protected `main`
commit, and publishes only `0.1.0-beta.1` under `next` with npm provenance.
It requires an existing `npm-prerelease` environment with required reviewers,
self-review disabled, and a single allowed deployment branch, `main`.
All six scoped package names must first exist and have npm trusted publishers
configured for this repository, workflow, and environment. Bootstrap ownership
and environment setup are external prerequisites; this workflow does not
create them or fall back to an npm token.

The publish job accepts only same-run, same-source qualified artifacts and
preflights all six registry versions before writing anything. Existing versions
are skipped only when their SHA-512 integrity matches the qualified tarball,
`next` already points at that version, npm registry signatures verify, and
keyless Sigstore provenance binds the exact artifact, source commit, protected
main workflow, manual event, and original workflow run. The pinned public Sigstore
verifier is release-tooling-only, not an SDK runtime dependency. Registry signature
checks use isolated installs with lifecycle scripts disabled; foreign-platform
packages are installed only for verification, never executed.

After a partial publication or lost publish response, rerun the failed jobs of
the **same workflow run**, retaining its qualified artifacts and unchanged current
`main` source. Verified existing uploads are preserved; only missing native
packages are published, followed by the SDK. Mismatched integrity, provenance,
tags, source, or unavailable registry evidence abort before any new upload.
There is no automatic publish retry or retagging. If source or artifacts changed,
do not bypass these checks: use a separately reviewed roll-forward release.
The final smoke checks a credential-free registry installation and provenance.

No tag, merge, or publication is part of PR preparation. Existing Rust publishing
workflows trigger on `main`; merging the version changes is a release action.
