# 2026-11-14: three released versions and one branch on three surfaces

One repeat per cell. Read the direction, not the percentages: the intervals
below overlap, so this run does not establish a difference on its own.

## Setup

- Agent: Pi 1.4.0, `openrouter/deepseek/deepseek-v4-flash`, thinking high.
- Browser: Chrome 157 beta, the same binary for every version.
- Cases: 10 tasks × 3 surfaces × 3 versions × 1 repeat, 4 at a time. The MCP
  surface used Pi's code mode.
- Versions:

| Label | CLI | MCP server | Skill | What changed |
|---|---|---|---|---|
| v0.12 | 0.12.1 | 0.9.0 | from tag `eoka-cli-v0.12.1` | before blocked-tab errors |
| v0.13 | 0.13.0 | 0.9.1 | from tag `eoka-cli-v0.13.0` | blocked-tab errors (#120) |
| v0.14 | 0.14.1 | 0.10.0 | from tag `eoka-cli-v0.14.1` | typed engine, 31 MCP tools on it (#123) |

`manifest.json` has the binary and skill hashes. `results.json` has every case.
Two cases the first run did not reach (the leave-page task through MCP on v0.13
and v0.14) were run afterwards and are in `baseline-leave-results.json`; the
tables below include them.

## Result

| Version | Scored | Passed | 95% interval | Stuck | Recovered after a blocked-tab error | Tokens |
|---|---|---|---|---|---|---|
| v0.12 | 30 | 26 | 70%–95% | 5 | 0 | 6.8 M |
| v0.13 | 30 | 29 | 83%–99% | 1 | 8 | 4.5 M |
| v0.14 | 30 | 30 | 89%–100% | 0 | 8 | 3.9 M |

By surface (passed/scored, stuck in brackets):

| Version | tool | skill | mcp |
|---|---|---|---|
| v0.12 | 8/10 (2) | 8/10 (2) | 10/10 (1) |
| v0.13 | 9/10 (1) | 10/10 (0) | 10/10 (0) |
| v0.14 | 10/10 (0) | 10/10 (0) | 10/10 (0) |

What did not pass:

- v0.12: the confirm dialog through the skill, the leave-page dialog through
  the skill and the tool, and the contact form through the tool. All four ran
  into the time cap or the call budget. v0.12 has no blocked-tab errors, so the
  agent was never told a dialog was open.
- v0.13: the leave-page dialog through the tool. The agent was shown
  `dialog_open` and still ran out of time.
- v0.14: nothing.

Eight cases in each of v0.13 and v0.14 passed after the agent was shown
`dialog_open`, `navigation_pending` or `navigation_timeout`. These are the
dialog, leave-page and silent-server tasks.

## What this says and does not say

- The MCP replies that #123 changed did not cost anything measurable: the MCP
  surface passed 10 of 10 on both v0.13 and v0.14.
- Getting stuck is what costs tokens. The six stuck cases used 380,000 to
  760,000 tokens each; the median case that was not stuck used 109,000.
- With one repeat, 26 of 30 against 30 of 30 is not a statistically established
  difference.

## The branch that puts the MCP server on the engine

Measured at commit `6f4075d` of `refactor/mcp-on-the-engine`, release builds,
same agent, tasks and browser. Results are in `branch/`.

| Surface | Scored | Passed | Stuck | Recovered |
|---|---|---|---|---|
| tool | 9 | 7 | 2 | 1 |
| skill | 8 | 8 | 0 | 3 |
| mcp | 8 | 7 | 2 | 2 |

Five cases were not run because the token cap was reached: pricing on all
three surfaces and pagination through the skill and MCP.

Three cases failed. v0.14 passed all three, so each transcript was read:

- **Download through MCP.** The agent never called `download_start`. It clicked
  the download link six times and checked `download_list`, which stayed empty,
  until the time cap. The download tools were not exercised. Pi does not pass
  an MCP server's instructions to the model, so the instructions rewritten on
  this branch were not seen by either agent.
- **Leave-page dialog through the tool.** The agent was shown `dialog_open` with
  the hint that accepting leaves the page and dismissing stays. It dismissed,
  then wandered. v0.14 gave the same error and hint and its agent accepted;
  v0.13 also failed this case.
- **Contact form through the tool.** Fourteen of the agent's thirty calls were
  rejected step requests with a malformed `expect`, and it ran out of calls.
  Parsing a step is unchanged on this branch.

None of the three shows a reply that this branch changed. One repeat cannot
separate an agent choosing differently from a real regression: 22 of 25 against
30 of 30 has overlapping intervals (70%–96% and 89%–100%), and the branch was
not run a second time. So the failed cases were replayed without an agent, the
same calls against v0.14 and the branch on the same fixture:

- The MCP download run's calls, including `download_start`: identical replies,
  and both saved the file.
- The leave-page flow (fill, click with the dialog open, read, dismiss, click
  again, accept), twice on each version: identical replies. The only
  difference anywhere is between two runs of v0.14 itself, in a reply that
  depends on timing.
- `eoka step` parsing is unchanged on this branch. `fill` on a missing target
  now reports `action_timeout` instead of `operation_pending`, which is the
  intended effect of the engine's two-second report grace.

The leave-page replay also shows a defect both versions have: after the dialog
is dismissed, clicking the link again starts no navigation, opens no dialog
and reports `operation_pending` after eight seconds. That is where the failing
agent got lost. The first click was still waiting for its URL expectation and
held the tab's action gate, so the second click was never dispatched.

Three changes were made after this run because of what the transcripts showed.
None has been measured with an agent:

- Dismissing a leave-page dialog stops the operations running on that tab and
  names them. Replayed, the second click opens the dialog again and accepting
  it reaches the summary page.

- A rejected step request now prints the shapes a step and its `expect` take.
- The instructions the MCP server sends say that `download_start` saves a file
  and that clicking a download link does not. Pi does not show these to the
  model, so this helps only clients that do.

## Cost

The three released versions took 15.2 million tokens ($0.84) and the branch
6.8 million ($0.37). Runs are far larger than the 21,000 tokens the
earlier single-tool pilots used: about 30,000 to 100,000 through the tool,
80,000 to 165,000 through the skill and 130,000 to 240,000 through MCP at the
median. Five repeats of this matrix would be about 75 million tokens.
