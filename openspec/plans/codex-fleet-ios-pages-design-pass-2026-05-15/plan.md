# iOS background chrome + design polish + live indicators across all 5 fleet pages

Plan slug: `codex-fleet-ios-pages-design-pass-2026-05-15`

## Problem

All 5 fleet pages (fleet/waves/review/watcher/plan) already partially use the iOS palette (#007AFF · #34C759 · #FF3B30 · #FF9500) in their pills, chips, and status badges — but the PAGE-LEVEL chrome is still plain-black with no rounded card background, inconsistent header treatment, no live indicators that visually signal the data is fresh, and (on fleet) huge unused right-side space because ACTIVE/RESERVE stack vertically inside a fixed ~85-col card. The 'iOS palette · rounded cards' note in the watcher footer hints at the intent but the page bodies haven't been wrapped in iOS card chrome. This plan opens 5 parallel lanes — one per page — each adding a new <page>_ios_design.rs ratatui module that delivers (a) an iOS-styled outer page card with palette gradient + 1px rounded border, (b) a consistent header strip matching the watcher footer palette stripe, (c) refined inner hierarchy and column-fill so no pane wastes width, and (d) an animated LiveIndicator sub-widget that pulses on tick. Wiring each new module into its crate's main.rs is intentionally OUT OF SCOPE here — that ships in a follow-up integration plan once all 5 land, so the 5 lanes have zero file_scope contention.

## Acceptance Criteria

- All 5 tasks have depends_on=[] and file_scope disjoint — no two tasks list the same file.
- Each task lands a single NEW .rs module file inside its target crate; no edits to existing main.rs / lib.rs / shared helpers in this plan.
- Each module exports a public IosPageDesign ratatui Widget whose render(frame, area, state) consistently uses the iOS palette (#007AFF accent, #34C759 success, #FF3B30 destructive, #FF9500 warning) for status pills, headings, and dividers.
- Each module embeds a public LiveIndicator sub-widget that takes a tick counter / Instant and renders an animated state — pulsing dot when fresh (<2s since last update), steady when idle, fade/dim when stale (>10s).
- Each new module compiles standalone via cargo check -p <crate> and ships one insta snapshot test for the fresh-tick state and one for the stale-tick state.
- Each subtask description points at the matching reference screenshot so the agent can match palette/spacing/hierarchy intent.

## Roles

- [planner](./planner.md)
- [architect](./architect.md)
- [critic](./critic.md)
- [executor](./executor.md)
- [writer](./writer.md)
- [verifier](./verifier.md)

## Operator Flow

1. Refine this workspace until scope, risks, and tasks are explicit.
2. Publish the plan with `colony plan publish codex-fleet-ios-pages-design-pass-2026-05-15` or the `task_plan_publish` MCP tool.
3. Claim subtasks through Colony plan tools before editing files.
4. Close only when all subtasks are complete and `checkpoints.md` records final evidence.
