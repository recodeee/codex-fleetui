#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="${TMPDIR:-/tmp}/claude-viz-test-ios-palette-$$"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP"
printf '{"tasks":[]}\n' > "$TMP/plan.json"
: > "$TMP/active.txt"

export FLEET_TICK_SOURCE_ONLY=1
export FLEET_TICK_REPO="$TMP"
export FLEET_TICK_PLAN_JSON="$TMP/plan.json"

# shellcheck source=/dev/null
source "$ROOT/scripts/codex-fleet/fleet-tick.sh"

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  [[ "$actual" == "$expected" ]] || fail "$label: expected $expected, got $actual"
}

assert_file_contains() {
  local file="$1" needle="$2" label="$3"
  grep -Fq -- "$needle" "$file" || fail "$label missing from $file"
}

assert_eq $'\033[38;2;0;122;255m' "$IOS_BLUE" "systemBlue truecolor"
assert_eq $'\033[38;2;52;199;89m' "$IOS_GREEN" "systemGreen truecolor"
assert_eq $'\033[38;2;255;59;48m' "$IOS_RED" "systemRed truecolor"
assert_eq $'\033[38;2;255;149;0m' "$IOS_ORANGE" "systemOrange truecolor"
assert_eq "$IOS_GREEN" "$G" "running green uses iOS palette"
assert_eq "$IOS_BLUE" "$TEAL" "heading blue uses iOS palette"

card="$({ ios_card_top "SMOKE"; ios_card_row "${B}content${R}"; ios_card_bottom; })"
card_plain=$(strip_ansi "$card")
[[ "$card_plain" == *"╭─ SMOKE"* ]] || fail "card top border"
[[ "$card_plain" == *"│  "* ]] || fail "card horizontal padding"
[[ "$card_plain" == *"╰"* ]] || fail "card bottom border"

export FLEET_TICK_SOURCE_ONLY=0
export FLEET_TICK_ONCE=1
export FLEET_TICK_INTERVAL=1
export FLEET_TICK_REPO="$TMP"
export FLEET_TICK_PLAN_JSON="$TMP/plan.json"
export FLEET_TICK_STATE_OUT="$TMP/live-fleet-state.txt"
export FLEET_TICK_PLAN_OUT="$TMP/live-plan-design.txt"
export FLEET_TICK_WAVES_OUT="$TMP/live-waves.txt"
export FLEET_TICK_ACTIVE_FILE="$TMP/active.txt"
export FLEET_TICK_PID_FILE="$TMP/fleet-tick.pid"

bash "$ROOT/scripts/codex-fleet/fleet-tick.sh"

assert_file_contains "$FLEET_TICK_STATE_OUT" "╭─ CODEX-FLEET LIVE STATE" "live-state header card"
assert_file_contains "$FLEET_TICK_STATE_OUT" "╭─ ACTIVE" "active card"
assert_file_contains "$FLEET_TICK_STATE_OUT" "╭─ RESERVE" "reserve card"
assert_file_contains "$FLEET_TICK_STATE_OUT" "╭─ FLEET FOOTER" "footer card"
assert_file_contains "$FLEET_TICK_STATE_OUT" "#007AFF/#34C759/#FF3B30/#FF9500" "palette legend"
assert_file_contains "$FLEET_TICK_STATE_OUT" $'\033[38;2;0;122;255m' "systemBlue escape"
assert_file_contains "$FLEET_TICK_STATE_OUT" $'\033[38;2;52;199;89m' "systemGreen escape"
assert_file_contains "$FLEET_TICK_STATE_OUT" $'\033[38;2;255;59;48m' "systemRed escape"
assert_file_contains "$FLEET_TICK_STATE_OUT" $'\033[38;2;255;149;0m' "systemOrange escape"

printf 'ios palette tests passed\n'
