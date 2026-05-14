# Split rust/fleet-ui/src/overlay.rs into per-widget module files for parallel evolvability

Plan slug: `codex-fleet-overlay-modulesplit-2026-05-14`

## Problem

Phase 5 (codex-fleet-overlays-phase5-2026-05-14) lands ContextMenu, Spotlight, ActionSheet, and SessionSwitcher as four widgets inlined into a single file: rust/fleet-ui/src/overlay.rs. While that monolith ships a coherent first cut, every future widget tweak — a Spotlight UX change, a SessionSwitcher card refresh, a ContextMenu palette swap — serializes behind a `task_claim_file` lock on overlay.rs. Four panes cannot edit four widgets concurrently; the fleet collapses to a single-writer queue any time the overlays are in scope. This plan, which runs after phase5's sub-0 (ContextMenu widget) ships, splits overlay.rs into a module tree where each widget owns its own file (`overlay/context_menu.rs`, `overlay/spotlight.rs`, `overlay/action_sheet.rs`, `overlay/session_switcher.rs`) and overlay.rs shrinks to a `mod` table plus the shared helpers (`centered_overlay`, `render_overlay`, `card_shadow`). Result: future widget changes can parallelize across four panes because they no longer share a file claim.

## Acceptance Criteria

- rust/fleet-ui/src/overlay/context_menu.rs exists and contains ONLY the ContextMenu widget plus its unit tests.
- rust/fleet-ui/src/overlay/spotlight.rs exists and contains ONLY the Spotlight widget + SpotlightState plus its unit tests.
- rust/fleet-ui/src/overlay/action_sheet.rs exists and contains ONLY the ActionSheet widget plus its unit tests.
- rust/fleet-ui/src/overlay/session_switcher.rs exists and contains ONLY the SessionSwitcher widget plus its unit tests.
- rust/fleet-ui/src/overlay.rs becomes a module tree (`pub mod context_menu;`, `pub mod spotlight;`, `pub mod action_sheet;`, `pub mod session_switcher;`) plus shared helpers (centered_overlay, render_overlay, card_shadow) and re-exports — no widget bodies remain.
- All four existing insta snapshot tests (overlay_context_menu, overlay_spotlight, overlay_action_sheet, overlay_session_switcher) still pass after the move; snapshot .snap files move alongside their test files.

## Roles

- [planner](./planner.md)
- [architect](./architect.md)
- [critic](./critic.md)
- [executor](./executor.md)
- [writer](./writer.md)
- [verifier](./verifier.md)

## Operator Flow

1. Refine this workspace until scope, risks, and tasks are explicit.
2. Publish the plan with `colony plan publish codex-fleet-overlay-modulesplit-2026-05-14` or the `task_plan_publish` MCP tool.
3. Claim subtasks through Colony plan tools before editing files.
4. Close only when all subtasks are complete and `checkpoints.md` records final evidence.
