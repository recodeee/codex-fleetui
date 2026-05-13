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

# ── multi-row layout: top + bottom accent rows are uniform dark fills ────────
# Earlier draft tried #{W:...} per-window fills with fixed-width literals to
# produce a "browser tab extending into accent rows" effect — but the literal
# widths didn't align with the variable-width tab labels and wiped the label
# row. Use uniform dark BG fills instead: tab strip still feels taller, but
# labels always survive.
#
# Tmux quirk: clear any inherited per-index status-format first, then only
# write the indices we actually want to customise. Indices >= HEIGHT are
# ignored by tmux but we unset them for cleanliness.
for idx in 0 1 2 3 4; do
  tmux set-option -u -g -t "$SESSION" "status-format[$idx]" >/dev/null 2>&1 || true
done

if [[ "$HEIGHT" -ge 2 ]]; then
  # Build a uniform-dark accent line — wide enough to cover any pane width.
  ACCENT_LINE="#[fg=#0a0a0a,bg=#0a0a0a]$(printf '%*s' 300 '')"
  # status-format[0] (top) = uniform dark padding above the tabs row.
  tmux set-option -g -t "$SESSION" "status-format[0]" "$ACCENT_LINE" >/dev/null
  # The "main" row carrying status-left + tab list + status-right is the LAST
  # row (closest to the panes). Leave its format default by NOT setting an
  # override — tmux falls back to its built-in template that honours
  # status-left / window-status-(current-)format / status-right.
  # For HEIGHT=3, also pad the row between top accent and tabs (status-format[1]).
  if [[ "$HEIGHT" -ge 3 ]]; then
    tmux set-option -g -t "$SESSION" "status-format[1]" "$ACCENT_LINE" >/dev/null
  fi
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
