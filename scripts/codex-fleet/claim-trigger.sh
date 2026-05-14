#!/usr/bin/env bash
# claim-trigger - event-driven dispatcher for ready Colony plan sub-tasks.
#
# Watches OpenSpec plan files plus Colony event/WAL files and, after a short
# debounce, sends one ready claim prompt to the first idle codex-fleet pane.
# This is intentionally a narrow trigger; force-claim.sh remains the broader
# polling/backstop dispatcher.
#
# Usage:
#   bash scripts/codex-fleet/claim-trigger.sh
#   bash scripts/codex-fleet/claim-trigger.sh --once
#   bash scripts/codex-fleet/claim-trigger.sh --once --dry-run
#
# Env:
#   CLAIM_TRIGGER_REPO=<repo-root>     (default: autodetect from script location)
#   CLAIM_TRIGGER_SESSION=codex-fleet
#   CLAIM_TRIGGER_WINDOW=overview
#   CLAIM_TRIGGER_DEBOUNCE_MS=500
#   CLAIM_TRIGGER_LOG=/tmp/claude-viz/claim-trigger.log
#   CLAIM_TRIGGER_PLAN_JSON=/path/to/plan.json   # optional single-plan pin
set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"

REPO="${CLAIM_TRIGGER_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}}"
SESSION="${CLAIM_TRIGGER_SESSION:-codex-fleet}"
WINDOW="${CLAIM_TRIGGER_WINDOW:-overview}"
DEBOUNCE_MS="${CLAIM_TRIGGER_DEBOUNCE_MS:-500}"
LOG="${CLAIM_TRIGGER_LOG:-/tmp/claude-viz/claim-trigger.log}"
PLAN_ROOT="${CLAIM_TRIGGER_PLAN_ROOT:-$REPO/openspec/plans}"
COLONY_DIR="${CLAIM_TRIGGER_COLONY_DIR:-$HOME/.colony}"
ONCE=0
DRY=0

for arg in "$@"; do
  case "$arg" in
    --once) ONCE=1 ;;
    --dry-run) DRY=1 ;;
    --debounce-ms=*) DEBOUNCE_MS="${arg#--debounce-ms=}" ;;
    --help|-h)
      sed -n '2,32p' "$0"
      exit 0
      ;;
    *)
      printf 'claim-trigger: unknown arg: %s\n' "$arg" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$(dirname "$LOG")"
ts() { date +%H:%M:%S; }
log() { printf '[%s] %s\n' "$(ts)" "$*" | tee -a "$LOG"; }

debounce_seconds() {
  awk -v ms="$DEBOUNCE_MS" 'BEGIN { printf "%.3f", ms / 1000 }'
}

# Emit ready tasks as: slug<TAB>sub_idx<TAB>title.
# Duplicates force-claim.sh's readiness semantics so the event trigger and
# polling backstop agree on which local plan rows are claimable.
ready_tasks_all() {
  python3 - "$REPO" "${CLAIM_TRIGGER_PLAN_JSON:-}" <<'PYEOF'
import glob
import json
import os
import re
import sys

repo, pin = sys.argv[1], sys.argv[2]

if pin:
    plans = [pin] if os.path.isfile(pin) else []
else:
    plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")

def keyfn(path):
    slug = os.path.basename(os.path.dirname(path))
    match = re.search(r"(\d{4})-(\d{2})-(\d{2})$", slug)
    day = tuple(int(match.group(i)) for i in range(1, 4)) if match else (0, 0, 0)
    try:
        mtime = os.path.getmtime(path)
    except OSError:
        mtime = 0
    return (day, mtime)

plans.sort(key=keyfn, reverse=True)

for path in plans:
    try:
        with open(path, encoding="utf-8") as handle:
            data = json.load(handle)
    except Exception:
        continue

    tasks = data.get("tasks") or []
    if not tasks:
        continue

    slug = os.path.basename(os.path.dirname(path))
    status = {str(t.get("subtask_index")): (t.get("status") or "available") for t in tasks}
    for task in sorted(tasks, key=lambda row: row.get("subtask_index", 0)):
        if (task.get("status") or "available") != "available":
            continue
        deps = task.get("depends_on") or []
        if not all(status.get(str(dep)) == "completed" for dep in deps):
            continue
        title = (task.get("title") or "").replace("\t", " ")
        print(f"{slug}\t{task.get('subtask_index')}\t{title}")
PYEOF
}

idle_panes() {
  while IFS= read -r pane_idx; do
    [[ -z "$pane_idx" ]] && continue
    local tail
    tail=$(tmux capture-pane -t "$SESSION:$WINDOW.$pane_idx" -p -S -12 2>/dev/null | sed 's/\x1B\[[0-9;]*m//g' || true)
    [[ -n "$tail" ]] || continue
    if grep -qE "Working \([0-9]+[ms]" <<<"$tail"; then continue; fi
    if grep -qE "Reviewing approval request" <<<"$tail"; then continue; fi
    if grep -qE "^› (Find and fix|Use /skills|Run /review|Improve documentation|Implement|Summarize|Explain|Write tests)" <<<"$tail"; then
      printf '%s\n' "$pane_idx"
    fi
  done < <(tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_index}' 2>/dev/null || true)
}

pane_agent_name() {
  local pane_idx="$1" target="$SESSION:$WINDOW.$pane_idx"
  local label
  label=$(tmux display-message -p -t "$target" '#{@panel}' 2>/dev/null || true)
  label="${label#[}"
  label="${label%]}"
  if [[ "$label" == codex-* || "$label" == claude-* ]]; then
    printf '%s\n' "$label"
    return 0
  fi

  local root_pid
  root_pid=$(tmux display-message -p -t "$target" '#{pane_pid}' 2>/dev/null || true)
  [[ "$root_pid" =~ ^[0-9]+$ ]] || return 1

  local -a queue=("$root_pid")
  local pid child agent
  while (( ${#queue[@]} > 0 )); do
    pid="${queue[0]}"
    queue=("${queue[@]:1}")
    if [[ -r "/proc/$pid/environ" ]]; then
      agent=$(tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | awk -F= '/^CODEX_FLEET_AGENT_NAME=|^CLAUDE_FLEET_AGENT_NAME=/{print $2; exit}' || true)
      if [[ -n "$agent" ]]; then
        printf '%s\n' "$agent"
        return 0
      fi
    fi
    while IFS= read -r child; do
      [[ "$child" =~ ^[0-9]+$ ]] && queue+=("$child")
    done < <(pgrep -P "$pid" 2>/dev/null || true)
  done
  return 1
}

ready_task_for_agent() {
  local agent="$1"
  command -v colony >/dev/null 2>&1 || return 1
  colony task ready \
    --session "$agent" \
    --agent "$agent" \
    --repo-root "$REPO" \
    --limit 1 \
    --json 2>>"$LOG" |
    python3 -c '
import json
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(1)

ready = data.get("ready") or []
if not ready:
    sys.exit(0)

task = ready[0]
slug = task.get("plan_slug")
idx = task.get("subtask_index")
title = (task.get("title") or "").replace("\t", " ")
if slug is None or idx is None:
    sys.exit(1)
print(f"{slug}\t{idx}\t{title}")
'
}

dispatch() {
  local pane_idx="$1" slug="$2" sub_idx="$3" title="$4"
  local prompt
  prompt="OVERRIDE current plan pinning. Claim sub-task ${sub_idx} of plan ${slug} via Colony task_plan_claim_subtask (force the agent slug to your CODEX_FLEET_AGENT_NAME). Title: ${title}. Implement it on a fresh agent worktree per AGENTS.md, run the narrowest verification, open + merge a PR, post a Colony note with evidence (branch, PR URL, MERGED state), then mark the sub-task completed."

  if (( DRY == 1 )); then
    printf '[dry] pane=%s plan=%s sub=%s title=%s\n' "$pane_idx" "$slug" "$sub_idx" "$title"
    return
  fi

  tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" -l "$prompt"
  tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" Enter
  printf '[dispatch] pane=%s plan=%s sub=%s title=%s\n' "$pane_idx" "$slug" "$sub_idx" "$title"
}

one_pass() {
  local task pane agent slug sub_idx title
  pane=$(idle_panes | head -1 || true)
  if [[ -z "$pane" ]]; then
    log "no idle pane"
    return 0
  fi

  agent=$(pane_agent_name "$pane" || true)
  if [[ -n "$agent" ]]; then
    task=$(ready_task_for_agent "$agent" | head -1 || true)
  fi
  if [[ -z "$task" ]]; then
    task=$(ready_tasks_all | head -1 || true)
  fi
  if [[ -z "$task" ]]; then
    log "no ready tasks"
    return 0
  fi

  IFS=$'\t' read -r slug sub_idx title <<<"$task"
  dispatch "$pane" "$slug" "$sub_idx" "$title" | tee -a "$LOG"
}

plan_watch_paths() {
  if [[ -n "${CLAIM_TRIGGER_PLAN_JSON:-}" ]]; then
    [[ -f "$CLAIM_TRIGGER_PLAN_JSON" ]] && printf '%s\n' "$CLAIM_TRIGGER_PLAN_JSON"
    return
  fi

  [[ -d "$PLAN_ROOT" ]] || return
  printf '%s\n' "$PLAN_ROOT"
  find "$PLAN_ROOT" -mindepth 2 -maxdepth 2 -type f -name plan.json 2>/dev/null | sort
}

colony_watch_paths() {
  [[ -d "$COLONY_DIR" ]] || return
  printf '%s\n' "$COLONY_DIR"
  # Colony uses SQLite WAL at ~/.colony/data.db-wal today. Keep *.wal and
  # *.events in the probe for future event-journal names used by supervisors.
  find "$COLONY_DIR" -maxdepth 2 -type f \( -name '*.wal' -o -name '*-wal' -o -name '*.events' \) 2>/dev/null | sort
}

watch_paths() {
  { plan_watch_paths; colony_watch_paths; } | awk 'NF && !seen[$0]++'
}

run_loop() {
  command -v inotifywait >/dev/null 2>&1 || {
    printf 'claim-trigger: inotifywait is required for loop mode\n' >&2
    exit 127
  }

  local -a paths=()
  while IFS= read -r path; do
    [[ -n "$path" ]] && paths+=("$path")
  done < <(watch_paths)

  if (( ${#paths[@]} == 0 )); then
    printf 'claim-trigger: no watch paths found under %s or %s\n' "$PLAN_ROOT" "$COLONY_DIR" >&2
    exit 1
  fi

  log "watching ${#paths[@]} path(s); colony_events=$(colony_watch_paths | paste -sd, - || true)"
  one_pass

  local delay
  delay=$(debounce_seconds)
  trap 'log "stopping"; exit 0' INT TERM
  inotifywait -m -e close_write,modify,create,moved_to --format '%w%f %e' "${paths[@]}" 2>>"$LOG" |
    while IFS= read -r event; do
      [[ -n "$event" ]] || continue
      sleep "$delay"
      log "event $event"
      one_pass
    done
}

if (( ONCE == 1 )); then
  one_pass
else
  run_loop
fi
