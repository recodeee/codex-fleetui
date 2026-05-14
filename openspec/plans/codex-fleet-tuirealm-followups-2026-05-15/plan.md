# tuirealm migration follow-ups (parallel)

## Context

PRs **#50, #52–#55, #58** ported every codex-fleet ratatui binary to the
tuirealm component pattern; PR **#63** extracted the duplicated
terminal-adapter setup + tmux click-routing into `rust/fleet-components/`
(consumed first by `fleet-tab-strip`). Four follow-ups were deliberately
deferred to keep those PRs minimum-surface — they live here as a parallel
plan because they don't share files and can run concurrently.

## Sub-tasks

| # | Title | Touches |
|---|---|---|
| 0 | Adopt fleet-components in fleet-state    | `rust/fleet-state/` |
| 1 | Adopt fleet-components in fleet-plan-tree| `rust/fleet-plan-tree/` |
| 2 | Adopt fleet-components in fleet-waves    | `rust/fleet-waves/` |
| 3 | Adopt fleet-components in fleet-watcher  | `rust/fleet-watcher/` |
| 4 | Adopt fleet-components in fleet-tui-poc  | `rust/fleet-tui-poc/` |
| 5 | Per-overlay split for fleet-tui-poc (depends on #4) | `rust/fleet-tui-poc/` |
| 6 | Generic `Model<T>` runner in fleet-components       | `rust/fleet-components/` + `rust/fleet-tab-strip/` |

Tasks **0–4** are independent (one binary each); claim any of them in any
order. **#5** waits for **#4** to land. **#6** is independent of
everything else.

## Definition of done

- Every dashboard binary uses `fleet_components::init_crossterm_adapter`
  + `shutdown_adapter` instead of the inline three-call setup/teardown.
- `fleet-tui-poc` has one Component per Overlay variant; the giant
  `handle_key` match disappears.
- `fleet-components::run` exists, generic over `(Id, Msg,
  C: Component + AppComponent)`, and at least `fleet-tab-strip` adopts
  it.
- `cargo test --workspace` stays green; every PR uses
  `gx branch finish --branch <agent/…> --base main --via-pr
  --wait-for-merge --cleanup`.

## Non-goals (kept out of this plan)

- Workspace ratatui bumps beyond 0.30.
- `tuirealm` feature flag changes (e.g. enabling `serialize`).
- Cross-binary visual or behavioural changes beyond the literal
  refactors above.
