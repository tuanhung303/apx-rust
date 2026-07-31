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

`apx-mcp` exposes one tool `apx` with a diff-only description ("like
apply_patch, but takes an APX diff script"), so the model sees a first-class
function schema instead of prose instructions about a CLI.

## Benchmarks


- [iter4 — Rust CLI vs apply_patch (DeepSeek flash low, blind)](docs/benchmarks/iter4.md)
- [iter5 — apx as a registered MCP tool vs apply_patch (DeepSeek flash low, blind)](docs/benchmarks/iter5.md)

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
