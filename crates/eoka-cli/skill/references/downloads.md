# Downloads and recovery

Use the same named session and `--agent` on every call. Keep JSON
responses, stderr and exit status. A successful start means admission, not file
completion. Record `data.id`, which is also the operation ID, and inspect
`data.state`. Supply a fresh `--operation-id` before starting when recovery from
a completely lost response is required; retain it rather than issuing a new start.

## Start and completion

Use an existing Eoka-owned browser; borrowed browsers cannot start downloads.
Observe first and select the actual scoped target or live text/CSS locator.
`download --click` performs the click itself. `--url` is only for a native
attachment URL, not the page hosting the control or a login page. It uses a
temporary tab. Snapshot labels and numeric observation indices are not targets.

```bash
eoka --agent --session files view
eoka --agent --session files download --click 'css:#download' --out ./archive.zip
eoka --agent --session files download status JOB_ID
eoka --agent --session files download wait JOB_ID --timeout-ms 120000
eoka --agent --session files download list
```

Replace the selector and `JOB_ID` with observed values. The output parent must
exist; relative paths resolve in the calling shell's directory. Publication never
overwrites an existing destination. `--expect-bytes` verifies a known size.
`--start-timeout-ms` bounds native-start detection (default 30000, maximum 300000);
`--timeout-ms` only bounds a wait. Neither wait timeout nor client exit cancels the
transfer. One file runs at a time per session.

| Evidence | Next step |
| --- | --- |
| Page explicitly reports unavailable/deleted/expired | Report that evidence; do not infer a daemon problem. |
| `action_status: not_attempted` | Inspect and select an available control before a new action. |
| `pending`, `downloading`, or `verifying` | Inspect or wait for the existing job. |
| `download_wait_timeout` | The job continues; reuse its ID. |
| `download_transport_error` | Inspect the known ID, or list history if the response was lost; do not replay the click. |
| `completed` | Record `published_path`, `received_bytes` and `sha256` from the verified receipt. |
| `failed` or `canceled` | Retain the error and staging/published paths; do not silently delete or retry. |

`download status` retrieving a failed job is read success, not download success.
Native completion, regular-file verification and publication establish success;
file existence alone does not. Inspect content only when the task requires content
analysis. If the page visibly remains loading, wait for its actual readiness
condition and observe again instead of sleeping or guessing a selector.

## Persistent history and restart recovery

History persists in the operation store under `EOKA_STATE_DIR` or the platform's
user state directory. Keep that directory and session identity when querying after
a daemon restart. `download status/list/wait` can start a history daemon without
launching Chrome; do not reopen the page merely to read the record.

Recovery can publish a staged file only when stored native-completion evidence,
byte counts, digest and file identity prove it is the original completed artifact.
It can also reconcile an already-linked output from interrupted publication.
Missing proof, altered files or unrelated destinations cannot become success.
Partial transfers do not automatically resume, browsers are not reattached, and
clicks are never replayed. Preserve uncertain artifacts for inspection.

Completed files and history survive session closure; closure cancels unfinished
work. To cancel explicitly, use `download cancel JOB_ID`, then inspect its state.
If history itself is unavailable, report the missing evidence rather than assuming
the download never ran or using a browser restart as a retry strategy.

## Sequential batches

1. Open and inspect each page in the named session. Record explicit unavailability
   when the page provides that evidence.
2. Start at most one file at a time. Let `download --click` perform the only click.
3. Save the response and ID before waiting. A nonzero wait exit can leave work active.
4. Finish the job, or cancel and confirm a terminal state, before navigating to the
   next item. Keep the browser open between files.
5. Report completed, failed, pending/uncertain and pre-existing files distinctly.
   A script must not print unconditional success after warnings or partial results.

Do not pre-click with JavaScript, use shell background jobs, or restart the browser
to coordinate a batch. SDK/MCP/Tack require absolute output paths on their host and
use `download.start/list/get/wait/cancel` (Tack prefix: `eoka.`).
