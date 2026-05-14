#!/usr/bin/env bash
# Test review-pane-scanner.sh: fixture-driven (tmux not required).
#   1. A pane capture containing an "Automatic approval review approved" block
#      should produce one pending + one decided event in the queue.
#   2. Rerunning the scanner on the same fixture must NOT re-emit (dedup).
#   3. A pane capture with no review block produces no events.
#   4. "denied" outcome lands in decisions with outcome=denied.
#   5. "pending" (no outcome line) emits a pending event but no decided event.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCAN_SH="$ROOT/scripts/codex-fleet/review-pane-scanner.sh"
QUEUE_SH="$ROOT/scripts/codex-fleet/review-queue.sh"

TMP="${TMPDIR:-/tmp}/claude-viz-test-review-pane-scanner-$$"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP"

export REVIEW_EVENTS_LOG="$TMP/review-events.jsonl"
export REVIEW_QUEUE_JSON="$TMP/live-review-queue.json"
export REVIEW_SCANNER_STATE="$TMP/scanner-state.txt"
export REVIEW_QUEUE_SH="$QUEUE_SH"

fail() { printf 'FAIL %s\n' "$1" >&2; exit 1; }

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  [[ "$actual" == "$expected" ]] || fail "$label: expected '$expected', got '$actual'"
}

assert_contains() {
  local needle="$1" haystack="$2" label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label: missing '$needle'"
}

# 1. Auto-approved block — should emit both pending + decided(approved).
cat > "$TMP/pane-approved.txt" <<'PANE'
Ran colony task ready --session
  019e2685-a80f-7e72-8461-88d413c4d746
  … +180 lines (ctrl + t to view)

● Working (10m 28s · esc to interrupt)

⚠ Automatic approval review approved
(risk: medium, authorization: high)
✓ Request approved for apply_patch
touching 3 files
● Working (10m 28s)
PANE

bash "$SCAN_SH" --once --fixture "$TMP/pane-approved.txt" --fixture-agent codex-ricsi-zazrifka

bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "0" "$(jq -r '.pending | length' <<<"$snap")" "approved → pending count 0"
assert_eq "1" "$(jq -r '.decisions | length' <<<"$snap")" "approved → decisions count 1"
assert_eq "approved" "$(jq -r '.decisions[0].outcome' <<<"$snap")" "decision outcome=approved"
assert_eq "1" "$(jq -r '.approved_today' <<<"$snap")" "approved_today=1"
# Title was hydrated from "apply_patch" + trailing "touching 3 files".
title=$(jq -r '.decisions[0].cmd' <<<"$snap")
assert_contains "apply_patch" "$title" "decision cmd contains apply_patch"
assert_contains "touching 3 files" "$title" "decision cmd contains trailing detail"
assert_eq "medium" "$(jq -r '.decisions[0].risk' <<<"$snap")" "decision risk=medium"
assert_eq "codex-ricsi-zazrifka" "$(jq -r '.decisions[0].agent' <<<"$snap")" "decision agent from fixture"

# 2. Rerunning the scanner on the same fixture must NOT add a second event pair.
bash "$SCAN_SH" --once --fixture "$TMP/pane-approved.txt" --fixture-agent codex-ricsi-zazrifka
bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "1" "$(jq -r '.decisions | length' <<<"$snap")" "dedup keeps decisions=1"
events_count=$(wc -l < "$REVIEW_EVENTS_LOG")
assert_eq "2" "$events_count" "dedup keeps events log at 2 (pending+decided)"

# 3. Pane with no review block emits nothing.
cat > "$TMP/pane-quiet.txt" <<'PANE'
● Working (1m 22s)
Reading scripts/codex-fleet/lib/_env.sh
PANE
: > "$REVIEW_SCANNER_STATE"  # clear seen-set so a "no match" run is meaningful
prev=$(wc -l < "$REVIEW_EVENTS_LOG")
bash "$SCAN_SH" --once --fixture "$TMP/pane-quiet.txt" --fixture-agent codex-quiet
after=$(wc -l < "$REVIEW_EVENTS_LOG")
assert_eq "$prev" "$after" "quiet pane emits no events"

# 4. Denied block lands in decisions with outcome=denied.
cat > "$TMP/pane-denied.txt" <<'PANE'
⚠ Automatic approval review denied
(risk: high, authorization: low)
✗ Request denied for curl https://api.colony.example
● Idle
PANE
bash "$SCAN_SH" --once --fixture "$TMP/pane-denied.txt" --fixture-agent codex-denied-agent
bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
# We accumulated: 1 approved + 1 denied = 2 decisions, approved_today still 1.
assert_eq "2" "$(jq -r '.decisions | length' <<<"$snap")" "after denied → decisions=2"
denied_outcome=$(jq -r '[.decisions[] | select(.outcome=="denied")] | .[0].outcome' <<<"$snap")
assert_eq "denied" "$denied_outcome" "denied decision present"
assert_eq "1" "$(jq -r '.approved_today' <<<"$snap")" "approved_today still 1 after denied"

# 5. Pending (no outcome line) emits a pending event only.
cat > "$TMP/pane-pending.txt" <<'PANE'
⚠ Automatic approval review pending
(risk: medium, authorization: high)
● Awaiting human reviewer…
PANE
bash "$SCAN_SH" --once --fixture "$TMP/pane-pending.txt" --fixture-agent codex-pending-agent
bash "$QUEUE_SH" build
snap=$(cat "$REVIEW_QUEUE_JSON")
assert_eq "1" "$(jq -r '.pending | length' <<<"$snap")" "pending block produces 1 awaiting"
assert_eq "codex-pending-agent" "$(jq -r '.pending[0].agent' <<<"$snap")" "pending agent from fixture"
assert_eq "medium" "$(jq -r '.pending[0].risk' <<<"$snap")" "pending risk=medium"
assert_eq "high"   "$(jq -r '.pending[0].auth' <<<"$snap")" "pending auth=high"

# 6. --dry-run does not mutate the events log.
prev=$(wc -l < "$REVIEW_EVENTS_LOG")
: > "$REVIEW_SCANNER_STATE"
bash "$SCAN_SH" --once --dry-run --fixture "$TMP/pane-approved.txt" --fixture-agent codex-dry 2>"$TMP/dry.log"
after=$(wc -l < "$REVIEW_EVENTS_LOG")
assert_eq "$prev" "$after" "dry-run leaves events log untouched"
dry_log=$(cat "$TMP/dry.log")
assert_contains "[dry-run] emit-pending" "$dry_log" "dry-run logged emit-pending"
assert_contains "[dry-run] emit-decided" "$dry_log" "dry-run logged emit-decided"

printf 'PASS test-review-pane-scanner\n'
