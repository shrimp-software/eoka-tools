# eoka-mcp

Stdio MCP server for Eoka browser automation. The MCP binary supports MCP `2026-07-28` with stateless `server/discover` requests. Shared browser primitives and the Rust `Session` API live in `eoka-server`.

Part of the [eoka-tools](https://github.com/shrimp-software/eoka-tools) workspace.

## MCP Server

```sh
cargo install eoka-mcp
claude mcp add eoka -- eoka-mcp
```

Includes CAPTCHA tools. For AWS WAF, provide `website_key`, `iv`, and `context` from `window.gokuProps`; use the returned `token` as the `aws-waf-token` cookie and retain the returned `user_agent`.

## Tools

The server's established tools keep their names. In addition, every operation in
the shared operation catalog that has no tool of its own is exposed under its
catalog name and runs on the current tab through the same engine as the `eoka`
CLI: `content`, `get_text`, `get_attribute`, `exists`, `reload`, `wait` (with
selector and expectation waits), `emulate`, `frames`, the `network_*`,
`intercept_*`, `js_*` and `wasm_*` families, and more. A new catalog operation
appears here without a server change.

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
