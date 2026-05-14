#!/usr/bin/env bash
#
# codex-fleet down — tear down the tmux fleet session and optionally
# wipe the staged CODEX_HOME directories. Auth tokens live in the staged
# auth.json files (mode 600); preserve them by default so a re-up
# doesn't have to re-stage from `~/.codex/accounts/`.
#
# Usage:
#   bash scripts/codex-fleet/down.sh                  # kill tmux session, preserve work-root
#   bash scripts/codex-fleet/down.sh --purge          # also rm -rf the work-root
#   bash scripts/codex-fleet/down.sh --session NAME   # non-default session name

set -euo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
PURGE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session) SESSION="$2"; shift 2 ;;
    --work-root) WORK_ROOT="$2"; shift 2 ;;
    --purge) PURGE=1; shift ;;
    -h|--help) sed -n '1,15p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

if ! command -v tmux >/dev/null 2>&1; then
  echo "fatal: tmux not on PATH" >&2
  exit 2
fi

if tmux has-session -t "$SESSION" 2>/dev/null; then
  tmux kill-session -t "$SESSION"
  echo "[codex-fleet] killed tmux session: $SESSION"
else
  echo "[codex-fleet] no tmux session named $SESSION (already down)"
fi

if [[ "$PURGE" -eq 1 ]]; then
  if [[ "$WORK_ROOT" == "/" || "$WORK_ROOT" == "$HOME" ]]; then
    echo "fatal: refusing to purge work-root $WORK_ROOT (looks like a sensitive path)" >&2
    exit 2
  fi
  rm -rf "$WORK_ROOT"
  echo "[codex-fleet] purged work-root: $WORK_ROOT"
else
  echo "[codex-fleet] work-root preserved: $WORK_ROOT (pass --purge to wipe)"
fi
