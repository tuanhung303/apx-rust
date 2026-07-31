# iter4 benchmark — apx (Rust CLI) vs apply_patch

Measured 2026-07-31 on macOS (stock codex CLI 0.146.0 via `cds` = codex +
`~/.codex/deepseek.config.toml`, model `deepseek-v4-flash`, reasoning effort `low`,
YOLO). Tool under test: the native Rust CLI from this repo
(`~/.local/bin/apx`, `apx 0.1.0 (apx-rust f7cc37a)`, built from `crates/apx-cli` +
`apx-core`). The Go build is preserved as `~/.local/bin/apx.go-bcd85fc`; a differential
smoke produced byte-identical applies.

8 sessions = 2 waves × 2 exercises × 2 arms, run as a `swarm` 2×2 grid
(`swarm-wave.sh --grid 2x2`, topics `apx-bench-iter4-w1` / `apx-bench-iter4-w2`):

- Exercise A — Go lease-registry refactor, graded on 18 checks.
- Exercise B — Python ledger→package migration, graded on 15 checks.
- apx arm: the prompt names no tool; `~/.codex/apx-instructions.md` steers edits
  through the local `apx` CLI heredoc.
- apply_patch control: every edit must go through `apply_patch`; apx and shell file
  writes are forbidden.

Tasks explicitly forbid subagent delegation (iter3 confound). All 8 sessions show
0 subagent spawns.

## Accuracy

| Round | apx | apply_patch |
|---|---|---|
| w1 A (Go, 18) | 15/18 (A6, A11, A12) | 17/18 (A6) |
| w1 B (Python, 15) | 15/15 | 15/15 |
| w2 A (Go, 18) | 17/18 (A6) | 16/18 (A6, A12) |
| w2 B (Python, 15) | 15/15 | 15/15 |
| **Total** | **62/68** | **63/68** |

- **A6 fails in every A round on both tools**: the check requires
  `internal/leasestore/store.go`, which the task never asks for — both agents put the
  `Expirer` interface in `sweeper.go` exactly as the task wording directs. The refactor
  compiles and all tests pass either way. Task-bound, not tool-bound.
- **A11/A12** (RWMutex adoption, read-only snapshot) flip between arms and waves —
  agent variance at low effort, not a tool difference.
- **A16** (go.mod untouched) passed for all arms at grading time; final `go.mod` is
  byte-identical to the seed (`module apxdf/a`, `go 1.26`) and no session wrote it.
  (Post-hoc re-grading cannot re-run A16: the seed dirs were cleaned.)

## Efficiency (counted from session JSONLs)

| Metric | apx (4 sessions) | apply_patch (4 sessions) | Δ |
|---|---|---|---|
| Edit payload (script/doc chars), avg | 7,371 | 7,960 | **−7.4%** |
| Edit invocations, avg | 5.5 | 4.25 | +1.25 |
| Session input tokens, avg | 2,522,944 | 1,470,800 | +71.5% |
| Session output tokens, avg | 25,789 | 17,965 | +43.6% |
| Reasoning tokens, avg | 20,146 | 13,879 | +45.2% |
| Rejections / failures | 0 | 0 | — |
| `translate` dry-runs | 3 (w1 B only) | 0 | — |

Per-arm payload (chars / calls):

| Arm | apx | apply_patch |
|---|---|---|
| w1 A | 8,997 / 5 | 8,665 / 4 |
| w1 B | 6,008 + 4,810 translate / 5 | 7,461 / 1 |
| w2 A | 7,321 / 5 | 7,512 / 6 |
| w2 B | 7,158 / 7 | 8,202 / 6 |

Session token counts include per-turn context re-sends (heavily cached); use as a
relative magnitude. The input/output token delta is conversational cost — more,
smaller tool calls (5.5 vs 4.25) and longer `exec_command` output — not edit payload.

## Stop → improve → re-benchmark (between waves)

Finding in w1 (blind B-apx): the agent re-emitted a whole file (`rsel 1:35` on
`ledger.py`, 1,913 chars) and ran 3 `translate` dry-runs (4,810 chars) to preview a
~20-line refactor.

Fix before w2: `~/.codex/apx-instructions.md` now **forbids whole-file `rsel` when the
changed regions are < 50% of lines** (use `tsel`/`bsel`/`type` instead). This is the
selector-economy rule committed to the Go oracle repo after iter3 (commit 44355e6),
now enforced at instruction level; the prior copy is backed up as
`~/.codex/apx-instructions.iter4-w1.md`. Wave-2 tasks additionally forbid delegation.

Effect on the B-apx arm (same exercise, same model):

| Metric | w1 B-apx | w2 B-apx | Δ |
|---|---|---|---|
| Edit payload | 10,818 (incl. 4,810 translate) | 7,158 | **−33.8%** |
| Session output tokens | 21,518 | 13,995 | **−35.0%** |
| `translate` dry-runs | 3 | 0 | −100% |
| Accuracy | 15/15 | 15/15 | flat |

## Findings

1. **Accuracy parity holds at parity-of-cost.** 62/68 vs 63/68 on identical fixtures;
   the only net difference is one A12 flip between arms (agent variance). A6 is
   task-bound and fails both tools in every A round.
2. **Payload economy flipped from iter3.** iter3 (Go CLI, pre-rule): apx 9,584 chars /
   5 calls vs apply_patch 6,817 / 1 (+41% penalty). iter4 (Rust CLI + enforced rule):
   apx 7,371 vs 7,960 (−7.4%). The rule "never re-emit a whole file when the change is
   localized" is what converts the saving; without it, low-effort models default to
   whole-file rewrites.
3. **The Rust CLI matches the Go oracle.** Differential smoke: byte-identical applies.
   Live `apx gain` on the shared metrics slot (HPATCH19): 842 successful calls,
   est. output tokens 918 apx vs 2,488 apply_patch (−63.1%); selector usage in the
   wild: `tsel` 146, `type` 265, `rsel` 80, `bsel` 40, `new`/`mv`/`rm` 52 combined.
4. **Prompt steering works on stock Codex + DeepSeek.** Blind apx arms used the `apx`
   heredoc CLI exclusively: zero `apply_patch`, zero shell file writes, verified by
   function-call inventory + forbidden-pattern scan across all 4 apx sessions.
5. **Conversational cost is the remaining gap.** More tool calls and longer exec
   outputs inflate session in/out tokens (+71.5% / +43.6%). File-edit payload is
   smaller; the surrounding dialogue is not. Next levers: instruction density (fewer
   exploratory reads) or batching multiple selectors per call.

## Reproduce

```bash
cargo build --release                                   # installs crates/apx-cli -> ~/.local/bin/apx
export CODEX_BIN=/Users/__blitzzz/.local/bin/cds        # cds = codex + deepseek.config.toml (deepseek-v4-flash)
export SWARM_CODEX_MODEL=deepseek-v4-flash SWARM_CODEX_EFFORT=low
export HERDR_ENV=1 HERDR_WORKSPACE_ID=w7

bash tmp/launch-iter4-w1.sh                             # wave 1, 2x2 grid
python3 tmp/grade_iter4.py a tmp/w1-a-apx tmp/w1-a-apx-seed
python3 tmp/analyze_sessions.py ~/.codex/sessions/2026/07/31/rollout-2026-07-31T20-07-01-*.jsonl

bash tmp/launch-iter4-w2.sh                             # wave 2, after selector-economy rule
python3 tmp/grade_iter4.py a tmp/w2-a-apx tmp/w2-a-apx-seed
```

The graders were validated against iter3 before use (reproduced iter3 A-apx: 5 calls,
1,825,536 input tokens, 24,880 output tokens). Session manifest:
`tmp/iter4-session-manifest.json`.

## Iteration history

- iter2 (explicit tool naming) / iter3 (blind) baselines: see the Go oracle repo,
  `apx/doc/benchmarks/BENCHMARK.md`.
- iter4 (this doc): Rust CLI + selector-economy rule enforced at instruction level.

## w3 + w4 — tuned tool surface (peek/check) and instruction rules

After iter4, the CLI gained two read-only modes designed to cut conversational
tokens (`apx peek` — selector-scoped file reads with line numbers; `apx check` —
validate-only, one-line success output, no patch-envelope echo), and the
instruction file gained batching, reading-economy, scratch-ban, and
check-not-translate rules (tuned by an OpenCode Kimi K3 pass; backups
`~/.codex/apx-instructions.iter4-w2.md` / `.iter5-pre.md` / `.iter5-w3.md`).
All changes additive: `cargo test --workspace` green (11/11 engine tests incl.
5 new peek tests), clippy clean, apply output byte-identical to the Go oracle.

Wave results (deepseek-v4-flash low, same exercises):

| Wave | Round | apx | apply_patch |
|---|---|---|---|
| w3 | A (Go) | 17/18 (A6) | 17/18 (A6) |
| w3 | B (Python) | 15/15 | 15/15 |
| w4 (clean B re-run) | B (Python) | 15/15 | 15/15 |

Tool-usage, clean arms only (w3c A + w4 B; w3 B arms invalidated — see below):

| Metric | apx (2 sessions) | apply_patch (2 sessions) | Δ |
|---|---|---|---|
| Accuracy (applicable) | 32/33 | 32/33 | parity |
| Edit invocations avg | 5.5 | 1.0 | +4.5 |
| Edit payload avg (chars) | 17,577 | 8,006 | +120% |
| Session input tokens avg | 3,436,979 | 895,901 | +284% |
| Session output tokens avg | 41,090 | 11,665 | +252% |
| `translate` dry-runs | 0 | 0 | — |
| Scratch-dir experiments | 0 | 0 | — |

Per-arm (chars / calls):

| Arm | apx | apply_patch |
|---|---|---|
| w3c A | 14,039 / 5 heredocs (2 peek, 1 check, 3 apply incl. one 6.7 KB batched script) | 5,574 / 1 |
| w4 B | 21,115 / 5 heredocs (3 check, 2 apply) | 10,438 / 1 |

### What the tuning changed

- Behavior improved: blind apx arms adopted `peek` (region reads instead of
  `cat`), `check` (zero `translate` dry-runs after w1), and the scratch-ban held
  (zero /tmp experiment dirs in w3c/w4, vs 5 scratch calls in w3).
- Batching landed partially: w3c-a-apx applied the core refactor in one 6.7 KB
  script (check + apply as a pair); w4-b-apx re-emitted the same ~5.4 KB script
  across 3 `check` iterations before applying.
- The token gap did NOT close. apx sessions still cost ~2.5–3× the
  apply_patch control in session tokens: each additional call is a full
  round-trip (reasoning + context re-send), and check-loop script re-emission
  dominates the payload (w4 B: 16,126 of 21,115 chars were check re-runs).
- Low-effort model variance is large: per-cell deltas between waves (e.g.
  w2-b-apx 7,158 vs w4-b-apx 21,115 chars on the same exercise) exceed the
  tool-level signal. Headline conclusions should come from the multi-wave
  pattern, not any single arm.

### Invalidated waves

- **w3b** — task-path double-replace bug sent all four agents to `w3bb-*`
  paths; agents self-corrected into visible directories and browsed previous
  answers. Not counted.
- **w3 B arms** — agents read completed answers (`w1-b-apx/store/ledger.py`),
  the grader rubric, and (patch arm) invoked the raw `apply_patch` binary via
  exec. Not counted. Environment fixed by archiving every fixture, seed, and
  grader outside the repo (`~/.apx-bench-archive/iter4/`) before w4.

### Standing conclusions after 5 waves (w1, w2, w3, w3c, w4)

1. **Accuracy parity is stable.** Every wave ends at parity within one
   variance flip; A6 is task-bound and fails both tools in every A round.
2. **apx has not beaten apply_patch on session tokens in any wave.** The
   iter4 edit-payload advantage (−7.4%) is real but small; the conversational
   cost of more, smaller calls (+43–250% out tokens depending on wave) erases
   it. The tool is accuracy-safe and payload-lean, but "token reduction" is
   not yet demonstrated in a controlled blind benchmark.
3. **The next lever is check-loop re-emission and call overhead**, not
   selector economy: make failed-`check` iteration cheap (e.g. shorter
   re-send, or a surface that lets the model amend the last script), and cut
   per-call round trips (one script per task, including reads).
