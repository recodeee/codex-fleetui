---
base_root_hash: missing-spec-root
slug: codex-fleet-overlays-phase5-2026-05-14
---

# CHANGE · codex-fleet-overlays-phase5-2026-05-14

## §P  proposal
# Phase 5 — Port iOS overlays from fleet-tui-poc into fleet-ui + wire keybindings into the four view binaries

## Problem

Phase 0-4 of the fleet-tui-ratatui-port openspec change are done: fleet-tui-poc validated the risks, fleet-ui shipped palette/chip/rail/card/overlay primitives (Wave 2 PR #10), fleet-data + fleet-watcher/state/plan-tree/waves shipped (Waves 3-5 PR #11), full-bringup wires the Rust renderers, and the live tmux right-click now serves the ratatui iOS menu via display-popup (PR #12). Phase 5 of openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md remains: lift the four overlay widgets that live inline in rust/fleet-tui-poc/src/main.rs (ContextMenu, Spotlight with text input + selection, ActionSheet, SessionSwitcher with clickable card buttons) into reusable widgets inside rust/fleet-ui/src/overlay.rs, then wire trigger keybindings into each view binary so the operator can pop an iOS-styled command surface from inside any dashboard. The current fleet-ui/overlay.rs only ships centered_overlay + render_overlay helpers — the rich per-overlay rendering (drop shadow, glass card, group cards, badge routing, blinking caret, dispatch) is all still trapped in the POC.

## Acceptance criteria

- rust/fleet-ui/src/overlay.rs exposes `ContextMenu`, `Spotlight`, `ActionSheet`, `SessionSwitcher` as reusable widgets with `Widget`-style APIs (`new(...)` constructors + `render(frame, area, state)` methods) — port from the inline POC code, preserving the iOS dark-glass palette, 3D drop shadow, hairline dividers, badge routing (LIVE = green, ⚠ REVIEW = orange), and interactive Spotlight state (query, selected index, caret tick).
- Each of the four view binaries (fleet-watcher, fleet-state, fleet-plan-tree, fleet-waves) responds to `?` (or `/`) by opening the Spotlight overlay, `m` by opening the ContextMenu overlay for the active pane, and `Esc` / `q` dismissing the overlay; while an overlay is open, dashboard tick updates pause repaint of the underlying view so the overlay doesn't flicker.
- Spotlight filters its catalogue live as the user types (case-insensitive substring on title + sub-line), with Up/Down moving the selection through filtered items and Enter dispatching the bound action (initial actions: split-window-h, split-window-v, resize-pane -Z, kill-pane, kill all, focus-pane-by-index).
- ContextMenu's shortcut letters (h, v, z, u, d, s, m, R, X) dispatch the matching tmux command directly from inside the binary (use fleet-data's tmux helper or std::process::Command); the bash-fallback path stays untouched.
- `insta` snapshot tests cover each overlay's default render at a fixed terminal size (80x40 for ContextMenu/ActionSheet, 100x40 for Spotlight, 140x40 for SessionSwitcher).
- openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md Phase 5 checkboxes are all ticked, with PR links in the completion notes.

## Sub-tasks

### Sub-task 0: fleet-ui::overlay::ContextMenu widget — port from POC

In rust/fleet-ui/src/overlay.rs, add a `ContextMenu` struct + impl. Inputs: sections (`Vec<Section>` where each Section is a list of MenuItem { icon, label, shortcut, destructive }), title, status_dot (Color), badge (Option<(text, fg, bg)>). The render method should reproduce the POC visuals exactly: rounded 48-wide glass card on IOS_BG_GLASS, drop shadow (use the helper from fleet-tui-poc's card_shadow — port it too), title row with status dot + pane label + right-aligned LIVE pill, hairline below title, sections separated by hairline rows, items rendered as ` [icon]  label                    [shortcut]` with destructive items in IOS_DESTRUCTIVE red. Source: rust/fleet-tui-poc/src/main.rs lines around `fn render_context_menu` (~lines 630-770). Add an insta snapshot test at 80x40 with the same 5 sections the POC uses.

File scope: rust/fleet-ui/src/overlay.rs, rust/fleet-ui/tests/overlay_context_menu.rs, rust/fleet-ui/src/lib.rs

### Sub-task 1: fleet-ui::overlay::Spotlight widget — port from POC with interactive state (depends on: 0)

In rust/fleet-ui/src/overlay.rs, add a `Spotlight` widget. Public state struct: `SpotlightState { pub query: String, pub selected: usize, pub tick: u64 }`. Public item type: `SpotlightItem { pub group: &'static str, pub icon: &'static str, pub title: &'static str, pub sub: &'static str, pub kbd: &'static str }`. Public filter fn: `pub fn filter<'a>(items: &'a [SpotlightItem], query: &str) -> Vec<&'a SpotlightItem>`. Render method takes (frame, area, state, items). Reproduce POC visuals: 78x42 glass card with drop shadow, search bar with ⌕ + query + blinking caret driven off `state.tick`, hairline, TOP HIT label, 3-row top-hit pill (systemBlue with darker icon chip + right-aligned `tmux · KBD` badge + chevron, subtitle on row 2), grouped remaining results with `IOS_CARD_BG` background per group, 2-row items (icon+title+kbd row, then subtitle row), selected row tinted `IOS_TINT_DARK` with white fg + IOS_TINT_SUB sub. Empty-state 'no matches' centered when filter returns empty. Footer hint row: `↵ open · ⌥↵ all panes · esc cancel · ✦ N items`. Source: rust/fleet-tui-poc/src/main.rs ~lines 760-1050. Insta snapshot test at 100x40 with the POC's default 9-item catalogue and query=`split`.

File scope: rust/fleet-ui/src/overlay.rs, rust/fleet-ui/tests/overlay_spotlight.rs, rust/fleet-ui/src/lib.rs

### Sub-task 2: fleet-ui::overlay::ActionSheet + SessionSwitcher widgets — port from POC (depends on: 1)

Add two more reusable widgets to rust/fleet-ui/src/overlay.rs. (a) `ActionSheet`: bottom-anchored 64-wide glass card with separate Cancel button below; takes groups (title + optional caption + items). 2-row tall items with 3x2 icon-chip bg, colored chip bg per item state (destructive=red-tint, warning=orange-tint, default=icon-chip-gray). Drop shadow on both the card and the Cancel pill. Source: POC main.rs ~lines 1055-1245. (b) `SessionSwitcher`: full-screen card stack — header (CODEX-FLEET · SESSION SWITCHER + worker count + awaiting-review count), top-right 'New worker' pill, horizontally-scrolling 30-wide tinted cards per session with active card highlighted (systemBlue border + LIVE green pill), MODEL/CONTEXT/RUNTIME rows, action button row (Focus / Queue / Pause / Kill), badge routing (LIVE=green, ⚠ REVIEW=orange, other=chip-gray). Source: POC main.rs ~lines 1247-1640. Insta snapshots at 80x40 and 140x40 respectively.

File scope: rust/fleet-ui/src/overlay.rs, rust/fleet-ui/tests/overlay_action_sheet.rs, rust/fleet-ui/tests/overlay_session_switcher.rs, rust/fleet-ui/src/lib.rs

### Sub-task 3: Wire Spotlight + ContextMenu keybindings into fleet-watcher (depends on: 1, 2)

In rust/fleet-watcher/src/main.rs, add overlay state (`overlay: Option<OverlayKind>`, where OverlayKind in { Spotlight(SpotlightState), ContextMenu }) to the binary's App struct. Key handler: when no overlay is active, `?` or `/` opens Spotlight (initial SpotlightState empty), `m` opens ContextMenu for the active pane (resolve via tmux env `$TMUX_PANE` or pass `--pane` at startup). When Spotlight is open: route printable keys to query, Up/Down to selected, Esc/q to close, Enter to dispatch the selected action via context_menu_tmux_args / direct std::process::Command. When ContextMenu is open: shortcut letters (h, v, z, u, d, s, m, R, X) dispatch the same tmux subcommands as the POC, Esc/q to close. While an overlay is open, skip the dashboard tick repaint so the overlay doesn't flicker; resume tick on dismiss. Render order: dashboard first, then overlay on top via fleet_ui::overlay widgets.

File scope: rust/fleet-watcher/src/main.rs, rust/fleet-watcher/Cargo.toml

### Sub-task 4: Wire same keybindings into fleet-state + fleet-plan-tree + fleet-waves (depends on: 3)

Replicate the overlay wiring from fleet-watcher (subtask 3) into the other three view binaries: rust/fleet-state/src/main.rs, rust/fleet-plan-tree/src/main.rs, rust/fleet-waves/src/main.rs. Key behavior identical: `?` / `/` opens Spotlight, `m` opens ContextMenu, `Esc` / `q` dismisses, dashboard tick pauses while overlay is up. To avoid four-way duplication, consider lifting the overlay-state + key handler into a small helper in fleet-ui (e.g. `fleet_ui::overlay::OverlayController` with `handle_key(&mut self, k: KeyEvent) -> OverlayOutcome` and `render(&mut self, frame, area)`) and call it from each binary's event loop. If the helper proves over-engineered, just duplicate cleanly — three small repetitions are better than a premature abstraction.

File scope: rust/fleet-state/src/main.rs, rust/fleet-plan-tree/src/main.rs, rust/fleet-waves/src/main.rs, rust/fleet-ui/src/overlay.rs

### Sub-task 5: Soak test all four binaries against the live fleet + tick the openspec checkboxes (depends on: 4)

In a real codex-fleet tmux session, run each of the four view binaries in turn. For each: (a) confirm `?` opens Spotlight, type a filter, select an item, confirm the dispatched tmux command runs against the active pane, (b) confirm `m` opens the ContextMenu for the active pane, press a shortcut, confirm dispatch, (c) confirm `Esc` dismisses without dispatch. Capture before/after screenshots side-by-side with the bash dashboards to confirm visual parity holds when overlays are dismissed. Tick the three Phase 5 checkboxes in openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md: 'Context-menu overlay (bordered list popup)', 'Spotlight palette overlay …', 'Trigger keybindings wired in each view binary'. Include PR links in the bullet annotations. Once Phase 5 is done, leave a short note in the tasks.md hand-off block that Phase 6 (retire bash) is now unblocked.

File scope: openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md


## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
