#!/usr/bin/env bash
# down.sh — kill the fleet's tmux server.
#
# Targets only the dedicated `codex-fleet` socket, so the operator's normal
# tmux server (default socket) is untouched. Idempotent — exits 0 even
# when the server isn't running.
#
# Usage:
#   ./scripts/codex-fleet/tmux/down.sh

set -euo pipefail

SOCKET="codex-fleet"

if tmux -L "$SOCKET" kill-server 2>/dev/null; then
  echo "[tmux/down] killed fleet tmux server (socket=$SOCKET)"
else
  echo "[tmux/down] no fleet tmux server running on socket '$SOCKET'"
fi
