# eoka-mcp

Stdio MCP server for Eoka browser automation, run by the `eoka` CLI as `eoka mcp`. It supports MCP `2026-07-28` with stateless `server/discover` requests. Shared browser primitives live in `eoka-server`.

Part of the [eoka-tools](https://github.com/shrimp-software/eoka-tools) workspace.

## MCP Server

Install the CLI (prebuilt binaries for Linux and macOS, no Rust needed), then register it:

```sh
curl -fsSL https://raw.githubusercontent.com/shrimp-software/eoka-tools/main/crates/eoka-cli/scripts/install.sh | sh
claude mcp add eoka -- eoka mcp
```

This crate is the library behind `eoka mcp`; Rust programs can call `eoka_mcp::run_server()` to serve it themselves.

Includes CAPTCHA tools. For AWS WAF, provide `website_key`, `iv`, and `context` from `window.gokuProps`; use the returned `token` as the `aws-waf-token` cookie and retain the returned `user_agent`.

## Tools

The server's established tools keep their names. In addition, every operation in
the shared operation catalog that has no tool of its own is exposed under its
catalog name and runs on the current tab through the same engine as the `eoka`
CLI: `content`, `get_text`, `get_attribute`, `exists`, `reload`, `wait` (with
selector and expectation waits), `emulate`, `frames`, the `network_*`,
`intercept_*`, `js_*` and `wasm_*` families, and more. A new catalog operation
appears here without a server change.

Every tool takes its description, input schema and annotations from the catalog.
A few established names are aliases over a catalog operation: `navigate` is
`open`, `extract` is `eval`, `batch` is `actions.batch`, the
`local_storage_*` and `session_storage_*` tools fix `session_storage`, the
`wait_*` tools show only their own `wait` fields, `screenshot` is an annotated
inline capture returned as an image, and `click_intercept_nav` is `click` with
`intercept_nav`.

Operations marked opt-in in the catalog are listed only when
`EOKA_MCP_ALL_TOOLS=1` is set. That set includes recording, shadow-root clicks,
and the operations that reach outside the page: `clone_from` (copies cookies and
storage from another local Chrome), `fake_camera`, and interception rules that
capture to or respond from local files. Catalog tools need an open page;
`navigate` first. Action tools generate an operation ID when none is given and
return the operation record.

## Action defaults

- `operation_id` is optional. One is generated when omitted and returned as `id`. Send your own UUID to retry safely after a lost response.
- `click`, double click and `type_key` can omit `expect`. They then run unverified and return `status: outcome_unknown` with a `note`, so read the page next and do not repeat the action blindly.
- `fill` and `select` find fields by label. Several matches return `ambiguous_target` instead of a guess.
- `agent_step` accepts the same targets as `click` and `fill`, as well as refs.
- `extract` and `exec` accept a function (called with `arguments`), an expression such as `document.title`, or statements run as an async function body (`return` gives the value). Syntax is checked before the code runs, so it runs exactly once. `raw=true` sends the code to CDP unchanged.
