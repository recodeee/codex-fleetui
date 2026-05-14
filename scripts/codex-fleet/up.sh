#!/usr/bin/env bash
#
# codex-fleet up — spawn a tmux session of codex worker panes, each
# logged in under a different `~/.codex/accounts/<email>.json` via an
# isolated `CODEX_HOME`. Each pane runs `codex` with the worker-loop
# prompt that calls Colony MCP tools to pull and execute tasks.
#
# Why CODEX_HOME isolation: codex reads `$CODEX_HOME/auth.json` (default
# `~/.codex/auth.json`) at startup. Switching the shared file from
# multiple panes would race; giving each pane its own CODEX_HOME means
# the panes never touch a shared file after init.
#
# Usage:
#   bash scripts/codex-fleet/up.sh                    # uses scripts/codex-fleet/accounts.yml
#   bash scripts/codex-fleet/up.sh --config /path     # custom config
#   bash scripts/codex-fleet/up.sh --dry-run          # print plan, do not spawn
#   bash scripts/codex-fleet/up.sh --no-attach        # leave detached (CI / smoke)
#
# Required:
#   - tmux on PATH
#   - codex CLI on PATH
#   - ~/.codex/accounts/<email>.json for every account referenced
#
# Tear down: `bash scripts/codex-fleet/down.sh`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="${SCRIPT_DIR}/accounts.yml"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
PROMPT_FILE="${SCRIPT_DIR}/worker-prompt.md"
FLEET_CONFIG_TMPL="${CODEX_FLEET_CONFIG_TMPL:-$SCRIPT_DIR/fleet-config.toml.tmpl}"
DRY_RUN=0
ATTACH=1

# Probe Colony / MCP health once, before any pane spawns. Exports
# FLEET_COLONY_* + FLEET_PATH used by fleet_render_config below. The
# preflight is non-fatal: when Colony is unhealthy it disables the MCP
# in the staged config rather than refusing bringup, so the worker
# prompt's shell-CLI fallback still has a chance to keep things moving.
# shellcheck source=lib/mcp-preflight.sh
. "$SCRIPT_DIR/lib/mcp-preflight.sh"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config) CONFIG="$2"; shift 2 ;;
    --session) SESSION="$2"; shift 2 ;;
    --work-root) WORK_ROOT="$2"; shift 2 ;;
    --prompt) PROMPT_FILE="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --no-attach) ATTACH=0; shift ;;
    -h|--help)
      sed -n '1,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *)
      echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

require_bin() {
  command -v "$1" >/dev/null 2>&1 || { echo "fatal: $1 not on PATH" >&2; exit 2; }
}

require_bin tmux
require_bin codex

[[ -f "$CONFIG" ]] || { echo "fatal: config not found: $CONFIG" >&2; exit 2; }
[[ -f "$PROMPT_FILE" ]] || { echo "fatal: prompt not found: $PROMPT_FILE" >&2; exit 2; }

# Parse accounts.yml without yq: we accept a deliberately tiny YAML subset
# of the form `- id: <name>\n  email: <addr>\n  skills: [a, b]`. Each
# entry starts with `- id:`. Empty / comment lines ignored. yq pulls in
# a dependency we do not need for this script.
parse_accounts() {
  python3 - "$1" <<'PY'
import json, re, sys
path = sys.argv[1]
items = []
cur = None
with open(path, "r", encoding="utf-8") as fh:
    for raw in fh:
        line = raw.rstrip()
        stripped = line.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("- id:"):
            if cur is not None:
                items.append(cur)
            cur = {"id": stripped.split(":", 1)[1].strip()}
            continue
        if cur is None:
            continue
        m = re.match(r"^\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*(.*?)$", line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if val.startswith("[") and val.endswith("]"):
            inner = val[1:-1]
            val = [v.strip().strip('"').strip("'") for v in inner.split(",") if v.strip()]
        else:
            val = val.strip('"').strip("'")
        cur[key] = val
if cur is not None:
    items.append(cur)
print(json.dumps(items))
PY
}

ACCOUNTS_JSON="$(parse_accounts "$CONFIG")"
ACCOUNT_COUNT="$(echo "$ACCOUNTS_JSON" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')"
if [[ "$ACCOUNT_COUNT" -eq 0 ]]; then
  echo "fatal: $CONFIG declares zero accounts" >&2
  exit 2
fi

echo "[codex-fleet] config: $CONFIG"
echo "[codex-fleet] session: $SESSION"
echo "[codex-fleet] work-root: $WORK_ROOT"
echo "[codex-fleet] accounts: $ACCOUNT_COUNT"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[codex-fleet] tmux session $SESSION already exists — run ./down.sh first" >&2
  exit 1
fi

# Stage CODEX_HOME directory for each account. Copy the account auth.json
# in, then symlink the user's config.toml so the pane inherits the same
# defaults (model, MCP servers, etc.) as a normal codex session. Each
# pane gets its own isolated CODEX_HOME so no two codex processes race
# on a shared file after init.
stage_account() {
  local acct_id="$1"
  local email="$2"
  local dst="$WORK_ROOT/$acct_id"
  local src="$HOME/.codex/accounts/$email.json"
  if [[ ! -f "$src" ]]; then
    echo "fatal: account auth file not found: $src (account-id=$acct_id email=$email)" >&2
    return 1
  fi
  mkdir -p "$dst"
  cp -f "$src" "$dst/auth.json"
  chmod 600 "$dst/auth.json"
  # Render a fleet-local config.toml instead of symlinking the operator's
  # interactive one. The worker prompt only calls `mcp__colony__*`; every
  # other MCP in `~/.codex/config.toml` (drawio, recodee, Higgsfield, …)
  # would burn 30-60s of pane startup time blocking on slow / unreachable
  # backends. fleet_render_config substitutes preflight-derived enable
  # flags and timeouts into the template.
  if [[ -f "$FLEET_CONFIG_TMPL" ]]; then
    if ! fleet_render_config "$FLEET_CONFIG_TMPL" "$dst/config.toml"; then
      echo "fatal: failed to render fleet config from $FLEET_CONFIG_TMPL" >&2
      return 1
    fi
  else
    echo "[codex-fleet] WARN fleet template missing ($FLEET_CONFIG_TMPL); falling back to symlinking ~/.codex/config.toml" >&2
    if [[ -f "$HOME/.codex/config.toml" ]]; then
      ln -sf "$HOME/.codex/config.toml" "$dst/config.toml"
    fi
  fi
  echo "[codex-fleet] staged $acct_id ($email) -> $dst"
}

# Plan-only mode: show what we'd do, then exit before touching tmux.
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "$ACCOUNTS_JSON" | python3 -c '
import json, sys
for a in json.load(sys.stdin):
    aid = a["id"]
    email = a.get("email", "<MISSING>")
    skills = a.get("skills", [])
    print(f"  - id={aid} email={email} skills={skills}")
'
  exit 0
fi

# Realize the plan: stage each account, spawn one pane per account.
mkdir -p "$WORK_ROOT"
echo "$ACCOUNTS_JSON" | python3 -c '
import json, sys
for a in json.load(sys.stdin):
    aid = a["id"]
    email = a.get("email", "")
    print(f"{aid}\t{email}")
' | while IFS=$'\t' read -r acct_id email; do
  if [[ -z "$email" ]]; then
    echo "fatal: account $acct_id missing email field in $CONFIG" >&2
    exit 2
  fi
  stage_account "$acct_id" "$email"
done

# Spawn the tmux session. First pane is window 0 with the first account;
# additional accounts open as horizontal splits and we tile at the end.
FIRST=1
echo "$ACCOUNTS_JSON" | python3 -c '
import json, sys
for a in json.load(sys.stdin):
    aid = a["id"]
    email = a.get("email", "")
    print(f"{aid}\t{email}")
' | while IFS=$'\t' read -r acct_id email; do
  pane_env=(
    "CODEX_HOME=$WORK_ROOT/$acct_id"
    "CODEX_FLEET_AGENT_NAME=codex-$acct_id"
    "CODEX_FLEET_ACCOUNT_EMAIL=$email"
  )
  # codex CLI takes the initial prompt as a positional argument and stays
  # interactive afterwards. Redirecting stdin from the prompt file (the
  # previous shape) made codex exit on EOF, killing the pane immediately.
  # Use --prompt-file when available (codex >= 0.x), otherwise fall back
  # to passing the file contents as the positional argument.
  #
  # --dangerously-bypass-approvals-and-sandbox: auto-approve all MCP tool
  # calls and shell commands. Fleet workers run unattended; without this
  # every Colony MCP call would hit the "Allow / Always allow / Cancel"
  # gate and stall the pull-loop.
  # --add-dir extends the workspace-write sandbox so workers can edit
  # files in sibling repos that the active plan targets (e.g.
  # /home/deadpool/Documents/recodee for gx-fleet-* plans). Without
  # this, workers hit `outside writable roots` and silently spin.
  pane_cmd="env ${pane_env[*]} codex --dangerously-bypass-approvals-and-sandbox --add-dir /home/deadpool/Documents/recodee --add-dir /home/deadpool/Documents/codex-fleet \"\$(cat '$PROMPT_FILE')\""
  if [[ $FIRST -eq 1 ]]; then
    tmux new-session -d -s "$SESSION" -n "codex-$acct_id" "$pane_cmd"
    FIRST=0
  else
    tmux split-window -t "$SESSION:0" "$pane_cmd"
    tmux select-layout -t "$SESSION:0" tiled >/dev/null
  fi
done

# Final tile + browser-style tab strip.
tmux select-layout -t "$SESSION:0" tiled >/dev/null
if [[ -x "$SCRIPT_DIR/style-tabs.sh" ]]; then
  CODEX_FLEET_SESSION="$SESSION" bash "$SCRIPT_DIR/style-tabs.sh" >/dev/null 2>&1 || true
else
  # fallback: at least put a sensible right-side info chip
  tmux set-option -g -t "$SESSION" status-right "codex-fleet | #{?pane_in_mode,COPY,RUN} | #(date +%H:%M)" >/dev/null
fi

echo "[codex-fleet] up: session=$SESSION, panes=$ACCOUNT_COUNT, work-root=$WORK_ROOT"
echo "[codex-fleet] attach with: tmux attach -t $SESSION"
echo "[codex-fleet] tear down with: bash $SCRIPT_DIR/down.sh"

if [[ $ATTACH -eq 1 && -t 1 ]]; then
  tmux attach -t "$SESSION"
fi
