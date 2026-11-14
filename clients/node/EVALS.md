# Node SDK evaluations

The other tests in `tests/` cover the transport, the browser lifecycle, packaging and release. The files below check what a caller of the SDK experiences: what happens when they make a mistake, when a page misbehaves, and when they copy an example out of the README. Every browser test drives a real Chrome through a real `eoka-server` against a local `node:http` fixture; nothing touches the network.

## Layers

| File | What it checks | Needs a browser |
| --- | --- | --- |
| `tests/mistakes.test.mjs` | One row per caller mistake. Each row makes one call on a fresh tab, with a time budget, and asserts the error code and the fragments of the message a caller needs. | yes |
| `tests/misbehaving-pages.test.mjs` | Tabs stay independent, a stuck tab does not block the others, calls queue instead of failing, and no process outlives the Node process. | yes |
| `tests/blocked-tabs.test.mjs` | A tab blocked by a JavaScript dialog or by its own stalled navigation refuses calls fast with a code that says why, can be unblocked, and does not slow other tabs. | yes |
| `tests/readme-examples.test.mjs` | Every fenced `js` block in `README.md` parses and type-checks; the first block (Quick start) runs twice in one directory. | only the run |
| `.github/workflows/node-published-smoke.yml` | Nightly: `npm install @cbxss/eoka@latest` into an empty directory on the minimum and the newest Node release that `engines` allows, then launch, open a local page, read its title and close. | GitHub-hosted |

### `mistakes.test.mjs`

| Row | Call | Must hold |
| --- | --- | --- |
| K4 | `fill` / `click` on `#missing` with `timeoutMs: 2000` | rejects within 4 s, `code === 'action_timeout'`, message names `#missing` and says "not found" |
| K4 | `fill('#missing', 'x')` with no options | rejects within 32 s with `action_timeout`, not a local deadline |
| K5 | `click` on a disabled button, `fill` on a readonly input | rejects within 4 s; message names the selector and says disabled / readonly |
| K6 | `click` with an expectation that never holds | rejects within 4 s with `action_timeout`; message names the expectation and says the click was dispatched; the fixture saw exactly one click |
| K7 | `actionOptions: { expect, timeoutMs }` | `ConfigurationError`, `requestState === 'not_sent'`, message names `timeoutMs` and `timeout_ms`; the fixture saw no click |
| K8 | `evaluate` with a function, with arguments, with a string | returns the value |
| K9 | a rejected read, action and configuration error | `error.stack` has a frame in the calling file |
| K11 | `fill('#missing', 'x', …)` with `timeoutMs` and with `actionOptions.timeout_ms` | across the message, its cause and its operation record: the word `exceeded` survives, and redacting the typed value never cuts into a neighbouring word |
| K12 | `screenshotToFile` onto an existing file | message names the absolute path and says it exists; the file is untouched |
| pinned | `getText('#missing')`, a throwing `evaluate`, a result over 1 MiB | `ElementNotFound`, the thrown message, `MessageTooLarge` |

Rows that share a call (for example K4, K9 and K11 on `fill('#missing')`) share one execution. The 30-second row runs on a second browser in the background so the file takes about as long as that row.

### `misbehaving-pages.test.mjs`

| Row | Scenario | Must hold |
| --- | --- | --- |
| K1 | two tabs each wait one second, in parallel | both done in under 1.6 s |
| K3 | 300 concurrent `title()` calls on one page | all resolve |
| pinned | `fill` on two tabs in parallel | each field holds its own text |
| K2 | tab A is inside `evaluate` of a promise that never settles | `b.title()` under 1 s, `browser.newPage()` under 3 s |
| K2 | tab A's `goto` to a server that never answers has rejected | the same two budgets, both right after the rejection and while `a.title()` is in flight; `a.title()` settles within 35 s |
| K14 | the Node process is killed with `SIGKILL` | the sidecar and every Chrome process are gone within 5 s |
| K14 | the script throws an uncaught exception | exit code is non-zero within 10 s and no child process is left 5 s later |
| K15 | tab A has been behind tab B for 3 s | `a.screenshot()` returns a non-empty PNG within 5 s; `a.screenshotToFile()` writes a non-empty file within 5 s; `b.screenshot()` still works within 5 s afterwards; both together resolve within 10 s |
| K16 | tab A has been behind tab B for 3 s | `a.click()` with an expectation resolves within 3 s as `verified_complete`; `a.hover()` resolves within 3 s; a click on B still resolves within 3 s; clicks on A and B together both resolve within 5 s; `a.fill()` resolves and the field holds the text (pinned); after every row that acts on A, A is still `hidden`, B still `visible`, and neither page saw a `visibilitychange` event |
| close race | eight browsers in one process, a `data:` page in each, all closed at once, three rounds | no `close()` rejects |

K14 spawns `tests/fixtures/eval-process-child.mjs` and follows its process tree with `ps`; it is skipped on Windows. Each K15 and K16 row has its own browser and pair of tabs, so a capture or an action that is still running cannot leak into another row; the K16 visibility rows read a counter installed in both pages before the three-second wait. The close-race row holds eight Chrome copies in the temporary directory at once (about 2.3 GB), so point `TMPDIR` at a disk with room; a long `TMPDIR` path stops Chrome from starting.

### `blocked-tabs.test.mjs`

| Row | Scenario | Must hold |
| --- | --- | --- |
| E1 | a page timer opens `alert('are you sure?')`; then `title()`, `evaluate('()=>1')`, `getText('h1')` or `exists('h1')` | rejects within 2 s as an `EokaError` with `code === 'DialogOpen'`; the message mentions the dialog and quotes its text; a separate row requires it to name `dialogAccept` or `dialogDismiss` |
| E2 | `page.ops.dialogAccept()` or `page.ops.dialogDismiss()` while the dialog is open | resolves within 2 s, and `title()` answers within 2 s afterwards |
| E3 | tab A has a dialog open | `b.title()` under 1 s |
| E4 | `click` on a button whose handler reports the click, calls `alert`, then writes `Deleted`; the click expects that text | rejects within 3 s with `code === 'dialog_open'`; the message mentions the dialog and says the click was dispatched; a separate row requires the hint; the fixture saw exactly one click; after `dialogAccept()` the text is `Deleted` |
| E5 | `evaluate("()=>confirm('sure?')")` | rejects within 2 s with `DialogOpen` |
| E6 | a page timer sets `location.href` to a URL that never answers; 6.5 s later | `title()` rejects within 2 s with `code === 'NavigationPending'` and names the URL path; `b.title()` under 1 s; `goto()` to a working page resolves within 8 s and `title()` answers within 2 s |
| E7 | `title()` sent 500 ms after that navigation starts | rejects with `NavigationPending` within 8 s |

Three browsers keep the groups apart: reads that may stay stuck behind a dialog, rows that clear their dialog, and stalled navigations. Every row has its own tab. A read that outlives its budget is abandoned; nothing queued on that tab can dismiss the dialog until the read ends, so the browser is closed at the end instead. The two navigation scenarios start when the file starts and run in the background.

### `readme-examples.test.mjs`

Blocks are found by their fence, so the test follows README edits. Parsing uses `node --check`. Type-checking compiles each block as a module against `dist/*.d.ts`, with `page`, `browser` and every SDK export in scope; a name that a fragment carries over from an earlier block is assumed to exist and reported as a diagnostic. The run rewrites every `http(s)` origin in the Quick start block to the fixture, resolves `@cbxss/eoka` to this checkout, and requires both runs to exit 0 and to load the fixture.

## Running

```sh
npm ci --ignore-scripts --omit=optional
npm run build
export EOKA_RELIABILITY_SERVER=/path/to/target/debug/eoka-server
export EOKA_RELIABILITY_CHROME=/path/to/chrome
node --test tests/mistakes.test.mjs tests/misbehaving-pages.test.mjs tests/blocked-tabs.test.mjs tests/readme-examples.test.mjs
```

Without `EOKA_RELIABILITY_SERVER` the browser tests are skipped and only the README parse and type checks run. CI sets both variables in the `Browser (clients)` job, which runs `tests/*.test.mjs`. Each test prints what it observed (`ℹ rejected in 2001 ms with … code=…`), so a failure shows the measured time, code and message. On a debug server expect about 35 s for `mistakes` (one row waits out the 30-second default), 90 to 120 s for `misbehaving-pages` (the close-race row alone launches 24 browsers), 15 to 30 s for `blocked-tabs` and 10 s for `readme-examples`; the files run in parallel.

The time budgets are part of the contract. Do not raise one to make a row pass.

## Not covered

- Real sites and real networks. Every page is a local fixture.
- `Browser.connect`, headed mode, macOS and Windows. The browser rows run wherever the two variables are set, which in CI is Linux x64.
- What `browser.close()` reports, outside the close-race row. The other suites close their browsers on a best-effort basis so that a cleanup error cannot mask or fake a contract row; `browser-lifecycle.test.mjs` and `reliability-browser.test.mjs` own that.
- Error wording beyond the fragments listed above.
- Unknown option keys outside `actionOptions`, such as `screenshot({ fullpage: true })`. The README type check catches them in examples; nothing asserts that the runtime rejects them.
- The catalog operations under `page.ops`, a frozen sidecar, and navigation failures other than a server that never answers.
- Whether someone who has only read the README can write a working script. That needs a model in the loop and is not built.
- The published package on pull requests. The smoke workflow runs nightly and on demand against whatever `latest` is on npm.
