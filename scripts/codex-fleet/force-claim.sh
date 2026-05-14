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
# Loop mode now starts claim-trigger.sh for event-driven wakeups and keeps this
# script's polling pass as a slow backstop.
#
# Operator-pre-approved: dispatching prompts into gx-fleet/codex-fleet
# panes is an allowed flow (see ~/.claude memory feedback_gx_fleet_dispatch_authorized).
#
# Usage:
#   bash scripts/codex-fleet/force-claim.sh                 # one-shot
#   bash scripts/codex-fleet/force-claim.sh --dry-run       # show plan, no dispatch
#   bash scripts/codex-fleet/force-claim.sh --loop          # event + poll every 30s
#   bash scripts/codex-fleet/force-claim.sh --loop --quit-when-empty
#                                                            # exit 0 after 3 consecutive
#                                                            # passes with no available/claimed
#                                                            # work across any plan
#   bash scripts/codex-fleet/force-claim.sh --loop --empty-threshold=5
#                                                            # require 5 consecutive empties
#   FORCE_CLAIM_SESSION=codex-fleet ...                      # tmux session override
#   FORCE_CLAIM_WINDOW=overview     ...                      # window with codex panes
#   FORCE_CLAIM_PLAN_JSON=/path/plan.json                    # pin to single plan
#   FORCE_CLAIM_EMPTY_THRESHOLD=3                            # env equivalent
#   CODEX_FLEET_CLAIM_MODE=both|event|poll                   # default: both
set -eo pipefail

REPO="${FORCE_CLAIM_REPO:-/home/deadpool/Documents/recodee}"
SESSION="${FORCE_CLAIM_SESSION:-codex-fleet}"
WINDOW="${FORCE_CLAIM_WINDOW:-overview}"
LOOP=0
DRY=0
INTERVAL="${FORCE_CLAIM_INTERVAL:-30}"
CLAIM_MODE="${CODEX_FLEET_CLAIM_MODE:-both}"
FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
CLAIM_TRIGGER_LOG="${CLAIM_TRIGGER_LOG:-$FLEET_STATE_DIR/claim-trigger.log}"
CLAIM_TRIGGER_PID=""
# --quit-when-empty: exit 0 after N consecutive passes where every plan is
# fully complete (no `available` and no `claimed` tasks anywhere). N comes
# from --empty-threshold or env FORCE_CLAIM_EMPTY_THRESHOLD (default 3) so
# a brief race where a new plan is being published doesn't kill the daemon.
QUIT_EMPTY=0
EMPTY_THRESHOLD="${FORCE_CLAIM_EMPTY_THRESHOLD:-3}"
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    --loop)    LOOP=1 ;;
    --interval=*) INTERVAL="${a#--interval=}" ;;
    --quit-when-empty) QUIT_EMPTY=1 ;;
    --empty-threshold=*) EMPTY_THRESHOLD="${a#--empty-threshold=}" ;;
  esac
done

case "$CLAIM_MODE" in
  both|event|poll) ;;
  *)
    printf 'force-claim: invalid CODEX_FLEET_CLAIM_MODE=%s (expected both|event|poll)\n' "$CLAIM_MODE" >&2
    exit 2
    ;;
esac

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

# Aggregate (available, claimed, completed, blocked) counts across every plan.
# Prints one line: "<available>\t<claimed>\t<completed>\t<blocked>". Used by
# the --quit-when-empty loop to decide when the fleet has truly run dry.
plans_status_summary() {
  python3 - "$REPO" "${FORCE_CLAIM_PLAN_JSON:-}" <<'PYEOF'
import os, sys, glob, json
repo, pin = sys.argv[1], sys.argv[2]
plans = [pin] if (pin and os.path.isfile(pin)) else glob.glob(f"{repo}/openspec/plans/*/plan.json")
avail = claimed = completed = blocked = 0
for p in plans:
    try:
        data = json.load(open(p))
    except Exception:
        continue
    for t in (data.get("tasks") or []):
        st = (t.get("status") or "available")
        if   st == "available": avail     += 1
        elif st == "claimed":   claimed   += 1
        elif st == "completed": completed += 1
        elif st == "blocked":   blocked   += 1
print(f"{avail}\t{claimed}\t{completed}\t{blocked}")
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

start_claim_trigger() {
  if [[ "$CLAIM_MODE" == "poll" ]]; then
    return 0
  fi
  if (( DRY == 1 )); then
    printf '[%s] claim-trigger skipped in dry-run mode (mode=%s)\n' "$(date +%T)" "$CLAIM_MODE"
    return 0
  fi

  local trigger="$REPO/scripts/codex-fleet/claim-trigger.sh"
  if [[ ! -x "$trigger" ]]; then
    printf '[%s] claim-trigger unavailable at %s; continuing with poll mode\n' "$(date +%T)" "$trigger" >&2
    return 0
  fi

  mkdir -p "$(dirname "$CLAIM_TRIGGER_LOG")"
  CLAIM_TRIGGER_REPO="$REPO" \
    CLAIM_TRIGGER_SESSION="$SESSION" \
    CLAIM_TRIGGER_WINDOW="$WINDOW" \
    CLAIM_TRIGGER_LOG="$CLAIM_TRIGGER_LOG" \
    "$trigger" >>"$CLAIM_TRIGGER_LOG" 2>&1 &
  CLAIM_TRIGGER_PID="$!"
  printf '[%s] claim-trigger started pid=%s mode=%s log=%s\n' \
    "$(date +%T)" "$CLAIM_TRIGGER_PID" "$CLAIM_MODE" "$CLAIM_TRIGGER_LOG"

  sleep 0.1
  if ! kill -0 "$CLAIM_TRIGGER_PID" 2>/dev/null; then
    local ec=0
    wait "$CLAIM_TRIGGER_PID" || ec=$?
    printf '[%s] claim-trigger exited early status=%s; poll backstop remains active\n' "$(date +%T)" "$ec" >&2
    CLAIM_TRIGGER_PID=""
  fi
}

stop_claim_trigger() {
  if [[ -n "$CLAIM_TRIGGER_PID" ]] && kill -0 "$CLAIM_TRIGGER_PID" 2>/dev/null; then
    kill "$CLAIM_TRIGGER_PID" 2>/dev/null || true
    wait "$CLAIM_TRIGGER_PID" 2>/dev/null || true
  fi
}

if (( LOOP == 1 )); then
  trap 'stop_claim_trigger' EXIT
  trap 'stop_claim_trigger; echo force-claim: stopping >&2; exit 0' INT TERM
  start_claim_trigger
  if [[ "$CLAIM_MODE" == "event" ]]; then
    if (( DRY == 1 )); then
      one_pass
      exit 0
    fi
    if [[ -z "$CLAIM_TRIGGER_PID" ]]; then
      printf 'force-claim: event mode requested but claim-trigger is not running\n' >&2
      exit 1
    fi
    wait "$CLAIM_TRIGGER_PID"
    exit $?
  fi

  empty_streak=0
  while true; do
    one_pass
    if (( QUIT_EMPTY == 1 )); then
      # Plans-rolled-up status. Quit only when no available AND no claimed
      # work remains across every plan — completed-only or blocked-only is
      # a stable end state.
      IFS=$'\t' read -r avail claimed completed blocked < <(plans_status_summary)
      if (( avail == 0 && claimed == 0 )); then
        empty_streak=$(( empty_streak + 1 ))
        printf '[%s] empty-streak=%d/%d  completed=%d blocked=%d\n' \
          "$(date +%T)" "$empty_streak" "$EMPTY_THRESHOLD" "$completed" "$blocked"
        if (( empty_streak >= EMPTY_THRESHOLD )); then
          printf '[%s] all plans drained — exiting cleanly\n' "$(date +%T)"
          exit 0
        fi
      else
        empty_streak=0
      fi
    fi
    sleep "$INTERVAL"
  done
else
  one_pass
fi
