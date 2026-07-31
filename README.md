# apx-rust

Rust migration of APX's selector-based editing engine.

## Target

APX runs as a native, in-process Codex tool:

```text
model custom tool call
        |
        v
Codex ToolContributor
        |
        v
codex-apx-extension (inside the Codex workspace)
        |
        v
apx-core (from this repository)
        |
        v
Codex filesystem and workspace-edit capabilities
```

The production path has no APX HTTP listener, Responses API proxy, loopback
request, launchd service, systemd service, or provider `base_url` override.

The existing Go repository remains the behavioral oracle until differential
tests, failure atomicity, diagnostics, and Codex end-to-end gates pass.

Until Codex gains freeform extension dispatch (Phase 6), an interim path
registers APX as a standard MCP tool on the stock-Codex surface:

```text
model MCP tool call
        |
        v
Codex MCP client ([mcp_servers.apx], stdio)
        |
        v
apx-mcp (crates/apx-mcp, this repository)
        |
        v
apx-core + apx-local (in-process)
        |
        v
workspace files
```

`apx-mcp` exposes two tools through a registered MCP server: `apx` (apply) with
a diff-only description ("like apply_patch, but takes an APX diff script"), so
the model sees a first-class function schema instead of prose instructions
about a CLI, and `peek` (read-only, selector-scoped region reads through the
same grammar).

## Quick install

Build and register on the stock-Codex surface:

```bash
cargo build --release
ln -sf "$PWD/target/release/apx" ~/.local/bin/apx
ln -sf "$PWD/target/release/apx-mcp" ~/.local/bin/apx-mcp
```

```toml
# ~/.codex/config.toml
[mcp_servers.apx]
command = "/Users/<you>/.local/bin/apx-mcp"
cwd = "/path/to/apx-rust"
startup_timeout_sec = 30.0
```

To steer your agents to `apx` instead of `apply_patch`, paste the ready block
from [docs/codex-agent-install.md](docs/codex-agent-install.md) into
`~/.codex/AGENTS.md`.

## Benchmarks vs apply_patch (current)

Blind harness, deepseek-v4-flash low, 2x2 grid, same fixtures for both tools
(self-run, not third-party; latest run: iter9):

| Metric | apx (current) | apply_patch (control) |
|---|---|---|
| Raw accuracy | 64/66 | 60/66 |
| Adjusted functional accuracy | 100% (A6 task-bound only) | 100% (A6 + measurement artifacts) |
| Session input tokens, avg | 1,410,147 | 1,352,034 |
| Edit invocations, avg | 1.75 | 3.5 |
| Edit payload (chars), avg | 9,105 | 8,685 |
| Output tokens, avg | 24,807 | 13,667 |
| Rejections | 1/4 (root-caused, fixed) | 0/4 |
| Atomicity | reject = zero changes applied | partial-apply risk on failure |


## Latest report

- [iter9 — rich edit report + # comment support (DeepSeek flash low, blind)](docs/benchmarks/iter9.md)

See [MIGRATION_PLAN.md](MIGRATION_PLAN.md) for the staged implementation and
cutover gates.

## Toolchain contract

- Development toolchain: Rust 1.97.1.
- Edition: Rust 2024.
- Initial MSRV: Rust 1.95.0, matching the inspected Codex workspace.
- CI must test both the MSRV and the pinned development toolchain.

No Rust crate is scaffolded until the Phase 0 contract fixtures are generated
from the Go implementation. This prevents a new implementation from silently
becoming its own specification.
