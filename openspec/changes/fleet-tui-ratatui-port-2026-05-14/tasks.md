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

## Freeze gate (between Phase 0 and Phase 1)

- [ ] `git -C ~/Documents/recodee worktree list | grep -c 'agent/'` returns 0 (or 1 if the implementer)
- [ ] codex-fleet extraction PRs merged (`codex-fleet-extract-p1-source-env-in-launch`, `parameterize-codex-fleet-for-extraction`, and any active dashboard-touching agent worktrees)
- [ ] Repo decision recorded: do the `fleet-ui` / `fleet-data` / `fleet-*` crates live in **`recodeee/codex-fleet`** or in **`recodeee/recodee`**? (See proposal.md — current draft is ambiguous; both have been suggested.)

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
- [ ] `accounts.rs` — `agent-auth list` parser → `Account` struct; fixture-based test
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
