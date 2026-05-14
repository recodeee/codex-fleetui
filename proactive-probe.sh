#!/usr/bin/env bash
# proactive-probe — keep a warm verdict map for reserve codex accounts.
#
# Every PROBE_PROACTIVE_INTERVAL seconds, scan ~/.codex/accounts/*.json,
# skip accounts already assigned to live fleet panes, run cap-probe.sh over
# the reserve set, then atomically publish:
#
#   /tmp/claude-viz/healthy-pool.txt
#
# Format: one line per reserve account:
#
#   email verdict until_epoch
#
# Usage:
#   bash scripts/codex-fleet/proactive-probe.sh
#   bash scripts/codex-fleet/proactive-probe.sh --once
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${REPO:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
ACCOUNTS_DIR="${ACCOUNTS_DIR:-$HOME/.codex/accounts}"
CACHE_DIR="${CACHE_DIR:-/tmp/claude-viz/cap-probe-cache}"
POOL_FILE="${POOL_FILE:-/tmp/claude-viz/healthy-pool.txt}"
LOG="${LOG:-/tmp/claude-viz/proactive-probe.log}"
PROBE_PROACTIVE_INTERVAL="${PROBE_PROACTIVE_INTERVAL:-300}"
CAP_PROBE_SCRIPT="${CAP_PROBE_SCRIPT:-$REPO/scripts/codex-fleet/cap-probe.sh}"
ONCE=0

usage() {
  sed -n '1,22p' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --once) ONCE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "proactive-probe: unknown arg: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$(dirname "$POOL_FILE")" "$(dirname "$LOG")" "$CACHE_DIR"

ts() { date +%H:%M:%S; }
log() { echo "[$(ts)] $*" >> "$LOG"; }

account_emails() {
  [[ -d "$ACCOUNTS_DIR" ]] || return 0
  find "$ACCOUNTS_DIR" -maxdepth 1 -type f -name '*.json' -printf '%f\n' \
    | sed 's/\.json$//' \
    | sort -u
}

current_fleet_emails() {
  local pid env_file email

  if [[ -n "${CODEX_FLEET_ACCOUNT_EMAIL:-}" ]]; then
    printf '%s\n' "$CODEX_FLEET_ACCOUNT_EMAIL"
  fi

  command -v pgrep >/dev/null 2>&1 || return 0
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    env_file="/proc/$pid/environ"
    [[ -r "$env_file" ]] || continue
    email=$({ tr '\0' '\n' < "$env_file" 2>/dev/null || true; } \
      | awk -F= '/^CODEX_FLEET_ACCOUNT_EMAIL=/{print $2; exit}')
    [[ -n "$email" ]] && printf '%s\n' "$email"
  done < <(pgrep -f '(^|/)codex([[:space:]]|$)' 2>/dev/null || true)
}

cache_line() {
  local email="$1" cf="$CACHE_DIR/${email}.json"
  python3 - "$email" "$cf" <<'PY'
import json
import sys

email, path = sys.argv[1], sys.argv[2]
try:
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
except Exception:
    print(f"{email} unknown 0")
    raise SystemExit

verdict = str(data.get("verdict") or "unknown")
try:
    until_epoch = int(data.get("until_epoch") or 0)
except Exception:
    until_epoch = 0
print(f"{email} {verdict} {until_epoch}")
PY
}

write_pool_atomically() {
  local tmp="$POOL_FILE.tmp.$$"
  trap 'rm -f "$tmp"' RETURN
  : > "$tmp"
  for email in "$@"; do
    cache_line "$email" >> "$tmp"
  done
  mv -f "$tmp" "$POOL_FILE"
  trap - RETURN
}

sweep_once() {
  local -a all=()
  local -a in_use=()
  local -a reserve=()
  local -A used=()
  local email probe_rc=0

  mapfile -t all < <(account_emails)
  mapfile -t in_use < <(current_fleet_emails | sort -u)

  for email in "${in_use[@]}"; do
    [[ -n "$email" ]] && used["$email"]=1
  done

  for email in "${all[@]}"; do
    [[ -n "$email" ]] || continue
    if [[ -z "${used[$email]+x}" ]]; then
      reserve+=("$email")
    fi
  done

  if [[ "${#reserve[@]}" -gt 0 ]]; then
    log "probing ${#reserve[@]} reserve account(s); skipped ${#in_use[@]} in-use account(s)"
    if CACHE_DIR="$CACHE_DIR" bash "$CAP_PROBE_SCRIPT" "${#reserve[@]}" "${reserve[@]}" >/dev/null 2>>"$LOG"; then
      :
    else
      probe_rc=$?
      log "cap-probe exited rc=$probe_rc; publishing cached verdicts anyway"
    fi
  else
    log "no reserve accounts to probe; skipped ${#in_use[@]} in-use account(s)"
  fi

  write_pool_atomically "${reserve[@]}"
  log "wrote $POOL_FILE with ${#reserve[@]} reserve verdict(s)"
}

log "proactive probe started (accounts=$ACCOUNTS_DIR interval=${PROBE_PROACTIVE_INTERVAL}s pool=$POOL_FILE)"
while :; do
  sweep_once
  [[ "$ONCE" -eq 1 ]] && exit 0
  sleep "$PROBE_PROACTIVE_INTERVAL"
done
