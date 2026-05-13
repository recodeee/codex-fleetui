#!/usr/bin/env bash
# style-tabs — apply browser-like tab styling to a codex-fleet tmux session.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh                  # default session
#   CODEX_FLEET_SESSION=other bash scripts/.../style-tabs.sh
#   STYLE_TABS_HEIGHT=3 bash scripts/.../style-tabs.sh      # taller strip (default)
#   STYLE_TABS_HEIGHT=2 bash scripts/.../style-tabs.sh      # compact strip
#
# Visual model (STYLE_TABS_HEIGHT=3 default):
#   ┌─────────────────────────────────────────────────────────────────────┐
#   │ ░░░░░░░░  top accent — active tab BG continues into this row  ░░░░░ │  row[0]
#   │  ◆ codex-fleet ▌ │    0  overview    │▎   2  plan  ✖    ▎│  …       │  row[1]
#   │ ░░░░░░░░  bottom accent — active tab BG continues into this row  ░░ │  row[2]
#   └─────────────────────────────────────────────────────────────────────┘
#
# Design:
# - 3-line status (default) so the tab strip feels much taller. Top + bottom
#   rows are filled to match the active tab BG color *above and below* the
#   active tab, creating a true browser-tab "extending down into content"
#   effect; inactive tabs stay recessed.
# - Tab label uses 3-space horizontal padding inside.
# - Close button uses heavy ✖ on its own amber chip so it reads as a button.
# - Activity flag = "hover" analog (background pane wrote → tab turns amber).
set -eo pipefail

SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
HEIGHT="${STYLE_TABS_HEIGHT:-3}"
case "$HEIGHT" in 1|2|3|4|5) ;; *) HEIGHT=3 ;; esac

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# Activity tracking — drives the orange "hover" highlight on background tabs.
tmux set-option -g -t "$SESSION" monitor-activity on >/dev/null
tmux set-option -g -t "$SESSION" monitor-bell on     >/dev/null
tmux set-option -g -t "$SESSION" visual-activity off >/dev/null
tmux set-option -g -t "$SESSION" visual-bell     off >/dev/null

tmux set-option -g -t "$SESSION" status on               >/dev/null
tmux set-option -g -t "$SESSION" status "$HEIGHT"         >/dev/null
tmux set-option -g -t "$SESSION" status-position top     >/dev/null
tmux set-option -g -t "$SESSION" status-interval 1       >/dev/null
tmux set-option -g -t "$SESSION" status-justify left     >/dev/null

# Base bar style — deep neutral so colored chips pop.
tmux set-option -g -t "$SESSION" status-style "bg=#0a0a0a,fg=#7a7a7a" >/dev/null

# ── status-left: session badge ───────────────────────────────────────────────
tmux set-option -g -t "$SESSION" status-left-length 40 >/dev/null
tmux set-option -g -t "$SESSION" status-left \
  "#[fg=#0a0a0a,bg=#e67e22,bold]  ◆ #S  #[fg=#e67e22,bg=#0a0a0a]▌ " >/dev/null

# ── status-right: live indicator + clock chip ────────────────────────────────
tmux set-option -g -t "$SESSION" status-right-length 64 >/dev/null
tmux set-option -g -t "$SESSION" status-right \
  "#[fg=#1a1a1a,bg=#0a0a0a]▐#[fg=#83c87e,bg=#1a1a1a,bold]  ● live  #[fg=#1a1a1a,bg=#0a0a0a]▌ #[fg=#bbbbbb,bg=#0a0a0a]  #(date +%H:%M:%S)  " >/dev/null

# ── tab separator — thin divider between inactive tabs ───────────────────────
tmux set-option -g -t "$SESSION" window-status-separator \
  "#[fg=#2a2a2a,bg=#0a0a0a] " >/dev/null

# ── inactive tab — 3-space padding inside, recessed BG ───────────────────────
tmux set-option -g -t "$SESSION" window-status-format \
  "#[fg=#1a1a1a,bg=#0a0a0a]#[fg=#7a7a7a,bg=#1a1a1a]   #I  #[fg=#aaaaaa]#W   #[fg=#1a1a1a,bg=#0a0a0a]" >/dev/null
tmux set-option -g -t "$SESSION" window-status-style \
  "fg=#7a7a7a,bg=#1a1a1a" >/dev/null

# ── active tab — bright accent fill, ▎ caps, bold label, heavy ✖ on amber chip
tmux set-option -g -t "$SESSION" window-status-current-format \
  "#[fg=#3a7ebf,bg=#0a0a0a]▎#[fg=#ffffff,bg=#3a7ebf,bold]   #I  #W   #[fg=#0a0a0a,bg=#ffd07a,bold] ✖ #[fg=#3a7ebf,bg=#0a0a0a]▎" >/dev/null
tmux set-option -g -t "$SESSION" window-status-current-style \
  "fg=#ffffff,bg=#3a7ebf,bold" >/dev/null

# ── 3-row layout: top accent row, label row, bottom accent row ───────────────
# The top + bottom rows continue the active-tab BG colour, so the active tab
# visually "extends" up and down (browser tab effect). Inactive tabs stay
# recessed (dim fill).
if [[ "$HEIGHT" -ge 3 ]]; then
  # status-format is an array. Row 0 is topmost when status-position=top.
  # The "main" row that uses status-left/status-right is the LAST one
  # (closest to the panes).
  # Row 0 (top accent) — same #{W:...} aware fill as the tabs row but content-empty
  tmux set-option -g -t "$SESSION" status-format[0] \
    "#[fg=#0a0a0a,bg=#e67e22]                                          #[bg=#0a0a0a]#[list=on align=left]#{W:#[bg=#0a0a0a] #[bg=#1a1a1a]                  #[bg=#0a0a0a] ,#[fg=#3a7ebf,bg=#0a0a0a]▎#[bg=#3a7ebf]                       #[fg=#3a7ebf,bg=#0a0a0a]▎}#[list=off]" >/dev/null

  # Row 2 (bottom accent) — same as top, mirrors the active-tab BG to make
  # the tab "merge" with the content area below.
  if [[ "$HEIGHT" -ge 3 ]]; then
    tmux set-option -g -t "$SESSION" status-format[2] \
      "#[fg=#0a0a0a,bg=#0a0a0a]                                          #[list=on align=left]#{W:#[bg=#0a0a0a] #[bg=#1a1a1a]                  #[bg=#0a0a0a] ,#[fg=#3a7ebf,bg=#0a0a0a]▎#[bg=#3a7ebf]                       #[fg=#3a7ebf,bg=#0a0a0a]▎}#[list=off]" >/dev/null
  fi

  # Middle row keeps tmux's default template (uses status-left/right + tabs).
  tmux set-option -u -g -t "$SESSION" status-format[1] >/dev/null 2>&1 || true
fi

# ── activity / bell highlights — "hover" analog ──────────────────────────────
tmux set-option -g -t "$SESSION" window-status-activity-style \
  "fg=#ffaa55,bg=#241a10,bold" >/dev/null
tmux set-option -g -t "$SESSION" window-status-bell-style \
  "fg=#ff5555,bg=#2a0a0a,bold" >/dev/null

# ── pane border styling — crisp under the tab strip ──────────────────────────
tmux set-option -g -t "$SESSION" pane-border-status top      >/dev/null
tmux set-option -g -t "$SESSION" pane-border-format \
  " #[fg=#ffd07a,bold]▭#[default] #[fg=#bbbbbb]#{@panel}#[default] " >/dev/null
tmux set-option -g -t "$SESSION" pane-border-style       "fg=#2a2a2a" >/dev/null
tmux set-option -g -t "$SESSION" pane-active-border-style "fg=#3a7ebf" >/dev/null

# ── message / mode styling ───────────────────────────────────────────────────
tmux set-option -g -t "$SESSION" message-style       "fg=#ffffff,bg=#3a7ebf,bold" >/dev/null
tmux set-option -g -t "$SESSION" message-command-style "fg=#ffffff,bg=#1a1a1a"    >/dev/null
tmux set-option -g -t "$SESSION" mode-style          "fg=#0a0a0a,bg=#ffd07a,bold" >/dev/null

# Immediate redraw.
tmux refresh-client -S -t "$SESSION" >/dev/null 2>&1 || true

echo "[style-tabs] applied browser-style tabs (height=$HEIGHT) to session=$SESSION"
