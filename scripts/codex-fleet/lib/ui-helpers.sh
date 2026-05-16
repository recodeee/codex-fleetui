# shellcheck shell=bash

if [[ -n "${__CODEX_FLEET_LIB_UI_HELPERS_SH:-}" ]]; then
  return 0 2>/dev/null || exit 0
fi
__CODEX_FLEET_LIB_UI_HELPERS_SH=1

: "${IOS_STATUS_CHIP_WIDTH:=9}"

strip_ansi() {
  sed -E $'s/\x1B\\[[0-9;]*m//g' <<<"${1:-}"
}

ios_visible_len() {
  local clean
  clean=$(strip_ansi "${1:-}")
  printf '%d' "${#clean}"
}

pct_color() {
  local n="${1:-}"
  [[ "$n" =~ ^[0-9]+$ ]] || { printf '%s' "${DIM:-}"; return; }
  if   (( n >= 90 )); then printf '%s' "${GRAD6:-}"
  elif (( n >= 75 )); then printf '%s' "${GRAD5:-}"
  elif (( n >= 60 )); then printf '%s' "${GRAD4:-}"
  elif (( n >= 45 )); then printf '%s' "${GRAD3:-}"
  elif (( n >= 30 )); then printf '%s' "${GRAD2:-}"
  elif (( n >= 15 )); then printf '%s' "${GRAD1:-}"
  else                     printf '%s' "${GRAD0:-}"
  fi
}

ios_status_chip_label() {
  local kind="${1:-idle}"
  local raw pad_len pad
  case "$kind" in
    run|running) raw="● running" ;;
    work|working|busy) raw="● working" ;;
    exhaust|exhausted|capped) raw="⚠ exhaust" ;;
    limit|limited|rate_limited|rate-limited) raw="◍ limited" ;;
    idle|*) raw="◌ idle" ;;
  esac
  pad_len=$(( IOS_STATUS_CHIP_WIDTH - ${#raw} ))
  (( pad_len < 0 )) && pad_len=0
  printf -v pad '%*s' "$pad_len" ""
  printf '%s%s' "$raw" "$pad"
}
