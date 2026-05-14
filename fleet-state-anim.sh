#!/usr/bin/env bash
# fleet-state-anim — display live-fleet-state.txt with ANSI passthrough.
#
# Why: GNU `watch -c` mangles 256-color SGR codes (drops 38;5;N extensions),
# turning the colorful gradient bars into flat grey rectangles. This script
# is a drop-in replacement that just paints the file in-place every tick.
#
# Usage:
#   bash scripts/codex-fleet/fleet-state-anim.sh           # loop
#   bash scripts/codex-fleet/fleet-state-anim.sh --once    # one frame
#   FLEET_STATE_FILE=/path/to/file FLEET_STATE_INTERVAL_MS=1000 ...
set -eo pipefail

FILE="${FLEET_STATE_FILE:-/tmp/claude-viz/live-fleet-state.txt}"
INTERVAL_MS="${FLEET_STATE_INTERVAL_MS:-1000}"
ONCE=0
for a in "$@"; do
  case "$a" in
    --once) ONCE=1 ;;
    --file=*) FILE="${a#--file=}" ;;
    --interval=*) INTERVAL_MS="${a#--interval=}" ;;
  esac
done
INTERVAL_S=$(awk -v ms="$INTERVAL_MS" 'BEGIN{printf "%.3f", ms/1000}')

DIM=$'\033[2m'; R=$'\033[0m'

render_frame() {
  printf '\033[H'
  if [[ -f "$FILE" ]]; then
    # Wipe trailing chars on every line so a previous wider frame doesn't
    # leave ghost columns on the right. \033[K clears from the cursor to
    # the end of the current row.
    while IFS='' read -r line; do
      printf '%s\033[K\n' "$line"
    done <"$FILE"
  else
    printf '%s[fleet-state-anim] waiting for %s ...%s\033[K\n' "$DIM" "$FILE" "$R"
  fi
  printf '\033[J'
}

if (( ONCE == 1 )); then
  render_frame
else
  printf '\033[?25l'
  trap 'printf "\033[?25h"; exit' INT TERM EXIT
  while true; do
    render_frame
    sleep "$INTERVAL_S"
  done
fi
