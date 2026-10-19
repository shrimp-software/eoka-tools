# @cbxss/eoka

Node.js SDK for eoka, a stealth Chrome automation sidecar.

```sh
npm install @cbxss/eoka@latest
```

Requires Node.js 24.21+ (ESM only). Supported platforms: Linux x64/arm64 (glibc 2.35+), macOS x64/arm64, Windows x64.

## Quick start

```js
import { Browser } from "@cbxss/eoka";

const browser = await Browser.launch();
try {
  const page = await browser.newPage("https://example.com");
  console.log(await page.title());
  await page.screenshotToFile({ path: "./example.png" });
} finally {
  await browser.close();
}
```

The first launch downloads a pinned Chrome for Testing build (about 200 MB) into a per-user cache. Later launches reuse it.

## Pages

```js
await page.goto("https://example.com/login");
await page.fill("#email", "ada@example.com");
await page.click("button[type=submit]", {
  actionOptions: { expect: { kind: "url", value: "/home", contains: true } },
});

await page.waitForText("Welcome");
const heading = await page.getText("h1");
const count = await page.evaluate("document.querySelectorAll('li').length");
```

String targets are CSS selectors. An action resolves only after its result is verified. `click`, `type`, `pressKey`, `submit` and `drag` need an `expect` that describes the result.

- **Reads:** `text`, `content`, `title`, `url`, `getText`, `getAttribute`, `exists`, `evaluate`
- **Waits:** `waitFor`, `waitForVisible`, `waitForText`
- **Actions:** `goto`, `fill`, `select`, `hover`, `click`, `type`, `pressKey`, `submit`, `drag`

## Shadow DOM and Turnstile

`page.frames()` lists every frame, including cross-origin OOPIFs. `page.clickShadow(selector, { frameId?, resolveTimeoutMs? })` searches the frame's light DOM and every shadow root (closed roots included), maps the matched node's center to viewport coordinates, and clicks with human-like movement. `resolveTimeoutMs` bounds the server-side resolution; `CallOptions.timeoutMs` remains the RPC deadline and defaults to the resolution deadline plus 10 seconds when omitted:

```js
const challenge = (await page.frames()).find(frame =>
  frame.url.includes("challenges.cloudflare.com"));
if (challenge) {
  await page.clickShadow('input[type="checkbox"]', { frameId: challenge.id });
}
```

`page.solveTurnstile({ timeoutMs })` is a best-effort helper for interactive Cloudflare challenge pages. The server finds the challenge frame, clicks the checkbox, and waits for the page to leave the challenge state, then resolves with `{ clicks, elapsedMs }` (zero clicks on a normal page). It is not a general CAPTCHA solver.

### Automatic challenge handling

`Browser.launch({ autoSolveCloudflare: true })` (and `Browser.connect`) has the server watch every tab and solve full-page Cloudflare challenges as they appear. It does not click Turnstile widgets embedded in normal forms. The Go SDK, CLI (`--auto-solve-cloudflare`) and MCP server (`EOKA_AUTO_SOLVE_CLOUDFLARE=1`) use the same server watcher.

```js
const browser = await Browser.launch({ autoSolveCloudflare: true });
try {
  const page = await browser.newPage("https://1337x.to/");
  console.log(await page.title()); // Torrent Search Engine | 1337x.to
} finally {
  await browser.close();
}
```

Tune it with `{ intervalMs, timeoutMs }` (defaults 500 and 30000). The server logs each challenge and result to its stderr. The watcher stops with `browser.close()`.

## Screenshots and recording

```js
const png = await page.screenshot(); // Buffer
await page.screenshot({ format: "webp", quality: 80 });
await page.screenshotToFile({ path: "./full.png", fullPage: true });

const recording = await page.record({ path: "./session.mp4" });
// ...
await recording.stop();
```

File output never overwrites an existing file, and the directory must already exist. Recordings are silent MP4s. For screenshots larger than 16 MiB, use `screenshotToFile`.

## Options

```js
await Browser.launch({
  headless: true,   // default
  sandbox: true,    // default; leave it on
  download: "auto", // "never" fails when Chrome isn't cached
  browserCacheDir: "/opt/eoka/cache",
  viewport: { width: 1280, height: 720, deviceScaleFactor: 1 },
  timeoutMs: 30000,
});
```

To install Chrome ahead of time, for example while building a Docker image:

```js
import { installBrowser } from "@cbxss/eoka";
await installBrowser({ browserCacheDir: "/opt/eoka/cache" });
```

To drive a Chrome you already run, use `Browser.connect({ browserWSEndpoint })`. Closing the SDK leaves that browser running.

| Environment variable | Purpose |
| --- | --- |
| `EOKA_BROWSER_CACHE_DIR` | Where Chrome is cached |
| `EOKA_RUNTIME_DIR` | Scratch directory for sessions |
| `EOKA_CHROME_PATH` | Use your own Chrome binary |
| `EOKA_SERVER_BIN` | Use your own `eoka-server` binary |
| `HTTPS_PROXY`, `NO_PROXY` | Proxy for the Chrome download |

## Errors

Every SDK error extends `EokaError` and has a stable `code`:

```js
import { EokaError } from "@cbxss/eoka";

try {
  await page.fill("#name", "Ada");
} catch (error) {
  if (error instanceof EokaError) console.error(error.code, error.message);
  throw error;
}
```

## Linux and Docker

Chrome needs its usual system libraries even in headless mode. On Debian:

```dockerfile
FROM node:24-trixie
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates fonts-liberation libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64 \
    libcairo2 libcups2t64 libdbus-1-3 libdrm2 libgbm1 libglib2.0-0t64 libnspr4 \
    libnss3 libpango-1.0-0 libssl3t64 libx11-6 libxcb1 libxcomposite1 libxdamage1 \
    libxext6 libxfixes3 libxkbcommon0 libxrandr2 \
    && rm -rf /var/lib/apt/lists/*
USER node
```

Run as a non-root user. On Ubuntu 23.10 and later, AppArmor may block Chrome's sandbox; see [Chromium's AppArmor notes](https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md). Alpine (musl) isn't supported.

## Examples

```sh
node node_modules/@cbxss/eoka/examples/install-browser.mjs
node node_modules/@cbxss/eoka/examples/offline.mjs
node node_modules/@cbxss/eoka/examples/capture.mjs ./captures
```

## Development

```sh
mise install
npm ci --ignore-scripts --omit=optional
npm run build
npm test
```

Releases are automatic: merge a version bump to `main` and approve the `npm-prerelease` deployment.
