# iter17 — BANNED-claim desc + inputSchema hint: third luna-low negative

Measured 2026-08-01, commit `913f732`.

## Change under test

After iter15 (bait bullets) and iter16 (front-loaded REPLACES + exact
`mcp__apx__apx` name hint) both failed to convert `gpt-5.6-luna low`, the
strongest remaining tool-surface lever was applied:

- Desc first line: `⚠️ apply_patch is BANNED in this session — your only
  file-edit tool is THIS one (tool name in your list: mcp__apx__apx;
  mcp__apx__peek is my read-only twin for line numbers…)`.
- Second surface: `inputSchema.properties.script.description` now carries
  the same directive (`Use THIS tool for every file edit — apply_patch is
  banned; one atomic script for the whole task.`), so the steering appears
  in the schema rendering of the tools list as well.

## Harness

Identical: `bench-home-nosteer` zero-instructions CODEX_HOME, blind
`gpt-5.6-luna low`, 2x1, fresh `tmp/w21-{a,b}-apx1` fixtures, task text
byte-identical to iter14-16 except fixture path.

## Accuracy (re-graded after commit — the A18/B15 blast-radius checks are a
grader artifact when the desc edit is uncommitted)

| Lane | Grade | Note |
|---|---|---|
| A (Go) | 17/18 | A6 task-bound only → 100% functional |
| B (Py) | 15/15 | clean |

## Tool usage — third zero

| Lane | apx | peek | apply_patch | exec | write_stdin | rejections |
|---|---|---|---|---|---|---|
| A (Go) | 0 | 0 | **2** | 3 | 1 | 0 |
| B (Py) | 0 | 0 | **1** | 8 | 0 | 0 |

## Tokens

| Lane | Input tokens | Output tokens | Editing tool |
|---|---|---|---|
| A (Go) | 109,531 | 2,714 | apply_patch 2 |
| B (Py) | 175,096 | 3,639 | apply_patch 1 |
| **total** | **284,627** | **6,353** | |

## Conclusion — structural, not lexical

Three desc styles (bait bullets, front-loaded REPLACES + name hint,
BANNED-claim + schema hint) produce identical luna-low behavior: 0 apx,
0 peek, stock apply_patch. Combined with the directed-probe evidence that
luna-low *can* find and call `mcp__apx__apx` when asked, the gate is
model-effort discovery: **luna-low never browses the MCP tool namespace
when deciding how to edit; it reflexively picks the builtin apply_patch.**
Desc-only steering is proven for `deepseek-v4-flash low` (iter14) and
disproven for `gpt-5.6-luna low` at effort `low` (iter15-17).

Next discriminating test: same desc at luna **medium** (effort gate
hypothesis) — see iter18.
