#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="${TMPDIR:-/tmp}/claude-viz-test-progress-rails-$$"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP"
printf '{"tasks":[]}\n' > "$TMP/plan.json"

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

assert_contains() {
  local haystack="$1" needle="$2" label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label missing expected sequence"
}

assert_file_contains() {
  local file="$1" needle="$2" label="$3"
  grep -Fq -- "$needle" "$file" || fail "$label missing from $file"
}

plain() {
  strip_ansi "$(ios_progress_rail "$1" "$2" "${3:-12}")"
}

assert_eq "▕░░░░░░░░░░░░▏" "$(plain 0 usage)" "usage empty rail"
assert_eq "▕██████░░░░░░▏" "$(plain 50 usage)" "usage half rail"
assert_eq "▕████████████▏" "$(plain 100 usage)" "usage full rail"
assert_eq "▕███░░░░░░░░░▏" "$(plain 25 done)" "done quarter rail"
assert_eq "▕█████████░░░▏" "$(plain 75 available)" "available three-quarter rail"

assert_contains "$(ios_progress_rail 0 usage)" "$IOS_GREEN" "usage empty endpoint is green"
assert_contains "$(ios_progress_rail 50 usage)" "$IOS_ORANGE" "usage midpoint is orange"
assert_contains "$(ios_progress_rail 100 usage)" "$IOS_RED" "usage high is red"
assert_contains "$(ios_progress_rail 0 done)" "$IOS_RED" "done low is red"
assert_contains "$(ios_progress_rail 50 done)" "$IOS_ORANGE" "done midpoint is orange"
assert_contains "$(ios_progress_rail 100 done)" "$IOS_GREEN" "done high is green"
assert_contains "$(ios_progress_rail 0 complete)" "$IOS_RED" "complete low is red"
assert_contains "$(ios_progress_rail 50 completion)" "$IOS_ORANGE" "completion midpoint is orange"
assert_contains "$(ios_progress_rail 100 cap)" "$IOS_RED" "cap high is red"
assert_contains "$(ios_progress_rail 100 available)" "$IOS_GREEN" "available high is green"
assert_contains "$(ios_progress_rail 50 availability)" "$IOS_ORANGE" "availability midpoint is orange"

assert_eq "▕████░░░░▏" "$(plain 50 usage 8)" "custom width rail"

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
: > "$FLEET_TICK_ACTIVE_FILE"

bash "$ROOT/scripts/codex-fleet/fleet-tick.sh"

assert_file_contains "$FLEET_TICK_STATE_OUT" "▕" "state output rail bracket"
assert_file_contains "$FLEET_TICK_STATE_OUT" "░" "state output rail empty cells"
assert_file_contains "$FLEET_TICK_PLAN_OUT" "▕" "plan output rail bracket"

printf 'progress rail tests passed\n'
