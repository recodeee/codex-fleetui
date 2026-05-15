#!/usr/bin/env bash
# pane-menu-copy-line.sh — copy CODEX_FLEET_MENU_LINE to the system clipboard
# and flash a tmux toast. Used by the MouseDown3Pane display-menu in
# style-tabs.sh — broken out into its own file because the alternative is
# embedding shell escapes inside a tmux `display-menu` item command, where
# tmux's own backslash/dollar handling silently eats the `\$` indirection
# (verified: `\$CODEX_FLEET_MENU_LINE` becomes an empty literal after parse).
#
# Routes the tmux call through scripts/codex-fleet/lib/_tmux.sh so the helper
# honors CODEX_FLEET_TMUX_SOCKET when set.
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../lib/_tmux.sh"

line="${CODEX_FLEET_MENU_LINE:-}"
printf '%s' "$line" | wl-copy
tmux display-message -d 1200 '─  line copied'
