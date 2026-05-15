#!/usr/bin/env bash
# test-claude-supervisor-classifier.sh — fixture-driven replay harness for
# the claude-supervisor classifier. Sources the pure lib at
# scripts/codex-fleet/lib/claude-supervisor-classifier.sh and runs
# classify_tail against every fixture under
# scripts/codex-fleet/test/fixtures/claude-supervisor-classifier/.
#
# Fixture naming:
#   <expected-label>__<short-name>.txt
# where <expected-label> is one of: busy | asking | blocked | quiet.
#
# Exit codes:
#   0 — every fixture matched its expected label
#   1 — at least one fixture mismatched
#   2 — usage error / missing lib

set -u
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLEET_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB="$FLEET_DIR/lib/claude-supervisor-classifier.sh"
FIXTURE_DIR="${1:-$SCRIPT_DIR/fixtures/claude-supervisor-classifier}"

if [ ! -f "$LIB" ]; then
  printf 'fatal: classifier lib not found at %s\n' "$LIB" >&2
  exit 2
fi
if [ ! -d "$FIXTURE_DIR" ]; then
  printf 'fatal: fixture dir not found at %s\n' "$FIXTURE_DIR" >&2
  exit 2
fi

# shellcheck source=../lib/claude-supervisor-classifier.sh
. "$LIB"

pass=0
fail=0
fails=()

shopt -s nullglob
for fixture in "$FIXTURE_DIR"/*.txt; do
  base="$(basename "$fixture")"
  expected="${base%%__*}"
  case "$expected" in
    busy|asking|blocked|quiet) ;;
    *)
      printf 'skip: %s (bad prefix, expected busy|asking|blocked|quiet)\n' "$base"
      continue
      ;;
  esac

  tail_content="$(cat "$fixture")"
  actual="$(classify_tail "$tail_content")"

  if [ "$actual" = "$expected" ]; then
    pass=$((pass+1))
    printf '  PASS  %-12s  %s\n' "$expected" "$base"
  else
    fail=$((fail+1))
    fails+=("$base: expected=$expected actual=$actual")
    printf '  FAIL  %-12s  %s  (got %s)\n' "$expected" "$base" "$actual"
  fi
done

printf '\n%d pass, %d fail\n' "$pass" "$fail"
if [ "$fail" -ne 0 ]; then
  printf '\nfailing fixtures:\n'
  printf '  - %s\n' "${fails[@]}"
  exit 1
fi
exit 0
