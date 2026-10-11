# Tracking downloads and batches

Use the same named session and `--agent` on each call. Keep full JSON responses,
stderr and exit status. A successful start means a job was accepted, not that the
file exists yet. Record `data.id` and inspect `data.state`.

| Evidence | Next step |
| --- | --- |
| Page explicitly reports unavailable/deleted/expired | Report that page evidence; do not infer a daemon problem. |
| `action_status: not_attempted` | Inspect the page and select an available control before a new action. |
| `pending`, `downloading`, or `verifying` | Inspect or wait for the existing job. |
| `download_wait_timeout` | The job continues. Reuse the returned job ID. |
| `download_transport_error` | The outcome is uncertain. Inspect the known job ID, or list jobs if the start response was lost. Do not replay the click. |
| `completed` | Record the verified published path as success. |
| `failed` or `canceled` | Retain the error and any staging/published paths; do not silently delete or retry. |

`download status` successfully retrieving a failed job does not make that job
successful. Check its state and nested error. If the daemon is gone, job history
cannot be recovered by restarting it. Report uncertainty and inspect retained
artifacts rather than assuming the transfer never ran.

For a sequential batch:

1. Open and inspect each page in the named session. Log an explicit unavailable
   result when the page provides that evidence.
2. Start at most one file at a time. Let `download --click` perform the only click.
3. Save the response and job ID before waiting. A bounded wait may return a
   nonzero exit status while the job remains active.
4. Finish the job, or explicitly cancel it and confirm a terminal state, before
   navigating to the next item. Do not close the browser between files.
5. Summarize successful, failed, pending/uncertain and pre-existing files
   separately. A script exits nonzero when any item failed or remains pending;
   never print unconditional success after warnings. File existence alone does
   not verify an earlier download.

Use `download --url ... --out ...` only for known attachment URLs. For a hosting
page, first observe and use the actual control. `--start-timeout-ms` bounds native
start detection; `--timeout-ms` bounds a wait. Neither cancels a running transfer.
Do not use shell background jobs, fixed sleeps, pre-clicking JavaScript, or
speculative daemon restarts to coordinate this workflow.
