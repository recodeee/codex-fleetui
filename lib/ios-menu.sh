# shellcheck shell=bash
# ios-menu.sh — bash chrome helpers for iOS-style tmux menus.
#
# Source from any script that needs:
#   - ios_segmented_control "A|B|C" <active_idx>
#   - ios_action_sheet      "Title" "Opt 1" "Opt 2" …
#   - ios_toast             <icon> "message" <hex_color>
#
# Uses 24-bit ANSI (works on kitty/iterm/most modern terminals); falls back
# to xterm-256 indices via ios_palette_hex_to_256 when TERM lacks truecolor.

if [[ -n "${__IOS_MENU_LOADED:-}" ]]; then return 0 2>/dev/null || exit 0; fi
__IOS_MENU_LOADED=1

# ── iOS system palette ──────────────────────────────────────────────────────
IOS_BLUE="#007AFF"     # active accent
IOS_GREEN="#34C759"    # success/done
IOS_RED="#FF3B30"      # destructive
IOS_ORANGE="#FF9500"   # warning/badge
IOS_YELLOW="#FFCC00"   # mode/copy
IOS_GRAY="#8E8E93"     # tertiary label
IOS_GRAY2="#AEAEB2"    # secondary label
IOS_BG="#000000"       # systemBackground (dark)
IOS_BG2="#1C1C1E"      # secondarySystemBackground
IOS_BG3="#2C2C2E"      # tertiarySystemBackground
IOS_WHITE="#FFFFFF"

# Map iOS hex → xterm-256 index for terminals without truecolor.
# Returns the 256-color index on stdout.
ios_palette_hex_to_256() {
  case "${1,,}" in
    '#007aff') echo 33 ;;
    '#34c759') echo 41 ;;
    '#ff3b30') echo 196 ;;
    '#ff9500') echo 208 ;;
    '#ffcc00') echo 220 ;;
    '#8e8e93') echo 245 ;;
    '#aeaeb2') echo 250 ;;
    '#000000') echo 16 ;;
    '#1c1c1e') echo 234 ;;
    '#2c2c2e') echo 236 ;;
    '#ffffff') echo 231 ;;
    *) echo 245 ;;
  esac
}

# Internal: emit a truecolor SGR. Args: hex_fg hex_bg [attrs...]
# attrs: bold, dim, italic, underline. Omitted → reset to plain.
_ios_sgr() {
  local fg="$1" bg="$2"; shift 2 || true
  local hexfg="${fg#\#}" hexbg="${bg#\#}"
  local r g b out=""
  # parse fg
  r=$((16#${hexfg:0:2})); g=$((16#${hexfg:2:2})); b=$((16#${hexfg:4:2}))
  out+="\033[38;2;${r};${g};${b}m"
  # parse bg
  r=$((16#${hexbg:0:2})); g=$((16#${hexbg:2:2})); b=$((16#${hexbg:4:2}))
  out+="\033[48;2;${r};${g};${b}m"
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
_ios_reset() { printf '\033[0m'; }

# ── ios_segmented_control "A|B|C" active_idx ────────────────────────────────
# Renders one line: rounded outer ends, inactive=gray on bg2, active=white on
# blue with bold weight. Each segment padded with one space on each side.
# Inner edges abut (no gap, no separator) — proper iOS segmented control.
#
# Args:
#   $1 — pipe-delimited segment labels
#   $2 — active index (0-based)
#   $3 — optional: container bg hex (default IOS_BG2)
#   $4 — optional: active bg hex (default IOS_BLUE)
ios_segmented_control() {
  local segments="$1" active="${2:-0}" container="${3:-$IOS_BG2}" hot="${4:-$IOS_BLUE}"
  local -a parts; IFS='|' read -r -a parts <<< "$segments"
  local n=${#parts[@]} i label is_active
  # Left rounded end
  _ios_sgr "$container" "$IOS_BG"; printf '╭'
  for (( i=0; i<n; i++ )); do
    label="${parts[i]}"
    if (( i == active )); then
      _ios_sgr "$IOS_WHITE" "$hot" bold; printf ' %s ' "$label"
    else
      _ios_sgr "$IOS_GRAY2" "$container"; printf ' %s ' "$label"
    fi
  done
  _ios_sgr "$container" "$IOS_BG"; printf '╮'
  _ios_reset; printf '\n'
}

# ── ios_action_sheet "Title" "Opt 1" "Opt 2" … ──────────────────────────────
# Renders multi-line action sheet:
#   ╭──────────────────────────────╮
#   │      <Title>                 │
#   ├──────────────────────────────┤
#   │  1. Opt 1                    │
#   │  2. Opt 2                    │
#   │     …                        │
#   ├──────────────────────────────┤
#   │     Cancel                   │ ← red, bold
#   ╰──────────────────────────────╯
#
# The caller is responsible for capturing stdin (e.g. `read -n1 choice`)
# inside the popup wrapper.
ios_action_sheet() {
  local title="$1"; shift
  local -a opts=("$@")
  local n=${#opts[@]} i
  local width=44                # fixed width — fits comfortably in popup -w 60
  local rule="$(printf '─%.0s' $(seq 1 $((width - 2))))"
  local thin="$(printf '─%.0s' $(seq 1 $((width - 2))))"

  # Top
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '╭%s╮\n' "$rule"
  # Title row
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_WHITE" "$IOS_BG2" bold; printf "%*s%-*s" 2 '' $((width - 4)) "$title"
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│\n'
  # Hairline separator
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '├%s┤\n' "$thin"
  # Option rows
  for (( i=0; i<n; i++ )); do
    local label="${opts[i]}"
    local key=$((i+1))
    _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│'
    _ios_sgr "$IOS_BLUE" "$IOS_BG"; printf "  %d. " "$key"
    _ios_sgr "$IOS_WHITE" "$IOS_BG"; printf "%-*s" $((width - 8)) "$label"
    _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│\n'
  done
  # Hairline before Cancel
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '├%s┤\n' "$thin"
  # Cancel row
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│'
  _ios_sgr "$IOS_RED" "$IOS_BG" bold; printf "%*sq. Cancel%-*s" 2 '' $((width - 13)) ""
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '│\n'
  # Bottom
  _ios_sgr "$IOS_GRAY" "$IOS_BG"; printf '╰%s╯' "$rule"
  _ios_reset; printf '\n'
}

# ── ios_toast <icon> "message" <hex_color> ──────────────────────────────────
# One-line rounded pill: solid color bg, bold white text, leading icon.
# Default color = IOS_BLUE.
ios_toast() {
  local icon="${1:- }" msg="${2:-}" color="${3:-$IOS_BLUE}"
  _ios_sgr "$color" "$IOS_BG"; printf '╭'
  _ios_sgr "$IOS_WHITE" "$color" bold; printf ' %s %s ' "$icon" "$msg"
  _ios_sgr "$color" "$IOS_BG"; printf '╮'
  _ios_reset; printf '\n'
}

# Debug: dump the palette.
_ios_menu_dump() {
  echo "iOS palette:"
  for var in IOS_BLUE IOS_GREEN IOS_RED IOS_ORANGE IOS_YELLOW IOS_GRAY IOS_WHITE; do
    local v="${!var}"
    _ios_sgr "$IOS_WHITE" "$v" bold; printf "  %-12s %s  " "$var" "$v"
    _ios_reset; printf '\n'
  done
}
