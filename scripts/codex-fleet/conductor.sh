#!/usr/bin/env bash
#
# conductor.sh — launch the interactive fleet conductor in a tmux window.
#
# The conductor is a `claude` CLI session with a custom system prompt that
# briefs it on the autonomous fleet daemons, Colony as the shared context
# bus, and the limited bash tool surface it should use. The operator
# attaches to `codex-fleet:conductor` and chats with it directly.
#
# Wired into full-bringup.sh as window `conductor` (skip with
# CODEX_FLEET_CONDUCTOR=0). Runs interactively (no --print loop). On exit
# the tmux window stays open via remain-on-exit so the operator can read
# any final output before relaunching.
#
# Optional env:
#   CLAUDE_CONDUCTOR_CONFIG_DIR  CLAUDE_CONFIG_DIR for this session.
#                                Defaults to host ~/.claude/. Set to a
#                                dedicated dir if you want to log in under
#                                a separate Anthropic account.
#   CLAUDE_CONDUCTOR_API_KEY     If set, exported as ANTHROPIC_API_KEY.
#                                Mutually exclusive with CONFIG_DIR.
#   CLAUDE_CONDUCTOR_MODEL       Model alias (default: sonnet).
#   CLAUDE_CONDUCTOR_LOG_DIR     Log dir (default /tmp/claude-viz).
#   CLAUDE_CONDUCTOR_ADD_DIRS    Extra --add-dir roots (space-separated).

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROMPT_FILE="$SCRIPT_DIR/conductor-system-prompt.md"
MODEL="${CLAUDE_CONDUCTOR_MODEL:-sonnet}"
LOG_DIR="${CLAUDE_CONDUCTOR_LOG_DIR:-/tmp/claude-viz}"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/conductor.log"

if [ ! -f "$PROMPT_FILE" ]; then
  echo "[conductor] fatal: system prompt missing at $PROMPT_FILE" >&2
  exit 2
fi

if ! command -v claude >/dev/null 2>&1; then
  echo "[conductor] fatal: claude CLI not on PATH" >&2
  exit 2
fi

# Auth selection. Defaults to host ~/.claude/ so the conductor reuses the
# operator's existing login. Operators wanting an isolated account can set
# CLAUDE_CONDUCTOR_CONFIG_DIR and pre-login via:
#   CLAUDE_CONFIG_DIR=<dir> claude /login
if [ -n "${CLAUDE_CONDUCTOR_CONFIG_DIR:-}" ]; then
  mkdir -p "$CLAUDE_CONDUCTOR_CONFIG_DIR"
  export CLAUDE_CONFIG_DIR="$CLAUDE_CONDUCTOR_CONFIG_DIR"
  if [ ! -f "$CLAUDE_CONFIG_DIR/auth.json" ] && [ ! -f "$CLAUDE_CONFIG_DIR/credentials.json" ]; then
    printf '[conductor] fatal: CLAUDE_CONDUCTOR_CONFIG_DIR=%s has no auth — run:\n' "$CLAUDE_CONFIG_DIR" >&2
    printf '  CLAUDE_CONFIG_DIR=%s claude /login\n' "$CLAUDE_CONFIG_DIR" >&2
    exit 2
  fi
elif [ -n "${CLAUDE_CONDUCTOR_API_KEY:-}" ]; then
  export ANTHROPIC_API_KEY="$CLAUDE_CONDUCTOR_API_KEY"
fi

ADD_DIR_FLAGS=( --add-dir "$REPO_ROOT" --add-dir "/tmp/claude-viz" )
if [ -n "${CLAUDE_CONDUCTOR_ADD_DIRS:-}" ]; then
  for d in $CLAUDE_CONDUCTOR_ADD_DIRS; do
    ADD_DIR_FLAGS+=( --add-dir "$d" )
  done
fi

# Banner so the operator can grep it from `tmux capture-pane`.
{
  printf '\n========== fleet conductor boot ==========\n'
  printf 'model=%s log=%s prompt=%s\n' "$MODEL" "$LOG_FILE" "$PROMPT_FILE"
  printf 'cwd=%s\n' "$REPO_ROOT"
  printf 'add-dir: %s\n' "${ADD_DIR_FLAGS[*]}"
  printf 'channels: /tmp/claude-viz/conductor-broadcasts.jsonl (fleet broadcast) · colony task_post (task-scoped) · tmux capture-pane (read-only)\n'
  printf '==========================================\n\n'
} | tee -a "$LOG_FILE"

cd "$REPO_ROOT"

# Interactive — no --print. --append-system-prompt injects the conductor
# brief on top of Claude Code's default system prompt so the model still
# has its standard tool list (Bash, Read, Edit, ...). Bash is the primary
# tool surface for the conductor.
exec claude \
  --model "$MODEL" \
  --permission-mode bypassPermissions \
  --dangerously-skip-permissions \
  --append-system-prompt "$(cat "$PROMPT_FILE")" \
  "${ADD_DIR_FLAGS[@]}"
