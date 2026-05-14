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
   `plan.json`, `codex-auth list`, and tmux scrollback with scripts.
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

## Repo decision: codex-fleet only

The `fleet-ui`, `fleet-data`, and `fleet-*` binary crates live in
**`recodeee/codex-fleet`**, under the existing `rust/` tree alongside
the POC. They do **not** go into `recodeee/recodee`.

Rationale: codex-fleet was extracted to its own repo on 2026-05-14
specifically to decouple the worker-pool product from the recodee
monorepo. Putting the new Rust crates in recodee would re-couple
them — a fleet-only consumer would have to depend on the recodee
workspace. Keeping everything in codex-fleet preserves the extraction
and lets the fleet ship as an independent product.

Side effect: this **removes** the freeze-gate dependency on recodee's
`agent/*` worktree count. Workers operating on codex-fleet do not
collide with recodee worktrees (different repo, different file tree),
so implementation can start as soon as the POC validates its three
risks. The freeze rule still applies to any work that *also* touches
recodee (e.g. updating `~/Documents/recodee/scripts/codex-fleet/`
references), but the rust/ port itself is unblocked.

## Status: POC IN FLIGHT, PHASE 1 PENDING POC VALIDATION

Sequencing:

1. POC (this PR) — operator runs `cargo run --release -p fleet-tui-poc`
   inside a fleet tmux pane, ticks the three boxes in `tasks.md`.
2. Once POC validates, Phase 1 (the real `fleet-ui` crate) starts.
3. Phases 2–6 follow as documented in `design.md`.

If the POC reveals an unexpected failure mode (e.g. tmux quantises
truecolor on the operator's terminal, double-framing happens, mouse
events don't pass through), the design adapts before Phase 1 starts.

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
