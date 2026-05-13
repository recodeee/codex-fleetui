#!/usr/bin/env bash
# style-tabs — apply browser-like tab styling to a codex-fleet tmux session.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh                     # default height=3
#   STYLE_TABS_HEIGHT=1 bash scripts/codex-fleet/style-tabs.sh # compact single row
#   STYLE_TABS_HEIGHT=5 bash scripts/codex-fleet/style-tabs.sh # very tall (5 rows)
#
# Visual model (HEIGHT=3, default):
#   ┌──────────────────────────────────────────────────────────────────────┐
#   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  row 0  (dark padding)
#   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  row 1  (dark padding)
#   │  ◆ codex-fleet ▌    0  overview    │▎   2  plan  ✖    ▎│ …    ● live │  row 2  (tabs)
#   └──────────────────────────────────────────────────────────────────────┘
#
# How taller-than-default works without blanking the strip:
#   tmux's `status N` for N>1 makes status-format an array. Each row index
#   that is NOT explicitly set renders BLANK — tmux does not fall back to a
#   default template per-row. So we have to:
#     1. Set status-format[N-1] to tmux's full default template (which
#        honours status-left + window-status-format + status-right). This
#        is the row that draws the actual tab strip.
#     2. Set status-format[0..N-2] to a uniform dark BG fill so the extra
#        rows act as visual padding without overdrawing anything.
#
# Earlier drafts (#1826/#1827) tried to mirror the active-tab BG into the
# accent rows using #{W:F1,F2} with fixed-width literals — but tab labels
# vary in width, so the fills landed at wrong x positions and overdrew the
# label row. Hence: uniform dark BG for padding rows, no per-tab geometry.
set -eo pipefail

SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
HEIGHT="${STYLE_TABS_HEIGHT:-3}"
case "$HEIGHT" in 1|2|3|4|5) ;; *) HEIGHT=3 ;; esac

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# tmux helper — set a global option. NB: never use `-t SESSION` with options
# that accept a numeric value like `status` because some tmux builds parse
# the value as a session index and bail with "unknown value: 1".
tx_set() { tmux set-option -g "$@" >/dev/null; }
tx_unset() { tmux set-option -gu "$@" >/dev/null 2>&1 || true; }

# Wipe any stale status-format[N] entries from prior runs before configuring.
for idx in 0 1 2 3 4; do tx_unset "status-format[$idx]"; done

# Activity tracking — drives the orange "hover" highlight on background tabs.
tx_set monitor-activity on
tx_set monitor-bell on
tx_set visual-activity off
tx_set visual-bell off

tx_set status on
tx_set status-position top
tx_set status-interval 1
tx_set status-justify left

tx_set status-style "bg=#0a0a0a,fg=#7a7a7a"

# ── status-left: session badge ───────────────────────────────────────────────
tx_set status-left-length 40
tx_set status-left "#[fg=#0a0a0a,bg=#e67e22,bold]  ◆ #S  #[fg=#e67e22,bg=#0a0a0a]▌ "

# ── status-right: live indicator + clock chip ────────────────────────────────
tx_set status-right-length 64
tx_set status-right \
  "#[fg=#1a1a1a,bg=#0a0a0a]▐#[fg=#83c87e,bg=#1a1a1a,bold]  ● live  #[fg=#1a1a1a,bg=#0a0a0a]▌ #[fg=#bbbbbb,bg=#0a0a0a]  #(date +%H:%M:%S)  "

# ── tab separator ────────────────────────────────────────────────────────────
tx_set window-status-separator "#[fg=#2a2a2a,bg=#0a0a0a] "

# ── inactive tab — padded label, recessed BG ─────────────────────────────────
tx_set window-status-format \
  "#[fg=#1a1a1a,bg=#0a0a0a]#[fg=#7a7a7a,bg=#1a1a1a]   #I  #[fg=#aaaaaa]#W   #[fg=#1a1a1a,bg=#0a0a0a]"
tx_set window-status-style "fg=#7a7a7a,bg=#1a1a1a"

# ── active tab — bright fill, ▎ caps, bold label, heavy ✖ on amber chip ──────
tx_set window-status-current-format \
  "#[fg=#3a7ebf,bg=#0a0a0a]▎#[fg=#ffffff,bg=#3a7ebf,bold]   #I  #W   #[fg=#0a0a0a,bg=#ffd07a,bold] ✖ #[fg=#3a7ebf,bg=#0a0a0a]▎"
tx_set window-status-current-style "fg=#ffffff,bg=#3a7ebf,bold"

# ── activity / bell highlights ───────────────────────────────────────────────
tx_set window-status-activity-style "fg=#ffaa55,bg=#241a10,bold"
tx_set window-status-bell-style     "fg=#ff5555,bg=#2a0a0a,bold"

# ── pane border styling ──────────────────────────────────────────────────────
tx_set pane-border-status top
tx_set pane-border-format " #[fg=#ffd07a,bold]▭#[default] #[fg=#bbbbbb]#{@panel}#[default] "
tx_set pane-border-style        "fg=#2a2a2a"
tx_set pane-active-border-style "fg=#3a7ebf"

# ── message / mode styling ───────────────────────────────────────────────────
tx_set message-style         "fg=#ffffff,bg=#3a7ebf,bold"
tx_set message-command-style "fg=#ffffff,bg=#1a1a1a"
tx_set mode-style            "fg=#0a0a0a,bg=#ffd07a,bold"

# ── multi-row status: dark padding rows + one tabs row ───────────────────────
# tmux's default template for status-format[0] — replicated verbatim so the
# tab strip row honours status-left / window-status-format / status-right.
# Source: `tmux show-options -gv status-format[0]` from a default tmux build.
DEFAULT_TABS_FORMAT='#[align=left range=left #{E:status-left-style}]#[push-default]#{T;=/#{status-left-length}:status-left}#[pop-default]#[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{W:#[range=window|#{window_index} #{E:window-status-style}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-format}#[pop-default]#[norange default]#{?loop_last_flag,,#{window-status-separator}},#[range=window|#{window_index} list=focus #{?#{!=:#{E:window-status-current-style},default},#{E:window-status-current-style},#{E:window-status-style}}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-current-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{window-status-separator}}}#[nolist align=right range=right #{E:status-right-style}]#[push-default]#{T;=/#{status-right-length}:status-right}#[pop-default]#[norange default]'

# Uniform dark padding row — 400 spaces is wider than any reasonable terminal.
PADDING_ROW="#[fg=#0a0a0a,bg=#0a0a0a]$(printf '%*s' 400 '')"

# Set status height as a number (NOT via `-t SESSION` — that misparses the int).
tmux set-option -g status "$HEIGHT" >/dev/null

# Last row index = HEIGHT - 1. That's where we put the actual tab strip.
last_idx=$(( HEIGHT - 1 ))
tx_set "status-format[$last_idx]" "$DEFAULT_TABS_FORMAT"

# Fill rows 0..last_idx-1 with dark padding.
for ((i=0; i<last_idx; i++)); do
  tx_set "status-format[$i]" "$PADDING_ROW"
done

# Immediate redraw.
tmux refresh-client -S >/dev/null 2>&1 || true

echo "[style-tabs] applied browser-style tabs (height=$HEIGHT, tabs on row $last_idx) to session=$SESSION"
