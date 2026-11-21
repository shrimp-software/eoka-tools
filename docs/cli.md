# CLI guide

Feature notes for the `eoka` CLI. Run `eoka <command> --help` for every flag.

## Install, update and the agent skill

Install the latest released CLI binary without a Rust toolchain (Linux and
macOS). The script verifies the download against the release `SHA256SUMS`;
set `EOKA_VERSION` to pin a release and `EOKA_INSTALL_DIR` to change the
destination (default `~/.local/bin`):

```sh
curl -fsSL https://raw.githubusercontent.com/shrimp-software/eoka-tools/main/crates/eoka-cli/scripts/install.sh | sh
```

Update an installed CLI to the latest GitHub release (Linux and macOS, x86_64
and aarch64). The download is verified against the release `SHA256SUMS` before
the running binary is replaced. Installed copies of the agent skill are then
rewritten to match the installed binary:

```sh
eoka update --check   # report installed and latest versions, and a stale skill
eoka update           # install; add --kill-daemons to stop old session daemons
eoka upgrade          # the same command
```

The binary carries its own agent skill. `eoka skill refresh` rewrites the
installed copies on its own, for example after `cargo install`; it looks under
`~/.agents/skills`, `~/.claude/skills`, `~/.codex/skills`, `~/.pi/agent/skills`,
`~/.config/opencode/skills`, `~/.cursor/skills` and `~/.gemini/skills`, and
under `$CODEX_HOME/skills` and `$CLAUDE_CONFIG_DIR/skills`. Only the skill's
own files are rewritten. `eoka update --keep-skill` leaves the copies alone.

Install the skill from the binary, without Node. With no directory it writes
`~/.agents/skills/eoka`, which Codex and other agents that read
`~/.agents/skills` use, and `~/.claude/skills/eoka` for Claude Code when
`~/.claude` exists. Name a directory for any other agent or for a
project-local copy:

```sh
eoka skill install
eoka skill install .claude/skills/eoka
```

Or install the Codex skill with the open skills CLI:

```sh
npx skills add https://github.com/shrimp-software/eoka-tools/tree/main/crates/eoka-cli/skill --skill eoka --global -a codex -y
```

Either way, `eoka update` keeps the installed copies current afterwards.

## Blocked and busy sessions

No CLI command blocks indefinitely. A tab with an open JavaScript dialog, or one
waiting for a server that never answers, is reported within about half a second
as `dialog_open` or `navigation_pending` with the command that clears it, instead
of a 30-second protocol timeout per command. A navigation that gets no response
before its deadline is stopped and reported as `navigation_timeout`, leaving the
tab on its previous page. While one command runs, another waits at most 5 s and
is then refused with `daemon_busy`, naming the running command. `eoka status`
reports what a session is running and what blocks its tab. MCP tools and Tack
receive the same codes. See [agent browsing](agent-browsing.md#blocked-and-busy-sessions).

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

## Local DataDome on the current tab

Build/install with `mise run install-cli`. On a challenged page in an existing session:

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
explicitly. See [the DataDome guide](../crates/eoka-solvers/DATADOME.md) for details.

## Local safety defaults

These apply to the CLI and MCP server:

- `file:` URLs are blocked unless `EOKA_ALLOW_FILE_URLS=1` is set. For the CLI, the session daemon reads this when it starts.
- State files are written owner-only (`0600`). The path must end in `.json`, and a save can only replace an empty file or an earlier state file. Symlinks are refused.
- Saved storage is only restored on its saved origin. Otherwise it is skipped with a warning.
- The CLI runtime directory is `$XDG_RUNTIME_DIR/eoka`, or `$TMPDIR/eoka-<uid>` (default `/tmp/eoka-<uid>`) as a fallback. It is owner-only, and a directory owned by another user is refused.
- On Linux, the local forwarder for authenticated HTTP proxies refuses connections from other users. Other processes run by the same user can still use it, and other platforms do not check the caller.
