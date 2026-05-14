#!/usr/bin/env bash
#
# codex-fleet supervisor: consume live-viz exhaustion events and spawn one
# replacement Codex takeover worker for each unprocessed stranded subtask.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONFIG="${CODEX_FLEET_SUPERVISOR_CONFIG:-$SCRIPT_DIR/accounts.yml}"
# Per-fleet state dir lets multiple parallel fleets each run their own
# supervisor + stall-watcher loop without colliding queues. FLEET_STATE_DIR
# is exported by full-bringup.sh; defaults to /tmp/claude-viz for back-compat
# (single-fleet operators see no change).
FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
QUEUE="${CODEX_FLEET_SUPERVISOR_QUEUE:-$FLEET_STATE_DIR/supervisor-queue.jsonl}"
ACTIVE_FILE="${CODEX_FLEET_ACTIVE_FILE:-$FLEET_STATE_DIR/fleet-active-accounts.txt}"
STATE_DIR="${CODEX_FLEET_SUPERVISOR_STATE_DIR:-$FLEET_STATE_DIR/supervisor}"
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
PROMPT_TEMPLATE="${CODEX_FLEET_TAKEOVER_PROMPT:-$SCRIPT_DIR/takeover-prompt.md}"
PLAN_JSON="${CODEX_FLEET_SUPERVISOR_PLAN_JSON:-${FLEET_TICK_PLAN_JSON:-$REPO_ROOT/openspec/plans/rust-ph13-14-15-completion-2026-05-13/plan.json}}"
USAGE_FILE="${CODEX_FLEET_SUPERVISOR_USAGE_FILE:-}"
DRY_RUN="${CODEX_FLEET_SUPERVISOR_DRY_RUN:-0}"
ONCE=0

usage() {
  sed -n '1,44p' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config) CONFIG="$2"; shift 2 ;;
    --queue) QUEUE="$2"; shift 2 ;;
    --active-file) ACTIVE_FILE="$2"; shift 2 ;;
    --state-dir) STATE_DIR="$2"; shift 2 ;;
    --work-root) WORK_ROOT="$2"; shift 2 ;;
    --prompt-template) PROMPT_TEMPLATE="$2"; shift 2 ;;
    --plan-json) PLAN_JSON="$2"; shift 2 ;;
    --once) ONCE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

require_bin() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "fatal: $1 not on PATH" >&2
    exit 2
  }
}

require_bin python3
if [[ "$DRY_RUN" != "1" ]]; then
  require_bin kitty
  require_bin codex
fi

[[ -f "$CONFIG" ]] || { echo "fatal: config not found: $CONFIG" >&2; exit 2; }
[[ -f "$PROMPT_TEMPLATE" ]] || { echo "fatal: prompt template not found: $PROMPT_TEMPLATE" >&2; exit 2; }

mkdir -p "$(dirname "$QUEUE")" "$(dirname "$ACTIVE_FILE")" "$STATE_DIR/prompts" "$STATE_DIR/runners"
touch "$QUEUE" "$ACTIVE_FILE" "$STATE_DIR/processed.keys"

parse_accounts() {
  python3 - "$CONFIG" <<'PY'
import json
import re
import sys

items = []
cur = None
with open(sys.argv[1], "r", encoding="utf-8") as fh:
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
        match = re.match(r"^\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*(.*?)$", line)
        if not match:
            continue
        key, value = match.group(1), match.group(2).strip()
        if value.startswith("[") and value.endswith("]"):
            value = [part.strip().strip('"').strip("'") for part in value[1:-1].split(",") if part.strip()]
        else:
            value = value.strip('"').strip("'")
        cur[key] = value
if cur is not None:
    items.append(cur)

for item in items:
    print(f"{item.get('id', '')}\t{item.get('email', '')}")
PY
}

usage_lines() {
  if [[ -n "$USAGE_FILE" ]]; then
    cat "$USAGE_FILE"
    return 0
  fi
  if command -v codex-auth >/dev/null 2>&1; then
    codex-auth list 2>/dev/null || true
  fi
}

weekly_for_email() {
  local email="$1"
  usage_lines | python3 -c '
import re
import sys

email = sys.argv[1]
for line in sys.stdin:
    if email not in line:
        continue
    match = re.search(r"weekly=([0-9]+)%", line)
    if match:
        print(match.group(1))
        break
' "$email"
}

pick_replacement() {
  local exhausted_agent="$1"
  local exhausted_id="${exhausted_agent#codex-}"
  while IFS=$'\t' read -r account_id email; do
    [[ -n "$account_id" && -n "$email" ]] || continue
    [[ "$account_id" == "$exhausted_id" ]] && continue
    grep -Fxq "$account_id" "$ACTIVE_FILE" 2>/dev/null && continue

    local weekly
    weekly="$(weekly_for_email "$email")"
    [[ "$weekly" =~ ^[0-9]+$ ]] || continue
    (( weekly < 90 )) || continue

    printf '%s\t%s\t%s\n' "$account_id" "$email" "$weekly"
    return 0
  done < <(parse_accounts)
  return 1
}

event_key() {
  python3 -c 'import hashlib, sys; print(hashlib.sha256(sys.stdin.read().encode("utf-8")).hexdigest())'
}

event_field() {
  local line="$1" field="$2"
  python3 -c '
import json
import sys

try:
    event = json.loads(sys.argv[1])
except Exception:
    print("")
    raise SystemExit
print(event.get(sys.argv[2], ""))
' "$line" "$field"
}

subtask_payload() {
  local agent="$1"
  python3 - "$PLAN_JSON" "$agent" <<'PY'
import json
import sys

path, agent = sys.argv[1], sys.argv[2]
try:
    with open(path, "r", encoding="utf-8") as fh:
        plan = json.load(fh)
except Exception:
    plan = {}

for task in plan.get("tasks", []):
    if task.get("claimed_by_agent") != agent:
        continue
    print(json.dumps({
        "plan_slug": plan.get("plan_slug", ""),
        "subtask_index": task.get("subtask_index", ""),
        "title": task.get("title", ""),
        "description": task.get("description", ""),
        "file_scope": task.get("file_scope", []),
        "status": task.get("status", ""),
    }))
    raise SystemExit

print(json.dumps({
    "plan_slug": plan.get("plan_slug", ""),
    "subtask_index": "",
    "title": "unknown claimed subtask",
    "description": "No matching claimed_by_agent entry was found in the configured plan.json.",
    "file_scope": [],
    "status": "unknown",
}))
PY
}

stage_account() {
  local account_id="$1" email="$2"
  local dst="$WORK_ROOT/$account_id"
  local src="$HOME/.codex/accounts/$email.json"

  [[ -f "$src" ]] || {
    echo "fatal: account auth file not found: $src" >&2
    return 1
  }

  mkdir -p "$dst"
  cp -f "$src" "$dst/auth.json"
  chmod 600 "$dst/auth.json"
  if [[ -f "$HOME/.codex/config.toml" ]]; then
    ln -sf "$HOME/.codex/config.toml" "$dst/config.toml"
  fi
}

render_prompt() {
  local out="$1" exhausted_agent="$2" replacement_agent="$3" replacement_email="$4" reason="$5" payload="$6"
  python3 - "$PROMPT_TEMPLATE" "$out" "$exhausted_agent" "$replacement_agent" "$replacement_email" "$reason" "$payload" <<'PY'
import json
import sys

template_path, out, exhausted, replacement, email, reason, payload = sys.argv[1:]
task = json.loads(payload)

with open(template_path, "r", encoding="utf-8") as fh:
    text = fh.read()

file_scope = "\n".join(f"- `{path}`" for path in task.get("file_scope", [])) or "- unknown"
replacements = {
    "{{EXHAUSTED_AGENT}}": exhausted,
    "{{REPLACEMENT_AGENT}}": replacement,
    "{{REPLACEMENT_EMAIL}}": email,
    "{{REASON}}": reason,
    "{{PLAN_SLUG}}": str(task.get("plan_slug", "")),
    "{{SUBTASK_INDEX}}": str(task.get("subtask_index", "")),
    "{{SUBTASK_TITLE}}": str(task.get("title", "")),
    "{{SUBTASK_DESCRIPTION}}": str(task.get("description", "")),
    "{{FILE_SCOPE}}": file_scope,
    "{{STATUS}}": str(task.get("status", "")),
}
for key, value in replacements.items():
    text = text.replace(key, value)

with open(out, "w", encoding="utf-8") as fh:
    fh.write(text)
PY
}

write_runner() {
  local runner="$1" account_id="$2" email="$3" prompt_file="$4"
  python3 - "$runner" "$WORK_ROOT/$account_id" "codex-$account_id" "$email" "$prompt_file" <<'PY'
import shlex
import sys

runner, codex_home, agent_name, email, prompt_file = sys.argv[1:]
body = "\n".join([
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    f"export CODEX_HOME={shlex.quote(codex_home)}",
    f"export CODEX_FLEET_AGENT_NAME={shlex.quote(agent_name)}",
    f"export CODEX_FLEET_ACCOUNT_EMAIL={shlex.quote(email)}",
    f"exec codex \"$(cat {shlex.quote(prompt_file)})\"",
    "",
])
with open(runner, "w", encoding="utf-8") as fh:
    fh.write(body)
PY
  chmod +x "$runner"
}

spawn_takeover() {
  local account_id="$1" runner="$2"
  local title="codex-takeover-$account_id"

  if [[ "$DRY_RUN" == "1" ]]; then
    printf '[dry-run] kitty --title %s %s\n' "$title" "$runner"
    return 0
  fi

  kitty --title "$title" "$runner" >/dev/null 2>&1 &
}

process_event() {
  local line="$1"
  [[ -n "${line// }" ]] || return 0

  local key
  key="$(printf '%s' "$line" | event_key)"
  grep -Fxq "$key" "$STATE_DIR/processed.keys" && return 0

  local agent reason
  agent="$(event_field "$line" agent)"
  reason="$(event_field "$line" reason)"
  [[ -n "$agent" ]] || {
    echo "warn: malformed supervisor event: $line" >&2
    return 0
  }

  local picked
  if ! picked="$(pick_replacement "$agent")"; then
    echo "warn: no replacement account below weekly<90% and inactive for $agent" >&2
    return 0
  fi

  local account_id email weekly
  IFS=$'\t' read -r account_id email weekly <<<"$picked"

  local payload prompt_file runner
  payload="$(subtask_payload "$agent")"
  prompt_file="$STATE_DIR/prompts/takeover-${account_id}-${key}.md"
  runner="$STATE_DIR/runners/run-${account_id}-${key}.sh"

  if [[ "$DRY_RUN" != "1" ]]; then
    stage_account "$account_id" "$email"
  fi
  render_prompt "$prompt_file" "$agent" "codex-$account_id" "$email" "${reason:-rate-limit}" "$payload"
  write_runner "$runner" "$account_id" "$email" "$prompt_file"
  spawn_takeover "$account_id" "$runner"

  [[ "$DRY_RUN" == "1" ]] || printf '%s\n' "$account_id" >> "$ACTIVE_FILE"
  printf '%s\n' "$key" >> "$STATE_DIR/processed.keys"
  echo "spawned takeover: codex-$account_id for $agent weekly=${weekly}%"
}

if [[ "$ONCE" == "1" ]]; then
  while IFS= read -r line; do
    process_event "$line"
  done < "$QUEUE"
  exit 0
fi

tail -n +1 -F "$QUEUE" | while IFS= read -r line; do
  process_event "$line"
done
