#!/usr/bin/env bash
#
# stall-watcher — auto-release stranded plan claims so the queue keeps moving,
# and hand the released slot to the supervisor for takeover-worker spawning.
#
# Failure mode this fixes: one codex agent claims a sub-task, then dies (cap
# hit, user kill, session expired, idle wait that never resumes). The
# stale claim blocks every downstream sub-task that depends on it; other
# agents poll `task_ready_for_agent`, see `ready: []`, sleep 60s, repeat.
# The whole fleet idles on a queue wedged by ONE dead claim.
#
# Loop:
#   1. `colony rescue stranded --older-than <threshold> --apply --json`
#   2. For each rescued agent, append a `takeover_recommended` event to the
#      supervisor queue so supervisor.sh spawns a fresh codex worker.
#   3. notify-send + log.
#
# Idempotent against re-runs: colony's rescue command is itself idempotent,
# and the supervisor queue dedupes by (agent, ts_min) key.
#
# Usage:
#   bash scripts/codex-fleet/stall-watcher.sh
#   STALL_WATCHER_OLDER_THAN=20m bash scripts/codex-fleet/stall-watcher.sh
#   STALL_WATCHER_INTERVAL=30 bash scripts/codex-fleet/stall-watcher.sh
#   bash scripts/codex-fleet/stall-watcher.sh --once       # single tick + exit
#   bash scripts/codex-fleet/stall-watcher.sh --dry-run    # scan only

set -eo pipefail

OLDER_THAN="${STALL_WATCHER_OLDER_THAN:-30m}"
INTERVAL="${STALL_WATCHER_INTERVAL:-60}"
# Per-fleet state dir — full-bringup.sh exports FLEET_STATE_DIR scoped to
# /tmp/claude-viz/fleet-<id> when --fleet-id is set; defaults to
# /tmp/claude-viz for single-fleet back-compat.
FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
QUEUE_FILE="${STALL_WATCHER_QUEUE:-$FLEET_STATE_DIR/supervisor-queue.jsonl}"
LOG_FILE="${STALL_WATCHER_LOG:-$FLEET_STATE_DIR/stall-watcher.log}"
NOTIFY="${STALL_WATCHER_NOTIFY:-1}"
ONCE=0
APPLY_FLAG="--apply"

while [ $# -gt 0 ]; do
  case "$1" in
    --once) ONCE=1; shift ;;
    --dry-run) APPLY_FLAG="--dry-run"; shift ;;
    --older-than) OLDER_THAN="$2"; shift 2 ;;
    --interval) INTERVAL="$2"; shift 2 ;;
    -h|--help) sed -n '1,30p' "$0"; exit 0 ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$(dirname "$QUEUE_FILE")" "$(dirname "$LOG_FILE")"
touch "$QUEUE_FILE" "$LOG_FILE"

log() {
  local ts; ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '%s [stall-watcher] %s\n' "$ts" "$*" | tee -a "$LOG_FILE" >&2
}

tick() {
  local out
  if ! out="$(colony rescue stranded --older-than "$OLDER_THAN" $APPLY_FLAG --json 2>>"$LOG_FILE")"; then
    log "colony rescue failed (non-zero exit)"
    return 0
  fi

  # Parse the JSON output: extract scanned count + each stranded agent.
  # Fail-soft: if jq is missing or output isn't JSON, log and move on.
  local scanned stranded_count
  if ! command -v jq >/dev/null 2>&1; then
    log "jq not on PATH; cannot parse rescue output"
    return 0
  fi
  scanned="$(printf '%s' "$out" | jq -r '.scanned // 0' 2>/dev/null || echo 0)"
  stranded_count="$(printf '%s' "$out" | jq -r '(.stranded // []) | length' 2>/dev/null || echo 0)"

  if [ "$stranded_count" -eq 0 ]; then
    log "scanned=$scanned stranded=0"
    return 0
  fi

  log "scanned=$scanned stranded=$stranded_count → rescuing"

  # For each stranded session, enqueue a takeover_recommended event so
  # supervisor.sh spawns a fresh codex worker for the same agent slot.
  printf '%s' "$out" \
    | jq -c '(.stranded // [])[] | {agent, session_id, task_ids, held_claim_count, last_activity}' \
    | while IFS= read -r row; do
        local agent ts ts_min reason
        agent="$(printf '%s' "$row" | jq -r '.agent // "unknown"')"
        ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        ts_min="$(date -u +%Y-%m-%dT%H:%M)"
        reason="stranded ($(printf '%s' "$row" | jq -r '.held_claim_count // 0') held claims)"
        # Append the takeover event in the schema supervisor.sh expects.
        # Fields match the existing /tmp/claude-viz/supervisor-queue.jsonl
        # entries (see queue_event_keys + parse loop in supervisor.sh).
        printf '{"ts":"%s","ts_min":"%s","agent":"%s","email":"","reason":"%s","action":"takeover_recommended"}\n' \
          "$ts" "$ts_min" "$agent" "$reason" >>"$QUEUE_FILE"
        log "queued takeover for agent=$agent reason=\"$reason\""
        if [ "$NOTIFY" = "1" ] && command -v notify-send >/dev/null 2>&1; then
          notify-send -t 4000 "codex-fleet: stranded claim rescued" \
            "agent=$agent — takeover queued"
        fi
      done
}

log "starting (older-than=$OLDER_THAN interval=${INTERVAL}s apply=$APPLY_FLAG queue=$QUEUE_FILE)"

if [ "$ONCE" = "1" ]; then
  tick
  exit 0
fi

while :; do
  tick || true
  sleep "$INTERVAL"
done
