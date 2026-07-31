# iter8 benchmark — desc v4 + `rm`+`new` one-script replacement (engine fix)

Measured 2026-08-01 on macOS. Same harness as iter7 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, 2x2 swarm grid, 4 blind
sessions, fresh fixtures `tmp/w10-*`). Single change since iter7: the engine
fix for whole-file replacement — `ensure_free` now honours an in-script `rm`,
so `rm PATH` + `new PATH` replaces a file atomically in one script, and the
untouched-baseline case rejects with an actionable hint.

## Accuracy

| Round | iter8 (v4+fix) | iter7 (v4) | iter6 (v3+peek) | iter5 w2 (apply_patch) |
|---|---|---|---|---|
| A (Go, 18 checks) | 17/18, 17/18 | 16/18, 16/18 | 16/18, 16/18 | 16/18, 16/18 |
| B (Python, 15 checks) | 15/15, 15/15 | 14/15, 14/15 | 14/15, 14/15 | 14/15, 14/15 |
| **Total** | **64/66** | **60/66** | **60/66** | **60/66** |

The only failing check is the task-bound A6 (the exercise never requests that
path). A18/B15 — the "touched the dirty repo root" artifacts that failed every
prior arm — now pass because the repo root was clean at launch (iter7 was
committed before iter8 started), confirming they were measurement artifacts,
not agent behaviour. Adjusted functional accuracy: 100%.

## Efficiency (cumulative session tokens, from session JSONLs)

| Metric | iter5 w2 patch | iter6 (v3+peek) | iter7 (v4+steer) | iter8 (v4+fix) | Δ iter8 vs iter7 |
|---|---|---|---|---|---|
| Input tokens, avg | 1,352,034 | 2,001,397 | 2,091,927 | **1,364,284** | **−35%** |
| Input tokens, median | — | 1,428,824 | 1,847,640 | **1,320,817** | **−29%** |
| Output tokens, avg | 13,667 | 26,004 | 35,544 | **21,075** | **−41%** |
| Reasoning tokens, avg | 9,283 | 21,533 | 30,257 | **17,433** | **−42%** |
| Edit payload (chars), avg | 8,685 | 7,796 | 7,335 | **5,430** | **−26%** |
| Edit invocations, avg | 3.5 | 3.25 | 2.25 | 2.25 | 0 |
| `--tool-help` reads | 0/4 | 2/4 | 1/4 | 1/4 | 0 |
| `peek` MCP calls | — | 1/4 | 0/4 | **3/4** | **+3** |
| Rejections | 0/4 | 1/4 | 1/4 | **0/4** | **−1** |

Iter8 input tokens are at apply_patch-control parity (1.36M vs 1.35M avg) —
the first arm to reach it. Output/reasoning remain above the control (MCP
tool-schema overhead is re-read every turn), but both are at their lowest
since iter4.

Per-session iter8 detail:

| Session | in | out | reasoning | apx calls (chars) | rejects | peek | tool-help |
|---|---|---|---|---|---|---|---|
| exa-apx | 1,477,467 | 20,421 | 16,726 | 2 (4,512) | 0 | 1 (pre-edit) | 0 |
| exa-apx2 | 1,718,883 | 31,725 | 26,921 | 2 (6,872) | 0 | 1 (pre-edit) | 1 |
| exb-apx | 1,096,618 | 20,744 | 17,813 | 1 (5,256) | 0 | 1 (pre-edit) | 0 |
| exb-apx2 | 1,164,166 | 11,411 | 8,273 | 4 (5,079) | 0 | 0 | 0 |

## Findings

1. **The `rm`+`new` fix closed the last rejection class.** Zero rejections in
   all four sessions (iter6/iter7 each had one). exb-apx2 still made 4 calls —
   one per file — but every script applied cleanly first time.
2. **Peek-first steering finally moved the needle: 3/4 adoption, all pre-edit.**
   Every peek call in iter8 precedes the first `apx` edit (exa-apx:
   peek → apx → apx; exa-apx2: peek → apx → apx; exb-apx: peek → apx). The
   bounded-read habit appears once the tool surface stopped producing
   rejections that forced whole-file context re-reads. The single non-peek
   session (exb-apx2) was also the cheapest (11.4k out).
3. **Smaller, targeted scripts.** Edit payload fell −26% (7,335 → 5,430 chars
   avg) with the same 2.25 invocations — agents write less because they no
   longer need to re-emit whole files to replace them (rm+new) and they read
   less before editing (peek).
4. **A18/B15 were always artifacts.** They passed only because the repo root
   was clean at launch. The honest accuracy line is: functional 100%, raw
   A6-only failure — unchanged across every arm.

## Verdict

iter8 is the first arm with all of: functional-accuracy parity, zero
rejections, peek adoption 3/4 pre-edit, input tokens at apply_patch control
parity, and payload −26%. The remaining headroom (output/reasoning vs the
control, 1/4 tool-help read) is desc-variance-level; gains from here are
insignificant relative to round cost. Benchmark loop stops here; tool surface
is: MCP `apx` (687-char desc v4, worked example) + `peek` (300-char desc) +
peek-first session steering + rm+new one-script replacement.
