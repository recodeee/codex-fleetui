---
base_root_hash: f43dddb0
slug: codex-fleet-ios-pages-design-pass-2026-05-15
---

# CHANGE · codex-fleet-ios-pages-design-pass-2026-05-15

## §P  proposal
# iOS background chrome + design polish + live indicators across all 5 fleet pages

## Problem

All 5 fleet pages (fleet/waves/review/watcher/plan) already partially use the iOS palette (#007AFF · #34C759 · #FF3B30 · #FF9500) in their pills, chips, and status badges — but the PAGE-LEVEL chrome is still plain-black with no rounded card background, inconsistent header treatment, no live indicators that visually signal the data is fresh, and (on fleet) huge unused right-side space because ACTIVE/RESERVE stack vertically inside a fixed ~85-col card. The 'iOS palette · rounded cards' note in the watcher footer hints at the intent but the page bodies haven't been wrapped in iOS card chrome. This plan opens 5 parallel lanes — one per page — each adding a new <page>_ios_design.rs ratatui module that delivers (a) an iOS-styled outer page card with palette gradient + 1px rounded border, (b) a consistent header strip matching the watcher footer palette stripe, (c) refined inner hierarchy and column-fill so no pane wastes width, and (d) an animated LiveIndicator sub-widget that pulses on tick. Wiring each new module into its crate's main.rs is intentionally OUT OF SCOPE here — that ships in a follow-up integration plan once all 5 land, so the 5 lanes have zero file_scope contention.

## Acceptance criteria

- All 5 tasks have depends_on=[] and file_scope disjoint — no two tasks list the same file.
- Each task lands a single NEW .rs module file inside its target crate; no edits to existing main.rs / lib.rs / shared helpers in this plan.
- Each module exports a public IosPageDesign ratatui Widget whose render(frame, area, state) consistently uses the iOS palette (#007AFF accent, #34C759 success, #FF3B30 destructive, #FF9500 warning) for status pills, headings, and dividers.
- Each module embeds a public LiveIndicator sub-widget that takes a tick counter / Instant and renders an animated state — pulsing dot when fresh (<2s since last update), steady when idle, fade/dim when stale (>10s).
- Each new module compiles standalone via cargo check -p <crate> and ships one insta snapshot test for the fresh-tick state and one for the stale-tick state.
- Each subtask description points at the matching reference screenshot so the agent can match palette/spacing/hierarchy intent.

## Sub-tasks

### Sub-task 0: fleet page · iOS bg + width-fill + live indicator

Create rust/fleet-state/src/ios_page_design.rs implementing the full iOS-styled fleet cockpit page. Match the existing fleet cockpit content (ACTIVE / RESERVE / FOOTER sections currently rendered by scripts/codex-fleet/fleet-tick.sh into /tmp/claude-viz/live-fleet-state.txt) but: (1) wrap the page in a rounded iOS outer card with the palette gradient header, (2) lay out ACTIVE | RESERVE as two SIDE-BY-SIDE cards at panel width >= 180 cols (vertical fallback below 180) so the cockpit fills the pane instead of leaving the right ~60% black, (3) fix the white-bar artifact in the WORKING ON column by computing it from the actual claimed-task text, not a fixed-width sentinel, (4) add a LiveIndicator pulsing dot at the header that ticks every 1s. Snapshot tests: wide layout (200 cols), narrow layout (90 cols), fresh tick, stale tick. Do NOT touch fleet-state/main.rs or scripts/codex-fleet/fleet-tick.sh in this plan.

File scope: rust/fleet-state/src/ios_page_design.rs

### Sub-task 1: waves page · iOS bg + spawn-timeline polish + live indicator

Create rust/fleet-waves/src/ios_page_design.rs implementing the iOS-styled waves spawn-timeline page. Match Image #22's content (W1..Wn wave cards each showing N tasks, done|partial|waiting state, progress rails) but wrap the whole page in a rounded iOS outer card with a header strip showing plan slug + 'parallel execution · live' badge + LiveIndicator. Wave cards become consistent iOS rounded cards with palette-mapped status: done=#34C759, partial=#FF9500, waiting=#FF3B30 outline, ready=#007AFF. Inner progress rails use the iOS green-fill style. Add a MAX PARALLEL / claimed / done / available counters strip in a single rounded sub-card. Snapshot tests: 8-wave layout, fresh+stale ticks. Do NOT touch fleet-waves/main.rs or scripts/codex-fleet/waves-anim-generic.sh.

File scope: rust/fleet-waves/src/ios_page_design.rs

### Sub-task 2: review page · iOS bg + approval queue polish + live indicator

Create rust/fleet-ui/src/review_ios_page_design.rs implementing the iOS-styled review approval queue page. Match Image #24's content (REV-xxx card with risk/auth pills, AUTO-REVIEWER RATIONALE block, FILES TOUCHED list, A/V/D action row, Recent decisions sidebar) but wrap in rounded iOS outer card with palette stripe. The risk pill colors must map: low=#34C759, medium=#FF9500, high=#FF3B30; auth pill uses #007AFF for high, #34C759 for low. Recent-decisions chips use the same mapping. Add a LiveIndicator pulsing at the queue header showing 'auto-reviewer on · last <Ns>'. Snapshot tests: pending REV card present, empty queue, fresh+stale ticks. Do NOT touch fleet-ui/lib.rs or any existing fleet-ui review code in this plan.

File scope: rust/fleet-ui/src/review_ios_page_design.rs

### Sub-task 3: watcher page · iOS bg + cap-pool polish + live indicator

Create rust/fleet-watcher/src/ios_page_design.rs implementing the iOS-styled fleet-watcher page. Match Image #25's content exactly (FLEET WATCHER header, PANES/CAPPED/SWAPPED/RANKED 4-up summary cards, ACCOUNT POOL healthy/capped pills, FLEET PANES table, CAP POOL burned-accounts table with reset ETA, RECENT ACTIVITY log) but wrap in rounded iOS outer card with the palette footer stripe already mentioned in the bottom of Image #25 ('iOS palette · #007AFF / #34C759 / #FF3B30 / #FF9500 · rounded cards'). All status pills use the canonical palette: working=#007AFF, idle=gray, approval=#FF9500, capped/exhausted=#FF3B30. Add a LiveIndicator near 'last sweep <ts> · next in <Ns>' that pulses on each sweep. Snapshot tests: idle fleet, with-capped, fresh+stale ticks. Do NOT touch fleet-watcher/main.rs or scripts/codex-fleet/watcher-board.sh.

File scope: rust/fleet-watcher/src/ios_page_design.rs

### Sub-task 4: plan page · iOS bg + active-now + wave-strip + recent-merges polish + live indicator

Create rust/fleet-plan-tree/src/ios_page_design.rs implementing the iOS-styled plan-tree page. Match Image #21's content (PLAN TREE header showing plan slug + N done/claimed/available, ACTIVE NOW agents-on-Colony-tasks list, WAVES W1->Wn strip with working/done/idle pills, RECENT MERGES git-log-oneline list at bottom) but wrap in rounded iOS outer card. ACTIVE NOW rows become palette-mapped: working=#007AFF dot, idle=gray, done=#34C759. WAVES strip pills follow waves page palette. RECENT MERGES list gets a subtle iOS-style left border accent (#007AFF). Add a LiveIndicator near the PLAN TREE header pulsing every 1s when fleet-data refresh is live. Snapshot tests: small plan (3 tasks), large plan (18 tasks), fresh+stale ticks. Do NOT touch fleet-plan-tree/main.rs or scripts/codex-fleet/plan-tree-anim.sh.

File scope: rust/fleet-plan-tree/src/ios_page_design.rs


## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
