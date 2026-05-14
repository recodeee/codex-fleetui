#!/usr/bin/env bash
# menu-action-sheet.sh — interactive iOS-style Action Sheet for tmux popup.
#
# Bound to Ctrl+B m via scripts/codex-fleet/tmux-bindings.conf.
# Renders an Action Sheet, reads a single keystroke, dispatches the action.
#
# Default options (override via CODEX_FLEET_ACTION_OPTIONS, pipe-delimited):
#   1. Tear down fleet        → bash scripts/codex-fleet/down.sh
#   2. Swap a pane            → user picks pane, runs cap-swap-daemon swap_pane
#   3. Retarget renderer      → user picks plan slug, runs plan-tree-pin.sh
#   4. Open watcher tab       → tmux select-window -t watcher
#   q. Cancel
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"

# shellcheck source=/dev/null
source "$SCRIPT_DIR/../lib/ios-menu.sh"

# Parse options (env override or defaults).
if [[ -n "${CODEX_FLEET_ACTION_OPTIONS:-}" ]]; then
  IFS='|' read -r -a OPTIONS <<< "$CODEX_FLEET_ACTION_OPTIONS"
else
  OPTIONS=(
    "Tear down fleet"
    "Swap a pane"
    "Retarget renderer at plan"
    "Open watcher tab"
  )
fi

# Render the sheet.
clear
ios_action_sheet "Fleet Actions" "${OPTIONS[@]}"
printf '\n'
_ios_sgr "$IOS_GRAY2" "$IOS_BG"; printf '  press 1-%d or q… ' "${#OPTIONS[@]}"
_ios_reset

# Read one char with 30s timeout.
read -rsn1 -t 30 choice || choice=q

# Dispatch.
clear
case "$choice" in
  1)
    ios_toast "⏻" "Tearing down fleet…" "$IOS_RED"
    printf '\n  Running: bash %s/scripts/codex-fleet/down.sh\n' "$REPO_ROOT"
    sleep 1
    bash "$REPO_ROOT/scripts/codex-fleet/down.sh"
    ;;
  2)
    ios_toast "⇄" "Swap a pane" "$IOS_BLUE"
    printf '\n  enter pane index (or window:pane): '
    read -r target
    if [[ -n "$target" ]]; then
      ios_toast "⇄" "Triggering cap-swap for $target" "$IOS_BLUE"
      # cap-swap-daemon swaps automatically when it detects a capped pane; we
      # nudge by sending SIGUSR1 if the daemon supports it, otherwise log.
      pkill -USR1 -f cap-swap-daemon.sh 2>/dev/null || printf '\n  cap-swap-daemon will pick up on next sweep cycle (~30s)\n'
    fi
    sleep 2
    ;;
  3)
    ios_toast "🎯" "Retarget renderer" "$IOS_BLUE"
    printf '\n  available plans:\n'
    for p in "$REPO_ROOT"/openspec/plans/*/plan.json; do
      slug="$(basename "$(dirname "$p")")"
      printf '    - %s\n' "$slug"
    done
    printf '\n  enter plan slug: '
    read -r slug
    if [[ -n "$slug" && -f "$REPO_ROOT/openspec/plans/$slug/plan.json" ]]; then
      bash "$REPO_ROOT/scripts/codex-fleet/plan-tree-pin.sh" "$slug"
      ios_toast "✓" "Pinned plan-tree to $slug" "$IOS_GREEN"
    else
      ios_toast "✕" "Slug not found: $slug" "$IOS_RED"
    fi
    sleep 2
    ;;
  4)
    tmux select-window -t "$SESSION:watcher" 2>/dev/null \
      || tmux select-window -t "$SESSION":5 2>/dev/null \
      || ios_toast "✕" "Watcher window not found" "$IOS_RED"
    ;;
  q|Q|'')
    ios_toast "○" "Cancelled" "$IOS_GRAY"
    sleep 1
    ;;
  *)
    ios_toast "?" "Unknown choice: $choice" "$IOS_ORANGE"
    sleep 1
    ;;
esac
