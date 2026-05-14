#!/usr/bin/env bash
# Smoke test for review-anim.sh: --once renders the demo fixture cleanly and
# external JSON queues override the fallback.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT="$ROOT/scripts/codex-fleet/review-anim.sh"
TMP="${TMPDIR:-/tmp}/claude-viz-test-review-anim-$$"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP"

fail() { printf 'FAIL %s\n' "$1" >&2; exit 1; }

assert_contains() {
  local needle="$1" haystack="$2" label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label: missing '$needle'"
}

strip_ansi() { sed -E $'s/\x1B\\[[0-9;]*m//g' <<<"$1"; }

# 1. Demo fixture render (no external JSON).
demo_out=$(REVIEW_ANIM_QUEUE_JSON="$TMP/missing.json" bash "$SCRIPT" --once)
demo_plain=$(strip_ansi "$demo_out")

assert_contains "REVIEW" "$demo_plain" "demo header"
assert_contains "1 awaiting" "$demo_plain" "demo awaiting count"
assert_contains "124 approved today" "$demo_plain" "demo approved-today"
assert_contains "REV-014" "$demo_plain" "demo review id"
assert_contains "apply_patch touching 3 files" "$demo_plain" "demo title"
assert_contains "AUTO-REVIEWER RATIONALE" "$demo_plain" "rationale label"
assert_contains "FILES TOUCHED" "$demo_plain" "files-touched label"
assert_contains "scripts/codex-fleet/lib/_env.sh" "$demo_plain" "file row 1"
assert_contains "docs/cockpit.md" "$demo_plain" "file row 3"
assert_contains "A · Approve" "$demo_plain" "approve button"
assert_contains "V · View diff" "$demo_plain" "view-diff button"
assert_contains "D · Deny" "$demo_plain" "deny button"
assert_contains "Recent decisions" "$demo_plain" "decisions header"
assert_contains "approved" "$demo_plain" "decision outcome"
assert_contains "escalated" "$demo_plain" "escalated outcome"
assert_contains "denied" "$demo_plain" "denied outcome"
assert_contains "Review — approval queue" "$demo_plain" "footer text"

# iOS palette truecolor codes present in the raw ANSI output. systemRed appears
# as a chip background (auth high, denied, Deny button) rather than foreground
# in the demo render, so the assertion targets the 48-channel variant.
assert_contains $'\033[38;2;0;122;255m' "$demo_out" "systemBlue ANSI"
assert_contains $'\033[38;2;52;199;89m' "$demo_out" "systemGreen ANSI"
assert_contains $'\033[48;2;255;59;48m' "$demo_out" "systemRed ANSI (bg)"
assert_contains $'\033[38;2;255;149;0m' "$demo_out" "systemOrange ANSI"
assert_contains $'\033[38;2;255;204;0m' "$demo_out" "systemYellow ANSI"

# 2. External JSON queue overrides the demo fixture.
cat > "$TMP/queue.json" <<'JSON'
{
  "approved_today": 7,
  "pending": [
    {
      "id": "REV-099",
      "age_seconds": 12,
      "title": "external probe",
      "agent": "codex-test-agent",
      "pane": 1,
      "risk": "high",
      "auth": "low",
      "rationale": "Synthetic test payload to confirm JSON load path.",
      "files": ["a.sh"]
    }
  ],
  "decisions": [
    { "cmd": "echo hello", "agent": "codex-test-agent", "age_minutes": 1,
      "risk": "low", "outcome": "approved" }
  ]
}
JSON

ext_out=$(REVIEW_ANIM_QUEUE_JSON="$TMP/queue.json" bash "$SCRIPT" --once)
ext_plain=$(strip_ansi "$ext_out")

assert_contains "REV-099" "$ext_plain" "external review id"
assert_contains "1 awaiting" "$ext_plain" "external awaiting count"
assert_contains "7 approved today" "$ext_plain" "external approved-today"
assert_contains "external probe" "$ext_plain" "external title"
assert_contains "Synthetic test payload" "$ext_plain" "external rationale text"
assert_contains "echo hello" "$ext_plain" "external decision cmd"

# Confirm the demo fixture leaked nothing once the external JSON is in play.
[[ "$ext_plain" == *"REV-014"* ]] && fail "external render leaked demo REV-014"
[[ "$ext_plain" == *"124 approved today"* ]] && fail "external render leaked demo approved-today"

# 3. Empty queue (zero pending) collapses to the calm empty-state card.
cat > "$TMP/empty.json" <<'JSON'
{ "approved_today": 250, "pending": [], "decisions": [] }
JSON
empty_out=$(REVIEW_ANIM_QUEUE_JSON="$TMP/empty.json" bash "$SCRIPT" --once)
empty_plain=$(strip_ansi "$empty_out")
assert_contains "0 awaiting" "$empty_plain" "empty awaiting count"
assert_contains "250 approved today" "$empty_plain" "empty approved-today"
assert_contains "queue clear" "$empty_plain" "empty-state message"

printf 'PASS test-review-anim\n'
