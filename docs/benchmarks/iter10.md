# iter10 benchmark — tighter report preview caps + closest-line hint

Measured 2026-08-01 on macOS. Same harness as iter9 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, blind 2x2 swarm grid, fresh
fixtures `tmp/w12-*` from the authoritative seeds in
`apx/doc/benchmarks/exercises.md`). Changes since iter9 (commit `165f2eb`):
report preview caps tightened (`REPORT_PREVIEW_LINES` 60->24,
`REPORT_PREVIEW_BYTES` 4096->2048, `REPORT_TOTAL_BYTES` 8192->4096; counts
and the omitted-lines marker are kept) and a closest-line hint appended to
selector-miss diagnostics (`tsel`/`bsel`, bigram-Dice similarity, floor 0.5,
<=400 candidates, snippet <=80 chars).

## Accuracy

| Round | iter10 | iter9 |
|---|---|---|
| A (Go, 18 checks) | 17/18, 17/18 | 17/18, 17/18 |
| B (Python, 15 checks) | 15/15, 15/15 | 15/15, 15/15 |
| **Total** | **64/66** | **64/66** |

Functional accuracy stays at 100% (the only miss is the task-bound A6 check
the exercise never requests). The tighter report did not cost accuracy;
0 rejections across all sessions (the closest-line hint never fired — no
selector missed).

## Efficiency (per-session sums, from session JSONLs)

| Session | Tool | Input tokens | Output tokens | Edit calls | Payload chars |
|---|---|---|---|---|---|
| A1 | apx iter10 | 872,030 | 20,180 | 1 | 7,186 |
| A2 | apx iter10 | 1,720,897 | 28,055 | 2 | 7,532 |
| B1 | apx iter10 | 1,622,459 | 19,728 | 2 | 7,476 |
| B2 | apx iter10 | 982,151 | 11,100 | 1 | 7,457 |
| **Total** | **apx iter10** | **5,197,537** | **79,063** | **6** | **29,651** |
| Total | apx iter9 | 5,640,587 | 99,229 | 7 | 36,421 |
| Total | apply_patch (iter5) | 5,408,136 | 54,666 | 14 | 34,740 |

Delta vs iter9: input **-7.9%**, output **-20.3%**, payload **-18.6%**,
edit calls 6 vs 7. Delta vs apply_patch control: input -3.9% (parity),
edit calls 6 vs 14 (2.3x fewer), payload -14.6%; output remains higher
(+44.6%) — the known model-deliberation cost of a coordinate grammar, now
partially offset by the trimmed report.

## Protocol note

The first w1 wave's two Go sessions read repo-root files (the tool's own
source in one case, benchmark docs in the other) before their first edit —
4.7M/2.5M input tokens, protocol drift, not tool behavior. The Go arm was
re-run blind (2x1 grid, identical task text, fresh `tmp/w13-*` fixtures);
the rerun sessions are the A1/A2 rows above and both are clean (5-9 exec
calls, 1-2 apx calls, no repo-root reads).

## Findings

1. **Output gap vs iter9 closed by one fifth.** The caps work: live reports
   measured 2.9-4.7 KB/call (was 3.4-5.1 KB) and agents still never re-read
   a file after their last edit — counts + omitted marker are enough
   grounding at 24 lines/2KB per file.
2. **One-shot scripts are now the norm.** 3 of 4 sessions finished their
   whole exercise in a single 7.2-7.5 KB script (B2: 7 file changes in one
   call, 982K input — the cheapest session ever recorded on these
   exercises; A1: full Go refactor in one 7.2 KB script).
3. **Hint is neutral-until-needed.** 0 selector misses this round, so the
   hint added nothing to cost (failure-only) and nothing to accuracy; its
   payoff is the re-read it saves when an anchor does miss.
4. **Blind-harness hygiene matters more than tool tuning.** The w1 Go
   contamination cost ~5M input tokens; fixture-dir sessions inside the
   repo invite "what is this repo?" spelunking. Future waves should pin
   the agent cwd to the fixture and keep repo-root reads out of budget
   steering.

## Status

Accuracy at parity, input now *below* the apply_patch control, output
-20% vs iter9 with the same functional result. The remaining output gap is
grammar construction, not report size; no further report trimming without
re-measuring grounding.
