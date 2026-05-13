#!/usr/bin/env bash
# style-tabs — apply browser-like tab styling to a codex-fleet tmux session.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh                # default session
#   CODEX_FLEET_SESSION=other bash scripts/.../style-tabs.sh
#
# Visual model:
#   ┌──────────────────────────────────────────────────────────────┐
#   │  ◆ codex-fleet ▌ │ 0  overview │ 1  fleet │▎ 2  plan  ✕ ▎│ … │  ● live  00:10:21
#   └──────────────────────────────────────────────────────────────┘
#
# - 2-line status (more vertical room) so the tab strip feels bigger
# - Active tab: bright fill BG, left/right ▎ caps, bold label, ✕ close indicator
# - Inactive tab: dim text, recessed dark BG
# - Hover analog: a tab with the `activity` flag (pane wrote since last view)
#   gets an orange highlight so background work pings
# - Tab separator: thin │ on dark BG (matches browser tab dividers)
set -eo pipefail

SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# Track changes so panes redraw the new bar immediately.
tmux set-option -g -t "$SESSION" monitor-activity on >/dev/null
tmux set-option -g -t "$SESSION" monitor-bell on     >/dev/null
tmux set-option -g -t "$SESSION" visual-activity off >/dev/null
tmux set-option -g -t "$SESSION" visual-bell     off >/dev/null

# Two-line status bar — top tabs row, bottom info row → larger "tab height" feel.
tmux set-option -g -t "$SESSION" status on             >/dev/null
tmux set-option -g -t "$SESSION" status 2              >/dev/null
tmux set-option -g -t "$SESSION" status-position top   >/dev/null
tmux set-option -g -t "$SESSION" status-interval 1     >/dev/null
tmux set-option -g -t "$SESSION" status-justify left   >/dev/null

# Base bar style (deep neutral so colored chips pop).
tmux set-option -g -t "$SESSION" status-style "bg=#0a0a0a,fg=#7a7a7a" >/dev/null

# ── status-left: session badge ───────────────────────────────────────────────
tmux set-option -g -t "$SESSION" status-left-length 36 >/dev/null
tmux set-option -g -t "$SESSION" status-left \
  "#[fg=#0a0a0a,bg=#e67e22,bold] ◆ #S #[fg=#e67e22,bg=#0a0a0a]▌ " >/dev/null

# ── status-right: live indicator + clock chip ────────────────────────────────
tmux set-option -g -t "$SESSION" status-right-length 64 >/dev/null
tmux set-option -g -t "$SESSION" status-right \
  "#[fg=#1a1a1a,bg=#0a0a0a]▐#[fg=#83c87e,bg=#1a1a1a,bold] ● live #[fg=#1a1a1a,bg=#0a0a0a]▌ #[fg=#bbbbbb,bg=#0a0a0a]#(date +%H:%M:%S) " >/dev/null

# ── tab separator (thin divider between inactive tabs) ───────────────────────
tmux set-option -g -t "$SESSION" window-status-separator \
  "#[fg=#2a2a2a,bg=#0a0a0a]│" >/dev/null

# ── inactive tab ─────────────────────────────────────────────────────────────
# Padded label, recessed BG, no close button. The closing ▎ on the BG flush
# right gives the recessed-pill effect without taking width.
tmux set-option -g -t "$SESSION" window-status-format \
  "#[fg=#5a5a5a,bg=#1a1a1a] #I  #W " >/dev/null
tmux set-option -g -t "$SESSION" window-status-style \
  "fg=#5a5a5a,bg=#1a1a1a" >/dev/null

# ── active tab ───────────────────────────────────────────────────────────────
# Bright accent fill, bold label, dedicated ✕ chip on a slightly hotter BG so
# the close indicator reads as a button. ▎ caps on both ends form the pill.
tmux set-option -g -t "$SESSION" window-status-current-format \
  "#[fg=#3a7ebf,bg=#0a0a0a]▎#[fg=#ffffff,bg=#3a7ebf,bold] #I  #W #[fg=#ffd07a,bg=#3a7ebf]  ✕ #[fg=#3a7ebf,bg=#0a0a0a]▎" >/dev/null
tmux set-option -g -t "$SESSION" window-status-current-style \
  "fg=#ffffff,bg=#3a7ebf,bold" >/dev/null

# ── activity / bell highlights (the "hover" analog) ──────────────────────────
# A pane with activity flag becomes orange so background work pings without
# stealing focus.
tmux set-option -g -t "$SESSION" window-status-activity-style \
  "fg=#ffaa55,bg=#241a10,bold" >/dev/null
tmux set-option -g -t "$SESSION" window-status-bell-style \
  "fg=#ff5555,bg=#2a0a0a,bold" >/dev/null

# ── pane border styling — keep it crisp under the tab strip ──────────────────
tmux set-option -g -t "$SESSION" pane-border-status top      >/dev/null
tmux set-option -g -t "$SESSION" pane-border-format \
  " #[fg=#ffd07a,bold]▭#[default] #[fg=#bbbbbb]#{@panel}#[default] " >/dev/null
tmux set-option -g -t "$SESSION" pane-border-style       "fg=#2a2a2a" >/dev/null
tmux set-option -g -t "$SESSION" pane-active-border-style "fg=#3a7ebf" >/dev/null

# ── message / mode styling (the popups that show when you press prefix) ──────
tmux set-option -g -t "$SESSION" message-style       "fg=#ffffff,bg=#3a7ebf,bold" >/dev/null
tmux set-option -g -t "$SESSION" message-command-style "fg=#ffffff,bg=#1a1a1a"    >/dev/null
tmux set-option -g -t "$SESSION" mode-style          "fg=#0a0a0a,bg=#ffd07a,bold" >/dev/null

# Force an immediate redraw so the new styling shows up before the next event.
tmux refresh-client -S -t "$SESSION" >/dev/null 2>&1 || true

echo "[style-tabs] applied browser-style tabs to session=$SESSION"
