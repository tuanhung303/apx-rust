# iter9 benchmark — rich edit report + `#` comment support

Measured 2026-08-01 on macOS. Same harness as iter8 (`cds`, model
`deepseek-v4-flash`, reasoning `low`, YOLO, 2x2 swarm grid, 4 blind
sessions, fresh fixtures `tmp/w11-*`). Changes since iter8: the rich edit
report (commit `7b30782` — per-change +/- counts, hunk-paired line preview,
explicit no-op, failure context `; in PATH`), and mid-round the parser now
skips `#` comment lines after the first blind session exposed a rejection.

## Accuracy

| Round | iter9 | iter8 | iter7 |
|---|---|---|---|
| A (Go, 18 checks) | 17/18, 17/18 | 17/18, 17/18 | 16/18, 16/18 |
| B (Python, 15 checks) | 15/15, 15/15 | 15/15, 15/15 | 14/15, 14/15 |
| **Total** | **64/66** | **64/66** | **60/66** |

Functional accuracy stays at 100% (the only miss is the task-bound A6 check
the exercise never requests). The report-format change did not cost accuracy.

## Efficiency (cumulative session tokens, from session JSONLs)

| Metric | iter8 | iter9 | Δ |
|---|---|---|---|
| Input tokens, avg | 1,364,284 | **1,410,147** | +3.4% |
| Output tokens, avg | 21,075 | **24,807** | +17.7% |
| Reasoning tokens, avg | 17,433 | **20,352** | +16.7% |
| Edit payload (chars), avg | 5,430 | **9,105** | +68% |
| apx invocations, avg | 2.25 | **1.75** | −22% |
| `peek` calls | 3/4 | 3/4 | 0 |
| `--tool-help` reads | 1/4 | 2/4 | +1 |
| Rejections | 0/4 | 1/4 (fixed mid-round) | +1 |

Output/reasoning growth is the designed rich report: per-change counts plus
hunk previews cost a few hundred tokens per call and buy the agent instant
grounding on what changed (K3 guardrail: reports stay ≤1.5 KB typical).
Input stayed inside the ±5% noise band around the iter8 record (1.36M);
apx invocations per task dropped to 1.75 avg while scripts got larger
(multi-change scripts, 9.1 KB avg payload).

Per-session iter9 detail (archive order):

| Session | in | out | reasoning | apx calls | rejects | peek | exec |
|---|---|---|---|---|---|---|---|
| w1-s1 | 1,784,577 | 23,063 | 18,916 | 2 | 0 | 1 | 10 |
| w1-s2 | 1,215,091 | 18,449 | 15,367 | 1 | 0 | 1 | 8 |
| w1-s3 | 969,500 | 20,654 | 17,330 | 1 | 0 | 0 | 6 |
| w1-s4 | 1,671,419 | 37,063 | 29,794 | 3 | 1 | 1 | 8 |

## Findings

1. **The one rejection was a comment-habit collision, and it is fixed.**
   The blind agent copied apply_patch-style header comments
   (`# ---- clock.go: rename nowFn -> clockNow ----`) into the script;
   the parser rejected every `#` line as `unknown or malformed command`
   (4 errors, atomic abort). Fix: the parser now skips blank and `#` lines;
   `#` inside heredoc/type content stays literal. This was the last
   apply_patch habit that could poison a script — the other 11 calls
   applied cleanly first time.
2. **Rich report is accuracy-neutral and grounding-positive.** All four
   sessions succeeded with the new `+1/-1` + `changed lines:` preview
   format; no agent misread a preview or re-read a file after its last edit
   (peeks were pre-edit steering: 3/4 sessions).
3. **K3 output-optimization review: caveman/wenyan is a no-go.** Sessions
   are input-dominated (1.41M in vs 25k out); a compressed dialect saves
   ~1–1.5k input/turn but risks comprehension, and MCP result payloads do
   not compress. The winning changes were the report itself (this round)
   and the `; in PATH` failure suffix — not a dialect.
4. **Rigorous test battery landed.** `engine_tests` grew 14 → 23 with
   escape/unicode hardening: emoji + ©®°/中文 roundtrips (incl. report
   preview), regex metachars literal (`arr[0]`, `a^b$c.d*e`), escaped
   quotes/backslashes (`\"`, `C:\\tmp`), JSON `\u00e9` decoding, heredoc
   exact-close (prefix/suffix/indent do NOT close), CRLF scripts,
   comment-only no-op, `bsel` anchors with escaped quotes + unicode,
   `#`-comment skip + command numbering, and `diff.rs` LCS unicode
   preservation.

## Status

No new rejection class since the `#` fix; accuracy at parity; input at
apply_patch parity; output bounded by design. Remaining gap vs control
(output/reasoning +17%) is driven by MCP schema re-reads per turn — a
Phase 6 (in-process extension) concern, not a surface-tuning one.
Benchmark loop for this phase stops here unless a new failure class
appears.
