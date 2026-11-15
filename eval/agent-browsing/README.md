# Existing-agent browsing evaluation

This harness runs one configured Pi provider, model and thinking level through
Eoka on local fixture tasks and scores each run from what the fixture server
saw. There is no Eoka agent planner. It answers one question: given the same
agent and the same tasks, does one build of the tools get it stuck less than
another?

Earlier pilots are recorded in [RESULTS.md](RESULTS.md) and
[DOWNLOAD-RESULTS.md](DOWNLOAD-RESULTS.md). They predate verify-by-default and
this harness's surfaces and tasks, so their numbers are not comparable with new
runs.

## Surfaces

`--surfaces` picks how the agent reaches the tools:

| Surface | What the agent gets |
|---|---|
| `tool` | One `browser` tool that passes arguments to `eoka --agent`, with a fixed prompt. The browser is already open at the task URL. |
| `skill` | A shell and the Eoka skill of that version. The agent opens the browser itself. |
| `mcp` | The `eoka-mcp` server of that version. `--mcp-exposure codemode` (Pi's default) lets the agent find and call tools from scripts; `direct` declares every tool to the model, as most MCP clients do. |

## Tasks

Price lookup, form submission, a new tab, a native download, a delayed and
replaced control, pagination, a confirm dialog on submit, a mirror that never
answers, a login with a redirect, and a leave-page dialog.

## Running

Prerequisites: Pi with a configured default model and credentials, Chrome on
`PATH` as `google-chrome`, and the binaries to compare. Give each version a
label; `mcp=` and `skill=` are needed only for those surfaces.

```sh
python3 eval/agent-browsing/run.py \
  --version new=./target/debug/eoka,mcp=./target/debug/eoka-mcp,skill=crates/eoka-cli/skill \
  --version old=/path/to/old/eoka,mcp=/path/to/old/eoka-mcp,skill=/path/to/old/skill \
  --surfaces tool skill mcp --repeats 5 --jobs 4 \
  --output /var/tmp/eoka-eval/run-1
python3 eval/agent-browsing/compare.py /var/tmp/eoka-eval/run-1/results.json
```

Choose a new output directory for each run. `--tasks` and `--repeats` select
scope, `--jobs` runs cases in parallel and `--token-cap` stops starting new
cases once that many tokens are spent. `--browser-use PATH` adds the Browser
Use CLI on the `tool` surface. Versions are interleaved, and the order flips on
alternate repeats.

Each case gets its own runtime, state and temporary directories, 240 seconds,
and on the `tool` surface 30 calls. The runner copies the binaries so a rebuild
cannot change a running comparison, and it records binary, skill, runner,
fixture and extension hashes, Chrome and Pi versions, the model, transcripts,
token usage and cost. Only local fixture content is sent to the configured
model; normal model inference charges apply. The runner ends the sessions and
processes it created.

## Reading results

`results.json` has one entry per case:

- `success` is decided by the fixture server and downloaded bytes, not by the
  agent's answer. `claimed_success` is what the agent said, and `false_success`
  is a claim the fixture contradicts.
- `stuck` is true when the case hit the time cap or the call budget, or made
  three identical calls in a row.
- `blocked_codes` lists the blocked-tab errors the agent was shown
  (`dialog_open`, `navigation_pending`, `navigation_timeout`,
  `page_unresponsive`, `daemon_busy`), and `recovered` is a success after at
  least one of them.
- `valid` is false if any request arrived without a Chrome user agent. Exclude
  those cases from browser-only comparisons. This audit is not a sandbox;
  read the transcripts for other shortcuts.
- An infrastructure error has `success: null` and must not count as a product
  failure.

`compare.py` prints pass counts with a 95% Wilson interval, stuck and recovered
counts and medians per version and surface, then pass counts per task.

This is a small synthetic local evaluation with one model. It does not
establish general website success rates, stealth effectiveness or production
reliability. With few repeats per cell the intervals are wide; say so when
quoting a difference.
