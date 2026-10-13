# Download completion follow-up

The original 100.716-second task spent only 287 ms inside all six browser tool
calls; the download command itself took 227 ms. Agent timestamps show an
approximately 83-second gap in one model turn. The other calls included an
unsupported `view --url`, an unsupported `cat`, another observation and an
operation lookup. This was agent/model delay after the file was already saved.

The shared download response now includes a `completion` message when native
completion, publication and the saved-file hash are present. It states that the
receipt confirms saving and distinguishes that from inspecting file contents.
CLI, MCP, Rust SDK and Tack receive this through the existing shared response.
Pending, failed, canceled and unverified records never receive this message.
Existing record fields and the durable store format are preserved.

## Three paired trials

The old and new binaries ran the same local download task with the same Pi
model and unchanged task, system and tool prompts. Execution order alternated
by pair. Each binary was frozen before the runs; no case was retried.

| Pair | Before calls | After calls | Before seconds | After seconds |
|---|---:|---:|---:|---:|
| 1 | 3 | 3 | 73.438 | 24.049 |
| 2 | 6 | 2 | 43.283 | 74.794 |
| 3 | 3 | 2 | 17.142 | 12.687 |
| Median | 3 | 2 | 43.283 | 24.049 |

All six runs saved exactly one correct file, with no duplicate server effects,
false success claims or prohibited HTTP/file shortcuts. Total tool calls fell
from 12 to 7. The new version still made one extra observation in its first run;
the other two ended after `view` and `download --wait`.

Median download-command time was 279 ms before and 248 ms after. No transfer
engine code changed, so that difference is not evidence of faster downloading.
The new version's 74.794-second run used only two tool calls: model latency
remains significant even when unnecessary checks are removed. Three trials do
not establish a reliable end-to-end speedup.

Validation: all 276 regular workspace tests, release checks, protocol conformance
and all 11 Chrome download tests passed. The browser regression checks the new
completion receipt after saving, daemon restart and same-ID recovery; the
protocol regression rejects premature completion and preserves legacy fields.
Checks used the local core 0.6.1 patch pending its publication.

## Evidence

- [Per-run results](results/download-completion/results.json).
- [All tool calls and outputs](results/download-completion/tools.jsonl).
- [Manifests](results/download-completion/manifests.json): identical model/prompt
  source identities and frozen binary hashes.
- [Receipt](results/download-completion/receipt.json): revisions, hashes and raw
  transcript location.
- [Pair execution script](results/download-completion/run_pairs.py), retained as
  executed with this workspace's absolute binary and output paths.

Before: `aa1b948`; after: `ef424d1`; core: `e443e8f`. The final default CLI build
matched the frozen candidate binary byte-for-byte.
