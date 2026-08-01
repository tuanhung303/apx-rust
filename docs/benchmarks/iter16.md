# iter16 — desc front-loading + exact tool-name hint: still no luna-low adoption

Measured 2026-08-01 on macOS, right after commit `0925f32`.

## Change under test

Previous desc (iter15, bait style: "HIGH-EFFICIENCY edit tool — fewest calls,
minimal params…") did not convert `gpt-5.6-luna low`. iter16 changed the
tool surface to test whether *wording* was the gate:

- First words of the apx desc: `REPLACES apply_patch — use me for EVERY file
  edit (apply_patch not allowed).`
- Explicit discovery hint: `Find me in your tool list: server \`apx\`, tool
  \`apx\`, usually exposed as \`mcp__apx__apx\`; \`mcp__apx__peek\` is my
  read-only twin`.
- peek desc mirrors the twin naming.

## Harness

Identical to iter13-15: isolated `CODEX_HOME`
(`~/.apx-bench-archive/bench-home-nosteer`), only `[mcp_servers.apx]`
registered, NO AGENTS.md, NO `model_instructions_file`. Blind
`gpt-5.6-luna low`, 2x1 grid, fresh fixtures `tmp/w20-{a,b}-apx1` (task text
byte-identical to iter14/15 except fixture path).

## Accuracy (grader: tmp/grade_iter4.py)

| Lane | Grade | Note |
|---|---|---|
| A (Go) | 17/18 | A6 is the known task-bound fail → 100% functional |
| B (Py) | 15/15 | clean |

(First pass showed A18/B15 "blast radius" failing — traced to the grader's
`git status` check seeing this session's own uncommitted `main.rs` desc edit;
re-graded after commit `0925f32` → both pass. Measurement artifact, not an
agent violation.)

## Tool usage — zero adoption again

| Lane | apx | peek | apply_patch | exec | write_stdin | rejections |
|---|---|---|---|---|---|---|
| A (Go) | 0 | 0 | **2** | 4 | 1 | 0 |
| B (Py) | 0 | 0 | **1** | 5 | 0 | 0 |

Same as iter15: luna-low completes both exercises with the stock
`apply_patch` tool. Counts are `tools.<name>(` invocations inside each
`custom_tool_call` item (the new SDK batches JS per item).

## Surface validation (the tool WAS there and usable)

- `apx-mcp` processes running for both iter16 sessions (spawned 12:18-12:19).
- Directed probe with the same harness/model: `ALL_TOOLS.filter(/apx/i)`
  returns `mcp__apx__apx` + `mcp__apx__peek`; luna quotes the new desc
  (first 120 chars = the REPLACES line); calling the tool returns proper
  atomic diagnostics (`…does not exist; no changes applied (atomic)`).
- Raw JSON-RPC handshake (`initialize` → `notifications/initialized` →
  `tools/list`) responds correctly.
- No `mcp: apx/apx (failed)` lifecycle noise inside the iter16 sessions.

## Tokens (per-session sums)

| Lane | Input tokens | Output tokens | Editing tool |
|---|---|---|---|
| A (Go) | 107,881 | 2,639 | apply_patch 2 |
| B (Py) | 107,991 | 3,293 | apply_patch 1 |
| **total** | **215,872** | **5,932** | |

## Finding

Wording is NOT the gate for luna-low. Two consecutive desc styles (bait
bullets iter15, front-loaded REPLACES + exact name hint iter16) produce
identical behavior: 0 apx, 0 peek, stock apply_patch. The tool is present,
discoverable when directed, and callable — luna-low simply never browses the
MCP namespace when deciding how to edit; it reflexively picks the builtin.
Desc-only steering therefore remains proven for `deepseek-v4-flash low`
(iter14: apx 7+peek 11 / apx 1 one-shot, 0 apply_patch) but NOT for
`gpt-5.6-luna low`.

## Next lever (iter17)

Strongest remaining wording: replace polite framing with a hard claim —
`apply_patch is BANNED/DISABLED in this session; your only file-edit tool is
THIS one (mcp__apx__apx)` — plus an inputSchema `script` property
description carrying the same directive (a second surface the model sees in
the tools list). If that also fails, the conclusion is structural
(model-effort discovery), not lexical.
