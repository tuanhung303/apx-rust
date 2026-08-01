# iter19 — client naming fix (non_prefixed_mcp_tool_names) still does NOT convert luna low

Measured 2026-08-01, right after commit `4b4fb98` (desc: name-agnostic
`REPLACES apply_patch — use me for every edit` front-load, `mcp__` literals
dropped) and commit `7c8b8b2` (iter18).

## Hypothesis (the structural discovery barrier)

iter15-18 root-caused luna's non-adoption to discovery: MCP tools register as
`mcp__apx__apx` / `mcp__apx__peek`, so in the alphabetically sorted tool list
they sit at position ~8, buried among ~130 `mcp__codex_apps__*` tools, while
the builtin `apply_patch` is position 1. iter19 tests the one server/client
lever available without touching AGENTS.md or the system prompt:

1. **`features.non_prefixed_mcp_tool_names = true`** (under-development
   feature in stock codex v0.146.0): MCP tools lose the `mcp__` prefix →
   `apx__apx`, `apx__peek` (verified by live probe: the model quotes the exact
   name `apx__apx` and the new desc when asked). These sort at position 2-3,
   immediately after `apply_patch`.
2. **Server renamed to `apply` was rejected**: probe results were flaky and
   the binary contains client-side `apply_patch_tool_type` handling — a name
   starting with `apply` risks colliding with builtin apply_patch logic.
3. **No config exists to disable/hide the builtin `apply_patch`**:
   `--strict-config` probes rejected `disabled_tools`, `tools.*`, and
   `tools.disabled`; `tool_suggest.disabled_tools` exists but only accepts
   `type = "connector" | "plugin"` (suggestion filtering, not tool removal).

## Harness

Same as iter15-18 (`~/.apx-bench-archive/bench-home-nosteer2`: isolated
CODEX_HOME, only `[mcp_servers.apx]`, no AGENTS.md, no instructions file)
plus `features.non_prefixed_mcp_tool_names = true` and
`suppress_unstable_features_warning = true`. Blind 2x1, `gpt-5.6-luna` low,
fresh fixtures `tmp/w23-{a,b}-apx1.fresh` re-created from the pristine
iter4 seeds (`~/.apx-bench-archive/iter4/bex3-{a,b}-apx-seed`); task text
byte-identical to iter14-18 except fixture paths.

> Methodology note: the first launch of this round reused `tmp/w23-{a,b}-apx1`
> which had been copied from the **post-iter18** state (already fully
> refactored). Both lanes finished in ~30s doing validation only, 0 edit
> tools, `git status` clean — a false positive. The round was re-run on
> fresh seed fixtures; only the re-run counts below.

## Accuracy (same grader as iter4)

| Lane | Grade | Note |
|---|---|---|
| A (Go) | 17/18 | A6 is the known task-bound fail (exercise never requests `store.go` equivalent) → 100% functional |
| B (Py) | 15/15 | clean |

## Tool usage — the finding that matters

| Lane | apx | peek | apply_patch | exec | rejections |
|---|---|---|---|---|---|
| A (Go) | 0 | 0 | **2** | 3 | 0 |
| B (Py) | 0 | 0 | **1** | 4 | 0 |

Even with native-looking names at position 2-3 and the front-loaded
`REPLACES apply_patch — use me for every edit` description, **luna low
completed both exercises entirely with the builtin `apply_patch`** — zero
apx, zero peek, and no mention of the apx tool anywhere in session
reasoning/messages (the only `apx` substrings are file paths).

The discovery fix is real (probe-verified: the model finds and quotes
`apx__apx` + desc when directed) but does not change blind behavior: luna
pattern-matches the familiar builtin name and never inspects the unknown
MCP tool, regardless of list position or description.

## Tokens (per-session sums, same method as iter15-18)

| Lane | Input tokens | Output tokens | Editing tool |
|---|---|---|---|
| A (Go) | 92,427 | 2,757 | apply_patch 2 |
| B (Py) | 95,370 | 3,009 | apply_patch 1 |
| **total** | **187,797** | **5,766** | |

Lowest input total of all luna-low rounds on these exercises
(iter15 233,128 / iter16 215,872 / iter17 284,627), but the drop is not
attributable to apx (0 calls) — candidate causes: shorter name-agnostic desc,
feature-flag tool rendering, or cache variance.

## Conclusion

The portability objective is **structurally unachievable server-side under
the hard "desc-only, no AGENTS.md/system prompt" rule for stock OpenAI
models**. Evidence across 6 rounds:

| Iter | Model / effort | Surface change | apx calls | apply_patch calls |
|---|---|---|---|---|
| iter14 | deepseek-v4-flash low | desc-only (bait) | **7+11 / 1** | 0 |
| iter15 | gpt-5.6-luna low | desc-only (bait) | 0 | 2 / 2 |
| iter16 | gpt-5.6-luna low | + front-load REPLACES + name hint | 0 | 2 / 1 |
| iter17 | gpt-5.6-luna low | + BANNED claim + schema hint | 0 | 2 / 1 |
| iter18 | gpt-5.6-luna medium | same desc | 0 | 1 / 1 |
| iter19 | gpt-5.6-luna low | + unprefixed names (pos 2-3) | 0 | 2 / 1 |

DeepSeek converts; OpenAI models do not, no matter the tool-surface wording
or naming. The only levers that demonstrably work for stock OpenAI models
require instructions outside the tool description (AGENTS.md /
`model_instructions_file`, proven in iter13), or a client-side interceptor
that removes/hides the builtin `apply_patch` — none exists in stock codex
v0.146.0 (verified via `--strict-config` probes and binary schema strings).

## Final probe (same day, after iter19): `tools.enabled_tools` whitelist

The last server-side candidate for hiding the builtin was a top-level
`[tools]` whitelist:

```
codex exec --strict-config -c 'tools={enabled_tools=["exec_command"]}' "say ok"
→ Error loading config.toml: unknown configuration field `tools.enabled_tools` in -c/--config override
```

`strings` on the real binary settles why: `enabled_tools` / `disabled_tools`
are fields of **PluginMcpServerConfig** (per-MCP-server tool filtering, e.g.
`[mcp_servers.apx] enabled_tools = [...]`), not a global `[tools]` table.
Every `tools.*` variant under `--strict-config` is rejected (mode/v2/enabled/
freeform/allow/deny/only/...), and `tool_suggest.disabled_tools` is
suggestion-only for `connector|plugin` types.

**Definitive negative, full surface exhausted:** stock codex v0.146.0 ships no
configuration that removes, hides, or demotes the builtin `apply_patch` for
OpenAI models. Portability therefore cannot be achieved server-side alone;
remaining options are client-side (interceptor/wrapper that rewrites
apply_patch into apx scripts) or instructions outside the tool desc
(AGENTS.md — proven iter13, but violates the hard desc-only rule).
