#!/usr/bin/env bash
# force-claim — dispatch available plan sub-tasks across ALL non-empty plans
# onto idle codex panes.
#
# Why multi-plan:
#   When one plan is complete, workers pinned to it idle ("plan already
#   complete"). The watcher should instead route them to ready work in
#   any other openspec plan. force-claim now scans every plan.json
#   under openspec/plans/, picks tasks whose deps are satisfied, and
#   dispatches across the pool.
#
# Behavior:
#   1. Enumerate plan.json under openspec/plans/*/
#   2. For each non-empty plan, list (slug, idx, title) for tasks whose
#      status=="available" and whose depends_on are all completed.
#   3. Sort plans by newest-first (date suffix tiebreaker on mtime).
#   4. Find idle codex panes (default-prompt placeholder, no Working /
#      Reviewing approval state).
#   5. Zip ready_tasks → panes 1:1 and send-keys a claim prompt.
#
# Operator-pre-approved: dispatching prompts into gx-fleet/codex-fleet
# panes is an allowed flow (see ~/.claude memory feedback_gx_fleet_dispatch_authorized).
#
# Usage:
#   bash scripts/codex-fleet/force-claim.sh                 # one-shot
#   bash scripts/codex-fleet/force-claim.sh --dry-run       # show plan, no dispatch
#   bash scripts/codex-fleet/force-claim.sh --loop          # poll every 10s
#   FORCE_CLAIM_SESSION=codex-fleet ...                      # tmux session override
#   FORCE_CLAIM_WINDOW=1            ...                      # window with codex panes
#   FORCE_CLAIM_PLAN_JSON=/path/plan.json                    # pin to single plan
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

# Emit ready (slug \t sub_idx \t title) across every non-empty plan, newest-first.
# Pin via FORCE_CLAIM_PLAN_JSON if the operator wants single-plan behaviour.
ready_tasks_all() {
  python3 - "$REPO" "${FORCE_CLAIM_PLAN_JSON:-}" <<'PYEOF'
import os, sys, re, glob, json
repo, pin = sys.argv[1], sys.argv[2]

if pin:
    plans = [pin] if os.path.isfile(pin) else []
else:
    plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")

def keyfn(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    try:
        mt = os.path.getmtime(p)
    except OSError:
        mt = 0
    return (d, mt)

plans.sort(key=keyfn, reverse=True)

for p in plans:
    try:
        data = json.load(open(p))
    except Exception:
        continue
    tasks = data.get("tasks") or []
    if not tasks:
        continue
    slug = os.path.basename(os.path.dirname(p))
    status = {str(t["subtask_index"]): (t.get("status") or "available") for t in tasks}
    for t in sorted(tasks, key=lambda x: x.get("subtask_index", 0)):
        st = t.get("status") or "available"
        if st != "available":
            continue
        deps = t.get("depends_on") or []
        if not all(status.get(str(d)) == "completed" for d in deps):
            continue
        title = (t.get("title") or "").replace("\t", " ")
        print(f"{slug}\t{t.get('subtask_index')}\t{title}")
PYEOF
}

# Identify idle codex panes — last 12 lines show the default-prompt placeholder,
# no `Working (…)`, no `Reviewing approval request`.
idle_panes() {
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
  local pane_idx="$1" slug="$2" sub_idx="$3" title="$4"
  # Prompt explicitly overrides any pinned-plan worker prompt — workers
  # that were told "PRIORITY plan = X" must switch to the named plan when
  # the watcher dispatches.
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
  local -a tasks=()
  while IFS=$'\t' read -r slug idx title; do
    [[ -z "$slug" ]] && continue
    tasks+=("$slug"$'\t'"$idx"$'\t'"$title")
  done < <(ready_tasks_all)

  local -a panes=()
  while read -r p; do
    [[ -z "$p" ]] && continue
    panes+=("$p")
  done < <(idle_panes)

  if (( ${#tasks[@]} == 0 )); then
    printf '[%s] no ready tasks across any plan\n' "$(date +%T)"
    return
  fi
  if (( ${#panes[@]} == 0 )); then
    printf '[%s] no idle codex panes (ready tasks=%d)\n' "$(date +%T)" "${#tasks[@]}"
    return
  fi

  local n=${#tasks[@]}
  (( ${#panes[@]} < n )) && n=${#panes[@]}
  local i slug sub title
  for ((i=0; i<n; i++)); do
    IFS=$'\t' read -r slug sub title <<<"${tasks[$i]}"
    dispatch "${panes[$i]}" "$slug" "$sub" "$title"
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
