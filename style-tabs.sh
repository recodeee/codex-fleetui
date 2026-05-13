#!/usr/bin/env bash
# style-tabs — apply browser-like tab styling to a codex-fleet tmux session.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh
#   CODEX_FLEET_SESSION=other bash scripts/.../style-tabs.sh
#
# Visual model (single-row strip):
#   ┌──────────────────────────────────────────────────────────────────────┐
#   │  ◆ codex-fleet ▌    0  overview    │▎   2  plan  ✖    ▎│ …    ● live │
#   └──────────────────────────────────────────────────────────────────────┘
#
# Design:
# - Single-row strip (status=1) — multi-row was attempted in earlier drafts
#   (#1826 / #1827) but tmux's status-format[N] requires explicit templates
#   for every row and the per-window-aligned accent fills kept overdrawing
#   the label row, wiping all tab labels. Sticking with a 1-row strip with
#   heavy inside-padding keeps labels reliably visible.
# - Active tab: bright blue fill, ▎ caps, bold label, heavy ✖ on amber chip
# - Inactive tab: dim label on recessed BG
# - Activity flag = "hover" analog: background pane writes → tab turns amber
set -eo pipefail

SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# tmux helper — set a global option (no session targeting; -t SESSION with the
# `status N` value gets misparsed as "session index N" on some tmux builds).
tx_set() { tmux set-option -g "$@" >/dev/null; }

# tmux helper — best-effort unset (silently ignore "no such option" errors).
tx_unset() { tmux set-option -gu "$@" >/dev/null 2>&1 || true; }

# Pre-unset any stale status-format[N] overrides from earlier script versions.
# Without this, broken array entries from previous runs blank the tab strip.
for idx in 0 1 2 3 4; do
  tx_unset "status-format[$idx]"
done

# Activity tracking — drives the orange "hover" highlight on background tabs.
tx_set monitor-activity on
tx_set monitor-bell on
tx_set visual-activity off
tx_set visual-bell off

tx_set status on
tx_set status-position top
tx_set status-interval 1
tx_set status-justify left

# Base bar style — deep neutral so colored chips pop.
tx_set status-style "bg=#0a0a0a,fg=#7a7a7a"

# ── status-left: session badge ───────────────────────────────────────────────
tx_set status-left-length 40
tx_set status-left "#[fg=#0a0a0a,bg=#e67e22,bold]  ◆ #S  #[fg=#e67e22,bg=#0a0a0a]▌ "

# ── status-right: live indicator + clock chip ────────────────────────────────
tx_set status-right-length 64
tx_set status-right \
  "#[fg=#1a1a1a,bg=#0a0a0a]▐#[fg=#83c87e,bg=#1a1a1a,bold]  ● live  #[fg=#1a1a1a,bg=#0a0a0a]▌ #[fg=#bbbbbb,bg=#0a0a0a]  #(date +%H:%M:%S)  "

# ── tab separator — thin divider between inactive tabs ───────────────────────
tx_set window-status-separator "#[fg=#2a2a2a,bg=#0a0a0a] "

# ── inactive tab — 3-space padding inside, recessed BG ───────────────────────
tx_set window-status-format \
  "#[fg=#1a1a1a,bg=#0a0a0a]#[fg=#7a7a7a,bg=#1a1a1a]   #I  #[fg=#aaaaaa]#W   #[fg=#1a1a1a,bg=#0a0a0a]"
tx_set window-status-style "fg=#7a7a7a,bg=#1a1a1a"

# ── active tab — bright fill, ▎ caps, bold label, heavy ✖ on amber chip ──────
tx_set window-status-current-format \
  "#[fg=#3a7ebf,bg=#0a0a0a]▎#[fg=#ffffff,bg=#3a7ebf,bold]   #I  #W   #[fg=#0a0a0a,bg=#ffd07a,bold] ✖ #[fg=#3a7ebf,bg=#0a0a0a]▎"
tx_set window-status-current-style "fg=#ffffff,bg=#3a7ebf,bold"

# ── activity / bell highlights — "hover" analog ──────────────────────────────
tx_set window-status-activity-style "fg=#ffaa55,bg=#241a10,bold"
tx_set window-status-bell-style     "fg=#ff5555,bg=#2a0a0a,bold"

# ── pane border styling — crisp under the tab strip ──────────────────────────
tx_set pane-border-status top
tx_set pane-border-format " #[fg=#ffd07a,bold]▭#[default] #[fg=#bbbbbb]#{@panel}#[default] "
tx_set pane-border-style        "fg=#2a2a2a"
tx_set pane-active-border-style "fg=#3a7ebf"

# ── message / mode styling ───────────────────────────────────────────────────
tx_set message-style         "fg=#ffffff,bg=#3a7ebf,bold"
tx_set message-command-style "fg=#ffffff,bg=#1a1a1a"
tx_set mode-style            "fg=#0a0a0a,bg=#ffd07a,bold"

# Force status height back to 1 in case a prior run set it higher.
tmux set-option -g status 1 >/dev/null 2>&1 || true

# Immediate redraw.
tmux refresh-client -S >/dev/null 2>&1 || true

echo "[style-tabs] applied browser-style tabs to session=$SESSION"
