# iter5 benchmark — apx via MCP vs apply_patch (stock Codex)

Measured 2026-07-31/08-01 on macOS. Same harness as iter4 (stock codex CLI
0.146.0 via `cds`, model `deepseek-v4-flash`, reasoning effort `low`, YOLO),
but the apx arm now goes through a **registered MCP tool** instead of CLI
steering: `~/.codex/deepseek.config.toml` has `[mcp_servers.apx]` running
`~/.local/bin/apx-mcp` (this repo, `crates/apx-mcp`). The tool description is
diff-only — "like apply_patch, but takes an APX diff script" — and the model
sees a first-class function schema, not prose instructions about a CLI.

12 sessions = 3 waves × 2 exercises × 2 arms (`swarm-wave.sh --grid 2x2`,
topics `apx-bench-iter5-w1` / `-w2-patch` / `-w3-apx`):

- Exercise A — Go lease-registry refactor, 18 checks.
- Exercise B — Python ledger→package migration, 15 checks.
- w1 apx arm (4): MCP apx, description v1.
- w2 apply_patch control (4): profile `deepseek-patch` — a copy of the deepseek
  profile **without** `[mcp_servers.apx]` and with neutral instructions; the
  only edit path is the built-in `apply_patch` (shell heredoc, exactly the
  iter4 control).
- w3 apx arm (4): MCP apx, description v2 (fragment-vs-`rsel` semantics and
  double-quote rule spelled out; see stop→improve section).

The w1 "patch-labeled" sessions are counted in the apx arm: their task text
demanded `apply_patch`, but the system-level instruction file steered them to
the MCP tool, so all four w1 sessions are genuine apx-MCP data.

## Accuracy

| Round | apx | apply_patch |
|---|---|---|
| w1 A (Go, 18) | 16/18, 16/18 | — |
| w2 A (Go, 18) | — | 16/18, 16/18 |
| w3 A (Go, 18) | 16/18, 16/18 | — |
| w1 B (Python, 15) | 14/15, 14/15 | — |
| w2 B (Python, 15) | — | 14/15, 14/15 |
| w3 B (Python, 15) | 14/15, 14/15 | — |
| **Total** | **120/132** | **60/66** |

Identical scores in every pair. The only real failing check is **A6**
(`internal/leasestore/store.go` must exist): the task never asks for a file
named `store.go` — both tools put the `Expirer` interface in `sweeper.go` and
every test passes (same as iter4). **A18/B15** ("blast radius") fail only
because the repo root is dirty with the uncommitted MCP work itself; an mtime
scan confirms no agent touched anything outside its fixture dir. Adjusted
functional accuracy: **100% for both tools, exact parity**.

## Efficiency (cumulative session tokens, from session JSONLs)

| Metric | apx w1 (desc v1) | apx w3 (desc v2) | apply_patch w2 | Δ w3 vs patch |
|---|---|---|---|---|
| Session input tokens, avg | 1,477,371 | 1,479,054 | 1,352,034 | **+9.4%** |
| Session output tokens, avg | 29,257 | 30,360 | 13,667 | +122% |
| Reasoning tokens, avg | 24,283 | 26,221 | 9,283 | +182% |
| Edit payload (chars), avg | 8,791 | 7,886 | 8,685 | **−9.2%** |
| Edit invocations, avg | 2.0 | 2.0 | 3.5 | −1.5 |
| `--tool-help` reads | 3/4 | 1/4 | 0/4 | — |
| Rejections (tool returned error) | 1/4 sessions | 1/4 sessions | 0/4 | — |
| Accuracy | parity | parity | parity | — |

Token counts include per-turn context re-sends (heavily cached on DeepSeek);
use them as a relative magnitude. Iter4's CLI-steering numbers for the same
exercises were input +284% / output +252% — the MCP surface closes the input
gap (+9.4%) and keeps payload smaller than apply_patch while the accuracy
stays at exact parity.

## Stop → improve → re-benchmark

**Finding (w1):** the model misused `tsel` for whole-line replacements. The
description said "`tsel FROM_LINE \"TEXT\"`, `rsel S:E`, then `type`" but never
said a selector's `type` replaces only the matched fragment. exa-apx's first
script used `tsel` on whole lines with multi-line `type <<PATCH`, garbled the
file, and needed a second 3,285-char script to repair; exb-patch-labeled hit a
`mv`-ordering rejection (`in` on a path that does not exist yet). Session
reasoning carried 16k tokens of "key semantics learned" planning.

**Fix (w1→w3, description v2):** the `apx-mcp` tool description now maps
selectors to what gets replaced — "`tsel`/`bsel` select a FRAGMENT (`type`
replaces only the matched fragment)", "`rsel S:E` selects COMPLETE LINES —
use it to replace whole lines or blocks", "`mv DEST` (select the source first
with `in PATH`)", and "selector text is always double-quoted (escape embedded
quotes as `\"`); single quotes are invalid". No parser change — the grammar
stays frozen against the Go oracle.

**Effect (w3):** repairs shrank from a 3,285-char re-edit to 81–644-char
targeted `rsel` fixes; w3 exa-apx2 completed the whole Go refactor in **one**
6,178-char script (7 file changes, zero follow-ups); `--tool-help` reads
dropped 3/4 → 1/4; the one w3 rejection was single-quote selector text, which
the v2 note addresses. Session-level output/reasoning tokens did **not** move
(30,360 vs 29,257) — the cost is dominated by the model constructing
line-numbered selectors, not by recovering from errors.

## Findings

1. **MCP registration kills the relearn.** First-turn tool discovery, zero
   crate spelunking, `--tool-help` reads 1/4 by w3. The iter4 "4x token" gap
   was instruction-steering overhead, not the tool.
2. **Input cost is at parity** (+9.4%), payload is −9.2% smaller than
   apply_patch with fewer invocations (2.0 vs 3.5).
3. **Accuracy is exact parity** across all 12 sessions; the only failing
   check is the task-bound A6, identical on both tools.
4. **Remaining gap is model deliberation:** output/reasoning ~2× apply_patch.
   A coordinate-based script grammar is not in the model's pretraining, so it
   reasons about line numbers and selectors; apply_patch hunks are native to
   the model. Description tuning made failures cheap but cannot remove the
   construction cost. Candidates for a future round: an MCP `peek` tool
   (selector-scoped reads), or sample scripts in the description.
5. **Variance dominates single arms** (w3 output spans 17k–41k). Headline
   conclusions come from the 12-session pattern, not any one cell.

## Reproduce

```bash
cargo build --release -p apx-mcp        # -> ~/.local/bin/apx-mcp (symlinked)
# deepseek profile: ~/.codex/deepseek.config.toml     [mcp_servers.apx]
# patch control:    ~/.codex/deepseek-patch.config.toml + ~/.local/bin/cds-patch
bash tmp/setup-iter5.sh                 # seeds tmp/w5-*, tmp/w6-*, tmp/w7-*
bash tmp/launch-iter5-w1.sh             # apx arm, desc v1
bash tmp/launch-iter5-w2-patch.sh       # apply_patch control (cds-patch)
bash tmp/launch-iter5-w3-apx.sh         # apx arm, desc v2
python3 tmp/grade_iter4.py a tmp/w5-a-apx ~/.apx-bench-archive/iter4/bex3-a-apx-seed
python3 tmp/analyze_sessions.py ~/.codex/sessions/2026/07/31/rollout-2026-07-31T23-56-53-*.jsonl
```

Session manifests and raw JSONLs: `~/.apx-bench-archive/iter5/{w1,w2,w3}-*`
(sha256 per session in each wave's `meta.json`).

## Iteration history

- iter4: Rust CLI steering — input +284%, output +252%, accuracy 62/68 vs
  63/68. Conclusion: surface, not engine, was the cost driver.
- iter5 (this doc): MCP tool registration — input +9.4%, output +122%,
  accuracy exact parity (120/132 vs 60/66), payload −9.2%.
