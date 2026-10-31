# Login and navigation migration

This is a breaking release: core 0.8.0, CLI/server/protocol 0.7.0, Rust SDK 0.8.0, runner 0.5.0, the existing DataDome adapter 0.4.0 for the core type change, Node 0.1.0-beta.7, and wire protocol 2.0. Upgrade paired components together. The binaries and registry packages must be published in the order below; a local core override is development evidence only.

## Sessions

`--session NAME` is the exact identity. `--headed` and `--headless` are creation settings and explicit mode checks, not suffixes. Once a session exists, omit launch settings to reuse its current mode and selected tab:

```sh
eoka --session sketchy --headed open https://app.sketchy.com/login
eoka --session sketchy view
eoka sessions --json
eoka --session sketchy doctor --json
```

Only `open`, or explicit `tab attach` with `--cdp`, can establish a browser session. Reads and ordinary input do not silently launch another browser. Existing `NAME-headed` and `NAME-live` sessions are reported; select their literal names. If their daemon uses the old protocol, use its matching CLI version to inspect or close it explicitly before migrating. Eoka never kills/restarts a browser to retry a command. A protocol or explicit configuration conflict fails before dispatch.

## Controls and form validation

Use `view` / `agent.observe` targets throughout CLI, MCP, Tack and downloads. Numeric indices and legacy snapshot `@e1` references are no longer action targets. `snapshot`, `observe`, and text search remain inspection tools. Live CSS/text locators are deliberate queries and may resolve a different matching node after a rerender. A scoped reference stays bound to its original document, tab and backend node.

`view --cursor CURSOR` continues a capture; `--role` and `--name` filter controls. Captures expose `total_controls` and `next_cursor`; more than 200 controls can be retrieved. A fresh capture invalidates its previous pagination cursor, while unchanged control references remain stable. The top document is covered; embedded documents require frame tools. Focused, editable, readonly, required and invalid state are included without input values.

Text fill/type now deliver native input per Unicode character by default, checking the original target before each character. Focus changes or replacement of the input stop further typing and preserve uncertainty about the partial action. `input_mode: "bulk"` or CLI `fill --input-mode bulk` explicitly selects one native insertion. These text modes do not assign DOM values or synthesize JavaScript input events. `focus` is available through `step`; use a real `press_key` Tab to trigger blur when a form requires it. A verified fill means the value matches; it does not mean the application validated or accepted the form.

Readiness supports `enabled`, `focused`, `document_ready`, and `all`. For example:

```sh
eoka --session sketchy --expect '{"kind":"enabled","target":{"query":{"kind":"css","selector":"button[type=submit]"}},"enabled":true}' wait --timeout 10000
```

Navigation waits for an interactive document under the action deadline. `open --expect JSON` additionally waits for application readiness. Fixed sleeps are not completion evidence. Expected URL/visible/enabled conditions can be combined with `all` (1–16 conditions, nesting up to 8).

## Actions, destinations and errors

`step` defaults to `verify`. Click, type, submit and key actions require an explicit expectation when intrinsic completion evidence is unavailable. `mode: "explore"` permits deliberate exploration and reports unverified dispatch as `outcome_unknown`.

`destination` is `current`, `new_tab`, or the default `current_or_new`. Verification can follow a new child tab correlated to the originating tab. Exactly one matching destination must exist among the observed candidates. An unrelated existing tab is never adopted, and multiple matching candidates fail as ambiguous. The receipt retains both the originating `target_id` and `destination_target_id`. CLI/MCP select the destination; Node/Go callers use the returned observation tab ID for a new page handle. Attachment/observation failures preserve the action receipt.

Action responses keep their operation record even when `ok` is false. `operation_error` is structured: code, bounded/redacted message, retryability and operation/phase context. Input values are not stored in diagnostic messages. Actual JavaScript exceptions, Chrome navigation errors and the last preparation/verification phase survive persistence and recovery.

| CLI exit | Meaning |
|---|---|
| 0 | Action verified, or a read completed successfully |
| 1 | Definite failure or rejection |
| 2 | Invalid CLI usage |
| 3 | Operation still running |
| 4 | Outcome unknown |

Use `operations get ID` to inspect a known operation, and `operations wait ID` for a completion result. Reading a record can succeed while its recorded action failed. A lost response never authorizes replay. Reusing an operation ID with the same request recovers the existing operation; changing the request is rejected.

## JavaScript and recording

`eval`/`exec` and sidecar `page.evaluate`/`page.execute` take a function and JSON arguments. Functions run once in a fresh lexical scope; promises are awaited. JSON arguments are parsed as data, preserving keys such as `__proto__`. Arguments reject unsafe integer values and nesting beyond 64; function/argument/result limits are 1 MiB. Undefined/non-JSON evaluation results fail explicitly; use `exec` when no return value is needed.

```sh
eoka --session demo eval '(value)=>{const local=value; return local}' --args '["hello"]'
eoka --session demo exec '()=>{document.title="Example"}'
eoka --session demo eval --raw 'document.title'
```

Node uses `page.evaluate(functionSource, args)` and `evaluateRaw(source)`; Go uses `Evaluate(ctx, functionSource, args...)` and `EvaluateRaw(ctx, source)`. Raw evaluation is the explicit global-scope escape hatch. Frame evaluation retains its separate frame-scoped contract. `exec` propagates exceptions; raw `--no-await` reports uncertain asynchronous completion.

Network recording start, stop, clear and session updates acknowledge application of the control before returning. Stop does not disable shared CDP network tracking. Callers can start recording and immediately trigger navigation without a scheduling sleep.

## Helpers and release gates

The three reconstructed Sketchy helpers are in [tools/sketchy](tools/sketchy/README.md). They require an explicit authenticated session, navigate to the requested lesson, validate video identity, and publish verified media without overwriting an existing file.

Before release, run core formatting, Clippy, all unit/browser tests, examples and Rust 1.85; tools conformance, all unit/browser tests, Clippy, Go race tests, Node generation/type/runtime/package checks, and helper tests. Use a short disk-backed `TMPDIR`, such as `/var/tmp/er`, for Chrome Unix sockets.

Authenticated macOS qualification must use the actual revised CLI and helpers: headed login with real blur and sequential input, validation and submit, popup navigation if present, correct lesson selection from a different current lesson, lesson listing, verified download, and repeat invocation without duplicate actions. Record versions, operation IDs and result states; do not record passwords, cookies or signed stream URLs.

Publish core 0.8.0 first. Once it is available in the registry, regenerate the tools lockfile without a core override and pass `mise run release-check`. Publish protocol, SDK, server, runner, MCP, Tack and CLI in dependency order. Build version-matched CLI/server binaries, generate checksums, qualify platform artifacts and then publish Node/Go distributions. The CLI installer requires its chosen version and checksum; it never falls back to the old v0.1.1 binary. Neither local tests nor staged version numbers prove a release exists.
