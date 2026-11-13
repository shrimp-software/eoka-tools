# Actions and recovery

These examples use CLI 0.10.0 in an already-open `work` session. Observe first and
substitute actual page targets, expected results and fresh operation UUIDs.
`step` takes an `action` whose target is a scoped string from `view` or a live
locator such as `css:#save` or `text:Save`. Expectations instead use typed
locator objects, as shown below. Do not copy
`step`'s action shape into the lower-level `action` command; inspect
`eoka tools manifest --json` for its `actions.perform` schema.

## Forms and keyboard

For a form with `#email`, `#save` and `#status`, where blur validates the email and
enables Save, use sequential fill, native Tab and a verified click. These selectors
are illustrative: use the controls and completion evidence on the actual page.
Generate a new canonical lowercase UUID for each new action; the example UUID
below must be replaced for a new submission and retained for its recovery.
After Tab, copy Save's target from the fresh `view` into `SAVE_TARGET_FROM_VIEW`.

```bash
eoka --agent --session work view
eoka --agent --session work fill 'css:#email' 'test@example.com'
eoka --agent --session work --expect '{"kind":"enabled","target":{"query":{"kind":"css","selector":"#save"}},"enabled":true}' key Tab
eoka --agent --session work view
eoka --agent --session work step '{"operation_id":"29002b83-2093-481b-a4f2-df5c8874a26f","action":{"kind":"click","target":"SAVE_TARGET_FROM_VIEW"},"expect":{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}}'
```

Fill verifies the value, not application acceptance. `step` fill/type use `text`;
`input_mode: "bulk"` opts into one native insertion instead of per-character input.
Focus changes or target replacement stop sequential input; already-sent characters
can leave an uncertain partial action. `step` also supports `focus` when needed.
Choose evidence specific to the action: an already-visible generic heading does
not prove a submission succeeded.

The equivalent convenience click is:

```bash
eoka --agent --session work --operation-id 29002b83-2093-481b-a4f2-df5c8874a26f --expect '{"kind":"text","target":{"query":{"kind":"css","selector":"#status"}},"value":"Saved"}' click 'css:#save'
```

Choose one submission interface; do not run both examples. Keep that interface
and its original request for recovery; switching from `step` to `click` is a
different request identity.

To send a key through `step`, use `press_key` with the same typed expectation:

```bash
eoka --agent --session work step '{"action":{"kind":"press_key","key":"Tab"},"expect":{"kind":"enabled","target":{"query":{"kind":"css","selector":"#save"}},"enabled":true}}'
```

This is an alternative to `key Tab` above. Printable keys preserve case;
`Shift+o` inserts uppercase `O` on the US-keyboard mapping. Shortcuts act on the
current focus. Inspect focus and choose the expected application effect before
dispatch, including in a focused frame.

## Action kinds

`eoka action` (`actions.perform`) accepts every kind below. `step` accepts the
subset marked *step*. Kinds marked *expect* are rejected without an expectation,
because dispatch alone proves nothing about their result.

| Kind | Fields | Notes |
| --- | --- | --- |
| `click` | `target`, `double`?, `human`? | *step*, *expect* |
| `fill` | `target`, `value`, `input_mode`? | *step*; replaces the value and verifies it |
| `type` | `target`, `text`, `input_mode`? | *step*, *expect*; inserts without clearing |
| `insert_text` | `text` | *expect*; types into the focused control |
| `press_key` | `key` | *step*, *expect* |
| `submit` | `target` | *expect*; focuses the target and presses Enter |
| `select` | `target`, `values` | *step*; verifies the selected values |
| `select_option` | `target`, `value` | one option by value, falling back to its exact label |
| `select_text` | `target`, `text` | one option by exact visible label |
| `upload` | `target`, `files` | sets a file input from local paths and verifies names and sizes |
| `drag` | `target`, `dx`, `dy` | *expect*; drags by a viewport offset |
| `hover` | `target`, `human`? | *step* |
| `focus` | `target` | *step* |
| `scroll_into_view` | `target` | *step* |
| `navigate` | `url`, `referrer`? | *step*; verifies a document commit |
| `navigate_with_headers` | `url`, `headers` | temporary headers, cleared afterwards |
| `reload` | none | verifies a document commit |
| `back`, `forward` | none | *step*; one history entry |
| `history` | `delta` | relative history offset; zero reloads |

Upload a file and drag a slider with `action`; both take a typed locator target:

```bash
eoka --agent --session work action '{"operation_id":"5d0c7a1e-8f0b-4c0e-9a56-0c1f4a6b2d11","action":{"kind":"upload","target":{"query":{"kind":"css","selector":"input[type=file]"}},"files":["/home/me/report.pdf"]}}'
eoka --agent --session work action '{"operation_id":"7b9e2f44-3a61-4d0d-b7c8-91a5e3f0c6aa","action":{"kind":"drag","target":{"query":{"kind":"css","selector":"#handle"}},"dx":120,"dy":0},"options":{"expect":{"kind":"text","target":{"query":{"kind":"css","selector":"#value"}},"value":"60"}}}'
```

## Readiness and destinations

`open` waits for an interactive document. Use `--expect` for application readiness,
or a bounded `wait` when only waiting is needed:

```bash
eoka --agent --session work --expect '{"kind":"enabled","target":{"query":{"kind":"css","selector":"#save"}},"enabled":true}' wait --timeout 10000
```

Expectations include `visible`, `text`, `url`, `enabled`, `focused` and
`document_ready`. `all` combines 1–16 conditions with nesting up to 8. For example:

```json
{"kind":"all","conditions":[{"kind":"document_ready"},{"kind":"enabled","target":{"query":{"kind":"css","selector":"#save"}},"enabled":true}]}
```

For a button opening a child tab, use `destination: "new_tab"` and an expectation
on that destination. Replace `TARGET_FROM_VIEW` and the expected selector:

```bash
eoka --agent --session work step '{"action":{"kind":"click","target":"TARGET_FROM_VIEW"},"destination":"new_tab","expect":{"kind":"visible","target":{"query":{"kind":"css","selector":"#ready"}}}}'
```

The default destination is `current_or_new`; `current` restricts verification to
the origin. New tabs must be correlated to the origin, and exactly one candidate
must match. Unrelated existing tabs are not adopted. CLI `step` selects the
verified destination; its observation gives the selected `tab_id`. Receipts keep
both `target_id` and `destination_target_id`. An observation/attachment error does
not erase an action receipt or authorize another click.

## Exploration and outcomes

When exploration is intentional and there is no predicted result, make it explicit:

```bash
eoka --agent --session work step '{"action":{"kind":"press_key","key":"Tab"},"mode":"explore"}'
```

Unverified dispatch returns `outcome_unknown` and exit 4. Inspect the new page state
before deciding what to do next; do not describe this as verified success.
The default `verify` mode rejects click/type/key without expectations before input.

A `step` response carries `data.status`, `operation_id`, `operation`,
`operation_error`, `observation` and `observation_error`. These are separate:
an action may have completed even if observing its result failed. The operation
record's lifecycle `state` and `outcome` also differ. For example, `finished` alone
is insufficient; verify `outcome: "verified_complete"` for action completion.

## Recover the original operation

Record the ID before sending consequential input when possible. CLI adapters
otherwise generate it before dispatch and retain it in response/error data.
If it was lost entirely, inspect operation history instead of guessing a new ID
and repeating the action. Use the same session and state directory:

```bash
eoka --agent --session work operations get 29002b83-2093-481b-a4f2-df5c8874a26f
eoka --agent --session work operations wait 29002b83-2093-481b-a4f2-df5c8874a26f --timeout-ms 30000
eoka --agent --session work operations list --limit 100
```

`get`/`list` succeeding only means the records were read. `wait` reports completion,
running or uncertainty. To stop further input, request `operations cancel ID`, then
inspect the resulting record; already-dispatched effects are not rolled back.

`step.timeout_ms` bounds waiting plus observation, while `action_timeout_ms` bounds
the action itself. A short response wait can return exit 3 while work continues.
A waiter timeout or disconnect does not cancel or repeat the action. Cancellation
and status remain available during long input.

Repeating the same ID with the same action, mode, expectation, action deadline and
tab binding recovers the original operation. Changing these inputs yields
`operation_id_conflict`. Response-wait budgets and observation limits may change.
CLI `step` retains the original binding after automatic popup selection. Prefer
status queries when recovering; never switch targets or create a replacement
operation to hide uncertainty.

Records persist under `EOKA_STATE_DIR` or the platform's user state directory.
History reads do not launch a replacement browser. A crashed runtime can leave
`interrupted`/`outcome_unknown`; durable dispatch intent is conservative evidence,
not proof that the website did or did not apply the action. Stored history does
not provide exactly-once execution of the website's business operation.
