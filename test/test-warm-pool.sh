#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP="${TMPDIR:-/tmp}/claude-viz-test-warm-pool-$$"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP"

export WARM_POOL_SOURCE_ONLY=1
export WARM_POOL_HEALTHY_POOL="$TMP/healthy-pool.txt"
export CODEX_FLEET_ACCOUNTS="$TMP/accounts.yml"
export CODEX_FLEET_ACTIVE_FILE="$TMP/active.txt"
export CODEX_FLEET_WORK_ROOT="$TMP/work"

# shellcheck source=/dev/null
source "$ROOT/scripts/codex-fleet/warm-pool.sh"

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

assert_eq() {
  local expected="$1" actual="$2" label="$3"
  [[ "$actual" == "$expected" ]] || fail "$label: expected $expected, got $actual"
}

cat >"$HEALTHY_POOL" <<'EOF'
alpha@example.test healthy 0
beta@example.test capped 1778760000
gamma@example.test unknown 0
legacy@example.test
alpha@example.test healthy 0
EOF

mapfile -t healthy < <(healthy_pool_emails)

assert_eq 2 "${#healthy[@]}" "healthy account count"
assert_eq "alpha@example.test" "${healthy[0]}" "first healthy account"
assert_eq "legacy@example.test" "${healthy[1]}" "legacy bare-email account"

printf 'warm-pool tests passed\n'
