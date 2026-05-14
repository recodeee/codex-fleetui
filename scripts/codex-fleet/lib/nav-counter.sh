#!/usr/bin/env bash
# nav-counter — print a single integer for a codex-fleet navmenu tab.
#
# Usage:
#   bash scripts/codex-fleet/lib/nav-counter.sh <tab>
#     tab ∈ { overview | fleet | plan | waves | review }
#
# Called every status-interval seconds from style-tabs.sh's
# window-status-format / window-status-current-format. Must be:
#   * fast (no network, no MCP)
#   * silent on stderr (tmux renders stderr as garbage in the bar)
#   * always exit 0 with a plain integer on stdout (fallback "0")
#
# Counter sources:
#   overview → live pane count of codex-fleet:overview
#   fleet    → /tmp/claude-viz/fleet-active-accounts.txt line count
#              (fallback: overview pane count when the file is empty/missing)
#   plan     → number of tasks in the newest openspec/plans/*/plan.json
#   waves    → number of distinct waves derived from depends_on depth in
#              that same plan.json (each task's wave = longest dependency
#              chain rooted at it + 1)
#   review   → /tmp/claude-viz/review-pending line count, else 0
#
# TODO(review): wire to `gh pr list --search "review-requested:@me" --json url`
# once we have a cheap cached query. Until then, the file-based count is the
# contract — drop a path into /tmp/claude-viz/review-pending to surface it.
set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/_tmux.sh"

TAB="${1:-}"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$HOME/Documents/codex-fleet}"

emit() {
  # Always print a non-negative integer, no newline issues.
  local n="${1:-0}"
  case "$n" in
    ''|*[!0-9]*) n=0 ;;
  esac
  printf '%s' "$n"
}

overview_panes() {
  local n
  n=$(tmux list-panes -t "$SESSION:overview" 2>/dev/null | wc -l | tr -d ' ')
  emit "$n"
}

case "$TAB" in
  overview)
    overview_panes
    ;;
  fleet)
    f="/tmp/claude-viz/fleet-active-accounts.txt"
    if [ -s "$f" ]; then
      n=$(grep -cve '^[[:space:]]*$' "$f" 2>/dev/null || echo 0)
      emit "$n"
    else
      overview_panes
    fi
    ;;
  plan|waves)
    out=$(python3 - "$REPO_ROOT" "$TAB" <<'PY' 2>/dev/null

import os, re, glob, json, sys
repo, tab = sys.argv[1], sys.argv[2]
plans = glob.glob(os.path.join(repo, "openspec/plans/*/plan.json"))
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    try:
        mt = os.path.getmtime(p)
    except OSError:
        mt = 0
    return (d, mt)
plans.sort(key=key, reverse=True)
if not plans:
    print(0); sys.exit(0)
try:
    with open(plans[0]) as f:
        d = json.load(f)
except Exception:
    print(0); sys.exit(0)
tasks = d.get("tasks") or d.get("subtasks") or []
if tab == "plan":
    print(len(tasks)); sys.exit(0)
# waves: prefer explicit `wave` keys if any task carries one, else derive
# from longest depends_on chain depth (a tasks's wave = 1 + max(wave of deps)).
waves_explicit = {t.get("wave") for t in tasks if t.get("wave") is not None}
if waves_explicit:
    print(len(waves_explicit)); sys.exit(0)
by_idx = {}
for i, t in enumerate(tasks):
    idx = t.get("subtask_index", i)
    by_idx[idx] = t
memo = {}
def depth(idx, seen=None):
    if idx in memo: return memo[idx]
    if seen is None: seen = set()
    if idx in seen: return 1
    seen = seen | {idx}
    t = by_idx.get(idx) or {}
    deps = t.get("depends_on") or []
    d = 1 + max((depth(dep, seen) for dep in deps if dep in by_idx), default=0)
    memo[idx] = d
    return d
depths = {depth(i) for i in by_idx}
print(len(depths) if depths else 0)
PY
)
    emit "${out:-0}"
    ;;
  review)
    f="/tmp/claude-viz/review-pending"
    if [ -s "$f" ]; then
      n=$(grep -cve '^[[:space:]]*$' "$f" 2>/dev/null || echo 0)
      emit "$n"
    else
      emit 0
    fi
    ;;
  *)
    emit 0
    ;;
esac
