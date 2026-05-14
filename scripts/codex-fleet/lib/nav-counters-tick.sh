#!/usr/bin/env bash
# nav-counters-tick — aggregate per-tab counters into a single JSON
# snapshot consumed by the Rust fleet-* binaries' in-binary tab strip.
#
# Reads each tab's count via nav-counter.sh (the source of truth for the
# tmux strip too — same numbers in both surfaces) and writes
# /tmp/claude-viz/fleet-tab-counters.json with the shape tab_strip.rs
# expects:
#   { "overview": N, "fleet": N, "plan": N, "waves": N, "review": N,
#     "updated_at": <unix_seconds> }
#
# tab_strip.rs renders "–" if the file is missing OR its updated_at is
# older than 30s, so the tick interval must stay under 30s.
#
# Usage:
#   bash scripts/codex-fleet/lib/nav-counters-tick.sh           # one-shot write
#   bash scripts/codex-fleet/lib/nav-counters-tick.sh --loop    # write every TICK_INTERVAL_SEC (default 5)

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NAV_COUNTER="$SCRIPT_DIR/nav-counter.sh"
OUT="${FLEET_TAB_COUNTERS_PATH:-/tmp/claude-viz/fleet-tab-counters.json}"
TICK_INTERVAL_SEC="${TICK_INTERVAL_SEC:-5}"

[[ -x "$NAV_COUNTER" ]] || { echo "fatal: nav-counter.sh not executable at $NAV_COUNTER" >&2; exit 2; }
mkdir -p "$(dirname "$OUT")"

write_once() {
  local overview fleet plan waves review now tmp
  overview=$(bash "$NAV_COUNTER" overview 2>/dev/null || echo 0)
  fleet=$(bash "$NAV_COUNTER" fleet 2>/dev/null || echo 0)
  plan=$(bash "$NAV_COUNTER" plan 2>/dev/null || echo 0)
  waves=$(bash "$NAV_COUNTER" waves 2>/dev/null || echo 0)
  review=$(bash "$NAV_COUNTER" review 2>/dev/null || echo 0)
  now=$(date +%s)
  tmp="$OUT.tmp.$$"
  printf '{"overview":%d,"fleet":%d,"plan":%d,"waves":%d,"review":%d,"updated_at":%d}\n' \
    "$overview" "$fleet" "$plan" "$waves" "$review" "$now" > "$tmp"
  mv -f "$tmp" "$OUT"
}

case "${1:-}" in
  --loop)
    while :; do
      write_once
      sleep "$TICK_INTERVAL_SEC"
    done
    ;;
  *)
    write_once
    ;;
esac
