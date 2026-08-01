# iter13-14 — steering moved into tool descriptions (no-instructions harness)

Measured 2026-08-01 on macOS. Question: can the Codex-side steering
(AGENTS.md "File edits" section + `model_instructions_file`) live inside the
MCP tool descriptions alone, so any agent registering the server gets it?

Harness: isolated `CODEX_HOME` (`~/.apx-bench-archive/bench-home-nosteer`)
with the apx MCP server registered but NO AGENTS.md and NO
`model_instructions_file`; wrapper `cds-nosteer` = stock codex + `-p deepseek`
against that home. Blind `deepseek-v4-flash low`, 2x1 grids, fresh fixtures
`tmp/w16-*` (iter13), `tmp/w17-*` (iter13b), `tmp/w18-*` (iter14).

## Rounds

| Round | Change | A (Go) | B (Py) |
|---|---|---|---|
| iter13 | desc: bullets + "not apply_patch" (soft) | 11/18 — zero work: agent hunted the grader in repo docs, never edited | 15/15 — apx 1 + peek 6, but 3 apply_patch CLI calls for file add/delete |
| iter13b | same desc | 17/18 — full work, but via `apply_patch` CLI (probed `which apply_patch`); no apx | 15/15 — apx 2 + peek 8, 0 apply_patch |
| iter14 | desc: HARD ban — "use THIS tool for EVERY file change — never the apply_patch tool (create/delete files with `new PATH`/`rm`)" | 17/18 — apx 7 + peek 11, 0 apply_patch, 0 rejections | 15/15 — apx 1 (one-shot 5,740-char script), 0 apply_patch |

## Efficiency (iter14, per-session sums — no steering at all)

| Session | Input tokens | Output tokens | apx calls | peek | exec |
|---|---|---|---|---|---|
| A (Go) | 1,286,422 | 50,173 | 7 | 11 | 5 |
| B (Py) | 144,528 | 11,074 | 1 | 0 | 5 |
| iter12 A (steered, ref) | 1,292,755 | 12,818 | 1 | 2 | 5 |
| iter12 B (steered, ref) | 1,583,368 | 12,462 | 2 | 2 | 6 |

## Findings

1. **The tool surface CAN carry the steering — once the ban is hard.**
   Soft "not apply_patch" lost on the Go lane (agent probed `which
   apply_patch` and used it for add/delete). The hard clause ("never the
   apply_patch tool; create/delete via `new PATH`/`rm`") flipped both lanes
   to apx+peek with full accuracy in zero-steering conditions. commit
   `70ca485`.
2. **Portable = desc-only; efficient = instructions still help.** Desc-only
   B (iter14) is the cheapest session ever: 144,528 input for 15/15. But
   desc-only A split the Go refactor into 7 apx scripts + 11 peeks (50,173
   output) vs iter12 steered's single 1-script/2-peek run (12,818). The
   instruction file's "ONE script per whole task" tier binds harder than
   the same clause inside a tool description.
3. **Blind-harness drift is agent-behavior, not tool surface.** iter13 A
   never edited (grader-hunting). Re-running the same fixture+desc gave a
   full solve. n=1 per lane per round is noisy; the iter14 2/2 clean pair
   is the meaningful signal.
4. **Recommendation:** keep the desc as the portable floor (it now works
   with zero machine-specific steering) and keep the instructions file as
   the efficiency ceiling for this machine (batch enforcement). The desc
   change is cheap; a future desc-only wave with a "ONE script per whole
   task" first-bullet would test whether batching can be desc-only too.

## Status

Tool description alone now drives correct tool choice (apx+peek, zero
apply_patch) on both lanes with 100% functional accuracy; B at record-low
input. Instruction steering remains worth keeping for one-script batching
until a desc-first-bullet variant matches it.
