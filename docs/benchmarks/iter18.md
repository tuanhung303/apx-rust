# iter18 — luna MEDIUM: effort is not the gate either (fourth negative)

Measured 2026-08-01, desc = commit `913f732` (BANNED-claim + schema hint),
model `gpt-5.6-luna` at effort **medium** (iter15-17 were effort low).

## Rationale

iter15-17 proved wording is not the gate for luna-low. Before concluding
the portability rule cannot hold for luna, the effort hypothesis had to be
tested: maybe low-effort luna never scans the MCP namespace, while
medium-effort would read the descs.

## Harness / accuracy

Identical zero-instructions harness, blind 2x1, fresh `tmp/w22-{a,b}-apx1`.

| Lane | Grade | Note |
|---|---|---|
| A (Go) | 17/18 | A6 task-bound only → 100% functional |
| B (Py) | 15/15 | clean |

## Tool usage — fourth zero

| Lane | apx | peek | apply_patch | exec | update_plan | write_stdin |
|---|---|---|---|---|---|---|
| A (Go) | 0 | 0 | **1** | 4 | 3 | 1 |
| B (Py) | 0 | 0 | **1** | 5 | 0 | 0 |

## Tokens

| Lane | Input tokens | Output tokens |
|---|---|---|
| A (Go) | 130,452 | 3,444 |
| B (Py) | 115,586 | 4,093 |
| **total** | **246,038** | **7,537** |

## Conclusion — the gate is the model family, not wording or effort

Four blind luna rounds (iter15-17 low ×3 desc styles, iter18 medium) all
yield 0 apx / 0 peek and stock apply_patch, with 100% functional accuracy
every time. Directed probes prove the tool is present, visible
(`ALL_TOOLS.filter(/apx/i)`), quoted correctly, and callable — luna simply
never chooses it on its own. `deepseek-v4-flash low` adopted apx+peek
desc-only in the same harness (iter14). Therefore:

- Desc-only steering: **proven** for deepseek-v4-flash low.
- Desc-only steering: **disproven** for gpt-5.6-luna (low and medium).

The hard portability rule ("no AGENTS.md/system-prompt changes, steering
only in tool descriptions") holds for the deepseek family only. For stock
Codex models that ship with a first-class builtin `apply_patch`, a
desc-level "ban" is not sufficient — the model's prior wins. Options to
pursue next are client-side (Codex plugin/app surface or tool-search
routing), not server-side description wording.
