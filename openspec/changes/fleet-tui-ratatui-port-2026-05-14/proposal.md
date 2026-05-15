# Port codex-fleet dashboards from bash to a ratatui Rust binary

## Summary

The codex-fleet dashboards (`watcher-board.sh`, `fleet-state-anim.sh`,
`plan-tree-anim.sh`, `waves-anim*.sh`) render iOS-styled status surfaces by
combining bash, awk, jq, `tmux capture-pane` screen-scrapes, and hand-rolled
ANSI/SGR math. The behaviour is correct and visually polished but the
implementation is fragile: each script carries battle-scar comments about
flicker, wrap fragments, SGR truncation, regex ordering, and
capture-pane lag.

Port the dashboards (only — not the orchestration layer) to a single Rust
crate built on [ratatui](https://github.com/ratatui-org/ratatui) +
crossterm, delivered as four small binaries that drop into the existing
tmux windows.

The orchestration scripts (`full-bringup.sh`, `cap-swap-daemon.sh`,
`force-claim.sh`, `cap-probe.sh`, the styling helpers) stay bash. The
boundary is: anything that *renders a frame* moves to Rust; anything that
*spawns / probes / dispatches* stays bash.

## Why

1. **Maintenance tax.** The bash dashboards carry comments like "MUST run
   before {other-script}", "regex order matters", "clamp_lines_to_pane
   filter for SGR awareness", "flicker fix after tmux 3.6". Every new
   feature pays this tax. ratatui's diff renderer makes the whole class
   of bugs disappear.
2. **Already in Rust.** The parent project already has `codex-lb-runtime`
   and `codex-gpu-embedder`. A `fleet-tui` crate fits the existing
   workspace and toolchain.
3. **The iOS design is more expressive in Rust.** Image #2 (the
   spotlight / context-menu overlays) is achievable with tmux's
   `display-menu`, but ratatui can render them as real interactive
   widgets with section headers, right-aligned keybind hints, and a
   selected-row accent — surfaces that are clumsy to build in tmux.
4. **Typed data layer.** Every dashboard currently re-parses
   `plan.json`, `agent-auth list`, and tmux scrollback with scripts.
   Centralising those into `serde` structs eliminates duplicated regex.

## Out of scope

- The orchestration scripts stay bash. No part of `full-bringup.sh`,
  `cap-swap-daemon.sh`, `force-claim.sh`, `cap-probe.sh`,
  `colony-state-pump.sh`, `style-tabs.sh`, or `stall-watcher.sh` is
  re-implemented.
- Replacing tmux as the host. The Rust dashboards run *inside* tmux
  panes, decorated by `style-tabs.sh` chrome (rounded `@panel`
  borders, iOS tab strip). tmux remains the multiplexer.
- A unified multi-tab single-binary TUI. Future option but the
  four-small-binaries shape is chosen first because it migrates
  incrementally — see design.md "binary shape" for rationale.

## Status: NOT YET STARTED

This change is **captured but not in flight**. Implementation must defer
until the codex-fleet extraction PRs (in flight as of 2026-05-14:
`codex-fleet-extract-p1-source-env-in-launch`,
`parameterize-codex-fleet-for-extraction`, plus several agent
worktrees touching the dashboards) have merged and the agent
worktree count is back to 1.

Justification: memory rule `feedback_freeze_before_cross_cutting_reorg`
— T3 cross-cutting reorgs collide with parallel agent worktrees that
read/write the files being moved. This proposal renames the rendering
path for all four dashboards, which is exactly that shape.

## Validation gate before unfreeze

```bash
git -C ~/Documents/recodee worktree list | grep -c '^.*agent/'
# expected: 0 (or 1 if the implementer themselves opened it)
```

Once the gate passes, the work flows in phases (see design.md). Each
phase ships an independent PR; do **not** merge until the prior phase's
runtime evidence is recorded in tasks.md.

## Companion deliverable: POC (this PR)

This change also ships a single-file Rust POC under `rust/fleet-tui-poc/`.
The POC is **not** Phase 1. It is the experiment that must pass before
Phase 1 starts:

1. Truecolor (`Color::Rgb(0, 122, 255)` = systemBlue) renders inside
   tmux without colour-quantisation.
2. ratatui's `BorderType::Rounded` does not double-frame against tmux's
   `pane-border-status top` + `pane-border-format ' #[…] ▭ #{@panel} '`.
3. crossterm mouse events reach the binary through tmux's `mouse on`
   pass-through (verified by clicking the rendered chip and seeing
   coordinates in the event log).

If any of those three fail in the POC, Phase 1 doesn't start until the
failure mode is understood. The POC is throwaway; it is in
`rust/fleet-tui-poc/`, not `rust/fleet-tui/`, and the proposal explicitly
authorises deleting it once the production crate exists.
