#!/usr/bin/env bash
# pane-context-menu.sh — iOS-style right-click context menu for fleet panes.
#
# Bound to MouseDown3Pane via scripts/codex-fleet/style-tabs.sh through a
# `display-popup -E -B` so we get a full pty and can draw the rounded card +
# accent shortcut chips that tmux's built-in `display-menu` cannot render.
#
# Usage: pane-context-menu.sh <pane_id>
#   pane_id   e.g. %47 — set by tmux #{pane_id} at bind time
#
# The line text under the cursor at right-click time is read from
# $CODEX_FLEET_MENU_LINE (set by the MouseDown3Pane binding via
# `set-environment -g CODEX_FLEET_MENU_LINE "#{q:mouse_line}"` so that
# embedded quotes/spaces survive into the popup pty).
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PANE_ID="${1:-}"
MOUSE_LINE="${CODEX_FLEET_MENU_LINE:-}"

if [[ -z "$PANE_ID" ]]; then
  echo "pane-context-menu.sh: missing pane_id arg" >&2
  exit 2
fi

# shellcheck source=/dev/null
source "$SCRIPT_DIR/../lib/ios-menu.sh"

CARD_W=54
INNER_W=$(( CARD_W - 2 ))

INDEX="$(tmux display -p -t "$PANE_ID" '#{pane_index}' 2>/dev/null || echo '?')"
PANES_IN_WIN="$(tmux display -p -t "$PANE_ID" '#{window_panes}' 2>/dev/null || echo 1)"
MARKED_ANYWHERE="$(tmux display -p -t "$PANE_ID" '#{pane_marked_set}' 2>/dev/null || echo 0)"
ZOOMED="$(tmux display -p -t "$PANE_ID" '#{window_zoomed_flag}' 2>/dev/null || echo 0)"
PANE_MARKED="$(tmux display -p -t "$PANE_ID" '#{pane_marked}' 2>/dev/null || echo 0)"

# ── chrome helpers (operate inside the popup's pty) ────────────────────────
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
draw_blank() {
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf '%*s' "$INNER_W" ''
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}

# Header row: ● pane <idx> · <pane_id>                         [ LIVE ]
draw_header() {
  local title="pane ${INDEX} · ${PANE_ID}"
  local chip="LIVE"
  local title_len=${#title}
  # Layout inside INNER_W: ' ● <title>' + pad + '[ LIVE ]' + ' '
  # Widths:                 1 1 1 +len     P    1+1+4+1+1   1   = INNER_W
  local chip_render_w=$(( ${#chip} + 4 ))
  local pad=$(( INNER_W - 3 - title_len - chip_render_w - 1 ))
  (( pad < 1 )) && pad=1

  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_GREEN" "$IOS_BG2"; printf ' ● '
  _ios_sgr "$IOS_WHITE" "$IOS_BG2" bold; printf '%s' "$title"
  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf '%*s' "$pad" ''
  _ios_sgr "$IOS_WHITE" "$IOS_GREEN" bold; printf ' %s ' "$chip"
  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf ' '
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}

# Item row layout (INNER_W = 52):
#   SP icon SP SP label PAD '[ ' key ' ]' SP
#    1  1   1  1   L     P    2   1   2  1   = 9 + L + P  →  P = INNER_W - 9 - L
#
# Args: icon label key [style: normal|danger|disabled]
draw_item() {
  local icon="$1" label="$2" key="$3" style="${4:-normal}"
  local label_pad=$(( INNER_W - 9 - ${#label} ))
  (( label_pad < 1 )) && label_pad=1

  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'

  case "$style" in
    danger)
      _ios_sgr "$IOS_RED" "$IOS_BG2"; printf ' %s  ' "$icon"
      _ios_sgr "$IOS_RED" "$IOS_BG2" bold; printf '%s' "$label"
      ;;
    disabled)
      _ios_sgr "$IOS_GRAY" "$IOS_BG2"; printf ' %s  ' "$icon"
      _ios_sgr "$IOS_GRAY" "$IOS_BG2"; printf '%s' "$label"
      ;;
    *)
      _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf ' %s  ' "$icon"
      _ios_sgr "$IOS_WHITE" "$IOS_BG2"; printf '%s' "$label"
      ;;
  esac

  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf '%*s' "$label_pad" ''
  if [[ "$style" == "disabled" ]]; then
    _ios_sgr "$IOS_GRAY" "$IOS_BG3"; printf '[ '
    _ios_sgr "$IOS_GRAY" "$IOS_BG3" bold; printf '%s' "$key"
    _ios_sgr "$IOS_GRAY" "$IOS_BG3"; printf ' ]'
  else
    _ios_sgr "$IOS_GRAY2" "$IOS_BG3"; printf '[ '
    _ios_sgr "$IOS_WHITE" "$IOS_BG3" bold; printf '%s' "$key"
    _ios_sgr "$IOS_GRAY2" "$IOS_BG3"; printf ' ]'
  fi
  _ios_sgr "$IOS_GRAY2" "$IOS_BG2"; printf ' '
  _ios_sgr "$IOS_BG3" "$IOS_BG"; printf '│'
  _ios_reset; printf '\n'
}

# ── conditional labels / styles ────────────────────────────────────────────
multi=normal
(( PANES_IN_WIN > 1 )) || multi=disabled
zoom_label="Zoom pane"
(( ZOOMED == 1 )) && zoom_label="Unzoom pane"
zoom_style="$multi"

swap_marked_style=normal
(( MARKED_ANYWHERE == 1 )) || swap_marked_style=disabled
mark_label="Mark pane"
(( PANE_MARKED == 1 )) && mark_label="Unmark pane"

# ── render ─────────────────────────────────────────────────────────────────
clear
printf '\n'   # small top margin so the popup doesn't crowd row 0

draw_top
draw_header
draw_hairline

draw_item '▣'  "Copy whole session"  'C'
draw_item '▢'  "Copy visible"        'c'
draw_item '─'  "Copy this line"      'l'
draw_hairline
draw_item '↟'  "Scroll to top"       '<'
draw_item '↡'  "Scroll to bottom"    '>'
draw_hairline
draw_item '⊟'  "Horizontal split"    'h'
draw_item '⊞'  "Vertical split"      'v'
draw_item '⤢'  "$zoom_label"         'z' "$zoom_style"
draw_hairline
draw_item '↑'  "Swap up"             'u' "$multi"
draw_item '↓'  "Swap down"           'd' "$multi"
draw_item '⇄'  "Swap with marked"    's' "$swap_marked_style"
draw_item '★'  "$mark_label"         'm'
draw_hairline
draw_item '↻'  "Respawn pane"        'R'
draw_item '✕'  "Kill pane"           'X' danger
draw_hairline
draw_item '?'  "Keyboard help…"      '?'
draw_bottom

_ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '\n   press a hotkey · ? or ctrl+h for help · esc cancels'
_ios_reset

# ── input + dispatch ───────────────────────────────────────────────────────
choice=''
read -rsn1 -t 30 choice || choice=''
clear

case "$choice" in
  C)  tmux capture-pane -t "$PANE_ID" -p -S - -E - | wl-copy
      tmux display-message -d 1500 '▣  Pane history copied' ;;
  c)  tmux capture-pane -t "$PANE_ID" -p | wl-copy
      tmux display-message -d 1500 '▢  Visible area copied' ;;
  l)  printf '%s' "$MOUSE_LINE" | wl-copy
      tmux display-message -d 1500 '─  Line copied' ;;
  '<') tmux copy-mode -t "$PANE_ID"
       tmux send-keys -X -t "$PANE_ID" history-top ;;
  '>') tmux copy-mode -t "$PANE_ID"
       tmux send-keys -X -t "$PANE_ID" history-bottom ;;
  h)  tmux split-window -h -t "$PANE_ID" ;;
  v)  tmux split-window -v -t "$PANE_ID" ;;
  z)  (( PANES_IN_WIN > 1 )) && tmux resize-pane -Z -t "$PANE_ID" ;;
  u)  (( PANES_IN_WIN > 1 )) && tmux swap-pane -U -t "$PANE_ID" ;;
  d)  (( PANES_IN_WIN > 1 )) && tmux swap-pane -D -t "$PANE_ID" ;;
  s)  (( MARKED_ANYWHERE == 1 )) && tmux swap-pane -t "$PANE_ID" ;;
  m)  tmux select-pane -m -t "$PANE_ID" ;;
  R)  tmux respawn-pane -k -t "$PANE_ID" ;;
  X)  tmux kill-pane -t "$PANE_ID" ;;
  '?'|$'\b'|$'\x08')
      # `?` from the menu row, or Ctrl+H (which most Linux terminals
      # deliver as 0x08 / backspace). Both open the iOS-style help popup
      # showing every fleet keybinding without dismissing this menu.
      bash "$SCRIPT_DIR/help-popup.sh" || true ;;
  *)  : ;;
esac
