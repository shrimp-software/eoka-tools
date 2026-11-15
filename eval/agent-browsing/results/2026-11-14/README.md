# 2026-11-14: three released versions on three surfaces

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

## Result

| Version | Scored | Passed | 95% interval | Stuck | Recovered after a blocked-tab error | Tokens |
|---|---|---|---|---|---|---|
| v0.12 | 30 | 26 | 70%–95% | 5 | 0 | 6.8 M |
| v0.13 | 29 | 28 | 83%–99% | 1 | 7 | 4.2 M |
| v0.14 | 29 | 29 | 88%–100% | 0 | 7 | 3.7 M |

By surface (passed/scored, stuck in brackets):

| Version | tool | skill | mcp |
|---|---|---|---|
| v0.12 | 8/10 (2) | 8/10 (2) | 10/10 (1) |
| v0.13 | 9/10 (1) | 10/10 (0) | 9/9 (0) |
| v0.14 | 10/10 (0) | 10/10 (0) | 9/9 (0) |

What did not pass:

- v0.12: the confirm dialog through the skill, the leave-page dialog through
  the skill and the tool, and the contact form through the tool. All four ran
  into the time cap or the call budget. v0.12 has no blocked-tab errors, so the
  agent was never told a dialog was open.
- v0.13: the leave-page dialog through the tool. The agent was shown
  `dialog_open` and still ran out of time.
- v0.14: nothing.

Seven cases in each of v0.13 and v0.14 passed after the agent was shown
`dialog_open`, `navigation_pending` or `navigation_timeout`. These are the
dialog, leave-page and silent-server tasks.

## What this says and does not say

- The MCP replies that #123 changed did not cost anything measurable: the MCP
  surface passed 9 of 9 on both v0.13 and v0.14.
- Getting stuck is what costs tokens. The six stuck cases used 380,000 to
  760,000 tokens each; the median case that was not stuck used 109,000.
- With one repeat, 26 of 30 against 29 of 29 is not a statistically established
  difference.
- Two cases were not run: the leave-page task through MCP on v0.13 and v0.14.
  The run stopped starting cases at its token cap.

## Cost

14.7 million tokens, $0.82. Runs are far larger than the 21,000 tokens the
earlier single-tool pilots used: about 30,000 to 100,000 through the tool,
80,000 to 165,000 through the skill and 130,000 to 240,000 through MCP at the
median. Five repeats of this matrix would be about 75 million tokens.
