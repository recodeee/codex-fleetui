#!/usr/bin/env bash
#
# spawn-fleet — start a new parallel codex fleet without colliding with the
# main `codex-fleet` session. Picks the next free fleet ID automatically.
#
# Each fleet gets its own:
#   - tmux session pair (`codex-fleet-N` + `fleet-ticker-N`)
#   - state dir (`/tmp/claude-viz/fleet-N/`)
#   - supervisor + stall-watcher queue (scoped to that state dir)
#   - plan slug (pass `--plan-slug X` to pin a specific plan)
#
# Account pool is currently SHARED across fleets — split via
# `scripts/codex-fleet/accounts.yml` or pass a fleet-specific config via
# CODEX_FLEET_SUPERVISOR_CONFIG.
#
# Usage:
#   bash scripts/codex-fleet/spawn-fleet.sh                           # auto-pick ID, newest plan
#   bash scripts/codex-fleet/spawn-fleet.sh --plan-slug <slug>        # auto-pick ID, given plan
#   bash scripts/codex-fleet/spawn-fleet.sh --fleet-id 3              # explicit ID
#   bash scripts/codex-fleet/spawn-fleet.sh --fleet-id 3 --plan-slug <slug>
#   bash scripts/codex-fleet/spawn-fleet.sh --n 4 --no-attach         # smaller fleet, no attach

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLEET_ID=""
PASSTHROUGH=()

while [ $# -gt 0 ]; do
  case "$1" in
    --fleet-id) FLEET_ID="$2"; shift 2 ;;
    -h|--help) sed -n '1,22p' "$0"; exit 0 ;;
    *) PASSTHROUGH+=("$1"); shift ;;
  esac
done

# Auto-pick when no explicit ID given.
if [ -z "$FLEET_ID" ]; then
  if tmux has-session -t "codex-fleet" 2>/dev/null; then
    PASSTHROUGH=("--auto-fleet-id" "${PASSTHROUGH[@]}")
  else
    echo "[spawn-fleet] default codex-fleet session is free — bringing up the main fleet"
  fi
else
  PASSTHROUGH=("--fleet-id" "$FLEET_ID" "${PASSTHROUGH[@]}")
fi

exec bash "$SCRIPT_DIR/full-bringup.sh" "${PASSTHROUGH[@]}"
