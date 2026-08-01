# iter15 — luna low vs deepseek low token cost; desc-only steering did NOT convert luna

Measured 2026-08-01 on macOS, right after commit `1bc6e64` (desc bait:
HIGH-EFFICIENCY / minimal-params / batching-first framing).

Two questions:
1. Which model family is cheaper on the same harness — `gpt-5.6-luna low` or
   `deepseek-v4-flash low`? (user: "luna hay deepseek ai ton token hon")
2. Does the hard portability rule hold — steering ONLY in tool descriptions,
   with NO AGENTS.md / `model_instructions_file` — for a second model family?

## Harness

Same zero-instructions setup as iter13-14: isolated `CODEX_HOME`
(`~/.apx-bench-archive/bench-home-nosteer`), only `[mcp_servers.apx]`
registered, no AGENTS.md, no instructions file. Blind 2x1 grid, fresh
fixtures `tmp/w19-{a,b}-apx1`; task text is byte-identical to iter14's
`tmp/w18-*` tasks except the fixture path. Model: `gpt-5.6-luna`, effort low,
provider openai (stock `codex-nosteer`, no `-p deepseek`).

## Accuracy (same grader as iter4)

| Lane | Grade | Note |
|---|---|---|
| A (Go) | 17/18 | A6 is the known task-bound fail (exercise never requests `store.go` equivalent) → 100% functional |
| B (Py) | 15/15 | clean |

## Tool usage — the finding that matters

| Lane | apx | peek | apply_patch | exec | update_plan | rejections |
|---|---|---|---|---|---|---|
| A (Go) | 0 | 0 | **2** | 3 | 2 | 0 |
| B (Py) | 0 | 0 | **2** | 5 | 0 | 0 |

Luna-low completed both exercises **entirely with the stock `apply_patch`
tool** — zero apx, zero peek. (Session items are JS batches in the new SDK;
counts above come from `tools.<name>(` invocations inside each
`custom_tool_call` item, not the item label.)

Root cause (verified by probe): the MCP tool registers as
**`mcp__apx__apx`**, not bare `apx`. Asked to "list the file-editing tools
available to you", luna-low answered only `codex`, `apply_patch` — it never
enumerated the MCP namespace. Asked to *call* apx, it found
`mcp__apx__apx` via `ALL_TOOLS.filter(/apx/i)` and invoked it successfully.
So the tool is present and callable; low-effort luna simply defaults to the
familiar builtin and never looks past it.

Conclusion: the hard-rule "desc-only steering is fully portable" is
**proven for `deepseek-v4-flash low` (iter14) but NOT for `gpt-5.6-luna
low`**. Accuracy is unaffected because apply_patch is the control tool —
iter15 is effectively a luna control round, which is what makes the token
comparison below valid as a model comparison (not a tool comparison).

## Tokens — luna vs deepseek (per-session sums, same harness/tasks/effort)

| Round (model) | Lane | Input tokens | Output tokens | Editing tool |
|---|---|---|---|---|
| iter14 deepseek-v4-flash low | A (Go) | 1,286,422 | 50,173 | apx 7 + peek 11 |
| iter14 deepseek-v4-flash low | B (Py) | 144,528 | 11,074 | apx 1 (one-shot) |
| **iter14 total** | | **1,430,950** | **61,247** | |
| iter15 gpt-5.6-luna low | A (Go) | 107,866 | 3,208 | apply_patch 2 |
| iter15 gpt-5.6-luna low | B (Py) | 125,262 | 3,054 | apply_patch 2 |
| **iter15 total** | | **233,128** | **6,262** | |

Luna-low: **≈6.1x fewer input tokens, ≈9.8x fewer output tokens** on the
same blind tasks.

Caveats (do not over-read):
- This is a *model* comparison, not an apx-vs-apply_patch comparison —
  luna used apply_patch, deepseek used apx MCP.
- Confounds: apply_patch args are tiny and native (2 calls), apx MCP args
  are more verbose (7 scripts + 11 peeks on lane A); deepseek emits far
  more reasoning/output tokens; n=1 per lane.
- Token counts are not cost.

## Next levers (to convert low-effort models without AGENTS.md steering)

1. Desc front-loading: first 5 words "REPLACES apply_patch — use me for
   every edit", keep the grammar bullets after (test on luna low).
2. Model-side: probe luna *medium* — the failure is discovery, not wording.
3. Accept a two-tier reality: desc-only covers deepseek-family; keep
   AGENTS.md steering for stock models until (1) or (2) proves otherwise.
