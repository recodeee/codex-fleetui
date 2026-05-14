#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="${TMPDIR:-/tmp}/claude-viz-test-status-chips-$$"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP"
mkdir -p "$TMP/bin"
printf '{"tasks":[]}\n' > "$TMP/plan.json"
: > "$TMP/active.txt"
cat > "$TMP/bin/codex-auth" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "list" ]]; then
  printf '  chip@example.com type=ChatGPT seat (Business) 5h=0%% weekly=10%%\n'
fi
SH
chmod +x "$TMP/bin/codex-auth"
export PATH="$TMP/bin:$PATH"

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

running_chip=$(ios_worker_chip running)
idle_chip=$(ios_worker_chip idle)
exhausted_chip=$(ios_worker_chip exhausted)
limited_chip=$(ios_worker_chip rate_limited)
working_chip=$(ios_worker_chip working)

assert_eq "◖ ● running ◗" "$(strip_ansi "$running_chip")" "running chip text"
assert_eq "◖ ◌ idle    ◗" "$(strip_ansi "$idle_chip")" "idle chip text"
assert_eq "◖ ⚠ exhaust ◗" "$(strip_ansi "$exhausted_chip")" "exhausted chip text"
assert_eq "◖ ◍ limited ◗" "$(strip_ansi "$limited_chip")" "limited chip text"
assert_eq "◖ ● working ◗" "$(strip_ansi "$working_chip")" "working chip text"

expected_len=$(ios_visible_len "$running_chip")
for chip in "$idle_chip" "$exhausted_chip" "$limited_chip" "$working_chip"; do
  assert_eq "$expected_len" "$(ios_visible_len "$chip")" "status chip visible width"
done

assert_contains "$running_chip" "$IOS_BG_GREEN" "running chip bg"
assert_contains "$idle_chip" "$IOS_BG_GRAY" "idle chip bg"
assert_contains "$exhausted_chip" "$IOS_BG_RED" "exhausted chip bg"
assert_contains "$limited_chip" "$IOS_BG_ORANGE" "limited chip bg"
assert_contains "$working_chip" "$IOS_BG_BLUE" "working chip bg"

assert_eq "#34C759" "$(ios_status_chip_hex running)" "running chip hex"
assert_eq "#8E8E93" "$(ios_status_chip_hex idle)" "idle chip hex"
assert_eq "#FF3B30" "$(ios_status_chip_hex exhausted)" "exhausted chip hex"
assert_eq "#FF9500" "$(ios_status_chip_hex rate_limited)" "rate-limited chip hex"
assert_eq "#007AFF" "$(ios_status_chip_hex working)" "working chip hex"

tmux_chip=$(tmux_status_chip running)
assert_eq "#[bg=#34C759,fg=#FFFFFF,bold]◖ ● running ◗#[default]" "$tmux_chip" "tmux running chip"
assert_eq "#[bg=#8E8E93,fg=#FFFFFF,bold]◖ ◌ idle    ◗#[default]" "$(tmux_status_chip idle)" "tmux idle chip"
assert_contains "$(tmux_status_chip exhausted)" "#[bg=#FF3B30,fg=#FFFFFF,bold]" "tmux exhausted chip"
assert_contains "$(tmux_status_chip rate_limited)" "#[bg=#FF9500,fg=#FFFFFF,bold]" "tmux limited chip"
assert_contains "$(tmux_status_chip working)" "#[bg=#007AFF,fg=#FFFFFF,bold]" "tmux working chip"

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

assert_file_contains "$FLEET_TICK_STATE_OUT" "WORKER" "worker status heading"
assert_file_contains "$FLEET_TICK_STATE_OUT" "◖" "status chip left radius"
assert_file_contains "$FLEET_TICK_STATE_OUT" "◗" "status chip right radius"
assert_file_contains "$FLEET_TICK_STATE_OUT" "◌ idle" "reserve idle chip"
assert_file_contains "$FLEET_TICK_STATE_OUT" "chip" "stub account rendered"
assert_file_contains "$FLEET_TICK_STATE_OUT" $'\033[48;2;142;142;147m' "idle chip background"

printf 'status chip tests passed\n'
