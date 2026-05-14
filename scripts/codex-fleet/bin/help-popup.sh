#!/usr/bin/env bash
# help-popup.sh — iOS-style read-only popup listing every fleet keybinding.
#
# Bound to `prefix Ctrl+H` via scripts/codex-fleet/tmux-bindings.conf, and
# invoked from the pane right-click context menu's `?` / `Ctrl+H` rows.
# Two entry points, one screen, so the operator never has to remember
# which surface owns the binding.
#
# The popup is read-only: any keystroke (incl. esc/q) closes it. No
# dispatch — pressing `n` here does NOT spawn a worker; for that, dismiss
# this and open the matching menu/overlay.
#
# Glyph convention: Linux-friendly. Spotlight is `Ctrl K` (was ⌘K in the
# fleet-tui-poc displays before this PR), context-menu hotkeys are bare
# single letters, tmux bindings use the user's `prefix` (default Ctrl+B).
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=/dev/null
source "$SCRIPT_DIR/../lib/ios-menu.sh"

CARD_W=72
INNER_W=$(( CARD_W - 2 ))

# ── chrome helpers (mirror pane-context-menu.sh so the visual lineage is
#    identical and a future refactor can hoist these into ios-menu.sh) ──
draw_top() {
  _ios_sgr "$IOS_BG3" "$IOS_BG"
  printf '╭'; _ios_repeat '─' "$INNER_W"; printf '╮'
  _ios_reset; printf '\n'
}
draw_bottom() {
  _ios_sgr "$IOS_BG3" "$IOS_BG"
  printf '╰'; _ios_repeat '─' "$INNER_W"; printf '╯'
  _ios_reset; printf '\n'
}
draw_hairline() {
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_BG3" "$IOS_BG2"; _ios_repeat '─' "$INNER_W"
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}
draw_section() {
  local title="$1"
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_BLUE" "$IOS_BG2" bold; printf ' %-*s' "$(( INNER_W - 1 ))" "$title"
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}
# A help row is two halves: bold key chip on the left, plain label on the right.
draw_row() {
  local key="$1" label="$2"
  local key_w=14
  local label_w=$(( INNER_W - key_w - 3 ))
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_GRAY2" "$IOS_BG3"; printf ' '
  _ios_sgr "$IOS_WHITE" "$IOS_BG3" bold; printf '%-*s' "$key_w" "$key"
  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf '  '
  _ios_sgr "$IOS_WHITE" "$IOS_BG2"; printf '%-*s' "$label_w" "$label"
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}

# ── render ────────────────────────────────────────────────────────────────
clear
printf '\n'

draw_top
# Header row, same shape as the pane context menu so the popup feels like
# a sibling surface rather than a new screen.
_ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
_ios_sgr "$IOS_BLUE" "$IOS_BG2"; printf ' ⌨ '
_ios_sgr "$IOS_WHITE" "$IOS_BG2" bold; printf 'codex-fleet · keyboard help'
local_pad=$(( INNER_W - 28 - 4 ))
(( local_pad < 1 )) && local_pad=1
_ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf '%*s' "$local_pad" ''
_ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
_ios_reset; printf '\n'
draw_hairline

draw_section 'Pane right-click menu'
draw_row 'C'         'Copy whole session scrollback to clipboard'
draw_row 'c'         'Copy visible viewport to clipboard'
draw_row 'l'         'Copy the line under the cursor'
draw_row '<'         'Scroll the pane to the top of its history'
draw_row '>'         'Scroll the pane to the bottom of its history'
draw_row 'h / v'     'Split the pane horizontally / vertically'
draw_row 'z'         'Zoom or unzoom the active pane'
draw_row 'u / d'     'Swap with the pane above / below'
draw_row 's'         'Swap with the tmux-marked pane (mark first with m)'
draw_row 'm'         'Mark / unmark this pane'
draw_row 'R'         'Respawn the pane (kills, then restarts)'
draw_row 'X'         'Kill the pane'
draw_row '? / Ctrl+H' 'Open this help screen'
draw_hairline

draw_section 'tmux bindings (after the prefix, default Ctrl+B)'
draw_row 'prefix m'      'Open the iOS Action Sheet (tear-down / swap / retarget)'
draw_row 'prefix Tab'    'Open the Section Jump grid (Overview / Fleet / Plan / Waves / Review)'
draw_row 'prefix Ctrl+H' 'Open this help screen'
draw_hairline

draw_section 'Spotlight palette (inside fleet-tui-poc)'
draw_row 'Ctrl+K'    'Open the Spotlight command palette'
draw_row 'Ctrl+N'    'Spawn a new codex worker (Linux equivalent of macOS ⌘N)'
draw_row 'Ctrl+B'    'Switch worktree (Linux equivalent of macOS ⌘B)'
draw_row '↑ / ↓'     'Move the selection cursor'
draw_row '↵'         'Activate the highlighted item'
draw_row 'esc'       'Dismiss the palette'
draw_hairline

draw_section 'Section Jump grid'
draw_row '1 – 5'     'Jump to Overview, Fleet, Plan, Waves, or Review'
draw_row '↵'         'Open the highlighted section in the current pane'
draw_row 'esc'       'Dismiss the grid'

draw_bottom
_ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '\n   any key to close'
_ios_reset

# Read one char then exit. The popup harness closes when this script returns.
read -rsn1 -t 60 _ || :
clear
