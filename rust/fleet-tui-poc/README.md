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

## Keys

| Key       | View                                                        |
|-----------|-------------------------------------------------------------|
| `0`, Esc  | Phase-0 validation harness (chip + event log) — the default |
| `1`       | iOS context menu over codex multi-pane backdrop             |
| `2`       | iOS spotlight palette over codex multi-pane backdrop        |
| `3`       | iOS action sheet over codex multi-pane backdrop             |
| `4`       | iOS session switcher (full-screen)                          |
| `q`       | Quit                                                        |

`0`/`Esc` from any overlay returns to the harness; from the harness it
quits.

## Expected output (default view)

```
╭ ◆  fleet-tui-poc  (1·2·3·4 palettes  ·  q quit) ────────────────────╮
│ ◖ ●   working   ◗                                                   │
│ click the chip; coords appear below. expect ✓ ON CHIP when…         │
│   (12, 4)  Down(Left)  ✓ ON CHIP                                    │
│   (3, 1)   Down(Left)  off chip                                     │
│                                                                     │
╰─────────────────────────────────────────────────────────────────────╯
```

The chip should look identical to the `[ working ]`-style chips that
`fleet-tick.sh` renders elsewhere — same hue, same caps.

## iOS palette previews (keys 1–4)

Each palette is a ratatui port of an artboard from the
`terminal-ios-style` design handoff (Claude Design, 2026-05-14). They
preview the surfaces Phase 5 of the openspec change owns
(`overlay.rs`), so the visual feasibility is checked before
`fleet-ui` scaffolds.

- **1 · Context menu** — UIKit long-press menu pinned to the active
  pane. Five sections (copy / search / split / swap / lifecycle),
  destructive Kill in `IOS_DESTRUCTIVE`, shortcut chips on the right.
- **2 · Spotlight** — search-first palette. Query line + systemBlue
  Top-Hit bar + three grouped result lists (Pane / Session / Fleet)
  with monospace shortcut chips.
- **3 · Action sheet** — bottom-anchored grouped sheet with a
  separate `Cancel` button — the iOS hallmark — in `IOS_TINT`.
- **4 · Session switcher** — app-switcher-style cards, one per codex
  worker. Active worker has the systemBlue border + LIVE badge; per
  card shows pane status, task, model · context · runtime, and a row
  of Focus / Queue / Pause / Kill actions.

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
