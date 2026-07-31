# iter6 benchmark — description v3 (K3-tuned, −41%) + MCP `peek` tool

Measured 2026-08-01 on macOS. Same harness as iter5 (stock codex CLI via
`cds`, model `deepseek-v4-flash`, reasoning effort `low`, YOLO) with two
tool-surface changes to `crates/apx-mcp`:

1. **`apx` description v3**: 1,347 → 798 source chars (−41%), produced by Kimi
   K3 (`kimi-for-coding/k3-256k` via OpenCode) from a detox-context task
   (current desc + observed-failure list + hard constraints; output contract
   ≤800 chars). The fragment rule is compressed to one phrase —
   "FRAGMENT-only, replaces match, never line" — and `rsel` stays labeled
   `COMPLETE LINES`.
2. **New read-only `peek` tool**: selector-scoped region reads through the same
   grammar (`evaluate_peek` + `FsBaseline`, same root/cwd resolution and
   fail-closed diagnostics; never writes). Model-visible description 300 chars.

4 sessions = 2 exercises × 2 reps, apx arm only (`swarm-wave.sh --grid 2x2`,
topic `apx-bench-iter6-w1-apx`, fresh fixtures `tmp/w8-*`). Controls are
iter5's w2 (apply_patch) and w3 (apx desc v2) arms; all numbers re-derived from
archived session JSONLs with the same analyzer.

## Accuracy

| Round | apx (desc v3 + peek) | iter5 w3 (desc v2) | iter5 w2 (apply_patch) |
|---|---|---|---|
| A (Go, 18 checks) | 16/18, 16/18 | 16/18, 16/18 | 16/18, 16/18 |
| B (Python, 15 checks) | 14/15, 14/15 | 14/15, 14/15 | 14/15, 14/15 |
| **Total** | **60/66** | **60/66** | **60/66** |

Exact parity again. The only failing check is the task-bound A6
(`internal/leasestore/store.go`: the task never requests that filename) plus
A18/B15 (repo root dirty with our own uncommitted work; an mtime scan confirms
agents stayed inside their fixtures). Adjusted functional accuracy: 100%.

## Efficiency (cumulative session tokens, from session JSONLs)

| Metric | iter5 w2 patch | iter5 w3 (v2) | iter6 (v3+peek) | Δ iter6 vs w3 |
|---|---|---|---|---|
| Input tokens, avg | 1,352,034 | 1,479,054 | 2,001,397 | +35% (outlier) |
| Input tokens, median | — | 1,503,642 | 1,428,824 | **−5%** |
| Output tokens, avg | 13,667 | 30,360 | 26,004 | **−14%** |
| Reasoning tokens, avg | 9,283 | 26,221 | 21,533 | **−18%** |
| Edit payload (chars), avg | 8,685 | 7,886 | 7,796 | −1% |
| Edit invocations, avg | 3.5 | 2.0 | 3.25 | +1.25 |
| `--tool-help` reads | 0/4 | 1/4 | 2/4 | — |
| Rejections | 0/4 | 1/4 | 1/4 | — |
| Accuracy | 60/66 | 60/66 | 60/66 | — |

Per-session iter6 detail:

| Session | in | out | reasoning | apx calls (chars) | peek | rejects |
|---|---|---|---|---|---|---|
| exa-apx | 1,251,620 | 25,753 | 22,346 | 1 (6,035) | 0 | 0 |
| exa-apx2 | 4,222,106 | 43,493 | 35,966 | 10 (11,597) | 1 | 1 |
| exb-apx | 1,606,028 | 21,063 | 17,529 | 1 (6,395) | 0 | 0 |
| exb-apx2 | 925,833 | 13,706 | 10,292 | 1 (7,156) | 0 | 0 |

Token counts include per-turn context re-sends (heavily cached on DeepSeek);
use them as a relative magnitude.

## Findings

1. **Deliberation cost keeps falling.** Output −14% and reasoning −18% vs
   iter5 w3 with identical accuracy and payload. The K3 desc removes prose the
   model re-reads every turn; the compressed fragment rule did not reintroduce
   the iter5-w1 `tsel` misuse (the #1 failure class).
2. **One-shot editing is now the mode:** 3/4 sessions completed the whole
   exercise in a single `apx` script (6,035 / 6,395 / 7,156 chars) with zero
   rejections — including exb-apx, which read `--tool-help` at discovery and
   still wrote the entire Python migration in one call.
3. **The only rejection was a stale line number, not grammar confusion:** after
   editing `sweeper_test.go`, exa-apx2's next `tsel` targeted pre-edit line
   numbers ("found 0 of 1 requested matches") and recovered with one retry.
   iter5 w3's single rejection was a single-quote syntax error — a class desc
   v3 addresses ("single quotes invalid").
4. **`peek` works but adoption is 1/4.** exa-apx2 used it to rehearse selector
   semantics on a scratch file before real edits (peek once + 4 tiny apx
   practice calls, then a clean refactor). The other three sessions kept
   reading via exec `cat`/`nl`; displacing exec reads needs a pointer in the
   `apx` description (no budget left at 788 chars) or a peek-first steering
   line in the session instruction file.
5. **`--tool-help` reads rose 1/4 → 2/4, both at discovery time** (before the
   first edit), and both sessions still delivered one-shot or near-one-shot
   scripts. Signal: the description is apply-sufficient but some agents want
   the full grammar up front; a one-line worked example is the candidate fix.
6. **Input avg +35% is variance, not regression.** The increase is entirely
   exa-apx2's rehearsal turns (per-turn context re-sends, heavily cached).
   Median input is −5% vs w3; per-session input spans 0.93M–4.2M.

## Stop → improve → re-benchmark (this round)

iter5 finding 4 named two candidates: a peek tool and/or sample scripts in the
description. This round implemented both directions partially:

- `peek` MCP tool (read-only, same grammar, fail-closed) — added.
- Description v3 via **detox-context + Kimi K3**: task = current desc + the
  four observed failure classes + hard constraints, output contract ≤800 chars.
  K3 returned 798 chars (normalized for Rust escaping; model-visible 788) after
  two self-trimming passes; its verification note confirms all hard constraints
  present. A sample-script clause was dropped by K3 for budget.

No parser change; grammar still frozen against the Go oracle.

## Reproduce

```bash
cargo build --release -p apx-mcp        # -> ~/.local/bin/apx-mcp (symlinked)
# deepseek profile: ~/.codex/deepseek.config.toml  [mcp_servers.apx]
bash tmp/launch-iter6-w1-apx.sh         # apx arm, desc v3 + peek (Herdr workspace wB)
python3 tmp/grade_iter4.py a tmp/w8-a-apx1 ~/.apx-bench-archive/iter4/bex3-a-apx-seed
python3 tmp/analyze_sessions.py ~/.codex/sessions/2026/08/01/rollout-2026-08-01T00-52-*.jsonl
```

Session manifests and raw JSONLs: `~/.apx-bench-archive/iter6/{w1-sessions,w1-wave}`.

## Iteration history

- iter4: Rust CLI steering — input +284%, output +252%, accuracy 62/68 vs
  63/68. Surface, not engine, was the cost driver.
- iter5: MCP tool registration — input +9.4%, output +122%, accuracy exact
  parity, payload −9.2%.
- iter6 (this doc): desc v3 (−41% chars, K3-tuned) + `peek` tool — output
  −14%, reasoning −18% vs iter5 w3, accuracy exact parity, payload −1%, median
  input −5%; input avg +35% from one rehearsal-heavy session.
