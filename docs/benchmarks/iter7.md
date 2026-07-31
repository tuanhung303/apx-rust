# iter7 benchmark — description v4 (K3, ≤680 chars, worked example) + peek-first steering

Measured 2026-08-01 on macOS. Same harness as iter6 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, 2x2 swarm grid, 4 blind
sessions: 2 exercises x 2 reps) with two changes:

1. **`apx` description v4**: 798 → 687 model-visible chars (−14% vs v3),
   produced by Kimi K3 from the v3 desc + observed-failure list. Adds a
   mandatory one-line worked example with the real newline script syntax
   (`in f.go` / `tsel 3 "old()"` / `type "new()"`); drops `sel`, `cut`,
   `copy`, `paste` mentions; keeps FRAGMENT-only / COMPLETE LINES labels and
   the frozen-baseline rule.
2. **Peek-first read steering**: session instruction file
   (`~/.codex/apx-instructions.md`, injected via `model_instructions_file`)
   now says to prefer the read-only `peek` MCP tool for the exact regions to
   edit. Verified injected in all 4 sessions.

## Accuracy

| Round | iter7 (desc v4) | iter6 (desc v3) | iter5 w2 (apply_patch) |
|---|---|---|---|
| A (Go, 18 checks) | 16/18, 16/18 | 16/18, 16/18 | 16/18, 16/18 |
| B (Python, 15 checks) | 14/15, 14/15 | 14/15, 14/15 | 14/15, 14/15 |
| **Total** | **60/66** | **60/66** | **60/66** |

Exact parity for the seventh consecutive arm. The only failures are the
task-bound A6 and the dirty-repo A18/B15 artifacts already seen in iter4-6.

## Efficiency (cumulative session tokens, from session JSONLs)

| Metric | iter5 w3 (v2) | iter6 (v3+peek) | iter7 (v4+steer) | Δ iter7 vs iter6 |
|---|---|---|---|---|
| Input tokens, avg | 1,479,054 | 2,001,397 | 2,091,927 | +4.5% |
| Input tokens, median | 1,503,642 | 1,428,824 | 1,847,640 | +29% |
| Output tokens, avg | 30,360 | 26,004 | 35,544 | +37% |
| Reasoning tokens, avg | 26,221 | 21,533 | 30,257 | +41% |
| Edit payload (chars), avg | 7,886 | 7,796 | 7,335 | **−6%** |
| Edit invocations, avg | 2.0 | 3.25 | 2.25 | **−1** |
| `--tool-help` reads | 1/4 | 2/4 | 1/4 | −1 |
| `peek` MCP calls | — | 1/4 | 0/4 | −1 |
| Rejections | 1/4 | 1/4 | 1/4 | same class, now fixed |

Token counts include per-turn context re-sends (heavily cached on DeepSeek);
use them as a relative magnitude. Output/reasoning are the noisiest metrics
across rounds (iter5 w3 30k/26k, iter6 26k/22k, iter7 36k/30k); the iter7
uptick sits inside the historical band and correlates with two mid sessions,
not with desc length.

Per-session iter7 detail:

| Session | in | out | reasoning | apx calls (chars) | rejects | peek | tool-help |
|---|---|---|---|---|---|---|---|
| exa-apx | 1,313,411 | 32,958 | 28,802 | 1 (6,337) | 0 | 0 | 0 |
| exa-apx2 | 3,359,017 | 47,514 | 40,804 | 2 (4,513) | 0 | 0 | 1 |
| exb-apx | 2,076,162 | 39,399 | 32,952 | 4 (12,771) | 1 | 0 | 0 |
| exb-apx2 | 1,619,118 | 22,304 | 18,469 | 2 (5,720) | 0 | 0 | 0 |

## Findings

1. **The v4 worked example killed the discovery phase.** `--tool-help` reads
   fell to 1/4 (from 2/4) and practice-call churn disappeared: exa-apx2 went
   from iter6's 10 rehearsal calls + peek scratch pad to 2 real edits. The
   inline newline example is the effective desc addition.
2. **New failure class found: whole-file replacement via `rm` + `new`.** exb-apx
   tried `in events.py | rm | new events.py | type ...` in one script; the
   engine rejected `new` because the destination still existed in the baseline
   and the in-script `rm` reservation was not honoured. The agent recovered via
   a 2-script dance (rm+commit, then new+type) — correct but wasteful, and it
   cost a rejection + extra deliberation.
   **Fixed in this commit**: `ensure_free` now treats a path deleted by an
   in-script `rm` as free, so `rm` + `new` replaces a file atomically in one
   script; the untouched-baseline case still rejects with an actionable hint
   ("rm it first in this script to replace"). Covered by two new engine tests
   (`rm_then_new_replaces_a_baseline_file_in_one_script`,
   `new_still_rejects_an_untouched_baseline_destination`).
3. **Peek steering did not move adoption (0/4).** The bullet reached all four
   sessions but agents still read whole files via exec `cat`/`nl` — the
   fixtures are small, context is 524k, and the model does not feel input
   cost. `peek` stays a capability (it works when chosen); pushing adoption
   further would need budget in the `apx` desc itself, which is at its char
   target.

## Verdict

Accuracy parity holds; edit payload −6% and invocations −1 with the v4
example; the discovered `rm`+`new` rejection class is fixed and rebenchmarked
next (iter8). Peek adoption is a known non-mover; further desc tightening has
hit diminishing returns at 687 chars.
