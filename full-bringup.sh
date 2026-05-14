#!/usr/bin/env bash
# full-bringup — one command that brings up a complete codex fleet:
#   * pre-spawn git cleanup
#   * publish the priority plan to Colony if it's only on disk
#   * tmux session `codex-fleet` with overview / fleet / plan / waves windows
#   * sibling `fleet-ticker` session with fleet-tick + cap-swap + state-pump
#
# Designed to be the SINGLE entry point so half-fleets (workers without
# dashboards / published plans / cap-swap) cannot happen.
#
# Usage:
#   bash scripts/codex-fleet/full-bringup.sh [--plan-slug <slug>] [--n <N>] [--no-attach]
#
# If --plan-slug is omitted, picks the newest openspec/plans/* by trailing
# YYYY-MM-DD slug suffix (matches plan-anim-generic.sh / waves-anim-generic.sh).

set -eo pipefail

REPO="${REPO:-/home/deadpool/Documents/recodee}"
SESSION="${SESSION:-codex-fleet}"
TICKER_SESSION="${TICKER_SESSION:-fleet-ticker}"
WAKE="${WAKE:-/tmp/codex-fleet-wake-prompt.md}"
N_PANES=8
ATTACH=1
PLAN_SLUG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --plan-slug) PLAN_SLUG="$2"; shift 2 ;;
    --n) N_PANES="$2"; shift 2 ;;
    --no-attach) ATTACH=0; shift ;;
    *) echo "unknown arg: $1"; exit 2 ;;
  esac
done

log() { printf '\033[36m[full-bringup]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[full-bringup]\033[0m %s\n' "$*"; }
die() { printf '\033[31m[full-bringup] FATAL:\033[0m %s\n' "$*"; exit 1; }

cd "$REPO"

# 1. Refuse if fleet already up
if tmux has-session -t "$SESSION" 2>/dev/null; then
  die "tmux session '$SESSION' already exists. Run scripts/codex-fleet/down.sh first."
fi

# 2. Pick the priority plan slug
if [ -z "$PLAN_SLUG" ]; then
  PLAN_SLUG=$(python3 - <<PY
import os, re, glob
plans = glob.glob("$REPO/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]),int(m[2]),int(m[3])) if m else (0,0,0)
    return (d, os.path.getmtime(p))
plans.sort(key=key, reverse=True)
print(os.path.basename(os.path.dirname(plans[0])) if plans else "")
PY
)
fi
[ -n "$PLAN_SLUG" ] || die "no plan slug provided and no openspec/plans/* found"
[ -f "openspec/plans/$PLAN_SLUG/plan.json" ] || die "plan workspace missing: openspec/plans/$PLAN_SLUG/plan.json"
log "priority plan: $PLAN_SLUG"

# 3. Pre-spawn git cleanup (prevents 'incorrect old value provided' inside agent-branch-start.sh)
log "pruning stale remote refs"
git -C "$REPO" remote prune origin 2>&1 | sed 's/^/  /' || true
git -C "$REPO" fetch --prune origin 2>&1 | sed 's/^/  /' >/dev/null || true

# 4. Ensure the plan is published to Colony so task_plan_list shows it
log "ensuring plan is published"
if colony plan publish "$PLAN_SLUG" --agent claude --session "full-bringup-$(date +%s)" 2>&1 | sed 's/^/  /'; then
  log "publish: ok (or already published — publish is idempotent)"
else
  warn "publish returned non-zero; check above. Workers may not see this plan in task_ready_for_agent."
fi

# 5. Verify wake prompt exists
if [ ! -f "$WAKE" ]; then
  warn "wake prompt missing at $WAKE; using scripts/codex-fleet/worker-prompt.md"
  WAKE="$REPO/scripts/codex-fleet/worker-prompt.md"
  [ -f "$WAKE" ] || die "no wake prompt found at $WAKE either"
fi

# 6. Pick candidate accounts. Two-stage filter:
#    (a) Score by codex-auth's 5h% * weekly% (fast, but unreliable for codex
#        CLI's own rolling cap).
#    (b) Probe each candidate with `codex exec` to detect the *real* cap
#        state. Take top 3N candidates so we can skip up to 2N capped
#        accounts and still end up with N healthy ones.
TOP=$((N_PANES * 3))
log "picking $TOP candidate accounts (will probe + filter down to $N_PANES healthy)"
CANDIDATES=$(codex-auth list 2>/dev/null | N="$TOP" python3 -c '
import os, sys, re
n = int(os.environ["N"])
rows = []
for line in sys.stdin:
    em = re.search(r"([\w.+-]+@[\w.-]+\.[a-z]+)", line)
    if not em: continue
    email = em.group(1)
    h5m = re.search(r"5h=(\d+)%", line); wkm = re.search(r"weekly=(\d+)%", line)
    if not h5m or not wkm: continue
    h, w = int(h5m.group(1)), int(wkm.group(1))
    if h < 40 or w < 25: continue
    rows.append((h*w, email))
rows.sort(reverse=True)
for _, email in rows[:n]:
    print(email)
')
[ -n "$CANDIDATES" ] || die "no candidate accounts found (need 5h>=40%, wk>=25% in codex-auth list)"
CAND_N=$(printf "%s\n" "$CANDIDATES" | wc -l)
log "ranked $CAND_N candidates by codex-auth score; running live probe..."

# (b) Live probe — keep only candidates whose codex CLI is actually usable.
HEALTHY_EMAILS=$(bash "$REPO/scripts/codex-fleet/cap-probe.sh" "$N_PANES" $CANDIDATES 2>/tmp/cap-probe.err) || true
HEALTHY_N=$(printf "%s\n" "$HEALTHY_EMAILS" | grep -c "@" || true)
if [ "$HEALTHY_N" -lt "$N_PANES" ]; then
  warn "cap-probe found only $HEALTHY_N/$N_PANES healthy accounts"
  warn "$(cat /tmp/cap-probe.err 2>/dev/null)"
  [ "$HEALTHY_N" -eq 0 ] && die "no healthy accounts; check /tmp/claude-viz/cap-probe.log"
fi
log "$HEALTHY_N healthy account(s) confirmed by live probe"

# Map healthy emails to id|email format expected downstream
ACCOUNTS=$(printf "%s\n" "$HEALTHY_EMAILS" | python3 -c '
import sys
m={"magnoliavilag":"magnolia","gitguardex":"gg","pipacsclub":"pipacs"}
for line in sys.stdin:
    email = line.strip()
    if not email: continue
    part, dom = email.split("@", 1)
    dom = dom.split(".", 1)[0]
    dom = m.get(dom, dom)
    print(f"{part}-{dom}|{email}")
')
COUNT=$(echo "$ACCOUNTS" | grep -c "|")
log "final account list: $COUNT"

# 7. Stage CODEX_HOMEs
log "staging per-account CODEX_HOMEs"
while IFS='|' read -r id email; do
  [ -z "$id" ] && continue
  d="/tmp/codex-fleet/$id"
  mkdir -p "$d"
  cp "$HOME/.codex/accounts/$email.json" "$d/auth.json"
  chmod 600 "$d/auth.json"
  [ -e "$d/config.toml" ] || ln -s "$HOME/.codex/config.toml" "$d/config.toml"
done <<< "$ACCOUNTS"

# 8. Create the main session with overview window
log "creating tmux session: $SESSION"
tmux new-session -d -s "$SESSION" -n overview -x 274 -y 76
tmux set-option -t "$SESSION" -g mouse on
tmux set-option -w -t "$SESSION:overview" remain-on-exit on

# Status bar styling is owned by style-tabs.sh (iOS-style 3-row tab strip,
# rounded pane borders, sticky right-click menu). It runs after windows are
# created so window-status-format applies to all six tabs. Do NOT set
# per-session `status on/off` here — a boolean session-local value shadows the
# global numeric `status N` style-tabs sets, clamping back to 1 row and
# silently hiding the tab strip.

# 9. Split overview into N panes (default 2 columns x 4 rows = 8)
ROWS=$((N_PANES / 2))
tmux split-window -h -t "$SESSION:overview" -p 50
# Column A: split horizontally (rows-1) times
for i in $(seq 1 $((ROWS - 1))); do
  pct=$((100 - 100 / (ROWS - i + 1)))
  tmux split-window -v -t "$SESSION:overview.$((i - 1))" -p "$pct"
done
# Column B: similar
COL_B_START=$ROWS
for i in $(seq 1 $((ROWS - 1))); do
  pct=$((100 - 100 / (ROWS - i + 1)))
  tmux split-window -v -t "$SESSION:overview.$((COL_B_START + i - 1))" -p "$pct"
done
tmux select-layout -t "$SESSION:overview" tiled

# 10. Spawn codex into each pane with CODEX_GUARD_BYPASS=1
log "launching $N_PANES codex workers"
PANE_IDS=( $(tmux list-panes -t "$SESSION:overview" -F '#{pane_id}') )
i=0
while IFS='|' read -r id email; do
  [ -z "$id" ] && continue
  pid="${PANE_IDS[$i]}"
  tmux set-option -p -t "$pid" '@panel' "[codex-$id]"
  tmux respawn-pane -k -t "$pid" \
    "env CODEX_GUARD_BYPASS=1 CODEX_HOME=/tmp/codex-fleet/$id CODEX_FLEET_AGENT_NAME=codex-$id CODEX_FLEET_ACCOUNT_EMAIL=$email codex \"\$(cat $WAKE)\""
  i=$((i + 1))
done <<< "$ACCOUNTS"

# 11. Create fleet / plan / waves windows
log "creating fleet / plan / waves windows"
tmux new-window -d -t "$SESSION:" -n fleet "bash $REPO/scripts/codex-fleet/fleet-state-anim.sh"
tmux new-window -d -t "$SESSION:" -n plan  "bash $REPO/scripts/codex-fleet/plan-tree-anim.sh"
tmux new-window -d -t "$SESSION:" -n waves "bash $REPO/scripts/codex-fleet/waves-anim-generic.sh"
tmux new-window -d -t "$SESSION:" -n review  "bash $REPO/scripts/codex-fleet/review-board.sh"
tmux new-window -d -t "$SESSION:" -n watcher "bash $REPO/scripts/codex-fleet/watcher-board.sh"
# legacy plain watcher (replaced by graphical watcher-board.sh):
# tmux new-window ... "watch -n 2 -t -c 'cat /tmp/claude-viz/cap-swap-status.txt 2>/dev/null; echo; echo --- recent swaps ---; tail -20 /tmp/claude-viz/cap-swap.log 2>/dev/null'"
tmux set-option -w -t "$SESSION:plan"  remain-on-exit on
tmux set-option -w -t "$SESSION:waves" remain-on-exit on

# 11b. Apply canonical iOS-style chrome (3-row tab strip at top, rounded pane
# borders with `▭ #{@panel}` headers, sticky right-click menu). Runs after
# windows exist so window-status-format covers all six tabs.
log "applying iOS-style chrome"
CODEX_FLEET_SESSION="$SESSION" bash "$REPO/scripts/codex-fleet/style-tabs.sh" >/dev/null 2>&1 \
  || warn "style-tabs.sh failed (chrome will fall back to tmux defaults)"

# 12. Sibling fleet-ticker session: ticker + cap-swap + state-pump
log "creating sibling session: $TICKER_SESSION"
if tmux has-session -t "$TICKER_SESSION" 2>/dev/null; then
  tmux kill-session -t "$TICKER_SESSION"
fi
# ticker uses fleet-tick-daemon.sh wrapper — re-spawn-safe vs the raw
# fleet-tick.sh which `set -eo pipefail`-crashes mid-tick on any failed
# regex / capture-pane and silently halts the live viz.
tmux new-session -d -s "$TICKER_SESSION" -n ticker     "bash $REPO/scripts/codex-fleet/fleet-tick-daemon.sh"
tmux new-window  -d -t "$TICKER_SESSION:" -n cap-swap  "bash $REPO/scripts/codex-fleet/cap-swap-daemon.sh"
tmux new-window  -d -t "$TICKER_SESSION:" -n state-pump "bash $REPO/scripts/codex-fleet/colony-state-pump.sh"
tmux new-window  -d -t "$TICKER_SESSION:" -n review-detector "bash $REPO/scripts/codex-fleet/plan-complete-detector.sh"
# force-claim scans ALL openspec plans every 15s, finds deps-satisfied
# `available` tasks, and dispatches them onto idle codex panes via
# tmux send-keys. Keeps the fleet pulled into ready work when its
# originally-pinned plan completes.
tmux new-window  -d -t "$TICKER_SESSION:" -n force-claim "bash $REPO/scripts/codex-fleet/force-claim.sh --loop --interval=15"

# stall-watcher: every 60s, `colony rescue stranded --apply` releases claims
# held > 30m without progress, then enqueues a takeover_recommended event
# per rescued agent into /tmp/claude-viz/supervisor-queue.jsonl.
# supervisor: consumes that queue and spawns fresh kitty + codex workers for
# the rescued slots with the takeover prompt. Together they unwedge the queue
# when one agent dies holding sub-task claims that block downstream subs.
tmux new-window  -d -t "$TICKER_SESSION:" -n stall-watcher "bash $REPO/scripts/codex-fleet/stall-watcher.sh"
tmux new-window  -d -t "$TICKER_SESSION:" -n supervisor    "bash $REPO/scripts/codex-fleet/supervisor.sh"

# 12b. Chrome the ticker session too so attaching to it shows the same iOS
# tab strip / rounded pane borders / sticky menu as the main session.
CODEX_FLEET_SESSION="$TICKER_SESSION" bash "$REPO/scripts/codex-fleet/style-tabs.sh" >/dev/null 2>&1 \
  || warn "style-tabs.sh failed for $TICKER_SESSION"

# 12c. Verify chrome actually rendered (catches the session-local `status on`
# shadow regression where tmux clamps to 1 row and silently hides the tab
# strip). Expected: status_height=3.
chrome_h=$(tmux display-message -p -t "$SESSION:overview" '#{?status_height,#{status_height},#{e|-|:#{client_height},#{window_height}}}' 2>/dev/null || echo "?")
if [ "$chrome_h" = "3" ]; then
  log "iOS chrome verified: status_height=$chrome_h"
else
  warn "iOS chrome looks wrong: status_height=$chrome_h (expected 3)"
fi

log "DONE."
log "  main session:    tmux attach -t $SESSION"
log "  ticker session:  tmux attach -t $TICKER_SESSION"
log "  cap-swap log:    tail -f /tmp/claude-viz/cap-swap.log"
log "  state pump log:  tail -f /tmp/claude-viz/colony-state-pump.log"
log "  force-claim:     tmux attach -t $TICKER_SESSION \\; select-window -t force-claim"

if [ "$ATTACH" = "1" ] && [ -t 1 ]; then
  exec tmux attach -t "$SESSION"
fi
