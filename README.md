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

## Benchmarks

- [iter4 — Rust CLI vs apply_patch (DeepSeek flash low, blind)](docs/benchmarks/iter4.md)

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
