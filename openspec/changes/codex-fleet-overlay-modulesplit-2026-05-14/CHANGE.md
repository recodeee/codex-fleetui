---
base_root_hash: f43dddb0
slug: codex-fleet-overlay-modulesplit-2026-05-14
---

# CHANGE · codex-fleet-overlay-modulesplit-2026-05-14

## §P  proposal
# Split rust/fleet-ui/src/overlay.rs into per-widget module files for parallel evolvability

## Motivation

Phase 5 (codex-fleet-overlays-phase5-2026-05-14) lands all four iOS overlay widgets — ContextMenu, Spotlight, ActionSheet, SessionSwitcher — inlined into a single file at `rust/fleet-ui/src/overlay.rs`. That monolith ships a coherent first cut, but every subsequent widget tweak serializes behind a `task_claim_file` lock on overlay.rs. Four panes cannot edit four widgets concurrently; the fleet degenerates into a single-writer queue any time the overlays are in scope. This change splits each widget into its own module file so future widget evolution parallelizes across panes.

## Scope

Code-organization only. No behavior change, no visual change, no API change beyond the internal module path. All existing public re-exports (`fleet_ui::overlay::ContextMenu`, `fleet_ui::overlay::Spotlight`, etc.) continue to resolve. All four insta snapshots remain byte-identical.

## Acceptance criteria

- `rust/fleet-ui/src/overlay/context_menu.rs` exists and contains only the ContextMenu widget + its tests.
- `rust/fleet-ui/src/overlay/spotlight.rs` exists and contains only the Spotlight widget + SpotlightState + its tests.
- `rust/fleet-ui/src/overlay/action_sheet.rs` exists and contains only the ActionSheet widget + its tests.
- `rust/fleet-ui/src/overlay/session_switcher.rs` exists and contains only the SessionSwitcher widget + its tests.
- `rust/fleet-ui/src/overlay.rs` becomes a mod table plus shared helpers (`centered_overlay`, `render_overlay`, `card_shadow`) and re-exports — no widget bodies remain.
- `cargo test -p fleet-ui` is green; the four overlay snapshot tests pass byte-identical.

## Deltas

New files:
- `rust/fleet-ui/src/overlay/context_menu.rs` — ContextMenu widget + inline tests
- `rust/fleet-ui/src/overlay/spotlight.rs` — Spotlight + SpotlightState + inline tests
- `rust/fleet-ui/src/overlay/action_sheet.rs` — ActionSheet widget + inline tests
- `rust/fleet-ui/src/overlay/session_switcher.rs` — SessionSwitcher widget + inline tests

Modified files:
- `rust/fleet-ui/src/overlay.rs` — shrinks from full widget bodies down to `pub mod` declarations, shared helpers (`centered_overlay`, `render_overlay`, `card_shadow`), and re-exports.

## Sub-tasks

### Sub-task 0: Split ContextMenu into rust/fleet-ui/src/overlay/context_menu.rs

Create the new file and move the ContextMenu widget + inline tests out of overlay.rs. Add `pub mod context_menu;` + re-export to overlay.rs.

File scope: rust/fleet-ui/src/overlay/context_menu.rs, rust/fleet-ui/src/overlay.rs, rust/fleet-ui/tests/overlay_context_menu.rs

### Sub-task 1: Split Spotlight into rust/fleet-ui/src/overlay/spotlight.rs (depends_on_files: 0)

Create the new file and move Spotlight + SpotlightState out of overlay.rs. Shared overlay.rs claim serialized by `task_claim_file`.

File scope: rust/fleet-ui/src/overlay/spotlight.rs, rust/fleet-ui/tests/overlay_spotlight.rs

### Sub-task 2: Split ActionSheet into rust/fleet-ui/src/overlay/action_sheet.rs (depends_on_files: 0)

Create the new file and move ActionSheet out of overlay.rs. Shared overlay.rs claim serialized by `task_claim_file`.

File scope: rust/fleet-ui/src/overlay/action_sheet.rs, rust/fleet-ui/tests/overlay_action_sheet.rs

### Sub-task 3: Split SessionSwitcher into rust/fleet-ui/src/overlay/session_switcher.rs (depends_on_files: 0)

Create the new file and move SessionSwitcher out of overlay.rs. Shared overlay.rs claim serialized by `task_claim_file`.

File scope: rust/fleet-ui/src/overlay/session_switcher.rs, rust/fleet-ui/tests/overlay_session_switcher.rs

### Sub-task 4: Cleanup overlay.rs (depends_on_artifacts: 0, 1, 2, 3)

Verify all four `pub mod` entries are in place, delete any residual widget bodies, re-run `cargo test -p fleet-ui`. Trivial cleanup gate.

File scope: rust/fleet-ui/src/overlay.rs

## Risks

- **Snapshot test churn.** Even a stray whitespace difference between the source-moved-out-of-overlay.rs and the source-as-it-now-lives-in-context_menu.rs can shift formatting and break the insta snapshot. Mitigation: copy the widget body byte-for-byte, then run `cargo insta review` ONLY if a diff appears, and gate merge on zero unintended diffs.
- **Re-export breakage.** Downstream `use fleet_ui::overlay::ContextMenu;` paths must keep resolving. Mitigation: each sub-task adds the `pub use self::<modname>::<Widget>;` re-export when it moves the widget out.
- **Concurrent edits to overlay.rs.** Sub-0/1/2/3 all touch overlay.rs's mod table. Mitigation: `task_claim_file` on overlay.rs serializes writes; dispatch stays parallel via the `depends_on_files: [0]` hint.

## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
