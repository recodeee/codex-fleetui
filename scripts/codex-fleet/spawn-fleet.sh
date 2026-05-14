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
#   bash scripts/codex-fleet/spawn-fleet.sh --runtime claude          # Claude CLI workers (fallback when codex is capped)
#
# Runtime selection:
#   --runtime codex   (default) spawn `codex --dangerously-bypass-approvals-and-sandbox`
#                     in each pane, authed against the matching account row from
#                     scripts/codex-fleet/accounts.yml. Requires healthy codex accounts.
#   --runtime claude  Spawn `claude --print --permission-mode bypassPermissions <wake>`
#                     in each pane instead. Use when every codex account is capped
#                     or the worker pool is exhausted. Trade-off: claude CLI has
#                     its own auth + permission flow; the wake-prompt is identical
#                     so the worker behaviour matches once authed.

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FLEET_ID=""
RUNTIME=""
PASSTHROUGH=()

while [ $# -gt 0 ]; do
  case "$1" in
    --fleet-id) FLEET_ID="$2"; shift 2 ;;
    --runtime) RUNTIME="$2"; shift 2 ;;
    --runtime=*) RUNTIME="${1#*=}"; shift ;;
    -h|--help) sed -n '1,22p' "$0"; exit 0 ;;
    *) PASSTHROUGH+=("$1"); shift ;;
  esac
done

# Forward the runtime choice through to full-bringup.sh / the pane respawn
# step. `codex` is the default (the existing CODEX_HOME + auth flow).
# `claude` is the fallback for when every codex account is capped /
# unavailable — it spawns the local `claude` CLI with the worker prompt
# instead of `codex`. The actual branching lives in full-bringup.sh's
# pane-spawn block; here we just export the choice.
if [ -n "$RUNTIME" ]; then
  case "$RUNTIME" in
    codex|claude) export CODEX_FLEET_RUNTIME="$RUNTIME" ;;
    *) echo "[spawn-fleet] fatal: unknown --runtime '$RUNTIME' (expected codex|claude)" >&2; exit 2 ;;
  esac
fi

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
