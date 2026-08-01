# iter12 benchmark — read-hygiene steering (priority tiers)

Measured 2026-08-01 on macOS. Same harness as iter11 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, blind 2x1 swarm grid, fresh
fixtures `tmp/w15-*` from the authoritative seeds). Change since iter11:
`~/.codex/apx-instructions.md` restructured into explicit priority tiers —
1. EDITS always via the `apx` MCP tool (one batched script per task),
2. READS default via `peek` (line numbers ARE selector coordinates, never
hand-count), 3. FALLBACK exec `cat`/`nl` only for whole-file context or
listing — "never cat/nl a whole file just to edit one region".

## Accuracy

| Round | iter12 | iter11 |
|---|---|---|
| A (Go, 18 checks) | 17/18 | 17/18 |
| B (Python, 15 checks) | 15/15 | 15/15 |
| **Total** | **32/33** | **32/33** |

Functional accuracy stays at 100% (only miss remains the task-bound A6
check the exercise never requests). 0 apx rejections, 0 apply_patch calls.

## Efficiency (per-session sums, from session JSONLs)

| Session | Tool | Input tokens | Output tokens | Edit calls | Payload chars | peek | exec (chars) |
|---|---|---|---|---|---|---|---|
| A (Go) | apx iter12 | 1,292,755 | 12,818 | 1 | 4,951 | 2 | 5 (844) |
| B (Py) | apx iter12 | 1,583,368 | 12,462 | 2 | 7,841 | 2 | 6 (1,458) |
| **Total** | **apx iter12** | **2,876,123** | **25,280** | **3** | **12,792** | 4 | 11 (2,302) |
| Total | apx iter11 | 4,833,881 | 54,412 | 4 | 15,273 | 6 | 19 (4,643) |

Delta vs iter11: input **-40.5%**, output **-53.5%**, payload **-16.2%**,
edit calls 3 vs 4, exec bytes -50%. Per lane: A input -42.6% / output
-59.8%; B input -38.7% / output -44.6%.

## Findings

1. **Read hygiene is the token lever, not description length.** The
   priority-tier wording cut per-session input ~40% with the SAME accuracy —
   exec reads dropped from 4,643 chars to 2,302 (Go: 1,508->844; Python:
   3,135->1,458) as agents switched to peek for region reads.
2. **One-script-per-task held.** Go finished the full refactor in a single
   4,951-char apx script; Python used 2 (7,841 chars). Zero apply_patch
   drift, zero rejections, zero repo-root reads.
3. **One residual drift:** the Python agent still batched `nl -ba` over all
   4 files in a single exec call early on (whole-file, line-numbered). It
   no longer re-reads after edits; a follow-up could forbid whole-file
   `nl`/`cat` batches entirely (peek supports multi-file scripts already:
   `in f1\nrsel 1:20\nin f2\nrsel 1:20`).

## Status

Best per-session cost recorded on these exercises while holding 100%
## Status

Best per-session cost recorded on these exercises while holding 100%
functional accuracy. Per-session averages are now at parity with the
apply_patch control (iter12 avg input 1,438,062 vs control 1,352,034,
+6.4%; avg output 12,640 vs 13,667, -7.5%) — with 3 edit calls vs 14,
-63% payload per edit, zero rejections, and atomicity guaranteed. The
remaining per-session input premium is agent discovery behavior, not tool
surface; the next lever would be forbidding whole-file `nl`/`cat` batches
in favor of multi-file `peek` scripts.
