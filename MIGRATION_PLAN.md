# APX Rust Migration Plan

## 1. Verified end state

APX is a native custom tool compiled into Codex. A model emits the existing
freeform APX script, Codex dispatches it to an in-process Rust executor, and the
executor evaluates and commits the resulting change set through host-owned
filesystem capabilities.

The completed path has:

- no local HTTP server;
- no Responses API proxy;
- no `127.0.0.1` or Unix-socket hop;
- no launchd or systemd unit;
- no APX provider `base_url`;
- no APX process to start, discover, or health-check;
- no production translation through a textual patch carrier.

The standalone `apx` CLI remains available for local use and parity testing.

## 2. Repository decision

Use a separate `apx-rust` repository and keep the Go `apx` repository frozen as
the differential oracle during migration.

Only host-neutral crates belong in `apx-rust`. The Codex adapter belongs inside
the Codex workspace:

```text
apx-rust/
├── crates/
│   ├── apx-core/       parser, corrections, evaluator, diagnostics, change set
│   ├── apx-local/      capability-safe local filesystem and transactions
│   └── apx-cli/        compatibility CLI, local hooks, and gain
└── fixtures/
    ├── corpus/
    └── golden/

openai/codex/
└── codex-rs/ext/apx/   ToolContributor and Codex capability adapters
```

Do not put `apx-codex` in the external repository. Codex's extension, tool, and
filesystem crates are internal workspace packages. A reverse dependency from
`apx-rust` to Codex would create brittle source-identity or repository cycles.
The dependency direction must remain:

```text
Codex -> codex-apx-extension -> apx-core
```

## 3. Toolchain policy

Pin repository development to Rust 1.97.1 because it contains the LLVM
miscompilation fix released after Rust 1.97.0.

Set crate `rust-version` to `1.95` initially because the inspected Codex
workspace currently pins Rust 1.95.0. Use Rust 2024 edition, but do not use
1.96+ library or language features until Codex raises its toolchain.

CI matrix:

1. Rust 1.95.0: `cargo test --workspace`.
2. Rust 1.97.1: format, Clippy, tests, docs, and benchmarks.
3. macOS, Linux, and Windows for filesystem semantics.

A Codex toolchain bump is a separate change and is not a prerequisite for the
first integration.

## 4. Host integration gaps

### 4.1 Freeform extension dispatch

Codex can describe a custom/freeform tool with `ToolSpec::Freeform`, and tool
calls can carry `ToolPayload::Custom`. The current extension adapter still
matches only `ToolPayload::Function`.

Required Codex change:

1. Derive `ExtensionToolAdapter::matches_kind` from `executor.spec()`.
2. Match function and namespace specs to function payloads.
3. Match freeform specs to custom payloads.
4. Add adapter and router tests proving a contributed freeform tool receives
   the exact script bytes.

Do not permanently wrap APX scripts in `{"script":"..."}`. A function-tool
fallback is acceptable only for a short compile/E2E spike.

### 4.2 Transactional filesystem capability

`ToolEnvironment` exposes `ExecutorFileSystem` plus sandbox context, but the
inspected filesystem trait has reads, writes, directory creation, removal, and
copy only. It has no rename/replace transaction primitive, file mode, or fsync
contract. Directly sequencing `write_file` and `remove` would weaken APX's
current rollback and mode-preservation guarantees.

Required Codex change:

1. Add a host-owned workspace edit capability to `ToolEnvironment`.
2. Accept a typed, fully preflighted change set with add/update/delete/move
   operations and expected baseline identities.
3. Validate sandbox permissions and final-component symlinks before mutation.
4. Stage outputs, preserve relevant file mode, commit by replace/rename, and
   roll back partial installs.
5. Return an applied delta for Codex diff UI and audit events.

The capability may initially be backed by the local executor only, but its trait
must also permit remote executor implementations.

Serverless bridge allowed during integration:

- APX evaluates to a typed change set.
- The Codex adapter converts that set to an in-memory apply-patch action and
  calls the Codex apply-patch library directly.
- There is still no HTTP process or provider proxy.

This bridge is not the final commit engine because the inspected apply-patch
writer applies hunks sequentially and cannot prove APX-equivalent rollback.

### 4.3 Registry wiring

Add `codex-rs/ext/apx` to the Codex workspace and install it in every extension
registry used by interactive Codex, `codex exec`, app-server clients, and prompt
inspection. Gate exposure with one Codex feature/config flag, defaulting off
during shadow testing.

The tool must use the selected `ToolEnvironment.cwd`, filesystem capability, and
sandbox context. It must not reconstruct workspace authority from model text or
turn metadata.

Relative paths resolve against the selected host environment's workspace base.
Absolute paths may be accepted when that same host capability and sandbox
authorize them, including paths outside the Git worktree. Parent directories are
created by the transaction backend only after full preflight succeeds.

For the first release, reject a tool call unless all referenced paths resolve to
one unambiguous environment. Do not guess between multiple local or remote
environments, and do not allow one APX transaction to span environment
boundaries.

### 4.4 Correction state

The router currently owns bounded rejected-call history and rebuilds correction
scripts. Move correction parsing and rebuilding into `apx-core`.

The Codex adapter should keep only bounded thread-scoped correction state and use
conversation history as a resume fallback. Corrected calls must re-run full
parsing, baseline validation, and transaction preflight; prior rejection state
never grants filesystem authority.

### 4.5 Hooks

Standalone APX may retain explicitly configured local outcome hooks. The Codex
extension must not spawn arbitrary shell hook commands inside the host process.
Map useful events to Codex-native lifecycle/telemetry hooks or disable them with
a clear compatibility note.

## 5. Rust API shape

The core API should be host-neutral and change-set-first:

```rust
pub struct Engine<F> {
    file_system: F,
}

impl<F: ApxFileSystem> Engine<F> {
    pub async fn evaluate(
        &self,
        workspace: &Workspace,
        script: &str,
    ) -> Result<Evaluation, Diagnostic>;
}

pub struct Evaluation {
    pub changes: ChangeSet,
    pub report: String,
    pub invocation: InvocationMetrics,
}

#[async_trait]
pub trait ChangeCommitter {
    async fn commit(&self, changes: &ChangeSet) -> Result<AppliedChangeSet, CommitError>;
}
```

Rules:

- parsing and editing never call `std::fs`;
- evaluation is read-only and deterministic against one immutable baseline;
- commit is a separate explicit step;
- paths are typed and normalized once;
- diagnostics carry stable reason codes plus human repair text;
- correction rebuilding is pure core logic; bounded session storage is a host
  concern;
- standalone and Codex adapters use the same evaluator;
- translation to apply-patch exists only as a compatibility/differential
  adapter, not as the core representation.

## 6. Migration phases

### Phase 0: Freeze the Go contract

Deliverables:

- Record the Go oracle revision.
- Export all existing parser, selector, path, transaction, diagnostic, and
  report cases into portable fixtures.
- Add fixture generation for successful outputs and structured failures.
- Capture APX Exercise A/B benchmark inputs and raw results.
- Document platform-dependent cases instead of normalizing them away.

Gate:

- The Go test suite passes.
- Fixture generation is deterministic on two consecutive runs.
- Every current command and failure reason appears in the fixture inventory.

### Phase 1: Create the Rust workspace

Deliverables:

- Create `apx-core`, `apx-local`, and `apx-cli`.
- Add Rust 2024 workspace settings with `rust-version = "1.95"`.
- Add formatting, Clippy, test, docs, and dependency-audit CI.
- Ban unsafe code in host-neutral crates unless a reviewed filesystem primitive
  requires a narrowly scoped exception.

Gate:

- Empty infrastructure builds on Rust 1.95.0 and 1.97.1.
- No Codex crate dependency exists in `apx-rust`.

### Phase 2: Port syntax and diagnostics

Deliverables:

- Port command parsing, paths, quoted strings, heredocs, and source locations.
- Preserve literal newline, UTF-8 boundary, and active-file diagnostics.
- Emit a typed AST and stable diagnostic reason codes.
- Differential-run every syntax fixture against Go.

Status (2026-07-31): COMPLETE — exact parity against frozen oracle bcd85fc2f817e7c405f8b92953cd3ad4db165759.

Gate:
- Exact parity for acceptance/rejection, source span, reason code, and rendered
  diagnostic for the frozen corpus.
- Fuzzing never panics and all accepted scripts round-trip through the AST.

Evidence:

- `crates/apx-core/tests/corpus_parity.rs`: 112/112 corpus cases exact
  (accept/reject, source span, reason code, rendered diagnostic).
- `crates/apx-core/tests/roundtrip.rs`: every accepted corpus script
  parse -> serialize -> parse is AST-identical.
- `crates/apx-core/tests/fuzz_safety.rs`: 20k random + 7k corpus-derived
  mutations never panic; diagnostics never contain raw control characters.
- Blind differential vs the frozen oracle: 656,030 cases, 0 mismatches
  (300k + 300k fuzz, 6,030 targeted, 20k tsel cross-product, 30k path fuzz).
- `go_is_print` Unicode table byte-verified against Go `unicode.IsPrint`
  (711 ranges, 63,558 code points, 0 divergence); `go_is_control` matches
  Go `unicode.IsControl` (C0 + C1).
- Corpus regeneration is pinned: `scripts/parity.py verify` regenerates from
  the frozen oracle and byte-compares the committed corpus (CI does the same).
- Known follow-up for Phase 3: Go evaluation errors render a path `%q` context
  segment; parse-time errors never set it, so this gate is unaffected.

### Phase 3: Port the evaluator

Deliverables:

- Port immutable-baseline file loading and active-file state.
- Port `sel`, `tsel`, `bsel`, `rsel`, clipboard operations, `commit`, `new`,
  `mv`, and `rm`.
- Port whitespace recovery only where the Go oracle permits it.
- Produce typed add/update/delete/move changes.

Gate:

- Exact final content, change kind, report, and diagnostic parity.
- Ambiguous anchors, overlapping selections, and stale coordinates fail closed.
- Unicode and CRLF fixtures match the Go result byte-for-byte.

### Phase 4: Port standalone filesystem commits

Deliverables:

- Implement `apx-local` with capability-safe root confinement and validated
  absolute aliases. Prefer capability APIs over canonicalize-plus-prefix checks
  that introduce symlink TOCTOU windows.
- Port auto-parent creation, mode preservation, staging, backup, replace, and
  rollback behavior.
- Reject final-component symlinks and unsupported file types.
- Add injected-failure tests at every staging and commit step.

Gate:

- Any preflight failure leaves the workspace unchanged.
- Any injected commit failure either restores the exact baseline or returns the
  existing explicit rollback-failed class.
- No temporary or backup file survives successful or rolled-back execution.

### Phase 5: Port CLI and differential runner

Deliverables:

- Match `apx`, `apx translate`, and help behavior.
- Keep Go and Rust runners available to one differential harness.
- Add corpus replay and randomized state-machine tests.
- Preserve the apply-patch translator only for parity and bridge use.

Gate:

- All 343 inspected Go tests are represented by Rust unit, integration, or
  golden tests.
- Go and Rust agree on final bytes, reports, diagnostics, and translated patch
  for the frozen and randomized corpus.

### Phase 5.5: Register APX as an MCP tool (interim)

Deliverables:

- Add `crates/apx-mcp`: a minimal stdio MCP server exposing one `apx` tool whose
  `inputSchema` carries `script` (required) plus optional `root`/`cwd`. The
  server calls `parse`/`evaluate`/`apply` from `apx-core` + `apx-local`
  in-process, mirroring the CLI apply flow exactly.
- Keep the tool description diff-only ("like apply_patch, but takes an APX diff
  script"); the full grammar stays available via `apx --tool-help`.
- Register the server under `[mcp_servers.apx]` on the stock-Codex benchmark
  surface (`~/.codex/deepseek.config.toml`); do not register it in the apx-fork
  default config, where the fork already injects its own tool.

Gate:

- A stock `codex` MCP client lists the `apx` tool from the schema with no
  instruction-file steering.
- Blind sessions call the tool on first edit: zero `--tool-help` reads, zero
  crate spelunking, zero check loops.
- Accuracy stays at apply_patch parity and session tokens stop exceeding the
  apply_patch control by the iter4 margin.

This path is the interim surface until Phase 6 lands: it pays a per-call
process boundary and is superseded by the in-process freeform extension.

### Phase 6: Add serverless Codex MVP


Deliverables:

- Add freeform extension dispatch support to Codex.
- Add `codex-rs/ext/apx`.
- Register the existing APX grammar and compact/full descriptions.
- Call `apx-core` in-process.
- Store bounded correction state in the extension's thread store and test resume
  reconstruction.
- Use the in-process apply-patch bridge only if the typed transaction capability
  is not ready.
- Expose a feature flag for APX prefer/exclusive behavior without changing the
  model provider.

Gate:

- A real Codex rollout shows a native APX custom tool call.
- The requested files change and the normal diff UI is emitted.
- No APX listener exists, no provider URL is overridden, and no extra process is
  started.
- `codex`, `codex exec`, and app-server-backed sessions all see the same tool.
- Multiple-environment calls fail closed unless every referenced path resolves
  unambiguously to one environment.
- No standalone shell hook runs inside Codex.

### Phase 7: Add typed host transactions

Deliverables:

- Add the Codex workspace-edit capability.
- Adapt `ChangeSet` directly into that capability.
- Preserve sandbox, symlink, mode, rollback, and diff UI semantics.
- Remove production patch serialization from the APX extension.

Gate:

- Codex fault-injection tests prove APX-equivalent failure atomicity.
- Direct typed commit passes local and remote executor contract tests.
- The serverless bridge can be disabled without changing tool behavior.

### Phase 8: Metrics and gain v2

Deliverables:

- Port command success/error metrics after engine parity.
- Replace router/request counters with extension-native counters.
- Version the metrics schema and start a clean v2 gain baseline.
- Keep a read-only legacy report command only if comparison is still useful.

Do not preserve the old binary file by default. Its router-specific token and
request fields do not describe the direct extension path. A versioned reset is
more truthful than pretending continuity.

Gate:

- Gain compares native APX output, tool-definition input, diagnostics, and
  fallback use without allocating whole-request provider usage to APX.
- Raw benchmark fixtures and rollout IDs accompany every headline result.

### Phase 9: Shadow, cut over, and retire Go

Deliverables:

- Run Go and Rust in differential shadow mode on the benchmark corpus.
- Run Luna-low misuse exercises and complex editing tasks against the native
  extension.
- Fix only safe engine defects; retain fail-closed ambiguity and conflict
  rejections.
- Make Rust the default after two clean iterations with no new safe improvement.
- Tag the final Go oracle, archive router migration notes, and remove router
  configuration from active setup docs.

Gate:

- Two consecutive benchmark iterations produce no unexplained parity,
  transaction, or integration failures.
- Rust is no slower than the Go CLI on core evaluation and is materially faster
  than the HTTP proxy end-to-end path.
- `lsof` and process inspection show no APX server.
- Fresh Codex sessions work without launchd/systemd and without provider config.

Only after this gate may the Go router and Go implementation become archived
maintenance code.

## 7. Acceptance suite

The migration is complete only when all of these pass:

1. Parser and diagnostic golden parity.
2. Selector and clipboard state-machine parity.
3. Path confinement, absolute alias, auto-parent, symlink, binary-delete, and
   move coverage.
4. Multi-file fault-injection rollback coverage.
5. `cargo test --workspace` on MSRV and current toolchain.
6. Rustfmt and Clippy with warnings denied.
7. Cross-platform CI.
8. Codex custom/freeform extension routing tests.
9. Codex sandbox and remote filesystem contract tests.
10. Real interactive and `codex exec` native-tool E2E.
11. No-listener/no-proxy process and configuration proof.
12. APX Exercise A/B plus complex multi-file benchmark comparison.

## 8. Risk register

| Risk | Mitigation |
| --- | --- |
| Rewrite changes semantics | Freeze Go fixtures first; differential-test every phase. |
| Codex extension types drift | Keep the adapter inside Codex; pin `apx-core` by tag/revision. |
| Rust 1.97.1 breaks Codex build | Keep APX MSRV at Codex 1.95 until a separate bump lands. |
| Direct writes weaken atomicity | Block final cutover on a typed host transaction capability. |
| Remote filesystems lack rename | Define transaction capability at the host boundary, not as local `std::fs` assumptions. |
| Freeform tool cannot dispatch | Land the small payload-kind adapter change before native E2E. |
| Diff UI disappears | Return a host applied-delta event from the transaction capability. |
| Metrics claim false continuity | Start versioned gain v2; preserve raw legacy evidence separately. |
| Two implementations drift | Freeze Go after oracle tag and retire it only after cutover gates. |
| New repo becomes dependency maze | Keep three initial crates; split only after measured need. |
| Correction retries lose router state | Move rebuilding into core and keep bounded thread state in the Codex adapter. |
| Shell hooks bypass host policy | Keep them local-only; use Codex lifecycle events in-process. |

## 9. First implementation wave

The first coding wave should do only:

1. Generate Phase 0 fixture schema and representative fixtures from Go.
2. Scaffold the three Rust crates and CI.
3. Port the parser plus diagnostics for `in`, `new`, `mv`, and `rm`.
4. Add a differential test runner for those commands.
5. Prepare the minimal Codex freeform adapter patch as a separate commit.

Do not port transactions or remove the router in this wave. The wave is complete
when the Rust parser matches the Go oracle and the Codex adapter test proves that
custom payload bytes can reach a contributed extension executor.
