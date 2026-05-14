# fleet-tui-poc

**Throwaway.** This crate exists to validate three things before the
`fleet-ui` production crate scaffolds. Once `fleet-ui` lands in
`rust/fleet-ui/`, delete this directory.

## What it validates

See `openspec/changes/fleet-tui-ratatui-port-2026-05-14/design.md`
"Phase 0 — POC". Three risks:

1. **Truecolor inside tmux** — `Color::Rgb(0, 122, 255)` should render
   as the same systemBlue your `style-tabs.sh` active tab uses. If it
   banding-quantises to a 256-colour cube, your terminal isn't passing
   `RGB` capability through tmux. Check `tmux -V` (need ≥ 3.2) and
   `terminal-overrides` in `~/.tmux.conf` (current config has
   `xterm-kitty:RGB`).
2. **No double-framing** — the rounded `╭─╮╰─╯` card must not stack on
   top of tmux's `pane-border-status top` decoration. The POC pane
   should show one rounded border, not two concentric ones.
3. **Mouse pass-through** — clicking the blue chip should append a
   coordinate line tagged `✓ ON CHIP` to the event log. If no events
   arrive at all, tmux's `mouse on` isn't passing them to the binary;
   if events arrive but the `ON CHIP` tag never matches, ratatui's
   coordinate space differs from what we assume.

## Run

```bash
cd rust/fleet-tui-poc
cargo run --release
```

Inside a tmux pane of the fleet session (so you can compare against
the surrounding iOS chrome side-by-side):

```bash
tmux split-window -h -t codex-fleet:overview "cd $(pwd) && cargo run --release"
```

Press `q` or `Esc` to exit.

## Expected output

```
╭ ◆  fleet-tui-poc  (press q to quit) ───────────────────────────────╮
│ ◖ ●   working   ◗                                                   │
│ click the chip; coords appear below. expect ✓ ON CHIP when…         │
│   (12, 4)  Down(Left)  ✓ ON CHIP                                    │
│   (3, 1)   Down(Left)  off chip                                     │
│                                                                     │
╰─────────────────────────────────────────────────────────────────────╯
```

The chip should look identical to the `[ working ]`-style chips that
`fleet-tick.sh` renders elsewhere — same hue, same caps.

## What "validated" looks like

When the operator runs this and confirms in the OpenSpec change's
`tasks.md` Phase 0 checklist:

- truecolor matches the iOS-chrome systemBlue (no banding)
- no double-frame between rounded card and tmux pane border
- mouse clicks land coordinates with `✓ ON CHIP` tag

…then the freeze gate condition can be checked (`git worktree list |
grep -c agent/` returns 0) and Phase 1 (the real `fleet-ui` crate) is
clear to start.

## What "failed" looks like

- Banding → reconfigure terminal `terminal-overrides`, retry; if it
  still bands inside tmux but not bare, the design has to fall back
  to 256-colour palette (acceptable, slight aesthetic loss).
- Double-frame → `style-tabs.sh` `pane-border-status` has to be turned
  off for Rust-rendered panes, gated by env var.
- No mouse events → revisit whether tmux's `mouse on` propagates `SGR`
  mouse events to apps inside a pane; this is solvable but adds a
  design constraint.

Record the outcome in `openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md`.
