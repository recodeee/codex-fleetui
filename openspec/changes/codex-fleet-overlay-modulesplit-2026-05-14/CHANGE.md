---
base_root_hash: f43dddb0
slug: codex-fleet-overlay-modulesplit-2026-05-14
---

# CHANGE · codex-fleet-overlay-modulesplit-2026-05-14

## §P  proposal
# Split rust/fleet-ui/src/overlay.rs into per-widget module files for parallel evolvability

## Problem

Phase 5 (codex-fleet-overlays-phase5-2026-05-14) lands ContextMenu, Spotlight, ActionSheet, and SessionSwitcher as four widgets inlined into a single file: rust/fleet-ui/src/overlay.rs. While that monolith ships a coherent first cut, every future widget tweak — a Spotlight UX change, a SessionSwitcher card refresh, a ContextMenu palette swap — serializes behind a `task_claim_file` lock on overlay.rs. Four panes cannot edit four widgets concurrently; the fleet collapses to a single-writer queue any time the overlays are in scope. This plan, which runs after phase5's sub-0 (ContextMenu widget) ships, splits overlay.rs into a module tree where each widget owns its own file (`overlay/context_menu.rs`, `overlay/spotlight.rs`, `overlay/action_sheet.rs`, `overlay/session_switcher.rs`) and overlay.rs shrinks to a `mod` table plus the shared helpers (`centered_overlay`, `render_overlay`, `card_shadow`). Result: future widget changes can parallelize across four panes because they no longer share a file claim.

## Acceptance criteria

- rust/fleet-ui/src/overlay/context_menu.rs exists and contains ONLY the ContextMenu widget plus its unit tests.
- rust/fleet-ui/src/overlay/spotlight.rs exists and contains ONLY the Spotlight widget + SpotlightState plus its unit tests.
- rust/fleet-ui/src/overlay/action_sheet.rs exists and contains ONLY the ActionSheet widget plus its unit tests.
- rust/fleet-ui/src/overlay/session_switcher.rs exists and contains ONLY the SessionSwitcher widget plus its unit tests.
- rust/fleet-ui/src/overlay.rs becomes a module tree (`pub mod context_menu;`, `pub mod spotlight;`, `pub mod action_sheet;`, `pub mod session_switcher;`) plus shared helpers (centered_overlay, render_overlay, card_shadow) and re-exports — no widget bodies remain.
- All four existing insta snapshot tests (overlay_context_menu, overlay_spotlight, overlay_action_sheet, overlay_session_switcher) still pass after the move; snapshot .snap files move alongside their test files.

## Sub-tasks

### Sub-task 0: Split ContextMenu into rust/fleet-ui/src/overlay/context_menu.rs

Create rust/fleet-ui/src/overlay/context_menu.rs and move the ContextMenu struct, its impl, and its inline unit/snapshot tests out of overlay.rs into the new file. Update overlay.rs to add `pub mod context_menu;` and re-export `ContextMenu` so downstream `use fleet_ui::overlay::ContextMenu;` keeps working. If the snapshot test currently lives in rust/fleet-ui/tests/overlay_context_menu.rs, leave it in place; if any context-menu-specific test lived inline in overlay.rs, lift it to the new module file. Run `cargo test -p fleet-ui` and confirm the context_menu snapshot still matches.

File scope: rust/fleet-ui/src/overlay/context_menu.rs, rust/fleet-ui/src/overlay.rs, rust/fleet-ui/tests/overlay_context_menu.rs

### Sub-task 1: Split Spotlight into rust/fleet-ui/src/overlay/spotlight.rs

Create rust/fleet-ui/src/overlay/spotlight.rs and move Spotlight + SpotlightState + their impls + inline tests out of overlay.rs into the new file. If sub-0 has already added `pub mod spotlight;` to overlay.rs's mod table, no overlay.rs edit is needed here; otherwise add the mod line and the re-export. File-level claim on overlay.rs is shared with sub-0/2/3 — `task_claim_file` serializes the actual writes, but dispatch is parallel. Run `cargo test -p fleet-ui` and confirm overlay_spotlight snapshot still passes.

File scope: rust/fleet-ui/src/overlay/spotlight.rs, rust/fleet-ui/tests/overlay_spotlight.rs

### Sub-task 2: Split ActionSheet into rust/fleet-ui/src/overlay/action_sheet.rs

Create rust/fleet-ui/src/overlay/action_sheet.rs and move the ActionSheet widget + its inline tests out of overlay.rs into the new file. If sub-0 has already added `pub mod action_sheet;` to overlay.rs, no overlay.rs edit is needed here; otherwise add the mod line and the re-export. Shares overlay.rs with sub-0/1/3 via `task_claim_file`. Run `cargo test -p fleet-ui` and confirm overlay_action_sheet snapshot still passes.

File scope: rust/fleet-ui/src/overlay/action_sheet.rs, rust/fleet-ui/tests/overlay_action_sheet.rs

### Sub-task 3: Split SessionSwitcher into rust/fleet-ui/src/overlay/session_switcher.rs

Create rust/fleet-ui/src/overlay/session_switcher.rs and move the SessionSwitcher widget + its inline tests out of overlay.rs into the new file. If sub-0 has already added `pub mod session_switcher;` to overlay.rs, no overlay.rs edit is needed here; otherwise add the mod line and the re-export. Shares overlay.rs with sub-0/1/2 via `task_claim_file`. Run `cargo test -p fleet-ui` and confirm overlay_session_switcher snapshot still passes.

File scope: rust/fleet-ui/src/overlay/session_switcher.rs, rust/fleet-ui/tests/overlay_session_switcher.rs

### Sub-task 4: Cleanup overlay.rs — verify mod tree + remove any residual widget bodies (depends on: 0, 1, 2, 3)

Confirm rust/fleet-ui/src/overlay.rs now contains ONLY: (1) the four `pub mod` lines for context_menu/spotlight/action_sheet/session_switcher, (2) the shared helpers `centered_overlay`, `render_overlay`, `card_shadow`, and (3) any necessary re-exports. Delete any leftover widget code, dead imports, or stale tests. Run `cargo test -p fleet-ui` once more — all four overlay snapshots must still pass green. Final cleanup gate; trivial.

File scope: rust/fleet-ui/src/overlay.rs


## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
