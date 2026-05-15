## Why

- `fleet-pane-health` lists every tmux pane in the fleet session but does not surface which agent kind (codex / kiro / claude) owns the pane. The operator has to infer it from the `@panel` label, which is fine for the three present codex panels but breaks down once the fleet runs a mix of all three.
- A single-glance distinction between codex, kiro and claude is the most-asked-for improvement to the live dashboard — see `scripts/codex-fleet/full-bringup.sh` already spawning all three kinds.

## What Changes

- Add an `AgentKind` classifier (`codex` / `kiro` / `claude` / `unknown`) that derives the kind from the `@panel` label first and falls back to the `/tmp/claude-viz/{codex,kiro,claude}-worker-*.log` filename.
- Render a new `KIND` column between `PANE` and `PANEL` showing a colored badge (`CODX` blue / `KIRO` purple / `CLAU` orange) on each row.
- Add a `g` keybinding that toggles a grouped view: rows are sorted by kind and a `── group: <kind> ──` header (in the kind's tint) is injected between groups.
- Footer reports `g group: on|off` so the current view mode is always visible.
- Cover the classifier and the grouped/ungrouped renderers with unit tests plus a ratatui `TestBackend` integration test that asserts each kind's badge and group header lands in the rendered buffer.

## Impact

- Surfaces affected: `rust/fleet-pane-health/src/main.rs` only (no schema, no shared crate). Read-only data paths unchanged.
- Risk: low. The classifier is panel-driven so a stale log file cannot mis-label a pane. The grouping toggle starts off, preserving the existing row order at startup.
- Rollout: ship in the next PR; no scripts, supervisors, or config need to change.
- Follow-ups (not in this change): a `f` keybinding to filter to one kind; pulling the badge widget into `fleet-ui` once a second consumer needs it.
