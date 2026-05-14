# Phase 5 — Port iOS overlays from fleet-tui-poc into fleet-ui + wire keybindings into the four view binaries

Plan slug: `codex-fleet-overlays-phase5-2026-05-14`

## Problem

Phase 0-4 of the fleet-tui-ratatui-port openspec change are done: fleet-tui-poc validated the risks, fleet-ui shipped palette/chip/rail/card/overlay primitives (Wave 2 PR #10), fleet-data + fleet-watcher/state/plan-tree/waves shipped (Waves 3-5 PR #11), full-bringup wires the Rust renderers, and the live tmux right-click now serves the ratatui iOS menu via display-popup (PR #12). Phase 5 of openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md remains: lift the four overlay widgets that live inline in rust/fleet-tui-poc/src/main.rs (ContextMenu, Spotlight with text input + selection, ActionSheet, SessionSwitcher with clickable card buttons) into reusable widgets inside rust/fleet-ui/src/overlay.rs, then wire trigger keybindings into each view binary so the operator can pop an iOS-styled command surface from inside any dashboard. The current fleet-ui/overlay.rs only ships centered_overlay + render_overlay helpers — the rich per-overlay rendering (drop shadow, glass card, group cards, badge routing, blinking caret, dispatch) is all still trapped in the POC.

## Acceptance Criteria

- rust/fleet-ui/src/overlay.rs exposes `ContextMenu`, `Spotlight`, `ActionSheet`, `SessionSwitcher` as reusable widgets with `Widget`-style APIs (`new(...)` constructors + `render(frame, area, state)` methods) — port from the inline POC code, preserving the iOS dark-glass palette, 3D drop shadow, hairline dividers, badge routing (LIVE = green, ⚠ REVIEW = orange), and interactive Spotlight state (query, selected index, caret tick).
- Each of the four view binaries (fleet-watcher, fleet-state, fleet-plan-tree, fleet-waves) responds to `?` (or `/`) by opening the Spotlight overlay, `m` by opening the ContextMenu overlay for the active pane, and `Esc` / `q` dismissing the overlay; while an overlay is open, dashboard tick updates pause repaint of the underlying view so the overlay doesn't flicker.
- Spotlight filters its catalogue live as the user types (case-insensitive substring on title + sub-line), with Up/Down moving the selection through filtered items and Enter dispatching the bound action (initial actions: split-window-h, split-window-v, resize-pane -Z, kill-pane, kill all, focus-pane-by-index).
- ContextMenu's shortcut letters (h, v, z, u, d, s, m, R, X) dispatch the matching tmux command directly from inside the binary (use fleet-data's tmux helper or std::process::Command); the bash-fallback path stays untouched.
- `insta` snapshot tests cover each overlay's default render at a fixed terminal size (80x40 for ContextMenu/ActionSheet, 100x40 for Spotlight, 140x40 for SessionSwitcher).
- openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md Phase 5 checkboxes are all ticked, with PR links in the completion notes.

## Roles

- [planner](./planner.md)
- [architect](./architect.md)
- [critic](./critic.md)
- [executor](./executor.md)
- [writer](./writer.md)
- [verifier](./verifier.md)

## Operator Flow

1. Refine this workspace until scope, risks, and tasks are explicit.
2. Publish the plan with `colony plan publish codex-fleet-overlays-phase5-2026-05-14` or the `task_plan_publish` MCP tool.
3. Claim subtasks through Colony plan tools before editing files.
4. Close only when all subtasks are complete and `checkpoints.md` records final evidence.
