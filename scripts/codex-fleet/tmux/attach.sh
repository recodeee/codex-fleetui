#!/usr/bin/env bash
# attach.sh — attach to the fleet's tmux session on its dedicated socket.
#
# The fleet runs on a separate tmux server (`-L codex-fleet`) so the
# operator's normal `tmux attach` (default socket) never sees it. This
# wrapper exists so the operator doesn't have to remember the socket name.
#
# Usage:
#   ./scripts/codex-fleet/tmux/attach.sh
#   ./scripts/codex-fleet/tmux/attach.sh other-window-name

set -euo pipefail

SOCKET="codex-fleet"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"

if ! tmux -L "$SOCKET" has-session -t "$SESSION" 2>/dev/null; then
  echo "[tmux/attach] no fleet session '$SESSION' on socket '$SOCKET'" >&2
  echo "[tmux/attach] run ./scripts/codex-fleet/tmux/up.sh first." >&2
  exit 1
fi

if [[ $# -ge 1 ]]; then
  exec tmux -L "$SOCKET" attach -t "$SESSION:$1"
fi
exec tmux -L "$SOCKET" attach -t "$SESSION"
