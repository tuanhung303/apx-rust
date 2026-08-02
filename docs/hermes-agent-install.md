# Steering Hermes agents to apx (memory block)

Short version of the integration for Hermes. For Codex see
[docs/codex-agent-install.md](codex-agent-install.md).

## 1. Install (one line)

```bash
curl -fsSL https://raw.githubusercontent.com/tuanhung303/apx-rust/main/scripts/install.sh | bash
```

Requires a Rust toolchain (MSRV 1.95+). Clones to `~/src/apx-rust`, builds
`apx` + `apx-mcp`, symlinks them into `~/.local/bin`, and registers the MCP
server (`a`) with Hermes.

Verify:

```bash
hermes mcp list          # server `a` enabled, 2 tools
hermes mcp test a        # connects; lists px + peek
```

Restart Hermes so the tools load as `mcp_a_px` / `mcp_a_peek`.

## 2. Steering (the part that actually converts models)

Tool descriptions alone convert DeepSeek but NOT stock OpenAI models
(iter15-19 on Codex; the model's base instructions mandate apply_patch). The
proven lever is a **direct instruction** — on Hermes, one memory block.
Ask the agent to add this verbatim to its persistent memory (or a skill /
AGENTS.md):

```markdown
APX steering (all Hermes sessions): use mcp_a_px for EVERY file edit — ONE
atomic script (in/new/tsel/bsel/rsel/type/rm/mv/commit); rejection changes
NOTHING (fix per diagnostic, retry); never functions.patch/apply_patch for
multi-file or multi-hunk edits; mcp_a_peek for numbered region reads;
selectors double-quoted; line numbers frozen.
```

Evidence it works: iter20 — Luna medium completed the full hard exercise set
(Go + Python) with `mcp_a_px` only, zero `functions.patch`, zero rejections,
zero misses (docs/benchmarks/iter20.md).

## 3. Grammar cheat sheet (what the model needs to know)

- `in PATH` selects an existing file; `new PATH` creates it.
- `tsel FROM_LINE "TEXT" [N]` — first N exact 1-line matches.
- `bsel "START" "END"` — one fragment span (replaces the literal, never
  whole lines).
- `rsel S:E` — complete lines; `sel LINE S:E` — single line with range.
- `type "TEXT"` or `type <<PATCH` heredoc (PATCH after last content line).
- `rm`/`del` deletes; `mv DEST` moves after `in`; `commit` finalizes.
- Selectors are double-quoted (`\"` escaped); single quotes invalid.
- Line numbers are the frozen baseline — never renumber for earlier edits;
  inserted text is unselectable until `commit`.
- Rejections are atomic: `no changes applied (atomic)` — fix per the
  diagnostic and retry; nothing is half-applied.
