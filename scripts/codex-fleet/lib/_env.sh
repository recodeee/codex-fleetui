#!/usr/bin/env bash
# Shared codex-fleet environment defaults. Source this from fleet scripts.

_CODEX_FLEET_ENV_SOURCE="${BASH_SOURCE[0]}"
while [[ -L "$_CODEX_FLEET_ENV_SOURCE" ]]; do
  _CODEX_FLEET_ENV_DIR="$(cd -P -- "$(dirname -- "$_CODEX_FLEET_ENV_SOURCE")" && pwd)"
  _CODEX_FLEET_ENV_NEXT="$(readlink "$_CODEX_FLEET_ENV_SOURCE")"
  case "$_CODEX_FLEET_ENV_NEXT" in
    /*) _CODEX_FLEET_ENV_SOURCE="$_CODEX_FLEET_ENV_NEXT" ;;
    *) _CODEX_FLEET_ENV_SOURCE="$_CODEX_FLEET_ENV_DIR/$_CODEX_FLEET_ENV_NEXT" ;;
  esac
done

_CODEX_FLEET_ENV_DIR="$(cd -P -- "$(dirname -- "$_CODEX_FLEET_ENV_SOURCE")" && pwd)"

: "${CODEX_FLEET_REPO_ROOT:=$(cd "$_CODEX_FLEET_ENV_DIR/../../.." && pwd)}"
: "${CODEX_FLEET_SCRIPT_DIR:=$CODEX_FLEET_REPO_ROOT/scripts/codex-fleet}"
: "${CODEX_FLEET_LIB_DIR:=$CODEX_FLEET_SCRIPT_DIR/lib}"
: "${CODEX_FLEET_WORK_ROOT:=/tmp/codex-fleet}"

if [[ -z "${CODEX_FLEET_SESSION:-}" ]]; then
  if [[ -n "${FLEET_ID:-}" ]]; then
    CODEX_FLEET_SESSION="codex-fleet-$FLEET_ID"
  else
    CODEX_FLEET_SESSION="codex-fleet"
  fi
fi

if [[ -z "${CODEX_FLEET_TICKER_SESSION:-}" ]]; then
  if [[ -n "${FLEET_ID:-}" ]]; then
    CODEX_FLEET_TICKER_SESSION="fleet-ticker-$FLEET_ID"
  else
    CODEX_FLEET_TICKER_SESSION="fleet-ticker"
  fi
fi

if [[ -z "${CODEX_FLEET_STATE_DIR:-}" ]]; then
  if [[ -n "${FLEET_STATE_DIR:-}" ]]; then
    CODEX_FLEET_STATE_DIR="$FLEET_STATE_DIR"
  elif [[ -n "${FLEET_ID:-}" ]]; then
    CODEX_FLEET_STATE_DIR="/tmp/claude-viz/fleet-$FLEET_ID"
  else
    CODEX_FLEET_STATE_DIR="/tmp/claude-viz"
  fi
fi
: "${FLEET_STATE_DIR:=$CODEX_FLEET_STATE_DIR}"

: "${CODEX_FLEET_ACCOUNTS:=$CODEX_FLEET_SCRIPT_DIR/accounts.yml}"
: "${CODEX_FLEET_WORKER_PROMPT:=$CODEX_FLEET_SCRIPT_DIR/worker-prompt.md}"
: "${CODEX_FLEET_TAKEOVER_PROMPT:=$CODEX_FLEET_SCRIPT_DIR/takeover-prompt.md}"
: "${CODEX_FLEET_ACTIVE_FILE:=$CODEX_FLEET_STATE_DIR/fleet-active-accounts.txt}"
: "${CODEX_FLEET_SUPERVISOR_QUEUE:=$CODEX_FLEET_STATE_DIR/supervisor-queue.jsonl}"
: "${CODEX_FLEET_SUPERVISOR_STATE_DIR:=$CODEX_FLEET_STATE_DIR/supervisor}"

export CODEX_FLEET_REPO_ROOT
export CODEX_FLEET_SCRIPT_DIR
export CODEX_FLEET_LIB_DIR
export CODEX_FLEET_WORK_ROOT
export CODEX_FLEET_SESSION
export CODEX_FLEET_TICKER_SESSION
export CODEX_FLEET_STATE_DIR
export FLEET_STATE_DIR
export CODEX_FLEET_ACCOUNTS
export CODEX_FLEET_WORKER_PROMPT
export CODEX_FLEET_TAKEOVER_PROMPT
export CODEX_FLEET_ACTIVE_FILE
export CODEX_FLEET_SUPERVISOR_QUEUE
export CODEX_FLEET_SUPERVISOR_STATE_DIR

unset _CODEX_FLEET_ENV_SOURCE _CODEX_FLEET_ENV_DIR _CODEX_FLEET_ENV_NEXT
