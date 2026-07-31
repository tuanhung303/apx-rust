# Steering Codex agents to apx (system-prompt block)

Paste the block below into the **global** agent instructions
(`~/.codex/AGENTS.md`) or a repo `AGENTS.md`. It makes every Codex agent
prefer the `apx` edit tool over `apply_patch`, and tells agents how to
install it when it is missing — so a fresh agent on a new machine can
self-provision it.

## Block to paste

```markdown
## File edits: prefer apx over apply_patch
- Use the `apx` MCP tool (or `apx` CLI) for file edits: ONE atomic script with
  `in`/`new`/`tsel`/`bsel`/`rsel`/`type`/`rm`/`mv`/`commit`. A rejection
  changes NOTHING — fix per diagnostic and retry. Prefer apx for multi-file
  or multi-change edits; `apply_patch` only for trivial one-hunk fixes.
- If `apx`/`apx-mcp` is missing, install it:
  ```bash
  git clone https://github.com/tuanhung303/apx-rust ~/src/apx-rust
  cd ~/src/apx-rust && cargo build --release
  ln -sf "$PWD/target/release/apx" ~/.local/bin/apx
  ln -sf "$PWD/target/release/apx-mcp" ~/.local/bin/apx-mcp
  ```
- Register the MCP server on the stock-Codex surface:
  ```toml
  # ~/.codex/config.toml
  [mcp_servers.apx]
  command = "<abs path to ~/.local/bin/apx-mcp>"
  cwd = "<abs path to apx-rust checkout>"
  startup_timeout_sec = 30.0
  ```
- Grammar cheat sheet: `in PATH` selects an existing file, `new PATH` creates;
  `tsel FROM_LINE "TEXT" [N]` = first N exact 1-line matches; `bsel "START"
  "END"` = one fragment span (replaces the literal, never whole lines);
  `rsel S:E` = complete lines; `type "TEXT"` or `type <<PATCH` heredoc (PATCH
  after last content line); `rm`/`del`/`mv DEST`/`commit`; selectors are
  double-quoted (`\"` escaped), single quotes invalid; line numbers are the
  frozen baseline — never renumber for earlier edits; inserted text is
  unselectable until `commit`.
- Failure diagnostics are self-describing: `command N, source line N,
  operation, category: message; in PATH` + `no changes applied (atomic)`.
- Evidence: https://github.com/tuanhung303/apx-rust — docs/benchmarks/iter5.md
  (head-to-head vs apply_patch), docs/benchmarks/iter9.md (latest).
```

## Applying it

1. `codex` → `~/.codex/AGENTS.md` (all sessions, every repo).
2. Repo-local → `<repo>/AGENTS.md` (only that repo's agents).

To have another agent do it for you, tell it:

> Update `~/.codex/AGENTS.md`: append the "File edits: prefer apx over
> apply_patch" section from docs/codex-agent-install.md in apx-rust, verbatim.
> Keep everything already in the file.
