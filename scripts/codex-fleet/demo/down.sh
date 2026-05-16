#!/usr/bin/env bash
# Tear down the codex-fleet demo brought up by up.sh. Kills the tmux
# session, stops the tick simulator, removes synthetic state. Does NOT
# remove the openspec/plans/demo-refactor-wave-2026-05-16 fixture
# (that's committed to the repo). Safe to run repeatedly.
set -euo pipefail

SOCKET="${CODEX_FLEET_DEMO_SOCKET:-codex-fleet-demo}"
SESSION="codex-fleet-demo"
STATE_DIR="/tmp/claude-viz"
DEMO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$DEMO_DIR/../../.." && pwd)"
PLAN_SLUG="demo-refactor-wave-2026-05-16"
PLAN_RUNTIME="$REPO_ROOT/openspec/plans/$PLAN_SLUG/plan.json"

# Tick simulator
if [[ -f "$STATE_DIR/demo-tick.pid" ]]; then
    pid="$(cat "$STATE_DIR/demo-tick.pid")"
    kill "$pid" 2>/dev/null || true
    rm -f "$STATE_DIR/demo-tick.pid"
fi
pkill -f 'codex-fleet/demo/tick.sh' 2>/dev/null || true

# Tmux session
tmux -L "$SOCKET" kill-session -t "$SESSION" 2>/dev/null || true
tmux -L "$SOCKET" kill-server 2>/dev/null || true

# Synthetic state files
rm -f "$STATE_DIR/demo-active" \
      "$STATE_DIR/demo-current-account" \
      "$STATE_DIR/demo-tick.log" \
      "$STATE_DIR/fleet-tab-counters.json" \
      "$STATE_DIR/fleet-quality-scores.json" \
      "$STATE_DIR/plan-tree-pin.txt"
rm -rf "$STATE_DIR/demo-panes"

# Runtime plan copy (template stays in scripts/codex-fleet/demo/scenarios/).
if [[ -f "$PLAN_RUNTIME" ]]; then
    rm -f "$PLAN_RUNTIME"
    rmdir "$(dirname "$PLAN_RUNTIME")" 2>/dev/null || true
fi

echo "demo down."
