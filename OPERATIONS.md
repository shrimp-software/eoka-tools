# Durable actions and download recovery

Eoka-tools stores an operation before allowing a consequential browser action.
Use a canonical lowercase UUID generated before sending the request. Keep that
ID when the response is lost. Submitting the same ID and arguments returns the
existing record; different arguments produce `operation_id_conflict`. There is
no automatic replay of an uncertain action and no exactly-once guarantee for
the website's own business operation.

## CLI

```sh
eoka --operation-id 29002b83-2093-481b-a4f2-df5c8874a26f \
  --expect '{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}' \
  --action-timeout-ms 5000 click css:#save
eoka operations get 29002b83-2093-481b-a4f2-df5c8874a26f
eoka operations wait 29002b83-2093-481b-a4f2-df5c8874a26f --timeout-ms 30000
eoka operations list --limit 100
```

Click, double-click and key commands require an explicit expectation. Fill,
select and hover verify intrinsic results. Commands return an admitted operation
record; use `operations wait` to inspect completion. A waiter timing out or
disconnecting does not cancel the operation. `operations cancel ID` requests
cancellation; inspect again for the resulting record and cleanup status.
Operation queries report command success separately from the action outcome;
check `outcome == "verified_complete"` before proceeding.

For every core action, including navigation, typing, upload and submit, use
`eoka action '<ActionPerformArgs JSON>'`. For example:

```json
{
  "operation_id": "5be9b0f6-dca5-4502-95f6-1fda96cfcf44",
  "action": {
    "kind": "fill",
    "target": {"query": {"kind": "css", "selector": "#name"}},
    "value": "Ada"
  },
  "options": {"timeout_ms": 5000}
}
```

Locators are strict. A serialized `target.index: 0` explicitly selects the first
match. Snapshot backend-node references remain pinned; live CSS/text queries can
resolve a replacement before dispatch. Failed or stale actions are not retried
after primary input. Raw evaluation, held-input and coordinate commands remain
low-level primitives outside this contract.

## Other clients

The protocol crate exports the core `Action`, `LocatorSpec`, `Expectation`,
`ActionOptions` and receipt schemas. CLI/Tack paths are `actions.perform` and
`operations.get/list/wait/cancel`; MCP tools use the corresponding underscore
names. MCP convenience actions require `operation_id`; MCP batches accept an
array of complete action requests, wait sequentially, and stop at the first
unverified result. Each record remains independently inspectable.
MCP `click_intercept_nav` now returns an admitted operation and requires the
same ID/expectation fields. Its page diagnostic captures only history
push/replace calls; read `window.__eoka_nav` after waiting. JavaScript cannot
reliably replace Chrome's `location` navigation methods.

The Rust SDK exposes `perform`, `operation`, `wait_operation` and
`cancel_operation`. The Go client exposes `Page.Perform` and
`Browser.Operation/WaitOperation/CancelOperation`; existing Go action methods
accept `ActionOptions`. Client-generated IDs remain in transport errors.
New clients send a distinct action command, including for download starts, so
older daemons reject unsupported durable requests before input. Restart daemons
explicitly after upgrading.

Runner YAML accepts `perform` with the same request shape. Existing click,
fill, type, select, key and navigation steps use the operation manager; click,
type and key steps require `expect`. Optional `operation_id` and `timeout_ms`
fields apply to action steps. Generated IDs are logged. `try_click_any` assigns
stable UUIDv5 candidate IDs from its group ID and logs them; it only advances
past a candidate proven unavailable before dispatch. Unknown outcomes stop the
sequence. Whole-workflow retries (`on_failure.retry.attempts > 1`) are rejected
because restarting the workflow could replay a completed or uncertain action.
User-authored JavaScript remains a raw primitive.

## Evidence and storage

An operation's outcome is `not_dispatched`, `outcome_unknown`, or
`verified_complete`. Its lifecycle is independently `running`, `finished`,
`interrupted`, or `expired`. Durable dispatch intent is written before input;
a crash after intent is conservatively unknown even if Chrome may never have
received the command. A surviving receipt can provide stronger evidence.

SQLite uses WAL and FULL synchronous writes in a private state directory:
`EOKA_STATE_DIR`, otherwise `$XDG_STATE_HOME/eoka` or the platform's per-user
state location. Runtime ownership uses OS file locks and random runtime IDs,
not PID existence. A runtime never adopts another live runtime's jobs.
Status queries recover abandoned records without launching a browser.
Cancellation of a live foreign job must go to its owning runtime.

Records omit action inputs, selectors, expected values and page contents. A
keyed digest binds arguments to IDs without retaining their plaintext. Download
records necessarily retain output/staging paths, byte counts, a redacted source
URL, native GUID and completion evidence. File publication adds SHA-256 and
file identity. Treat the local state directory as private user data.

On store initialization, completed operation receipts older than 30 days are
compacted to tombstones that retain the ID/digest, preventing accidental reuse.
Download history and user artifacts are retained. List queries for operations
are bounded; download history is currently unpaginated. No automatic artifact
deletion, browser reattachment or partial-transfer resumption is implemented.

## Downloads

```sh
eoka --operation-id 6c1869ac-e6f4-4b4c-8109-d14f953acb4b \
  download --click css:#generate-report --out ./report.zip
eoka download status 6c1869ac-e6f4-4b4c-8109-d14f953acb4b
eoka download wait 6c1869ac-e6f4-4b4c-8109-d14f953acb4b
```

The download ID is also its operation ID. Starting returns admission promptly.
The trigger has its own start deadline. Transfer lifetime and client wait
timeouts are separate; losing a client does not repeat the trigger or cancel
the transfer. One active file per session and Eoka-launched Chrome ownership
remain required.

A native completion event is persisted before file verification. Publication
checks regular-file identity, expected byte counts and SHA-256, records intent,
links without overwriting an existing destination, synchronizes the parent,
then commits completion. Restart recovery can publish a provably completed
staged artifact or reconcile the same previously linked inode. An unrelated
destination, altered digest or missing completion proof is never accepted.
Partial or uncertain artifacts are retained for inspection and no click is
replayed. Completed files/history remain available after daemon restart.
Download status/list/wait can start a history daemon when the old daemon is gone;
these queries do not launch Chrome.

## Paired release

This branch requires eoka 0.6.0. Build against a local core checkout with a
command-line Cargo patch; no path override is committed. Publish core first,
then protocol, SDK, server, runner, MCP/Tack and CLI (and the updated DataDome
adapter before CLI). Normal conformance and release checks remain required.
Local release checks can use that same patch. Registry-only builds require the
new core version to exist on crates.io. After publishing core, regenerate the
workspace lockfile against the registry without the local patch before running
the registry-only release gates.
