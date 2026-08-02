# iter20 — Hermes steering: memory-block converts Luna medium, zero misses on the full hard set

Measured 2026-08-02 on macOS. First benchmark of apx on the **Hermes**
surface (previous iterations were Codex surfaces). Question: can a stock
OpenAI model (gpt-5.6-luna, medium reasoning) be steered to apx inside
Hermes, and can it complete the full hard exercise set with zero misses?

## Setup

- Harness: `hermes chat --yolo -m gpt-5.6-luna`, `/reasoning medium`,
  spawned as two swarm panes (2x1 grid, Herdr), one per exercise.
- MCP server registered on the Hermes surface (`mcp_servers.a` →
  tools `mcp_a_px` / `mcp_a_peek`; at launch time the names were still
  `mcp__apx__apx` / `mcp__apx__peek`, which is what the sessions used).
- **Steering: one memory block only** (Hermes memory, injected into every
  session's system prompt): "use mcp_a_px for EVERY file edit — ONE atomic
  script … rejection changes NOTHING (fix per diagnostic, retry); never
  functions.patch/apply_patch for multi-file or multi-hunk edits;
  mcp_a_peek for numbered region reads". No AGENTS.md, no prompt surgery.
- Fixtures: fresh `tmp/w23-{a,b}-apx1` from the authoritative iter4 seeds
  (`~/.apx-bench-archive/iter4/bex3-{a,b}-apx-seed`), task text byte-identical
  to iter10-19 modulo fixture paths.
- Grading: `grade_iter4.py` (A: 18 checks, B: 15 checks).

## Accuracy

| Exercise | Grade | Note |
|---|---|---|
| A (Go) | **17/18** | A6 task-bound only — the exercise never requests `internal/leasestore/store.go`; 100% functional (same as iter10-19) |
| B (Python) | **15/15** | clean |
| **Total** | **32/33** | **zero real misses** |

All functional checks pass; the single A6 miss is the known task-bound check
no exercise in the series has ever requested.

## Tool adoption — the headline

Both panes **discovered and used apx without any prompt instruction
mentioning it** (the memory block was the only carrier):

```
Pane A (Go):  Tool Search("apx atomic file edit tool") → Tool Describe("mcp__apx__apx") → Mcp Apx Apx × 2
Pane B (Py):  Tool Search("apx atomic file edit tool") → Tool Describe("mcp__apx__apx") → Mcp Apx Apx × 2
```

- **Zero `functions.patch` calls** in either session.
- **Zero apx rejections** — every script applied on the first or second call.
- Sessions ended at 37.8k / 63.7k context usage (272k budget) — bounded,
  no re-read loops after edits.

This is the first positive portability result for a stock OpenAI model on
these exercises (iter15-19: luna low/medium on Codex always used builtin
apply_patch regardless of tool name/position/description).

## Why it worked here and not on Codex

iter19 root-caused the Codex failure to the model's base instructions
("Always use apply_patch …") delivered in the provider catalog. A tool
description is advisory next to that. Two levers differ on Hermes:

1. **Memory is a direct system-prompt instruction.** The memory block ranks
   as a user-level directive, not an advisory tool schema — it wins over the
   base-instruction habit exactly like the proven AGENTS.md block (iter13).
2. **Tool discovery is model-driven.** Hermes exposes `tool_search` /
   `tool_describe`, so a model that *wants* to comply can find and load the
   unfamiliar MCP tool instead of pattern-matching a familiar builtin.

Conclusion: the gate is instruction placement, not model family. Desc-only
steering converts DeepSeek (iter14); instruction-level steering (memory /
AGENTS.md) converts OpenAI models too.

## Status

Steering block is live in Hermes memory (all sessions). Tool names now
`mcp_a_px` / `mcp_a_peek` (server `a`). Follow-ups: rerun with the renamed
tools to confirm the steering text matches the new names verbatim; benchmark
Luna high/xhigh for an effort curve; benchmark on Hermes + Codex side by side.
