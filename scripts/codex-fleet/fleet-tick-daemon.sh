#!/usr/bin/env bash
# fleet-tick-daemon — respawn-safe wrapper around fleet-tick.sh.
#
# fleet-tick.sh runs with `set -eo pipefail` and any failed pipe / regex /
# subshell kills the long-running loop. The pane silently dies, the file
# stops updating, and the viz panes go stale. This wrapper runs the script
# in one-shot mode (FLEET_TICK_ONCE=1) inside a top-level loop, so a single
# tick crashing doesn't kill the daemon — the next tick gets a fresh shell.
#
# Usage:
#   bash scripts/codex-fleet/fleet-tick-daemon.sh
#   FLEET_TICK_INTERVAL=2 bash scripts/codex-fleet/fleet-tick-daemon.sh
set +e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Autodetect REPO from the clone location; env override wins.
REPO="${FLEET_TICK_DAEMON_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}}"
SCRIPT="$SCRIPT_DIR/fleet-tick.sh"
INTERVAL="${FLEET_TICK_INTERVAL:-2}"
LOG_DIR="${FLEET_TICK_DAEMON_LOG_DIR:-/tmp/claude-viz}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/fleet-tick-daemon.log"

trap 'echo "[$(date +%T)] daemon stopping" >>"$LOG"; exit 0' INT TERM

echo "[$(date +%T)] daemon starting (interval=${INTERVAL}s, script=$SCRIPT)" >>"$LOG"

while true; do
  start=$(date +%s)
  FLEET_TICK_ONCE=1 bash "$SCRIPT" >>"$LOG.out" 2>>"$LOG.err"
  rc=$?
  if (( rc != 0 )); then
    echo "[$(date +%T)] tick rc=$rc — continuing" >>"$LOG"
  fi
  now=$(date +%s)
  elapsed=$(( now - start ))
  remain=$(( INTERVAL - elapsed ))
  (( remain < 0 )) && remain=0
  sleep "$remain"
done
