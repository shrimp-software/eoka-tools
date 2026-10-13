# Existing-agent browsing comparison

The first completed pilot is recorded in [RESULTS.md](RESULTS.md), with measured
results, limitations, tool traces and artifact identities.

This pilot runs the same configured Pi provider/model/thinking level and task
instructions against Eoka and Browser Use's actual CLI. Each backend gets its
own documented tool interface, isolated browser state and fresh fixture state.
There is no Eoka agent planner or simulated competitor. The fixtures cover price
lookup, form submission, a new tab, native download, a delayed/replaced control,
and pagination. The fixture server and downloaded bytes determine success.

Prerequisites: Pi with a configured default model and credentials, Chrome, a
built Eoka CLI, and Browser Use installed in an isolated environment:

```sh
uv venv /var/tmp/eoka-agent-eval/venv
uv pip install --python /var/tmp/eoka-agent-eval/venv/bin/python \
  browser-use==0.13.11 browser-harness==0.1.13
python3 eval/agent-browsing/run.py \
  --eoka ./target/debug/eoka \
  --browser-use /var/tmp/eoka-agent-eval/venv/bin/browser-use \
  --output /var/tmp/eoka-agent-comparison
```

Choose a new output directory for each run. `--tasks`, `--backends` and
`--repeats` select scope. Each case permits 30 tool calls and 240 seconds.
The runner copies the Eoka binary so recompilation cannot change a running
comparison. It records binary/source hashes, Chrome/Browser Use/Pi versions,
model selection, all tool calls, agent transcripts, token usage and elapsed time.
Only local fixture content is sent to the configured model; normal model
inference charges apply. The runner closes the browsers/daemons it created.

`results.json` includes independently scored success, server side effects,
duplicates, model-declared success, false success, calls, time and token usage.
An infrastructure error has `success: null` and must not count as a product
failure. Missing or malformed model declarations are ungraded claims, not proof
of zero false success. Read the transcripts for unsupported shortcuts or invalid
test setup before drawing conclusions.

The fixture also records requests whose user agent is not Chrome and marks that
case `valid: false`, even when its output is correct. Exclude such cases from
browser-only comparisons. This audit is not a sandbox; trace review remains
necessary to check for other prohibited shortcuts.

This is a small synthetic local pilot. It does not establish general website
success rates, stealth effectiveness, login handling, or production reliability.
Backend tool interfaces and launch defaults differ; both use the same installed
Chrome version. Browser Use has raw Python/CDP composition, and Eoka retains its
ordinary CLI tools, so tool-call counts include backend-specific batching.
