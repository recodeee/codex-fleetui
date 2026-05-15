#!/usr/bin/env bash
# pane-context-menu.sh — iOS-style right-click context menu for fleet panes.
#
# Transparency model: this fallback bash menu intentionally emits foreground-only
# ANSI (`38;2;R;G;B`) so tmux popup `-B` leaves the underlying pane visible. Do
# not use solid background SGR (`48;2;...`) in this script.
#
# Smoke test:
#   printf 'hello-bg\nhello-bg\nhello-bg' && CODEX_FLEET_MENU_LINE=demo bash scripts/codex-fleet/bin/pane-context-menu.sh '%0' < /dev/null
# Capture PR evidence with `script -qfec "<command above>" /tmp/pane-context-menu.typescript`
# or an equivalent terminal recording.
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

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/../lib/_tmux.sh"

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
IOS_SEPARATOR="#3A3A3C"
IOS_DISABLED="#48484A"

INDEX="$(tmux display -p -t "$PANE_ID" '#{pane_index}' 2>/dev/null || echo '?')"
PANES_IN_WIN="$(tmux display -p -t "$PANE_ID" '#{window_panes}' 2>/dev/null || echo 1)"
MARKED_ANYWHERE="$(tmux display -p -t "$PANE_ID" '#{pane_marked_set}' 2>/dev/null || echo 0)"
ZOOMED="$(tmux display -p -t "$PANE_ID" '#{window_zoomed_flag}' 2>/dev/null || echo 0)"
PANE_MARKED="$(tmux display -p -t "$PANE_ID" '#{pane_marked}' 2>/dev/null || echo 0)"

# Smart top row: if tmux already has selection text in its paste buffer, surface
# it as a one-tap "Copy selection · <preview>…" row so the operator's most
# probable next action is on the cursor by default. BUFFER_SIZE==0 means no
# recent selection — the row is hidden and the menu falls back to its prior
# top item ("Copy whole session").
BUFFER_SIZE="$(tmux show-buffer 2>/dev/null | wc -c | tr -d ' ')"
BUFFER_SIZE="${BUFFER_SIZE:-0}"
BUFFER_SAMPLE="$(tmux show-buffer 2>/dev/null | head -c 30 | tr -d '\n')"

# ── chrome helpers (operate inside the popup's pty) ────────────────────────
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
draw_blank() {
  menu_fg "$IOS_SEPARATOR"; printf '│'
  printf '%*s' "$INNER_W" ''
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

# Header row: ● pane <idx> · <pane_id>                         [ LIVE ]
draw_header() {
  local title="pane ${INDEX} · ${PANE_ID}"
  local chip="LIVE"
  local title_len=${#title}
  # Layout inside INNER_W: ' ● <title>' + pad + '[ LIVE ]' + ' '
  # Widths:                 1 1 1 +len     P    1+1+4+1+1   1   = INNER_W
  local chip_render_w=$(( ${#chip} + 2 ))
  local pad=$(( INNER_W - 3 - title_len - chip_render_w - 1 ))
  (( pad < 1 )) && pad=1

  menu_fg "$IOS_SEPARATOR"; printf '│'
  menu_fg "$IOS_GREEN"; printf ' ● '
  menu_fg "$IOS_WHITE" bold; printf '%s' "$title"
  printf '%*s' "$pad" ''
  menu_fg "$IOS_GREEN" bold; printf ' %s ' "$chip"
  printf ' '
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

# Item row layout (INNER_W = 52):
#   SP icon SP SP label PAD '· ' key SP
#    1  1   1  1   L     P    2   K   1       = 7 + K + L + P
#
# Args: icon label key [style: normal|danger|disabled]
draw_item() {
  local icon="$1" label="$2" key="$3" style="${4:-normal}" focus_key="${5:-}"
  local shortcut="· $key"
  local label_pad=$(( INNER_W - 5 - ${#label} - ${#shortcut} ))
  (( label_pad < 1 )) && label_pad=1

  menu_fg "$IOS_SEPARATOR"; printf '│'

  if [[ "$focus_key" == "$key" ]]; then
    menu_fg "$IOS_BLUE" underline
    printf ' %s  %s%*s%s ' "$icon" "$label" "$label_pad" '' "$shortcut"
    printf '\033[24m'
  else
    case "$style" in
      danger)
        menu_fg "$IOS_RED"; printf ' %s  ' "$icon"
        menu_fg "$IOS_RED" bold; printf '%s' "$label"
        printf '%*s' "$label_pad" ''
        menu_fg "$IOS_RED"; printf '%s ' "$shortcut"
        ;;
      disabled)
        menu_fg "$IOS_DISABLED"; printf ' %s  %s%*s%s ' "$icon" "$label" "$label_pad" '' "$shortcut"
        ;;
      *)
        menu_fg "$IOS_GRAY"; printf ' %s  ' "$icon"
        menu_fg "$IOS_WHITE"; printf '%s' "$label"
        printf '%*s' "$label_pad" ''
        menu_fg "$IOS_GRAY2"; printf '%s ' "$shortcut"
        ;;
    esac
  fi
  menu_fg "$IOS_SEPARATOR"; printf '│'
  _ios_reset; printf '\n'
}

render_menu() {
  local focus_key="${1:-}"

  clear
  printf '\n'   # small top margin so the popup doesn't crowd row 0

  draw_top
  draw_header
  draw_hairline

  # Smart top row — anticipates the operator's next action. When tmux already
  # holds selection text (BUFFER_SIZE > 0), the first row is "Copy selection ·
  # <preview>…" and the cursor lands on it. The hotkey is 'S' (capital S —
  # 's' stays bound to "swap with marked").
  if (( BUFFER_SIZE > 0 )); then
    local sel_label="Copy selection · ${BUFFER_SAMPLE}…"
    # Trim label so the row stays inside INNER_W when buffer samples are long.
    local sel_max=$(( INNER_W - 5 - 4 ))   # 5 chrome + "· S " chip
    if (( ${#sel_label} > sel_max )); then
      sel_label="${sel_label:0:sel_max}"
    fi
    draw_item '✓'  "$sel_label"           'S' normal "$focus_key"
    draw_hairline
  fi

  draw_item '▣'  "Copy whole session"     'C' normal "$focus_key"
  draw_item '▢'  "Copy visible"           'c' normal "$focus_key"
  draw_item '─'  "Copy this line"         'l' normal "$focus_key"
  draw_item '⤓'  "Paste from clipboard"   'p' normal "$focus_key"
  draw_hairline
  draw_item '↟'  "Scroll to top"       '<' normal "$focus_key"
  draw_item '↡'  "Scroll to bottom"    '>' normal "$focus_key"
  draw_hairline
  draw_item '⊟'  "Horizontal split"    'h' normal "$focus_key"
  draw_item '⊞'  "Vertical split"      'v' normal "$focus_key"
  draw_item '⤢'  "$zoom_label"         'z' "$zoom_style" "$focus_key"
  draw_hairline
  draw_item '↑'  "Swap up"             'u' "$multi" "$focus_key"
  draw_item '↓'  "Swap down"           'd' "$multi" "$focus_key"
  draw_item '⇄'  "Swap with marked"    's' "$swap_marked_style" "$focus_key"
  draw_item '★'  "$mark_label"         'm' normal "$focus_key"
  draw_hairline
  draw_item '↻'  "Respawn pane"        'R' normal "$focus_key"
  draw_item '✕'  "Kill pane"           'X' danger "$focus_key"
  draw_hairline
  draw_item '?'  "Keyboard help…"      '?' normal "$focus_key"
  draw_bottom

  menu_fg "$IOS_GRAY"; printf '\n   ↑/↓ move  ·  ⏎ select  ·  hotkey jumps  ·  esc cancels'
  _ios_reset
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

# ── selectable item table (must stay in sync with render_menu rows) ────────
# Order matches the visual order rendered in render_menu so arrow-nav walks
# the menu top-to-bottom. Disabled rows keep their hotkey wired (parity with
# the prior single-keystroke behavior) but are skipped during arrow walks.
# When BUFFER_SIZE > 0 the smart "Copy selection" row (hotkey 'S') is prepended
# at index 0 so the focus seeker below lands on it by default.
ITEM_KEYS=()
ITEM_STYLES=()
if (( BUFFER_SIZE > 0 )); then
  ITEM_KEYS+=('S')
  ITEM_STYLES+=(normal)
fi
ITEM_KEYS+=(C c l p '<' '>' h v z u d s m R X '?')
ITEM_STYLES+=(normal normal normal normal \
              normal normal \
              normal normal "$zoom_style" \
              "$multi" "$multi" "$swap_marked_style" normal \
              normal danger \
              normal)

# Walk the ITEM_STYLES table, skipping 'disabled' entries, wrapping around.
next_enabled_idx() {
  local cur="$1" total="${#ITEM_KEYS[@]}" step i idx
  for (( step=1; step<=total; step++ )); do
    idx=$(( (cur + step) % total ))
    if [[ "${ITEM_STYLES[$idx]}" != "disabled" ]]; then
      printf '%d' "$idx"; return 0
    fi
  done
  printf '%d' "$cur"
}
prev_enabled_idx() {
  local cur="$1" total="${#ITEM_KEYS[@]}" step i idx
  for (( step=1; step<=total; step++ )); do
    idx=$(( (cur - step + total) % total ))
    if [[ "${ITEM_STYLES[$idx]}" != "disabled" ]]; then
      printf '%d' "$idx"; return 0
    fi
  done
  printf '%d' "$cur"
}

# Seed focus on the first enabled row.
focused_idx=0
while (( focused_idx < ${#ITEM_KEYS[@]} )) \
   && [[ "${ITEM_STYLES[$focused_idx]}" == "disabled" ]]; do
  focused_idx=$(( focused_idx + 1 ))
done
(( focused_idx >= ${#ITEM_KEYS[@]} )) && focused_idx=0

# ── input loop: arrow-nav + hotkey jumps + enter selects ───────────────────
choice=''
while true; do
  render_menu "${ITEM_KEYS[$focused_idx]}"

  ch=''
  if ! read -rsn1 -t 60 ch; then
    # Timeout or EOF (e.g. stdin closed) — cancel without dispatch.
    choice=''
    break
  fi

  if [[ -z "$ch" ]]; then
    # Enter sometimes arrives as an empty read on some terminals.
    choice="${ITEM_KEYS[$focused_idx]}"
    break
  fi

  if [[ "$ch" == $'\x1b' ]]; then
    # Possible CSI sequence (arrow keys) — peek the next two bytes briefly.
    esc2=''
    read -rsn2 -t 0.05 esc2 || esc2=''
    case "$esc2" in
      '[A') focused_idx="$(prev_enabled_idx "$focused_idx")"; continue ;;
      '[B') focused_idx="$(next_enabled_idx "$focused_idx")"; continue ;;
      *)    choice=''; break ;;   # bare ESC cancels
    esac
  fi

  if [[ "$ch" == $'\r' || "$ch" == $'\n' ]]; then
    choice="${ITEM_KEYS[$focused_idx]}"
    break
  fi

  # Any other byte is treated as a hotkey jump (existing behavior).
  choice="$ch"
  break
done

feedback_key=''
case "$choice" in
  S|C|c|l|p|'<'|'>'|h|v|z|u|d|s|m|R|X|'?') feedback_key="$choice" ;;
  $'\b'|$'\x08') feedback_key='?' ;;
esac
if [[ -n "$feedback_key" ]]; then
  render_menu "$feedback_key"
  sleep 0.08
fi
clear

case "$choice" in
  S)  # Smart top-row: copy the existing tmux paste buffer (the recent
      # selection) into the SYSTEM clipboard through the dual-clipboard helper
      # so wl-copy + tmux buffer stay in sync. No-op if the buffer is empty
      # (BUFFER_SIZE > 0 gates the row, but re-check to be safe).
      if [[ "${BUFFER_SIZE:-0}" -gt 0 ]]; then
        tmux show-buffer | bash "$SCRIPT_DIR/pane-menu-clip-dual.sh"
        tmux display-message -d 1200 '✓  selection copied'
      fi ;;
  C)  tmux capture-pane -t "$PANE_ID" -p -S - -E - | wl-copy
      tmux display-message -d 1500 '▣  Pane history copied' ;;
  c)  tmux capture-pane -t "$PANE_ID" -p | wl-copy
      tmux display-message -d 1500 '▢  Visible area copied' ;;
  l)  printf '%s' "$MOUSE_LINE" | wl-copy
      tmux display-message -d 1500 '─  Line copied' ;;
  p)  # Paste from the SYSTEM clipboard (Wayland wl-paste). Falls through to
      # tmux's own buffer if wl-paste is unavailable. Images are detected via
      # MIME type and saved to /tmp/clipboard-paste-<ts>.<ext>; the saved path
      # is then pasted as text so downstream CLIs (codex/claude) can read it.
      if command -v wl-paste >/dev/null 2>&1; then
        mimes="$(wl-paste --list-types 2>/dev/null || true)"
        img_mime="$(printf '%s\n' "$mimes" | grep -m1 -oE '^image/(png|jpeg|gif|webp|bmp)' || true)"
        if [ -n "$img_mime" ]; then
          ext="${img_mime#image/}"; [ "$ext" = "jpeg" ] && ext="jpg"
          out="/tmp/clipboard-paste-$(date +%s).$ext"
          if wl-paste --type "$img_mime" > "$out" 2>/dev/null && [ -s "$out" ]; then
            printf '%s' "$out" | tmux load-buffer -b _menu_paste -
            tmux paste-buffer -b _menu_paste -t "$PANE_ID" -p
            tmux delete-buffer -b _menu_paste 2>/dev/null || true
            tmux display-message -d 1800 "⤓  Image pasted as path: $out"
          else
            rm -f "$out" 2>/dev/null || true
            tmux display-message -d 1500 '⚠  Image paste failed'
          fi
        else
          wl-paste --no-newline 2>/dev/null | tmux load-buffer -b _menu_paste -
          tmux paste-buffer -b _menu_paste -t "$PANE_ID" -p
          tmux delete-buffer -b _menu_paste 2>/dev/null || true
          tmux display-message -d 1500 '⤓  Pasted from clipboard'
        fi
      else
        tmux paste-buffer -t "$PANE_ID" -p
        tmux display-message -d 1500 '⤓  Pasted from tmux buffer (no wl-paste)'
      fi
      ;;
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
