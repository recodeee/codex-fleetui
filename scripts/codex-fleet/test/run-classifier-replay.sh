#!/usr/bin/env bash
# run-classifier-replay.sh — replay harness for the codex-fleet supervisor
# classifier. Iterates every .txt fixture under
# scripts/codex-fleet/test/classifier-fixtures/, classifies it via the shared
# classifier library, compares against the sibling .label file, and prints a
# confusion matrix + overall accuracy.
#
# Categories produced by the classifier (see
# scripts/codex-fleet/lib/claude-supervisor-classifier.sh):
#   busy | asking | blocked | quiet
#
# Fixture naming convention:
#   <category>-NNN[-tag].txt   sibling   <category>-NNN[-tag].label
# where <category> is one of working|asking|blocked|done. The harness maps
# working→busy and done→quiet before comparing against classify_tail output;
# both spellings are accepted in the .label file.
#
# Determinism: forces a known-cheap model for any optional classifier sub-path
# that might otherwise reach for a paid LLM. Pure-bash classification does not
# call any model — this is belt-and-suspenders so replay never burns Opus.
#
# Exit codes:
#   0 — accuracy >= threshold (default 0.85)
#   1 — accuracy below threshold OR classifier missing / stub
#   2 — usage / setup error

set -u
set -o pipefail

# Pin a deterministic, cheap model in case any sub-tool the classifier loads
# checks an env var. The pure-bash classifier ignores this; future LLM-backed
# tiers should honour it.
export CODEX_FLEET_FORCE_MODEL="${CODEX_FLEET_FORCE_MODEL:-sonnet-4-6}"
export CLAUDE_SUPERVISOR_FORCE_MODEL="${CLAUDE_SUPERVISOR_FORCE_MODEL:-sonnet-4-6}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLEET_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="${1:-$SCRIPT_DIR/classifier-fixtures}"
THRESHOLD="${CODEX_FLEET_CLASSIFIER_THRESHOLD:-0.85}"

CLASSIFIER_LIB="$FLEET_DIR/lib/claude-supervisor-classifier.sh"
SUPERVISOR_SH="$FLEET_DIR/supervisor.sh"

# ---------- classifier loader ----------
# Preferred path: source the pure classifier lib and call classify_tail.
# Fallback path: if the supervisor.sh ever grows a callable classifier
# function with the same name, source that instead. If neither is available,
# install a "uncertain" stub so the harness still runs and exits non-zero
# with a clear message (per the lane spec).
classifier_source="none"

if [ -f "$CLASSIFIER_LIB" ]; then
  # shellcheck source=../lib/claude-supervisor-classifier.sh
  . "$CLASSIFIER_LIB"
  if declare -F classify_tail >/dev/null 2>&1; then
    classifier_source="lib"
  fi
fi

if [ "$classifier_source" = "none" ] && [ -f "$SUPERVISOR_SH" ]; then
  # TODO: if scripts/codex-fleet/supervisor.sh ever exposes a sourceable
  # classify_tail function, source it here. Today it is an event-loop
  # daemon that exec()s side effects at source time, so we do NOT source it
  # blindly — that would spawn replacement kitty workers during a replay.
  :
fi

if [ "$classifier_source" = "none" ]; then
  # Stub classifier: always echo "uncertain". The harness will then exit
  # non-zero with a message that points at the missing classifier lib.
  classify_tail() {
    printf 'uncertain\n'
  }
  classifier_source="stub"
fi

if [ ! -d "$FIXTURE_DIR" ]; then
  printf 'fatal: fixture dir not found: %s\n' "$FIXTURE_DIR" >&2
  exit 2
fi

# ---------- label normalisation ----------
# Accept the lane spec's working/done aliases as well as the classifier's
# native busy/quiet output, so fixtures named working-001.txt with label
# "working" still compare correctly.
normalise_label() {
  case "$1" in
    working|busy)      printf 'busy\n' ;;
    asking)            printf 'asking\n' ;;
    blocked)           printf 'blocked\n' ;;
    done|quiet)        printf 'quiet\n' ;;
    *)                 printf '%s\n' "$1" ;;
  esac
}

# ---------- replay loop ----------
shopt -s nullglob

declare -A counts
total=0
correct=0
fails=()

for fixture in "$FIXTURE_DIR"/*.txt; do
  base="$(basename "$fixture")"
  label_file="${fixture%.txt}.label"
  if [ ! -f "$label_file" ]; then
    printf 'skip: %s (missing sibling .label)\n' "$base"
    continue
  fi

  raw_expected="$(tr -d '\n\r' < "$label_file" | awk '{$1=$1; print}')"
  expected="$(normalise_label "$raw_expected")"

  tail_content="$(cat "$fixture")"
  actual="$(classify_tail "$tail_content")"

  total=$((total + 1))
  key="${expected}->${actual}"
  counts["$key"]=$(( ${counts[$key]:-0} + 1 ))

  if [ "$actual" = "$expected" ]; then
    correct=$((correct + 1))
    printf '  PASS  %-8s -> %-8s  %s\n' "$expected" "$actual" "$base"
  else
    fails+=("$base: expected=$expected actual=$actual")
    printf '  FAIL  %-8s -> %-8s  %s\n' "$expected" "$actual" "$base"
  fi
done

if [ "$total" -eq 0 ]; then
  printf 'fatal: no .txt fixtures found under %s\n' "$FIXTURE_DIR" >&2
  exit 2
fi

# ---------- confusion matrix ----------
categories=(busy asking blocked quiet uncertain)
printf '\nconfusion matrix (rows=expected, cols=actual):\n'
printf '  %-10s' ""
for c in "${categories[@]}"; do
  printf ' %-10s' "$c"
done
printf '\n'
for r in busy asking blocked quiet; do
  printf '  %-10s' "$r"
  for c in "${categories[@]}"; do
    printf ' %-10s' "${counts[${r}->${c}]:-0}"
  done
  printf '\n'
done

# ---------- accuracy ----------
accuracy="$(awk -v c="$correct" -v t="$total" 'BEGIN { if (t==0) print "0.000"; else printf "%.3f", c/t }')"
printf '\nclassifier source : %s\n' "$classifier_source"
printf 'fixtures total    : %d\n' "$total"
printf 'fixtures correct  : %d\n' "$correct"
printf 'accuracy          : %s (threshold %s)\n' "$accuracy" "$THRESHOLD"

if [ "${#fails[@]}" -gt 0 ]; then
  printf '\nfailing fixtures:\n'
  for f in "${fails[@]}"; do
    printf '  - %s\n' "$f"
  done
fi

if [ "$classifier_source" = "stub" ]; then
  printf '\nfatal: classifier lib not found at %s — running with "uncertain" stub.\n' "$CLASSIFIER_LIB" >&2
  printf 'restore the lib (or wire supervisor.sh to expose classify_tail) and re-run.\n' >&2
  exit 1
fi

pass_threshold="$(awk -v a="$accuracy" -v t="$THRESHOLD" 'BEGIN { print (a+0 >= t+0) ? "1" : "0" }')"
if [ "$pass_threshold" != "1" ]; then
  exit 1
fi
exit 0
