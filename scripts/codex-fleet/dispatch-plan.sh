#!/usr/bin/env bash
#
# dispatch-plan.sh — send claim+implement prompts to codex panes for every
# sub-task of a published Colony plan, one pane per sub-task.
#
# This is the "phase 4" automation step of the codex-fleet AUTO-DISPATCH
# PROTOCOL (see skills/codex-fleet/SKILL.md). It assumes:
#
#   1. The plan is on disk at openspec/plans/<slug>/plan.json
#   2. The plan is already published to Colony (task_plan_publish has been
#      called — `colony plan list` shows it as registered, not "unpublished")
#   3. A tmux session named via $CODEX_FLEET_SESSION (default codex-fleet-2)
#      exists with an `overview` window whose pane indexes 0..N-1 are each
#      running a codex CLI with CODEX_GUARD_BYPASS=1 set.
#
# What it does:
#   - Reads plan.json sub-tasks in subtask_index order.
#   - For each i in [0, min(num_subtasks, num_panes)):
#       * Generates a claim+implement prompt referencing sub-task i.
#       * Sends the prompt to pane i via `tmux send-keys -l` (literal).
#       * Sends Enter to submit.
#       * Does NOT send Escape (that clears the input box; learned the hard
#         way 2026-05-15).
#   - After all dispatches, waits 4s and verifies each pane shows
#     `• Working` in its capture. Panes still showing "Create a plan?"
#     dialog get a Shift+Tab (BTab) press to enter plan mode and start
#     processing. Panes showing "usage limit" get logged for the operator.
#
# Usage:
#   bash scripts/codex-fleet/dispatch-plan.sh <plan-slug>
#   bash scripts/codex-fleet/dispatch-plan.sh <plan-slug> --session codex-fleet
#   bash scripts/codex-fleet/dispatch-plan.sh <plan-slug> --max-panes 5
#   bash scripts/codex-fleet/dispatch-plan.sh <plan-slug> --plan-mode   # send BTab after Enter
#
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Route every tmux call through lib/_tmux.sh (socket support).
source "$SCRIPT_DIR/lib/_tmux.sh"

PLAN_SLUG=""
SESSION="${CODEX_FLEET_SESSION:-codex-fleet-2}"
MAX_PANES=""
PLAN_MODE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)    SESSION="$2"; shift 2 ;;
    --max-panes)  MAX_PANES="$2"; shift 2 ;;
    --plan-mode)  PLAN_MODE=1; shift ;;
    -h|--help)    sed -n '2,42p' "$0"; exit 0 ;;
    -*)           echo "fatal: unknown flag $1" >&2; exit 2 ;;
    *)            PLAN_SLUG="$1"; shift ;;
  esac
done

[[ -n "$PLAN_SLUG" ]] || { echo "fatal: plan-slug required" >&2; exit 2; }
PLAN_JSON="$REPO/openspec/plans/$PLAN_SLUG/plan.json"
[[ -f "$PLAN_JSON" ]] || { echo "fatal: plan not on disk: $PLAN_JSON" >&2; exit 2; }
tmux has-session -t "$SESSION" 2>/dev/null || { echo "fatal: tmux session $SESSION not running" >&2; exit 2; }
tmux list-windows -t "$SESSION" 2>/dev/null | grep -q ': overview' || \
  { echo "fatal: $SESSION has no 'overview' window" >&2; exit 2; }

# Pane count.
N_PANES=$(tmux list-panes -t "$SESSION:overview" -F '#{pane_index}' | wc -l)
echo "[dispatch-plan] session=$SESSION overview-panes=$N_PANES plan=$PLAN_SLUG"

# Per-subtask dispatch.
PY_OUT="$(python3 - "$PLAN_JSON" "$N_PANES" "${MAX_PANES:-$N_PANES}" <<'PY'
import json, sys
plan = json.load(open(sys.argv[1]))
n_panes = int(sys.argv[2])
max_panes = int(sys.argv[3])
limit = min(n_panes, max_panes, len(plan["tasks"]))
for i, t in enumerate(plan["tasks"][:limit]):
    title = t.get("title","").replace("\n"," ").replace("|","-")
    file_scope = " ".join(t.get("file_scope",[]))
    crate = ""
    for f in t.get("file_scope",[]):
        if f.startswith("rust/"):
            parts = f.split("/")
            if len(parts) >= 2:
                crate = parts[1]
                break
    print(f"{i}|{t['subtask_index']}|{crate}|{file_scope}|{title}")
PY
)"

if [[ -z "$PY_OUT" ]]; then
  echo "fatal: no sub-tasks found in plan" >&2
  exit 2
fi

DISPATCHED=()
while IFS='|' read -r pane_i sub_i crate files title; do
  [[ -z "$pane_i" ]] && continue
  prompt="Claim sub-task ${sub_i} of plan ${PLAN_SLUG} via colony task_plan_claim_subtask. Agent slug = env CODEX_FLEET_AGENT_NAME (read \$CODEX_FLEET_AGENT_NAME). session_id = fresh UUID. Title: ${title}. Implement on a new agent worktree per ${REPO}/AGENTS.md: gx branch start \"${PLAN_SLUG}-sub-${sub_i}\" \"\$CODEX_FLEET_AGENT_NAME\". Deliverable: ${files} (per the sub-task description in plan.json — do NOT edit main.rs / lib.rs / any existing file unless plan.json explicitly lists it). Run cargo check -p ${crate} and cargo test -p ${crate}. Ship via gx branch finish --via-pr --wait-for-merge --cleanup. Then task_post evidence (branch, PR URL, MERGED state) and task_plan_complete_subtask."
  echo "  → pane $pane_i  sub-$sub_i  crate=$crate"
  tmux send-keys -t "$SESSION:overview.$pane_i" -l "$prompt"
  tmux send-keys -t "$SESSION:overview.$pane_i" Enter
  DISPATCHED+=("$pane_i:sub-$sub_i:$crate")
done <<< "$PY_OUT"

echo "[dispatch-plan] dispatched ${#DISPATCHED[@]} prompts. settling 4s …"
sleep 4

# Verify each pane and recover stuck ones.
echo "[dispatch-plan] status:"
for entry in "${DISPATCHED[@]}"; do
  pane_i="${entry%%:*}"
  rest="${entry#*:}"
  cap="$(tmux capture-pane -t "$SESSION:overview.$pane_i" -p | tail -10)"
  if grep -q "Working" <<<"$cap"; then
    echo "  ✓ pane $pane_i ($rest) — Working"
  elif grep -q "Create a plan?" <<<"$cap"; then
    if (( PLAN_MODE == 1 )); then
      echo "  ⤷ pane $pane_i ($rest) — Plan-mode dialog open; sending BTab (enter plan mode)"
      tmux send-keys -t "$SESSION:overview.$pane_i" BTab
    else
      echo "  ! pane $pane_i ($rest) — Plan-mode dialog open. Press Esc in pane (clears input), or re-run with --plan-mode."
    fi
  elif grep -q "usage limit" <<<"$cap"; then
    acct="$(tmux show-environment -t "$SESSION" 2>/dev/null | grep CODEX_HOME || true)"
    echo "  ⚠ pane $pane_i ($rest) — usage limit hit. Account needs swap. Detail: $(grep -oE 'try again at [^.]*' <<<"$cap" | head -1)"
  else
    echo "  ? pane $pane_i ($rest) — no Working indicator yet; bottom snippet: $(echo "$cap" | tail -2 | tr '\n' ' ' | sed 's/  */ /g' | cut -c1-120)"
  fi
done

echo "[dispatch-plan] done."
