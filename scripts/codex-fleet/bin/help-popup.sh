#!/usr/bin/env bash
# help-popup.sh — iOS-style read-only popup listing every fleet keybinding.
#
# Bound to `prefix Ctrl+H` via scripts/codex-fleet/tmux-bindings.conf, and
# invoked from the pane right-click context menu's `?` / `Ctrl+H` rows.
# Two entry points, one screen, so the operator never has to remember
# which surface owns the binding.
#
# Transparency model: this script mirrors pane-context-menu.sh and emits
# foreground-only ANSI (`38;2;R;G;B`). When tmux popup is launched with
# `-B` (no border), the underlying pane shows through. Do NOT use solid
# background SGR (`48;2;...`) anywhere in this file.
#
# Glyph convention: Linux-friendly. Spotlight is `Ctrl K` (was ⌘K in the
# fleet-tui-poc displays before this PR), context-menu hotkeys are bare
# single letters, tmux bindings use the user's `prefix` (default Ctrl+B).
#
# Smoke test (standalone, one byte dismisses after render):
#   printf q | bash scripts/codex-fleet/bin/help-popup.sh
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=/dev/null
source "$SCRIPT_DIR/../lib/ios-menu.sh"

CARD_W=72
INNER_W=$(( CARD_W - 2 ))
IOS_SEPARATOR="#3A3A3C"

# ── fg-only SGR helper (mirrors pane-context-menu.sh::menu_fg) ────────────
# Emits truecolor foreground only. Background stays at the terminal default
# so tmux popup -B lets the pane show through (glass / transparent look).
menu_fg() {
  local fg="$1"; shift || true
  local hexfg="${fg#\#}"
  local r g b out=""

  r=$((16#${hexfg:0:2})); g=$((16#${hexfg:2:2})); b=$((16#${hexfg:4:2}))
  out+="\033[38;2;${r};${g};${b}m"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      bold) out+="\033[1m" ;;
      dim) out+="\033[2m" ;;
      italic) out+="\033[3m" ;;
      underline) out+="\033[4m" ;;
    esac
    shift
  done
  printf '%b' "$out"
}

menu_repeat() {
  local char="$1" count="$2" i
  for (( i=0; i<count; i++ )); do
    printf '%s' "$char"
  done
}

clear_popup() {
  clear 2>/dev/null || printf '\033[H\033[2J'
}

# ── chrome helpers (visual lineage matches pane-context-menu.sh) ───────────
draw_top() {
  menu_fg "$IOS_SEPARATOR"
  printf '╭'; menu_repeat '─' "$INNER_W"; printf '╮'
  _ios_reset; printf '\n'
}
draw_bottom() {
  menu_fg "$IOS_SEPARATOR"
  printf '╰'; menu_repeat '─' "$INNER_W"; printf '╯'
  _ios_reset; printf '\n'
}
draw_hairline() {
  menu_fg "$IOS_SEPARATOR"
  printf '├'; menu_repeat '─' "$INNER_W"; printf '┤'
  _ios_reset; printf '\n'
}
draw_section() {
  local title="$1"
  menu_fg "$IOS_SEPARATOR"; printf '│'
  menu_fg "$IOS_BLUE" bold; printf ' %-*s' "$(( INNER_W - 1 ))" "$title"
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

# Help row: yellow bold key chip, gray dot separator, white label.
# Layout inside INNER_W:
#   ' ' + key (key_w) + '  ' + '· ' + label (label_w) + ' '
#   1   +   key_w     +   2  +  2  +    label_w      +  1   = INNER_W
draw_row() {
  local key="$1" label="$2"
  local key_w=12
  local label_w=$(( INNER_W - key_w - 6 ))
  (( label_w < 1 )) && label_w=1

  menu_fg "$IOS_SEPARATOR"; printf '│'
  printf ' '
  menu_fg "$IOS_YELLOW" bold; printf '%-*s' "$key_w" "$key"
  printf '  '
  menu_fg "$IOS_GRAY"; printf '· '
  menu_fg "$IOS_WHITE"; printf '%-*s' "$label_w" "$label"
  printf ' '
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

# Header: ⌨ codex-fleet · keyboard help                            [ HELP ]
draw_header() {
  local title='⌨  codex-fleet · keyboard help'
  local chip='HELP'
  local title_len=${#title}
  local chip_render_w=$(( ${#chip} + 2 ))
  local pad=$(( INNER_W - 1 - title_len - chip_render_w - 1 ))
  (( pad < 1 )) && pad=1

  menu_fg "$IOS_SEPARATOR"; printf '│'
  menu_fg "$IOS_WHITE" bold; printf ' %s' "$title"
  printf '%*s' "$pad" ''
  menu_fg "$IOS_BLUE" bold; printf ' %s ' "$chip"
  printf ' '
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

# ── render ────────────────────────────────────────────────────────────────
clear_popup
printf '\n'

draw_top
draw_header
draw_hairline

draw_section 'Pane menu (right-click)'
draw_row 'C'         'Copy whole session'
draw_row 'c'         'Copy visible'
draw_row 'l'         'Copy this line'
draw_row 'p'         'Paste from clipboard'
draw_row '<'         'Scroll to top'
draw_row '>'         'Scroll to bottom'
draw_row 'h'         'Horizontal split'
draw_row 'v'         'Vertical split'
draw_row 'z'         'Zoom pane'
draw_row 'u / d'     'Swap up / Swap down'
draw_row 's'         'Swap with marked pane'
draw_row 'm'         'Mark / Unmark pane'
draw_row 'R'         'Respawn pane'
draw_row 'X'         'Kill pane'
draw_row '?'         'This help'
draw_hairline

draw_section 'Nav (inside the pane menu)'
draw_row '↑ / ↓'     'Move focus in pane menu'
draw_row '⏎'         'Select focused item'
draw_row 'esc'       'Cancel popup'
draw_hairline

draw_section 'tmux bindings (after prefix, default Ctrl+B)'
draw_row 'prefix m'      'iOS Action Sheet (tear-down / swap / retarget)'
draw_row 'prefix Tab'    'Section Jump grid (Overview / Fleet / Plan / …)'
draw_row 'prefix Ctrl+H' 'Open this help screen'
draw_hairline

draw_section 'Spotlight palette (inside fleet-tui-poc)'
draw_row 'Ctrl+K'    'Open the Spotlight command palette'
draw_row 'Ctrl+N'    'Spawn a new codex worker (Linux ⌘N)'
draw_row 'Ctrl+B'    'Switch worktree (Linux ⌘B)'
draw_row '↑ / ↓'     'Move the selection cursor'
draw_row '⏎'         'Activate the highlighted item'
draw_row 'esc'       'Dismiss the palette'
draw_hairline

draw_section 'Section Jump grid'
draw_row '1 – 5'     'Overview, Fleet, Plan, Waves, Review'
draw_row '⏎'         'Open the highlighted section in this pane'
draw_row 'esc'       'Dismiss the grid'

draw_bottom
menu_fg "$IOS_GRAY"; printf '\n   any key closes  ·  esc/q exits'
_ios_reset

# Read one char then exit. The popup harness closes when this script returns.
# When stdin is /dev/null (smoke test) read returns immediately so the popup
# renders and exits cleanly without hanging.
read -rsn1 -t 60 _ || :
clear_popup
