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

`apx-mcp` exposes two tools through a registered MCP server: `px` (apply) with
a diff-only description ("like apply_patch, but takes an APX diff script"), so
the model sees a first-class function schema instead of prose instructions
about a CLI, and `peek` (read-only, selector-scoped region reads through the
same grammar). On Hermes the server registers as `a`, so the tools appear as
`mcp_a_px` / `mcp_a_peek`; on Codex the server is `apx`, so they appear as
`apx__apx` / `apx__peek`.

## Quick install

### One-liner (from GitHub)

Requires a Rust toolchain (MSRV 1.95+). Clones into `~/src/apx-rust`, builds
release binaries, symlinks `apx` + `apx-mcp` into `~/.local/bin`, and
registers the MCP server with Hermes if `hermes` is on PATH:

```bash
curl -fsSL https://raw.githubusercontent.com/tuanhung303/apx-rust/main/scripts/install.sh | bash
```

Overrides: `APX_SRC_DIR`, `APX_BIN_DIR`, `APX_SKIP_MCP=1`. To verify:

```bash
hermes mcp list && hermes mcp test apx
```

Restart Hermes so the `mcp_a_px` / `mcp_a_peek` tools load.

### Manual build

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

To steer your agents to `apx` instead of `apply_patch`:
- **Hermes**: paste the block from [docs/hermes-agent-install.md](docs/hermes-agent-install.md) into memory — proven with Luna medium (iter20).
- **Codex**: paste the ready block from [docs/codex-agent-install.md](docs/codex-agent-install.md) into `~/.codex/AGENTS.md`.

Portability verdict (2026-08-02, iter20): tool-description-only steering
converts DeepSeek agents but **not** stock OpenAI models — their base
instructions mandate `apply_patch`, which no description can reliably
override. Instruction-level steering (AGENTS.md block for Codex, memory block
for Hermes) converts both: iter13 proved it for Codex, iter20 proved it for
Hermes (Luna medium, full hard set, zero misses).

## Benchmarks vs apply_patch (current)

Blind harness, deepseek-v4-flash low, 2x2 grid, same exercises for both tools
(self-run, not third-party; latest run: iter10). A = Go lease registry, B = Python
ledger. All token/payload rows are per-session sums, not averages.

| Session | Tool | Input tokens | Output tokens | Edit calls | Edit payload (chars) |
|---|---|---|---|---|---|
| A1 | apx (iter10) | 872,030 | 20,180 | 1 | 7,186 |
| A1 | apply_patch | 1,264,009 | 12,243 | 3 | 8,423 |
| A2 | apx (iter10) | 1,720,897 | 28,055 | 2 | 7,532 |
| A2 | apply_patch | 1,995,796 | 23,213 | 7 | 7,450 |
| B1 | apx (iter10) | 1,622,459 | 19,728 | 2 | 7,476 |
| B1 | apply_patch | 1,026,113 | 11,270 | 2 | 11,637 |
| B2 | apx (iter10) | 982,151 | 11,100 | 1 | 7,457 |
| B2 | apply_patch | 1,122,218 | 7,940 | 2 | 7,230 |
| **Total** | **apx (iter10)** | **5,197,537** | 79,063 | **6** | 29,651 |
| **Total** | **apply_patch** | **5,408,136** | 54,666 | **14** | 34,740 |

Raw accuracy: apx 64/66, apply_patch 60/66 (both 100% functional — the only miss is
the task-bound A6 check neither exercise requests). Atomicity: apx reject = zero
changes applied; apply_patch has partial-apply risk on failure.


## Latest report

- [iter20 — Hermes memory steering converts Luna medium: full hard set, zero misses (swarm, blind, 2x1)](docs/benchmarks/iter20.md)
- [iter19 — unprefixed MCP names (apx__apx at position 2): still apply_patch (blind, luna low, 2x1)](docs/benchmarks/iter19.md)
- [iter18 — luna MEDIUM: effort is not the gate either (blind, 2x1)](docs/benchmarks/iter18.md)
- [iter17 — BANNED-claim desc + inputSchema hint: third luna-low negative (blind, 2x1)](docs/benchmarks/iter17.md)
- [iter16 — desc front-loading + exact tool-name hint: still no luna-low adoption (blind, 2x1)](docs/benchmarks/iter16.md)
- [iter15 — luna low vs deepseek low token cost; desc-only steering did NOT convert luna (blind, 2x1)](docs/benchmarks/iter15.md)
- [iter13-14 — steering in tool descriptions, zero-instructions harness (DeepSeek flash low, blind, 2x1)](docs/benchmarks/iter13.md)
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
