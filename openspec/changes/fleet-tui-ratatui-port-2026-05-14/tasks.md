# Tasks — fleet-tui ratatui port

## Phase 0 — POC (THIS PR)

Status: in flight (this PR).

- [x] Scaffold `rust/fleet-tui-poc/` with `Cargo.toml` + `src/main.rs` + `README.md`
- [x] `cargo check -p fleet-tui-poc` passes
- [ ] Operator runs `cargo run -p fleet-tui-poc` inside a fleet tmux pane and confirms:
  - [ ] Truecolor systemBlue chip matches surrounding tmux iOS chrome (no quantisation)
  - [ ] No double-framing between `BorderType::Rounded` and tmux `pane-border-status`
  - [ ] Mouse-click on the chip logs coords (verifies crossterm receives tmux mouse events)

Once all three boxes are ticked by the operator, Phase 1 is unblocked.

## Gate between Phase 0 and Phase 1

- [x] **Repo decision LOCKED**: `fleet-ui` / `fleet-data` / `fleet-*` crates live in **`recodeee/codex-fleet`** (this repo). They do NOT go in `recodeee/recodee`. See proposal.md "Repo decision" section.
- [ ] POC three validation boxes ticked (above)

The recodee `agent/*` worktree freeze no longer gates this work — codex-fleet
is a separate repo, so the Rust port doesn't collide with recodee worktrees.

## Phase 1 — `fleet-ui` design system

- [ ] Scaffold `rust/fleet-ui/` library crate, register in workspace `Cargo.toml`
- [ ] `palette.rs` — port iOS colour consts from `scripts/codex-fleet/fleet-tick.sh`; snapshot test matches `SKILL.md` palette table
- [ ] `chip.rs` — port `ios_worker_chip` / `ios_status_chip_*`; snapshot test mirrors `test-status-chips.sh`
- [ ] `rail.rs` — port `ios_progress_rail` + `ios_axis_color`; snapshot test mirrors `test-progress-rails.sh`
- [ ] `card.rs` — `BorderType::Rounded` wrapper + 2-space padding; snapshot test
- [ ] `overlay.rs` — `centered_overlay` + `Clear` + bordered popup; snapshot test
- [ ] All `insta` snapshots reviewed + accepted

## Phase 2 — `fleet-data` typed data layer

- [ ] Scaffold `rust/fleet-data/` library crate, register in workspace `Cargo.toml`
- [ ] `plan.rs` — serde structs for `openspec/plans/*/plan.json`; `load(path)` + `newest_plan(repo_root)` helpers; fixture-based test
- [ ] `accounts.rs` — `codex-auth list` parser → `Account` struct; fixture-based test
- [ ] `panes.rs` — tmux pane introspection + `PaneState` enum; classification test against captured scrollback fixtures

## Phase 3 — `fleet-watcher` (proves the framework)

- [ ] Scaffold `rust/fleet-watcher/` binary crate
- [ ] Port `watcher-board.sh` end-to-end against `fleet-ui` + `fleet-data`
- [ ] Side-by-side screenshot in operator's actual fleet (bash vs Rust)
- [ ] 10-min soak test in a real fleet pane; identical `PaneState` classification for all 8 workers
- [ ] Phase 3 PR landed; Phase 4 unblocked

## Phase 4 — Remaining view binaries

- [ ] `rust/fleet-state/` — port `fleet-state-anim.sh` + render half of `fleet-tick.sh`
- [ ] `rust/fleet-plan-tree/` — port `plan-tree-anim.sh`, including Kahn topological levels
- [ ] `rust/fleet-waves/` — port `waves-anim*.sh`

## Phase 5 — Overlays

- [ ] Context-menu overlay (bordered list popup)
- [ ] Spotlight palette overlay (centered popup, `tui-input` text field, filtered list with section headers)
- [ ] Trigger keybindings wired in each view binary

## Phase 6 — Retire bash

- [ ] `full-bringup.sh` launches Rust binaries when `FLEET_DASHBOARD_RENDERER=rust` (default) or bash on fallback
- [ ] One release with both renderers; operator confirms parity
- [ ] Delete `*-anim.sh` and `watcher-board.sh`
- [ ] Update `skills/codex-fleet/SKILL.md` "Canonical visual design" table

## Out-of-scope follow-ups (separate plans, do not bundle here)

- Replace `tmux capture-pane` scraping with Colony presence-file signal
- Collapse the four binaries into one multi-tab `fleet-tui` binary
- Web/HTTP exporter that exposes the same state to a browser dashboard

## Wave-execution plan (for fleet workers)

Plan to be published to Colony as
`fleet-tui-ratatui-port-2026-05-14`. Workers operate on
**`recodeee/codex-fleet`** (clone, branch, PR — not recodee). All
paths below are relative to that repo root.

**Wave 1 — foundation (no deps)**

- `sub-0` Scaffold both library crates atomically: create
  `rust/fleet-ui/` + `rust/fleet-data/` (library crates), register
  both in a new workspace `Cargo.toml` at `rust/Cargo.toml`,
  declare empty modules in each `lib.rs`. Merged from the originally
  separate sub-0/sub-1 in the 15-prompt draft because both touched
  the same workspace `Cargo.toml` members line — a single subtask
  removes the only file-claim conflict in the entire fan-out.
  `cargo build` must pass with empty modules. **file_scope**: only
  `rust/Cargo.toml`, `rust/fleet-ui/Cargo.toml`,
  `rust/fleet-ui/src/lib.rs`, `rust/fleet-data/Cargo.toml`,
  `rust/fleet-data/src/lib.rs`.

**Wave 2 — `fleet-ui` design system (depends_on: 0)** — fan-out 5

- `sub-1` `palette.rs` — port iOS palette consts from
  `scripts/codex-fleet/fleet-tick.sh`. file_scope:
  `rust/fleet-ui/src/palette.rs` + snapshot test file.
- `sub-2` `chip.rs` — port `ios_worker_chip` / `ios_status_chip_*`.
  Mirror `scripts/codex-fleet/test-status-chips.sh` assertions in
  `insta`. depends_on: 1. file_scope: `rust/fleet-ui/src/chip.rs`.
- `sub-3` `rail.rs` — port `ios_progress_rail` + `ios_axis_color`.
  Mirror `test-progress-rails.sh`. depends_on: 1. file_scope:
  `rust/fleet-ui/src/rail.rs`.
- `sub-4` `card.rs` — `BorderType::Rounded` + 2-space padding +
  title slot. depends_on: 1. file_scope: `rust/fleet-ui/src/card.rs`.
- `sub-5` `overlay.rs` — `centered_overlay` + `Clear` + bordered
  popup. depends_on: 4. file_scope:
  `rust/fleet-ui/src/overlay.rs`.

**Wave 3 — `fleet-data` typed data layer (depends_on: 0)** — fan-out 3

- `sub-6` `plan.rs` — serde structs + `load(path)` +
  `newest_plan(repo_root)`. depends_on: 0. file_scope:
  `rust/fleet-data/src/plan.rs` + fixture.
- `sub-7` `accounts.rs` — `codex-auth list` parser → `Account`
  struct. depends_on: 0. file_scope:
  `rust/fleet-data/src/accounts.rs` + fixture.
- `sub-8` `panes.rs` — tmux pane introspection + `PaneState` enum.
  depends_on: 0. file_scope:
  `rust/fleet-data/src/panes.rs` + fixture.

**Wave 4 — view binaries (depends_on: full ui + data)** — fan-out 4

- `sub-9` `fleet-watcher` bin — port `watcher-board.sh` end-to-end.
  **First view; proves the framework.** depends_on: 2, 3, 4, 5, 7, 8.
  file_scope: `rust/fleet-watcher/`.
- `sub-10` `fleet-state` bin — port `fleet-state-anim.sh` + render
  half of `fleet-tick.sh`. depends_on: 2, 3, 4, 7, 8. file_scope:
  `rust/fleet-state/`.
- `sub-11` `fleet-plan-tree` bin — port `plan-tree-anim.sh`,
  including Kahn topological levels. depends_on: 2, 3, 4, 6, 8.
  file_scope: `rust/fleet-plan-tree/`.
- `sub-12` `fleet-waves` bin — port `waves-anim*.sh`. depends_on:
  2, 3, 4, 6. file_scope: `rust/fleet-waves/`.

**Wave 5 — overlays + bringup wiring (depends_on: all views)** — single

- `sub-13` Add the Image-2 context-menu + spotlight-palette overlays
  (using `overlay.rs`), update `scripts/codex-fleet/full-bringup.sh`
  to launch the four Rust binaries with bash fallback gated by
  `FLEET_DASHBOARD_RENDERER`, update `skills/codex-fleet/SKILL.md`
  "Canonical visual design" table. depends_on: 5, 9, 10, 11, 12.
  file_scope: `scripts/codex-fleet/full-bringup.sh`,
  `skills/codex-fleet/SKILL.md`, overlay-trigger code inside each
  bin (added — not modified-then-claimed; bin authors own their
  module trees).

Maximum concurrency profile:

```
W1 = 1, W2 = 5, W3 = 3, W4 = 4, W5 = 1
```

W2 and W3 run simultaneously (8 workers, no claim collisions —
disjoint file_scope). W4 starts when both drain. The fleet's
existing `force-claim.sh` + `plan_claim_subtask` honours the
`depends_on` field directly.
