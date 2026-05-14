#!/usr/bin/env bash
# force-claim — dispatch available plan sub-tasks onto idle codex panes.
#
# What it does:
#   1. Read the active openspec plan.json.
#   2. For each task with status="available" whose deps are completed,
#      enqueue a (sub_idx, title, plan_slug) work item.
#   3. Walk the codex-fleet:<window>.* panes, find ones whose tail shows the
#      default-prompt placeholder (idle worker) and no claim-in-flight.
#   4. Send a Colony claim prompt to each idle pane via `tmux send-keys`.
#
# Operator-pre-approved: dispatching prompts into gx-fleet/codex-fleet
# panes is an allowed flow (see ~/.claude memory feedback_gx_fleet_dispatch_authorized).
#
# Usage:
#   bash scripts/codex-fleet/force-claim.sh                 # one-shot
#   bash scripts/codex-fleet/force-claim.sh --dry-run       # show plan, no dispatch
#   bash scripts/codex-fleet/force-claim.sh --loop          # respawn every 10s
#   FORCE_CLAIM_SESSION=codex-fleet ...                      # tmux session override
#   FORCE_CLAIM_WINDOW=1            ...                      # window with codex panes
set -eo pipefail

REPO="${FORCE_CLAIM_REPO:-/home/deadpool/Documents/recodee}"
SESSION="${FORCE_CLAIM_SESSION:-codex-fleet}"
WINDOW="${FORCE_CLAIM_WINDOW:-1}"
LOOP=0
DRY=0
INTERVAL="${FORCE_CLAIM_INTERVAL:-10}"
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    --loop)    LOOP=1 ;;
    --interval=*) INTERVAL="${a#--interval=}" ;;
  esac
done

_latest_plan() {
  python3 - "$REPO" <<'PYEOF'
import os, sys, re, glob
repo = sys.argv[1]
plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    return (d, os.path.getmtime(p))
plans.sort(key=key, reverse=True)
print(plans[0] if plans else "")
PYEOF
}

PLAN_JSON="${FORCE_CLAIM_PLAN_JSON:-$(_latest_plan)}"
[[ -f "$PLAN_JSON" ]] || { echo "force-claim: no plan.json found" >&2; exit 2; }
PLAN_SLUG=$(basename "$(dirname "$PLAN_JSON")")

# Emit lines: <sub_idx>\t<title>  for tasks whose deps are all completed and
# whose own status is "available". jq does the dependency check.
ready_tasks() {
  jq -r '
    (.tasks | map({(.subtask_index|tostring): (.status // "available")}) | add) as $st
    | .tasks | sort_by(.subtask_index) | .[]
    | select(.status == "available" or .status == null)
    | select((.depends_on // []) | all(. as $d | $st[($d|tostring)] == "completed"))
    | "\(.subtask_index)\t\(.title)"
  ' "$PLAN_JSON"
}

# Identify idle codex panes — pane whose last 12 lines contain a `› default
# prompt` placeholder, no `Working (…)`, and no `Reviewing approval request`.
idle_panes() {
  local pane idle_idxs=()
  while read -r pane_idx; do
    [[ -z "$pane_idx" ]] && continue
    local tail
    tail=$(tmux capture-pane -t "$SESSION:$WINDOW.$pane_idx" -p -S -12 2>/dev/null | sed 's/\x1B\[[0-9;]*m//g')
    if echo "$tail" | grep -qE "Working \([0-9]+[ms]"; then continue; fi
    if echo "$tail" | grep -qE "Reviewing approval request"; then continue; fi
    if echo "$tail" | grep -qE "^› (Find and fix|Use /skills|Run /review|Improve documentation|Implement|Summarize|Explain|Write tests)"; then
      printf '%s\n' "$pane_idx"
    fi
  done < <(tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_index}' 2>/dev/null)
}

dispatch() {
  local pane_idx="$1" sub_idx="$2" title="$3"
  local prompt
  prompt="Claim sub-task ${sub_idx} of plan ${PLAN_SLUG} via Colony task_plan_claim_subtask, then execute it. Title: ${title}. When done, post a Colony note with evidence and mark the sub-task completed."
  if (( DRY == 1 )); then
    printf '[dry] pane=%s sub=%s title=%s\n' "$pane_idx" "$sub_idx" "$title"
    return
  fi
  # Send the prompt, then Enter. Use -l so backslashes / special chars in
  # the title don't trigger key parsing.
  tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" -l "$prompt"
  tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" Enter
  printf '[dispatch] pane=%s sub=%s title=%s\n' "$pane_idx" "$sub_idx" "$title"
}

one_pass() {
  # Read ready tasks + idle panes once. Zip them: first idle pane gets first
  # ready task, etc. Don't dispatch more than min(panes, tasks).
  local -a tasks=()
  while IFS=$'\t' read -r idx title; do
    [[ -z "$idx" ]] && continue
    tasks+=("$idx"$'\t'"$title")
  done < <(ready_tasks)

  local -a panes=()
  while read -r p; do
    [[ -z "$p" ]] && continue
    panes+=("$p")
  done < <(idle_panes)

  if (( ${#tasks[@]} == 0 )); then
    printf '[%s] no ready tasks for %s\n' "$(date +%T)" "$PLAN_SLUG"
    return
  fi
  if (( ${#panes[@]} == 0 )); then
    printf '[%s] no idle codex panes (claim count=%d)\n' "$(date +%T)" "${#tasks[@]}"
    return
  fi

  local n=${#tasks[@]}
  (( ${#panes[@]} < n )) && n=${#panes[@]}
  local i sub title
  for ((i=0; i<n; i++)); do
    IFS=$'\t' read -r sub title <<<"${tasks[$i]}"
    dispatch "${panes[$i]}" "$sub" "$title"
  done
}

if (( LOOP == 1 )); then
  trap 'echo force-claim: stopping >&2; exit 0' INT TERM
  while true; do
    one_pass
    sleep "$INTERVAL"
  done
else
  one_pass
fi
