# Local agent comparison: 2026-10-12

This is the original six-task baseline. The later download receipt improvement
and its controlled before/after trials are in [DOWNLOAD-RESULTS.md](DOWNLOAD-RESULTS.md).

Both backends completed all six tasks. Eoka used 29 tool calls; Browser Use used
81. Neither produced duplicate server effects or a false success declaration.
This is one trial per task with one model, not a general website success rate.

| Task | Eoka calls | Browser Use calls | Eoka seconds | Browser Use seconds |
|---|---:|---:|---:|---:|
| Price lookup | 3 | 8 | 4.0 | 8.8 |
| Form submission | 9 | 25 | 77.4 | 115.3 |
| New tab | 4 | 13 | 18.3 | 46.5 |
| Native download | 6 | 9 | 100.7 | 42.6 |
| Delayed/replaced control | 4 | 9 | 13.9 | 46.4 |
| Pagination | 3 | 17 | 10.9 | 49.3 |
| Total | 29 | 81 | 225.2 | 308.9 |

Reported model token totals were 129,099 for Eoka and 220,862 for Browser Use,
including cached input. End-to-end times include model inference and tool calls;
they are not isolated browser-engine timings. Aggregate time inside tool calls
was 19.579 seconds for Eoka and 17.461 for Browser Use. On the download task,
Eoka's native download finished in 227 ms, but the agent made four further calls
before answering. Fewer calls did not make that task faster.

The dynamic case produced exactly one `generate` POST per backend. The form
case produced exactly one POST with the requested name, email and role. Both
downloaded the same 36-byte CSV, checked against fixture bytes and SHA-256
`c7e1f983fbd73ae90509e553899d5ab5c442be038fda14f0e0bc1b54cb359708`.

## Method and limits

Pi 1.0.2 used `openrouter/deepseek/deepseek-v4-flash` with high thinking for both
backends; transcripts confirmed that model identity. Browser Use 0.13.11 used
its actual CLI through browser-harness 0.1.13. Both used installed Chrome
152.0.7977.13 beta, isolated browser state and fresh fixture state, with the same
task/system instructions and limits of 30 calls and 240 seconds per case.

This compares the text adapters and helper descriptions in [run.py](run.py).
It does not compare Browser Use's own planner or its full visual skill workflow.
Launch defaults, viewports, tool interfaces and batching differ. There were no
repeats, randomized order, controlled provider latency or production websites.
Eoka ran first for every pair. The result demonstrates these local workflows,
not general superiority or a reliable speed multiplier.

Tool inputs were reviewed for prohibited HTTP and local/source-file shortcuts.
All 12 final cases passed that audit and the fixture's Chrome-user-agent check.
CSV reads were restricted to files downloaded by the browser. Some model calls
were invalid or unproductive; those remain counted in the traces.

Development runs are excluded. They exposed an incorrect daemon-idle setting,
unstable references, a CLI observation-envelope bug, and one Browser Use HTTP
shortcut. The final run was frozen after fixing the product/harness issues;
there were no per-case retries in this reported run.

## Evidence

- [Per-case results](results/2026-10-12/results.json): independent fixture scores,
  server effects, model claims, tokens and timing.
- [All tool calls and outputs](results/2026-10-12/tools.jsonl).
- [Manifest](results/2026-10-12/manifest.json): model, versions and source/binary
  hashes. The pinned Eoka binary matched the final built CLI byte-for-byte.
- [Receipt](results/2026-10-12/receipt.json): source revisions and artifact hashes.

Core revision: `e443e8f`; tools implementation revision: `baddec1`. Full agent
transcripts and the frozen executable remain in the local raw directory recorded
in the receipt. Reproduce the run using the [runner instructions](README.md).
