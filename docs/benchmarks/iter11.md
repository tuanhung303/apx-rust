# iter11 benchmark — tool-description tuning (worked example + peek-first)

Measured 2026-08-01 on macOS. Same harness as iter10 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, blind swarm grid, fresh fixtures
`tmp/w14-*` from the authoritative seeds). Changes since iter10 (commit
`98bdd94`): `APX_MCP_DESCRIPTION` now carries a worked example (2-file batch +
heredoc) and the peek-coordinate hint; `PEEK_MCP_DESCRIPTION` states its line
numbers are the coordinates for `tsel`/`rsel`; `~/.codex/apx-instructions.md`
adds "peek before tsel/rsel, never hand-count". Candidate count reduced per
request: 2x1 grid (1 Go + 1 Python), down from 2x2.

## Accuracy

| Round | iter11 | iter10 |
|---|---|---|
| A (Go, 18 checks) | 17/18 | 17/18, 17/18 |
| B (Python, 15 checks) | 15/15 | 15/15, 15/15 |
| **Total** | **32/33** | **64/66** |

Functional accuracy stays at 100% (the only miss is the task-bound A6 check
the exercise never requests). 0 apx rejections, 0 apply_patch calls in both
sessions (previous rounds: 0 apply_patch as well, but this round the steering
held with zero drift).

## Efficiency (per-session sums, from session JSONLs)

| Session | Tool | Input tokens | Output tokens | Edit calls | Payload chars | peek calls |
|---|---|---|---|---|---|---|
| A (Go) | apx iter11 | 2,251,542 | 31,927 | 3 | 8,287 | 3 |
| B (Py) | apx iter11 | 2,582,339 | 22,485 | 1 | 6,986 | 3 |
| **Total** | **apx iter11 (2 sessions)** | **4,833,881** | **54,412** | **4** | **15,273** | 6 |
| Total | apx iter10 (4 sessions) | 5,197,537 | 79,063 | 6 | 29,651 | 0 |

## Findings

1. **Tool-usage shape changed exactly as steered.** peek went 0 -> 6 calls;
   the Python session finished its whole exercise in a SINGLE apx script
   (6,986 chars, 1 call — the batched-edit ideal); both sessions used apx
   exclusively (0 apply_patch) and never read repo-root files (the iter10 w1
   Go contamination did not recur).
2. **Per-session tokens did not drop.** iter11 A spent 2.25M input vs iter10
   A1's 0.87M for the identical 17/18 result. At n=1 per lane the
   session-to-session variance (discovery reads, validation loops) dwarfs any
   description-length effect; the dominant token driver is exec-side reads
   (`cat`/`nl` whole files during exploration), not tool-description size.
3. **Description tuning is a QOL/steering win, not a token win.** The worked
   example and coordinate hint changed *what* agents call (peek-first,
   batched scripts) but not *how much* they read. The next lever is read
   hygiene: steer agents to satisfy exploration with peek + targeted reads
   instead of whole-file `cat`/`nl`, then re-measure.

## Status

Accuracy parity held (32/33 raw, 100% functional). Behavior matches the
tuned surface (peek-first, one-script-per-task, zero apply_patch drift).
Token totals for the halved 2x1 grid are -7% input / -31% output vs the 2x2
iter10 total, but per-session averages rose — attributed to agent variance
and read behavior, not the description change. Re-benchmark after read-hygiene
steering if the per-session cost matters more than call-shape quality.
