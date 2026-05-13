#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="${TMPDIR:-/tmp}/claude-viz-test-progress-$$"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP"

export FLEET_TICK_SOURCE_ONLY=1
export FLEET_TICK_REPO="$TMP"
export FLEET_TICK_PLAN_JSON="$TMP/plan.json"

# shellcheck source=/dev/null
source "$ROOT/scripts/codex-fleet/fleet-tick.sh"

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  if [[ "$actual" != "$expected" ]]; then
    printf 'FAIL %s: expected %s, got %s\n' "$label" "$expected" "$actual" >&2
    exit 1
  fi
}

write_lines() {
  local count="$1"
  for ((i=1; i<=count; i++)); do
    printf 'detail line %02d\n' "$i"
  done
}

assert_eq 0 "$(subtask_progress_pct 0)" "missing evidence scores 0%"

partial="$TMP/${SUB_EVIDENCE[1]}"
mkdir -p "$(dirname "$partial")"
{
  printf '# Partial evidence\n\n'
  printf '## Why\n\n'
  write_lines 30
} > "$partial"

partial_pct="$(subtask_progress_pct 1)"
if (( partial_pct < 40 || partial_pct > 60 )); then
  printf 'FAIL partial evidence: expected 40-60%%, got %s%%\n' "$partial_pct" >&2
  exit 1
fi

full="$TMP/${SUB_EVIDENCE[2]}"
mkdir -p "$(dirname "$full")"
{
  printf '# Complete evidence\n\n'
  printf '## Why\n\n'
  write_lines 20
  printf '\n## What Changes\n\n'
  write_lines 20
  printf '\n## Impact\n\n'
  write_lines 20
  printf '\n## Verification\n\n'
  printf '%s\n' '- [x] cargo test -p codex-lb-runtime rollback_drill'
} > "$full"

assert_eq 100 "$(subtask_progress_pct 2)" "complete evidence scores 100%"
assert_eq "████░░░░" "$(subtask_progress_bar 50 8)" "50% mini-bar"

printf 'progress tests passed\n'
