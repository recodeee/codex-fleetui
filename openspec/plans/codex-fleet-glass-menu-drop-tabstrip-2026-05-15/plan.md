# Glass/transparent pane menu + drop the codex-fleet tab strip + overall design pass

Plan slug: `codex-fleet-glass-menu-drop-tabstrip-2026-05-15`

## Problem

The operator wants two visible design changes plus a polish pass. (1) The right-click pane action menu currently renders as a solid two-tone card (#1C1C1E inner / #2C2C2E border) that sits opaquely on top of the underlying pane. The operator wants it transparent / glassmorphic — the terminal's background (kitty supports background_opacity) should bleed through, with only hairline strokes, fg-only text, and a subtle iOS-blue underline on the focused row. (2) The 5-tab strip rendered at the top of the overview window by the standalone `rust/fleet-tab-strip` binary (spawned via `scripts/codex-fleet/overview-header.sh` and `scripts/codex-fleet/full-bringup.sh`) is to be removed entirely — including the binary, its workspace member, the spawn sites, and the downstream tmux pane-iteration excludes in `plan-watcher.sh` and `full-bringup.sh` that assume a `[codex-fleet-tab-strip]` panel exists. The dashboards already render their own iOS-chrome page headers (per the ios_page_design.rs modules merged in the previous plan), so the tab pane is now redundant. (3) An accompanying design pass: same transparent treatment for `help-popup.sh`, comment cleanup in `style-tabs.sh`, and removal of the now-orphaned `fleet_ui::tab_strip` module. The 8 lanes are flat-parallel — every lane edits exactly one disjoint file path (or one new disjoint set), depends_on=[] on all of them, so any fleet of 8 workers can claim and ship in parallel without `task_claim_file` contention. No shared helpers are added; each lane inlines the small bit of ANSI/transparency logic it needs.

## Acceptance Criteria

- All 8 sub-tasks land independently as 8 separate PRs against main with depends_on=[] honored; no two PRs touch the same file path.
- After Lane 0 ships, right-click on any codex-fleet pane renders the action menu with NO solid card background — the underlying pane's text shows through under the menu chrome (verified by sending Esc-prefixed paste of a known visible character before opening the menu). Hairline borders, title row, items, and shortcut chips all use fg-only ANSI; the focused row uses an iOS-blue underline rather than a fill.
- After Lane 1 ships, the help popup (prefix+Ctrl+H) renders with the same transparent treatment as Lane 0: hairline section dividers in #3A3A3C, section headers in iOS-blue fg, no solid card bg.
- After Lanes 2-5 ship, `bash scripts/codex-fleet/full-bringup.sh --n 4 --no-attach` brings up the overview window with NO `[codex-fleet-tab-strip]` pane at the top — the 4 workers occupy the full window. `tmux list-panes -t codex-fleet:0 -F '#{@panel}'` does not list the tab-strip panel. `plan-watcher.sh` no longer references it. `rust/Cargo.toml`'s `fleet-*` workspace glob no longer pulls in `fleet-tab-strip` because that directory is gone, and `cargo check --workspace` from `rust/` succeeds.
- After Lane 6 ships, `style-tabs.sh` no longer contains the 'in-binary tab strip is the navigation surface' branding in its echo / comments — the comments accurately describe the post-removal world (tmux status off by default, glass right-click menu is the canonical chrome layer).
- After Lane 7 ships, `rust/fleet-ui/src/lib.rs` no longer exports `pub mod tab_strip;`, `rust/fleet-ui/src/tab_strip.rs` is deleted, and `cargo check -p fleet-ui` succeeds.
- No regression in the existing dashboards: each `cargo check -p <crate>` for fleet-state / fleet-plan-tree / fleet-waves / fleet-watcher still passes after their respective lanes (none of those crates' main.rs are edited by this plan, so this should be a no-op verification).
- Every PR's final note records: branch, files changed, command + output evidence (cargo check / shellcheck / bringup smoke), PR URL, MERGED state, and sandbox cleanup proof per the Guardex completion contract.

## Roles

- [planner](./planner.md)
- [architect](./architect.md)
- [critic](./critic.md)
- [executor](./executor.md)
- [writer](./writer.md)
- [verifier](./verifier.md)

## Operator Flow

1. Refine this workspace until scope, risks, and tasks are explicit.
2. Publish the plan with `colony plan publish codex-fleet-glass-menu-drop-tabstrip-2026-05-15` or the `task_plan_publish` MCP tool.
3. Claim subtasks through Colony plan tools before editing files.
4. Close only when all subtasks are complete and `checkpoints.md` records final evidence.
