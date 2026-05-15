---
base_root_hash: f43dddb0
slug: codex-fleet-glass-menu-drop-tabstrip-2026-05-15
---

# CHANGE · codex-fleet-glass-menu-drop-tabstrip-2026-05-15

## §P  proposal
# Glass/transparent pane menu + drop the codex-fleet tab strip + overall design pass

## Problem

The operator wants two visible design changes plus a polish pass. (1) The right-click pane action menu currently renders as a solid two-tone card (#1C1C1E inner / #2C2C2E border) that sits opaquely on top of the underlying pane. The operator wants it transparent / glassmorphic — the terminal's background (kitty supports background_opacity) should bleed through, with only hairline strokes, fg-only text, and a subtle iOS-blue underline on the focused row. (2) The 5-tab strip rendered at the top of the overview window by the standalone `rust/fleet-tab-strip` binary (spawned via `scripts/codex-fleet/overview-header.sh` and `scripts/codex-fleet/full-bringup.sh`) is to be removed entirely — including the binary, its workspace member, the spawn sites, and the downstream tmux pane-iteration excludes in `plan-watcher.sh` and `full-bringup.sh` that assume a `[codex-fleet-tab-strip]` panel exists. The dashboards already render their own iOS-chrome page headers (per the ios_page_design.rs modules merged in the previous plan), so the tab pane is now redundant. (3) An accompanying design pass: same transparent treatment for `help-popup.sh`, comment cleanup in `style-tabs.sh`, and removal of the now-orphaned `fleet_ui::tab_strip` module. The 8 lanes are flat-parallel — every lane edits exactly one disjoint file path (or one new disjoint set), depends_on=[] on all of them, so any fleet of 8 workers can claim and ship in parallel without `task_claim_file` contention. No shared helpers are added; each lane inlines the small bit of ANSI/transparency logic it needs.

## Acceptance criteria

- All 8 sub-tasks land independently as 8 separate PRs against main with depends_on=[] honored; no two PRs touch the same file path.
- After Lane 0 ships, right-click on any codex-fleet pane renders the action menu with NO solid card background — the underlying pane's text shows through under the menu chrome (verified by sending Esc-prefixed paste of a known visible character before opening the menu). Hairline borders, title row, items, and shortcut chips all use fg-only ANSI; the focused row uses an iOS-blue underline rather than a fill.
- After Lane 1 ships, the help popup (prefix+Ctrl+H) renders with the same transparent treatment as Lane 0: hairline section dividers in #3A3A3C, section headers in iOS-blue fg, no solid card bg.
- After Lanes 2-5 ship, `bash scripts/codex-fleet/full-bringup.sh --n 4 --no-attach` brings up the overview window with NO `[codex-fleet-tab-strip]` pane at the top — the 4 workers occupy the full window. `tmux list-panes -t codex-fleet:0 -F '#{@panel}'` does not list the tab-strip panel. `plan-watcher.sh` no longer references it. `rust/Cargo.toml`'s `fleet-*` workspace glob no longer pulls in `fleet-tab-strip` because that directory is gone, and `cargo check --workspace` from `rust/` succeeds.
- After Lane 6 ships, `style-tabs.sh` no longer contains the 'in-binary tab strip is the navigation surface' branding in its echo / comments — the comments accurately describe the post-removal world (tmux status off by default, glass right-click menu is the canonical chrome layer).
- After Lane 7 ships, `rust/fleet-ui/src/lib.rs` no longer exports `pub mod tab_strip;`, `rust/fleet-ui/src/tab_strip.rs` is deleted, and `cargo check -p fleet-ui` succeeds.
- No regression in the existing dashboards: each `cargo check -p <crate>` for fleet-state / fleet-plan-tree / fleet-waves / fleet-watcher still passes after their respective lanes (none of those crates' main.rs are edited by this plan, so this should be a no-op verification).
- Every PR's final note records: branch, files changed, command + output evidence (cargo check / shellcheck / bringup smoke), PR URL, MERGED state, and sandbox cleanup proof per the Guardex completion contract.

## Sub-tasks

### Sub-task 0: Glass pane action menu — transparent chrome, fg-only ANSI, iOS-blue focus underline

Rewrite scripts/codex-fleet/bin/pane-context-menu.sh so the right-click action menu renders as a transparent / glassmorphic surface. Required changes: (1) Stop emitting solid bg SGR escapes — change the chrome helpers (draw_top / draw_bottom / draw_hairline / draw_blank / draw_header / draw_item) so every cell uses ONLY a 38;2;R;G;B foreground escape with NO 48;2;R;G;B background. Inline this directly inside the script — DO NOT modify scripts/codex-fleet/lib/ios-menu.sh. (2) Hairline borders (top, bottom, section dividers) use fg=#3A3A3C (iOS separator gray). (3) Item rows use fg=#FFFFFF for label, fg=#8E8E93 for icon column, fg=#AEAEB2 for shortcut chip text. The bracketing '[ K ]' chip becomes plain '· K' or 'K' in dim gray since there is no longer a card surface to contrast against. (4) Header row: fg=#FFFFFF bold for 'pane N · %ID', fg=#34C759 for the leading dot, fg=#34C759 bold for ' LIVE ' (no bg). (5) Focus indicator: the script today only reads ONE keystroke and dispatches — there is no live focus loop — so the 'focus row' is the row whose hotkey was just pressed. Add a brief 80ms 'feedback flash' where the chosen row briefly underlines (fg sequence: \033[4m … \033[24m) in iOS-blue before dispatching, then clears the popup. (6) Danger row (Kill pane) keeps fg=#FF3B30. Disabled rows use fg=#48484A. (7) Update the bottom hint line to 'press a hotkey  ·  ? for help  ·  esc cancels' in fg=#8E8E93. (8) Add a top-of-file comment describing the transparency model and a smoke-test stanza: `printf 'hello-bg\nhello-bg\nhello-bg' && CODEX_FLEET_MENU_LINE=demo bash scripts/codex-fleet/bin/pane-context-menu.sh '%0' < /dev/null` and capture the rendered output via `script` or terminal recording to attach as evidence in the PR. file_scope is exactly this one file — do not touch ios-menu.sh, help-popup.sh, or any other file.

File scope: scripts/codex-fleet/bin/pane-context-menu.sh

### Sub-task 1: Glass help popup — same transparent treatment as the action menu

Rewrite scripts/codex-fleet/bin/help-popup.sh to match Lane 0's transparency model. Required: (1) Drop every 48;2;R;G;B bg SGR — chrome helpers (draw_top / draw_bottom / draw_hairline / draw_section / any item draw) emit fg only. (2) Section headers (e.g. 'TMUX', 'CONTEXT MENU', 'SPOTLIGHT', etc.) render in fg=#007AFF bold with NO bg. (3) Keybinding rows: left column (the keystroke) in fg=#FFCC00 bold (iOS yellow), right column (the description) in fg=#FFFFFF, separator dot in fg=#8E8E93. (4) Section dividers use fg=#3A3A3C hairlines. (5) Outer corner glyphs (╭ ╮ ╰ ╯) and edge bars (│ ─) also fg-only in #3A3A3C. (6) Bottom hint: 'any key closes  ·  esc/q exits' in fg=#8E8E93. (7) Keep all currently listed bindings verbatim — this lane is purely visual. (8) Add a self-contained smoke test stanza at the top of the script (commented) showing how to invoke it standalone: `bash scripts/codex-fleet/bin/help-popup.sh < /dev/null` with a 1-byte stdin to dismiss. (9) file_scope is exactly this one file.

File scope: scripts/codex-fleet/bin/help-popup.sh

### Sub-task 2: Drop fleet-tab-strip pane spawn from overview-header.sh

Make scripts/codex-fleet/overview-header.sh a backwards-compatible no-op. Required: (1) Keep the file in place (other scripts source it / call it by path) but the body should now: log 'overview-header: tab strip removed (see plan codex-fleet-glass-menu-drop-tabstrip-2026-05-15)', skip the binary location lookup, skip the split-window / select-pane / set-option calls, and exit 0. (2) Remove the BIN= lookup at lines 64-69, the warn at line 69, the split-window/select-pane block, and the `tmux set-option -p ... '@panel' '[codex-fleet-tab-strip]'` at line 91. Keep the `tmux list-panes -t "$target" -F '#{@panel}' | grep -qFx '[codex-fleet-tab-strip]'` idempotence guard as a comment + skip path (so re-run on an OLD session with the strip pane still alive doesn't double-add). (3) Add a top-of-file comment block explaining the removal and pointing to the plan slug. (4) shellcheck must remain clean: `shellcheck scripts/codex-fleet/overview-header.sh` exits 0. (5) file_scope is exactly this one file — DO NOT touch full-bringup.sh, plan-watcher.sh, or rust/fleet-tab-strip in this lane (other lanes own those files).

File scope: scripts/codex-fleet/overview-header.sh

### Sub-task 3: Drop fleet-tab-strip references from full-bringup.sh

Edit scripts/codex-fleet/full-bringup.sh to fully remove every reference to the tab-strip pane. Required: (1) Delete lines 458-468 (STRIP_BIN=… block, the if-exists guard, HEADER_CMD= path, the warn line). Whatever currently consumes HEADER_CMD afterwards must be removed too — trace the variable forward and drop the spawn site cleanly. (2) Delete line 516 (`awk -F'|' '$1 == "[codex-fleet-tab-strip]" { print $2; exit }'`) and update the surrounding logic so it no longer needs to identify a 'header pane' — the worker grid simply occupies all panes in the window. (3) Delete line 527's awk filter (`'$1 != "[codex-fleet-tab-strip]"'`) — replace with a pass-through that lists all panes. (4) Add an inline comment near each removal pointing at the plan slug. (5) Smoke test: `bash -n scripts/codex-fleet/full-bringup.sh` parses clean, and `shellcheck` exits 0. (6) A live smoke run on a clean repo (`tmux -L test kill-server 2>/dev/null; CODEX_FLEET_TMUX_SOCKET=test bash scripts/codex-fleet/full-bringup.sh --n 2 --no-attach`) must succeed and `tmux -L test list-panes -t codex-fleet:0 -F '#{@panel}'` must NOT contain '[codex-fleet-tab-strip]'. Capture that exact command + its stdout/stderr into the PR description. (7) file_scope is exactly this one file.

File scope: scripts/codex-fleet/full-bringup.sh

### Sub-task 4: Drop the tab-strip-panel exclusion in plan-watcher.sh

Edit scripts/codex-fleet/plan-watcher.sh: at line 134 (and any nearby lines that participate in the same loop) remove the `[ "$panel" = "[codex-fleet-tab-strip]" ] && continue` skip. Required: (1) Remove the conditional cleanly without breaking the surrounding while-read loop indentation. (2) Update the comment block at lines 127-130 ('the fleet-tab-strip header pane (panel == [codex-fleet-tab-strip])') to remove the bullet about excluding the header pane. (3) Verify with `bash -n scripts/codex-fleet/plan-watcher.sh` and `shellcheck scripts/codex-fleet/plan-watcher.sh`. (4) Run a 1-tick dry-run on the current session if any is up — but DO NOT start a long-running daemon. (5) file_scope is exactly this one file.

File scope: scripts/codex-fleet/plan-watcher.sh

### Sub-task 5: Delete the standalone fleet-tab-strip rust crate

Delete the entire `rust/fleet-tab-strip/` Cargo crate. Required: (1) Remove rust/fleet-tab-strip/Cargo.toml and rust/fleet-tab-strip/src/main.rs (`git rm` both). (2) The workspace `rust/Cargo.toml` uses a `"fleet-*"` glob, so no edit there is strictly required — but VERIFY by running `cargo metadata --no-deps --manifest-path rust/Cargo.toml | jq '.packages[].name'` before and after, and confirm `fleet-tab-strip` is gone post-delete and that no other crate failed to resolve (the binary's `use fleet_ui::tab_strip::{Tab, TabHit, TabStrip}` is the only consumer of that import path; with the binary gone, the orphaned `fleet_ui::tab_strip` module is handled by Lane 7). (3) Run `cargo check --workspace --manifest-path rust/Cargo.toml` and capture the output as PR evidence — expected: success. (4) Update `rust/Cargo.lock` only if cargo writes to it as part of the check (let cargo handle it; DO NOT hand-edit the lockfile beyond what cargo produces). (5) file_scope is exactly these two source files — DO NOT touch rust/Cargo.toml in this lane (the glob keeps it stable; if cargo absolutely requires an edit, propose a follow-up PR rather than expanding scope).

File scope: rust/fleet-tab-strip/Cargo.toml, rust/fleet-tab-strip/src/main.rs

### Sub-task 6: Clean up style-tabs.sh comments + branding post-tab-strip-removal

Edit scripts/codex-fleet/style-tabs.sh to remove dead branding about the in-binary tab strip. Required: (1) The current echo near the bottom contains 'in-binary tab strip is the navigation surface' as the default `status_state` description — update it to 'tmux status hidden — pane chrome is the navigation surface' (or similar accurate phrasing). (2) Find the long header docstring's reference to '`rust/fleet-ui::tab_strip`' as the in-binary tab strip and replace it with a note that the tab strip was removed in plan codex-fleet-glass-menu-drop-tabstrip-2026-05-15 and `CODEX_FLEET_TMUX_STATUS=on` re-enables the iOS-pill status bar for users who explicitly want chrome. (3) All tmux pill/format options stay in place — `style-tabs.sh` still configures status-style / window-status-current-format / etc. so the bar looks correct WHEN re-enabled. (4) `bash -n` and `shellcheck` must remain clean. (5) file_scope is exactly this one file — DO NOT touch overview-header.sh, full-bringup.sh, plan-watcher.sh, or any rust/ file.

File scope: scripts/codex-fleet/style-tabs.sh

### Sub-task 7: Remove orphaned fleet_ui::tab_strip module

Delete the now-orphaned tab_strip module from the fleet-ui crate. Required: (1) Remove the `pub mod tab_strip;` declaration from rust/fleet-ui/src/lib.rs (line 14 per current grep). (2) Delete rust/fleet-ui/src/tab_strip.rs with `git rm`. (3) Run `cargo check -p fleet-ui --manifest-path rust/Cargo.toml` and capture the output as PR evidence (expected: success — Lane 5 already removed the only consumer at rust/fleet-tab-strip/src/main.rs). (4) Run `cargo check --workspace --manifest-path rust/Cargo.toml` as a regression smoke (expected: success). (5) Do NOT touch any other file in rust/fleet-ui/ (the chip / card / overlay / spotlight_filter modules remain intact). (6) file_scope is exactly these two files — do NOT touch any binary crate's main.rs, scripts/, or workspace manifest in this lane.

File scope: rust/fleet-ui/src/lib.rs, rust/fleet-ui/src/tab_strip.rs


## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
