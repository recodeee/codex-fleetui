#!/usr/bin/env bash
# Test review-queue.sh end-to-end:
#   1. emit-pending appends a well-formed event
#   2. build collapses events → snapshot with the expected counts/shape
#   3. emit-decided flips a pending review out of the queue and into decisions
#   4. The renderer (review-anim.sh --once) reads the snapshot and renders it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
QUEUE_SH="$ROOT/scripts/codex-fleet/review-queue.sh"
ANIM_SH="$ROOT/scripts/codex-fleet/review-anim.sh"

TMP="${TMPDIR:-/tmp}/claude-viz-test-review-queue-$$"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP"

export REVIEW_EVENTS_LOG="$TMP/review-events.jsonl"
export REVIEW_QUEUE_JSON="$TMP/live-review-queue.json"

fail() { printf 'FAIL %s\n' "$1" >&2; exit 1; }

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  [[ "$actual" == "$expected" ]] || fail "$label: expected '$expected', got '$actual'"
}

assert_contains() {
  local needle="$1" haystack="$2" label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label: missing '$needle'"
}

strip_ansi() { sed -E $'s/\x1B\\[[0-9;]*m//g' <<<"$1"; }

# 1. emit-pending writes a valid JSON event line.
bash "$QUEUE_SH" emit-pending \
  --id REV-001 \
  --title "apply_patch touching 3 files" \
  --agent codex-ricsi-zazrifka \
  --pane 4 \
  --risk medium \
  --auth high \
  --rationale "Bounded local edits within the claimed task file scope." \
  --file scripts/codex-fleet/lib/_env.sh \
  --file scripts/codex-fleet/down-kitty.sh \
  --file docs/cockpit.md \
  > "$TMP/emit1.json"

# One line in the log; same as what was returned on stdout.
log_lines=$(wc -l < "$REVIEW_EVENTS_LOG")
assert_eq "1" "$log_lines" "events log has one line"

emit_kind=$(jq -r '.kind' < "$TMP/emit1.json")
assert_eq "pending" "$emit_kind" "emitted event kind=pending"
emit_files=$(jq -r '.files | length' < "$TMP/emit1.json")
assert_eq "3" "$emit_files" "emitted event has three files"

# 2. build snapshot shows 1 pending, 0 decisions, 0 approved today (no decisions yet).
bash "$QUEUE_SH" build
[[ -r "$REVIEW_QUEUE_JSON" ]] || fail "build did not produce snapshot at $REVIEW_QUEUE_JSON"
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "1"   "$(jq -r '.pending | length' <<<"$snap")"   "snapshot pending count"
assert_eq "0"   "$(jq -r '.decisions | length' <<<"$snap")" "snapshot decisions count"
assert_eq "0"   "$(jq -r '.approved_today' <<<"$snap")"     "snapshot approved_today"
assert_eq "REV-001" "$(jq -r '.pending[0].id' <<<"$snap")"  "snapshot pending id"
assert_eq "medium"  "$(jq -r '.pending[0].risk' <<<"$snap")" "snapshot pending risk"
assert_eq "high"    "$(jq -r '.pending[0].auth' <<<"$snap")" "snapshot pending auth"
age=$(jq -r '.pending[0].age_seconds' <<<"$snap")
[[ "$age" =~ ^[0-9]+$ ]] || fail "pending age_seconds should be int, got '$age'"

# 3. emit-decided flips the pending out of the pending list and into decisions.
bash "$QUEUE_SH" emit-decided --id REV-001 --outcome approved > "$TMP/emit2.json"
emit2_kind=$(jq -r '.kind' < "$TMP/emit2.json")
assert_eq "decided" "$emit2_kind" "emitted event kind=decided"
emit2_outcome=$(jq -r '.outcome' < "$TMP/emit2.json")
assert_eq "approved" "$emit2_outcome" "emitted event outcome=approved"

bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "0" "$(jq -r '.pending | length' <<<"$snap")" "snapshot pending=0 after approve"
assert_eq "1" "$(jq -r '.decisions | length' <<<"$snap")" "snapshot decisions=1 after approve"
assert_eq "approved" "$(jq -r '.decisions[0].outcome' <<<"$snap")" "decision outcome"
assert_eq "apply_patch touching 3 files" "$(jq -r '.decisions[0].cmd' <<<"$snap")" "decision cmd hydrated from pending title"
assert_eq "codex-ricsi-zazrifka" "$(jq -r '.decisions[0].agent' <<<"$snap")" "decision agent hydrated from pending"
assert_eq "medium" "$(jq -r '.decisions[0].risk' <<<"$snap")" "decision risk hydrated from pending"
assert_eq "1" "$(jq -r '.approved_today' <<<"$snap")" "snapshot approved_today=1 after approve"

# 4. Renderer consumes the snapshot — pending list is empty now, so the calm
# empty-state card should appear, plus our recent approval should be in the rail.
render_out=$(REVIEW_ANIM_QUEUE_JSON="$REVIEW_QUEUE_JSON" bash "$ANIM_SH" --once)
render_plain=$(strip_ansi "$render_out")
assert_contains "0 awaiting" "$render_plain" "render reads snapshot pending count"
assert_contains "1 approved today" "$render_plain" "render reads approved_today"
assert_contains "queue clear" "$render_plain" "render shows empty-state card"
# The right-rail card is 42 wide and truncates long cmds with an ellipsis,
# so the assertion targets the stable prefix rather than the full title.
assert_contains "apply_patch touching" "$render_plain" "render shows the decision in the rail"
assert_contains "approved" "$render_plain" "render shows approved outcome pill"

# 5. A second pending after the approval should reappear in the pending list,
# proving the build is a pure function of the event log.
bash "$QUEUE_SH" emit-pending \
  --id REV-002 \
  --title "rm -rf .cap-probe-cache" \
  --agent codex-recodee-mite \
  --risk medium \
  --auth medium \
  --rationale "Cache wipe to force re-probe; reversible by re-running probe." \
  --file scripts/codex-fleet/cap-probe.sh \
  > /dev/null
bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "1" "$(jq -r '.pending | length' <<<"$snap")" "snapshot pending=1 after second emit"
assert_eq "REV-002" "$(jq -r '.pending[0].id' <<<"$snap")" "snapshot second pending id"
assert_eq "1" "$(jq -r '.decisions | length' <<<"$snap")" "decision rail still shows REV-001 approval"

# 6. clear empties the log and a rebuild produces an empty snapshot.
bash "$QUEUE_SH" clear
bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "0" "$(jq -r '.pending | length' <<<"$snap")" "snapshot empty after clear"
assert_eq "0" "$(jq -r '.decisions | length' <<<"$snap")" "decisions empty after clear"
assert_eq "0" "$(jq -r '.approved_today' <<<"$snap")" "approved_today=0 after clear"

# 7. show prints valid JSON (smoke).
show_out=$(bash "$QUEUE_SH" show)
jq -e . <<<"$show_out" >/dev/null || fail "show output is not valid JSON"

printf 'PASS test-review-queue\n'
