# codex-fleet demo

Bring up the production codex-fleet tmux layout driven by **synthetic data**
— no real codex sessions spawned, no API spend, no GitHub touched. The
existing `fleet-state`, `fleet-tab-strip`, `fleet-plan-tree`, and
`fleet-waves` binaries render against fake plan/pane/heartbeat files, so
what you see is exactly what the live fleet would look like with 8 agents
mid-flight on a refactor wave.

Intended for design iteration: change a renderer, kill+restart the demo,
see the result without paying for codex API calls.

## Run

```bash
bash scripts/codex-fleet/demo/up.sh           # bring up + auto-attach
bash scripts/codex-fleet/demo/up.sh --no-attach
bash scripts/codex-fleet/demo/up.sh --no-tick # static state, no animation
bash scripts/codex-fleet/demo/down.sh         # tear down
```

Prereqs: `tmux`, `jq`, and release builds of `fleet-state`, `fleet-plan-tree`,
`fleet-waves` (plus optionally `fleet-watcher`) in `rust/target/release/`
(or debug fallback). Build with:

```bash
cd rust && cargo build --release -p fleet-state -p fleet-plan-tree \
                                  -p fleet-waves -p fleet-watcher
```

## Layout

Tmux session `codex-fleet-demo` on socket `codex-fleet-demo`:

- **overview** — 8 worker panes in a 4×2 grid. Each pane displays
  scripted scrollback the `fleet-data::scrape` parser will extract a
  runtime + model + headline from. Pane `@panel` options are set to
  `[codex-<aid>]` so `fleet-data::panes::list_panes` maps them to
  accounts.
- **fleet** — `fleet-state` worker list (image G reference design).
  Renders its own iOS tab strip inline (via `fleet_ui::tab_strip` reading
  `fleet-tab-counters.json`); the standalone `fleet-tab-strip` binary was
  removed by PR #107.
- **plan** — `fleet-plan-tree` topo levels view.
- **waves** — `fleet-waves` spawn-timeline view.
- **watcher** *(if built)* — `fleet-watcher` if the binary is available.

## Scenario

`scripts/codex-fleet/demo/scenarios/refactor-wave/plan.json` (committed
template) — 12 tasks in 3 dependency waves modelled after the refactor
PRs (#154–#159): toposort/scrape/tab_strip splits + CliConvention trait +
shell helper lib + dependent follow-ups + a final docs/smoke gate. 8
agents (named after herbs) claim and complete tasks on a tick loop.

`up.sh` copies the template into
`openspec/plans/demo-refactor-wave-2026-05-16/plan.json` so the
dashboards can discover it via their normal `openspec/plans/<slug>/`
lookup. `tick.sh` mutates that runtime copy in place; `down.sh` removes
it, so the working tree stays clean across runs.

The `tick.sh` simulator mutates `plan.json` in place every 3s
(configurable via `CODEX_FLEET_DEMO_TICK_INTERVAL`):

1. Assign next ready task to each idle agent.
2. Bump runtime + rewrite pane scrollback (the workers' `tail -F` loops
   pick this up).
3. After ~4s the task moves `claimed → in_progress`; after ~18s it
   completes.
4. When all 12 tasks complete, loop back to wave 0 (set
   `CODEX_FLEET_DEMO_LOOP=0` to stop instead).

One agent (`clover`) is scripted to be "capped" when idle so the demo
exercises `PaneState::Capped` rendering.

## Synthetic files written

| Path | Owner | Purpose |
|------|-------|---------|
| `/tmp/claude-viz/fleet-tab-counters.json` | `up.sh` + `tick.sh` | `fleet-tab-strip` counter badges |
| `/tmp/claude-viz/fleet-quality-scores.json` | `up.sh` | `fleet-state` quality column |
| `/tmp/claude-viz/plan-tree-pin.txt` | `up.sh` | Pins plan-tree to the demo plan |
| `/tmp/claude-viz/demo-current-account` | `up.sh` | Marks the `*` row in agent-auth |
| `/tmp/claude-viz/demo-panes/<aid>.txt` | `tick.sh` | Per-pane fake scrollback (workers `tail -F` these) |
| `/tmp/claude-viz/demo-active` | `up.sh` | Sentinel; `tick.sh` exits when removed |
| `/tmp/claude-viz/demo-tick.pid` | `up.sh` | PID of background tick simulator |
| `openspec/plans/demo-refactor-wave-2026-05-16/plan.json` | committed | Plan fixture mutated in place by `tick.sh` |

`down.sh` removes everything in the table except the committed plan
fixture.

## Shim

`scripts/codex-fleet/demo/agent-auth` is prepended to `$PATH` for the
binaries. `fleet-data::accounts::load_via_agent_auth()` calls
`agent-auth list` as a subprocess — the shim emits 8 synthetic rows that
match the real parser's format.

## Extending

- **New scenario:** add `openspec/plans/<your-slug>/plan.json` and pass
  `CODEX_FLEET_DEMO_PLAN_SLUG=<your-slug>` (TODO: wire this through
  `up.sh` + `tick.sh` — currently hardcoded to `demo-refactor-wave-…`).
- **Different worker count:** change `WORKERS` + `AIDS` in `up.sh` and
  `AIDS` in `tick.sh`. Tmux layout also needs adjustment past 8.
- **Slow down / speed up:** `CODEX_FLEET_DEMO_TICK_INTERVAL=1` (faster)
  or `=10` (slower).

## Caveats

- The demo plan slug carries a date suffix so the "newest plan" picker in
  `fleet-plan-tree` selects it on a clean repo. If you have other plans
  with a newer date suffix, the `plan-tree-pin.txt` override kicks in.
- `fleet-watcher` is not wired into the demo yet — its data dependencies
  overlap with `fleet-state` but it also looks at colony/auto-reviewer
  state that isn't faked here.
- Workers' `current_command` shows `bash` (not `codex`), which
  `panes::classify` would flag as `Dead`. The classifier checks for
  "codex" in scrollback as an escape; the fixture text starts with
  `codex 0.42.0 — …` so this branch evaluates correctly. If you see
  Dead-state rendering, that's the cause.
