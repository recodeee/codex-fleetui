#!/usr/bin/env bash
#
# add-workers — spawn N additional codex panes into an existing fleet to
# pick up ready sub-tasks. Use when the plan board has `available` subs but
# the existing panes are dead, capped, or stuck idle.
#
# Picks N healthy accounts (cap-probe if present, else agent-auth list with
# the canonical 5h<100% / weekly<90% / not-already-active filter), then
# defaults to respawning dead/idle panes in the running tmux fleet's
# `overview` window. Falls back to kitty windows when no fleet session
# exists or when `--kitty` is passed. Records the new accounts in the active
# accounts file so cap-swap-daemon and supervisor don't re-spawn them.
#
# Usage:
#   bash scripts/codex-fleet/add-workers.sh 4                        # respawn into overview
#   bash scripts/codex-fleet/add-workers.sh 4 --kitty                # old behavior: new kitty windows
#   bash scripts/codex-fleet/add-workers.sh 4 --plan-slug <slug>
#   bash scripts/codex-fleet/add-workers.sh 4 --fleet-id 2
#   bash scripts/codex-fleet/add-workers.sh --auto                   # one worker per ready sub
#   bash scripts/codex-fleet/add-workers.sh 2 --dry-run               # preview pick + target

set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

N=""
PLAN_SLUG=""
FLEET_ID="${FLEET_ID:-}"
AUTO=0
DRY_RUN=0
# Spawn target: tmux (in-overview, default) or kitty (separate window).
# `tmux` mode prefers respawning dead/idle panes in the existing overview
# window over splitting (which would shrink every pane). Falls back to
# kitty when no tmux session is available.
TARGET="${ADD_WORKERS_TARGET:-tmux}"
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
PROMPT_TEMPLATE="${CODEX_FLEET_WORKER_PROMPT:-$SCRIPT_DIR/worker-prompt.md}"

while [ $# -gt 0 ]; do
  case "$1" in
    --plan-slug) PLAN_SLUG="$2"; shift 2 ;;
    --fleet-id) FLEET_ID="$2"; shift 2 ;;
    --auto) AUTO=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --kitty) TARGET="kitty"; shift ;;
    --tmux) TARGET="tmux"; shift ;;
    -h|--help) sed -n '1,21p' "$0"; exit 0 ;;
    [0-9]*) N="$1"; shift ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [ -n "$FLEET_ID" ]; then
  FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz/fleet-$FLEET_ID}"
else
  FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
fi
ACTIVE_FILE="$FLEET_STATE_DIR/fleet-active-accounts.txt"
mkdir -p "$FLEET_STATE_DIR"
touch "$ACTIVE_FILE"

log() { printf '\033[36m[add-workers]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[add-workers]\033[0m %s\n' "$*"; }
die() { printf '\033[31m[add-workers] FATAL:\033[0m %s\n' "$*" >&2; exit 1; }

# Resolve target plan slug. Falls back to newest openspec/plans/*.
if [ -z "$PLAN_SLUG" ]; then
  PLAN_SLUG=$(python3 - <<PY
import os, re, glob
plans = glob.glob("$REPO_ROOT/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    return (int(m[1]),int(m[2]),int(m[3])) if m else (0,0,0)
plans.sort(key=key, reverse=True)
print(os.path.basename(os.path.dirname(plans[0])) if plans else "")
PY
)
  [ -n "$PLAN_SLUG" ] || die "no plan slug given and no openspec/plans/*/plan.json found"
fi
PLAN_JSON="$REPO_ROOT/openspec/plans/$PLAN_SLUG/plan.json"
[ -f "$PLAN_JSON" ] || die "plan.json not found: $PLAN_JSON"

# Count ready sub-tasks (status in available/ready, deps satisfied).
ready_count() {
  python3 - <<PY
import json
with open("$PLAN_JSON") as fh:
    p = json.load(fh)
subs = p.get("subtasks", []) or p.get("tasks", [])
done_ids = {s.get("subtask_index") for s in subs if s.get("status") in ("completed","done")}
ready = 0
for s in subs:
    if s.get("status") not in (None, "available", "ready"):
        continue
    deps = s.get("depends_on") or s.get("deps") or []
    norm = []
    for d in deps:
        try: norm.append(int(d))
        except (TypeError, ValueError): norm.append(d)
    if all(d in done_ids for d in norm):
        ready += 1
print(ready)
PY
}

if [ "$AUTO" = "1" ]; then
  rc=$(ready_count)
  N="${rc:-0}"
  log "auto-counted ready subs: N=$N"
fi
[ -n "$N" ] || die "missing N (count of workers to add). Pass an integer or --auto."
[ "$N" -gt 0 ] || { log "N=$N — nothing to do"; exit 0; }

log "plan=$PLAN_SLUG  N=$N  active-file=$ACTIVE_FILE"

# Pick N healthy account IDs not already in $ACTIVE_FILE.
#
# Source priority (first to yield ≥1 row wins):
#   1. lib/discover-accounts.sh — every authenticated codex CLI home on
#      disk (/tmp/codex-fleet/*/auth.json). This is the canonical pool
#      because accounts.yml historically lagged: tonight the fleet had 18
#      authenticated accounts on disk but accounts.yml declared only 4,
#      so add-workers couldn't grow beyond 4. The discoverer closes that
#      gap with zero manual upkeep.
#   2. cap-probe.sh — same emails but pre-filtered through the 5h/weekly
#      cap budget. Use when present so we don't pick a capped account.
#   3. agent-auth list — legacy fallback for hosts without cap-probe.
#
# Each path emits TSV: `<aid>\t<email>` lines. Accounts already in
# $ACTIVE_FILE are filtered out at every layer.
pick_accounts() {
  local need="$1"
  # ── Source 1: disk discovery ─────────────────────────────────────────────
  if [ -x "$SCRIPT_DIR/lib/discover-accounts.sh" ]; then
    local discovered_tmp
    discovered_tmp="$(mktemp)"
    local discover_session="${CODEX_FLEET_SESSION:-codex-fleet${FLEET_ID:+-$FLEET_ID}}"
    ACTIVE_FILE="$ACTIVE_FILE" bash "$SCRIPT_DIR/lib/discover-accounts.sh" \
      --exclude-active --exclude-tmux "$discover_session" \
      > "$discovered_tmp" 2>/dev/null || true
    if [ -s "$discovered_tmp" ]; then
      # Optionally filter through cap-probe to drop capped accounts. If
      # cap-probe isn't available, take everything discovered as-is.
      if [ -x "$SCRIPT_DIR/cap-probe.sh" ]; then
        local healthy_emails
        healthy_emails="$(bash "$SCRIPT_DIR/cap-probe.sh" "$need" 2>/dev/null || true)"
        if [ -n "$healthy_emails" ]; then
          # Intersection: discovered ∩ healthy.
          while IFS=$'\t' read -r aid email; do
            [ -n "$aid" ] || continue
            if printf '%s\n' "$healthy_emails" | grep -Fxq "$email"; then
              printf '%s\t%s\n' "$aid" "$email"
            fi
          done < "$discovered_tmp" | head -n "$need"
          rm -f "$discovered_tmp"
          return
        fi
      fi
      # No cap-probe (or it returned nothing): take the first N discovered.
      head -n "$need" "$discovered_tmp"
      rm -f "$discovered_tmp"
      return
    fi
    rm -f "$discovered_tmp"
  fi
  # ── Source 2: cap-probe + accounts.yml email→aid map ────────────────────
  if [ -x "$SCRIPT_DIR/cap-probe.sh" ]; then
    local emails
    emails="$(bash "$SCRIPT_DIR/cap-probe.sh" "$need" 2>/dev/null || true)"
    if [ -n "$emails" ]; then
      printf '%s\n' "$emails" | while IFS= read -r email; do
        [ -n "$email" ] || continue
        local aid
        aid="$(python3 -c "
import sys, re
email = sys.argv[1]
try:
    with open('$SCRIPT_DIR/accounts.yml') as fh:
        txt = fh.read()
except FileNotFoundError:
    sys.exit(0)
m = re.search(r'-\s*id:\s*(\S+)\s*\n\s*email:\s*' + re.escape(email), txt)
print(m.group(1) if m else '')
" "$email")"
        if [ -n "$aid" ] && ! grep -Fxq "$aid" "$ACTIVE_FILE"; then
          printf '%s\t%s\n' "$aid" "$email"
        fi
      done
      return
    fi
  fi
  # Fallback: parse `agent-auth list` directly.
  if ! command -v agent-auth >/dev/null 2>&1; then
    return
  fi
  # NB: cannot use `agent-auth list | python3 - <<'PY' ...` — the `-` tells
  # python to read its script from stdin, which collides with the pipe.
  # Capture agent-auth output to a tempfile and pass it as a positional arg.
  local auth_tmp
  auth_tmp="$(mktemp)"
  trap "rm -f '$auth_tmp'" RETURN
  agent-auth list >"$auth_tmp" 2>/dev/null || return
  python3 - "$need" "$ACTIVE_FILE" "$SCRIPT_DIR/accounts.yml" "$auth_tmp" <<'PY'
import re, sys, os
need = int(sys.argv[1])
active = set()
if os.path.exists(sys.argv[2]):
    with open(sys.argv[2]) as fh:
        active = {ln.strip() for ln in fh if ln.strip()}
acc = {}
if os.path.exists(sys.argv[3]):
    with open(sys.argv[3]) as fh:
        txt = fh.read()
    for m in re.finditer(r'-\s*id:\s*(\S+)\s*\n\s*email:\s*(\S+)', txt):
        acc[m.group(2)] = m.group(1)
out = []
with open(sys.argv[4]) as fh:
    for line in fh:
        em = re.search(r'(\S+@\S+\.\S+)', line)
        h5 = re.search(r'5h=(\d+)%', line)
        wk = re.search(r'weekly=(\d+)%', line)
        if not (em and h5 and wk):
            continue
        email = em.group(1)
        if int(h5.group(1)) >= 100 or int(wk.group(1)) >= 90:
            continue
        aid = acc.get(email)
        if not aid or aid in active:
            continue
        out.append(f"{aid}\t{email}")
        if len(out) >= need:
            break
for row in out:
    print(row)
PY
}

mapfile -t PICKED < <(pick_accounts "$N")
PICKED_N="${#PICKED[@]}"
if [ "$PICKED_N" -lt "$N" ]; then
  warn "only $PICKED_N healthy unallocated accounts available (wanted $N)"
fi
[ "$PICKED_N" -gt 0 ] || die "no healthy accounts available — check agent-auth list / accounts.yml"

# Render worker prompt with plan slug pre-pinned so each new pane goes
# straight to the right plan instead of re-deriving via openspec scan.
WAKE_DIR="$WORK_ROOT/wake-prompts"
mkdir -p "$WAKE_DIR"
PROMPT_FILE="$WAKE_DIR/add-workers-$PLAN_SLUG-$(date -u +%Y%m%dT%H%M%S).md"
{
  printf '# Added worker — drain ready sub-tasks\n\n'
  printf 'You are an ADDITIONAL codex worker spawned mid-run by `add-workers.sh`.\n'
  printf 'Target plan: **%s**\n\n' "$PLAN_SLUG"
  printf 'Pass `plan_slug=%s` on every `task_ready_for_agent` call so you pick\n' "$PLAN_SLUG"
  printf 'up subs from this plan and not a stale one.\n\n'
  # `--` so bash's printf builtin stops parsing options before the `---` rule.
  printf -- '---\n\n'
  cat "$PROMPT_TEMPLATE"
} > "$PROMPT_FILE"

log "wrote prompt: $PROMPT_FILE"

if [ -n "$FLEET_ID" ]; then
  TMUX_SESSION="${CODEX_FLEET_SESSION:-codex-fleet-$FLEET_ID}"
else
  TMUX_SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
fi
TMUX_WINDOW="${CODEX_FLEET_WINDOW:-overview}"

# In tmux mode, find dead/idle panes we can respawn instead of cluttering
# the layout. A pane is "dead" if its last 80 lines match one of:
#   - "hit your usage limit"            (codex CLI billing cap)
#   - "Please run 'codex login'"        (auth expired)
#   - "session has ended"               (kitty closed underneath)
#   - "[exited]" / "[Process completed]"
#   - "No claimable task" + ≥3 minutes of identical content (idle waiter)
# Pane scan + respawn happens in spawn_worker() per call.
find_dead_pane() {
  [ "$TARGET" = "tmux" ] || return 1
  tmux has-session -t "$TMUX_SESSION" 2>/dev/null || return 1
  tmux list-panes -t "$TMUX_SESSION:$TMUX_WINDOW" -F '#{pane_id}' 2>/dev/null \
    | while IFS= read -r pid; do
        local tail
        tail="$(tmux capture-pane -p -t "$pid" -S -80 2>/dev/null || true)"
        case "$tail" in
          *"hit your usage limit"*|*"Please run \`codex login\`"*|*"Please run 'codex login'"*|\
          *"[Process completed]"*|*"[Process exited"*|*"[exited]"*|*"session has ended"*)
            printf '%s\n' "$pid"; return 0 ;;
        esac
      done | head -1
}

# Mark which pane IDs we've already respawned in this run so a single dead
# pane doesn't get respawned twice across iterations.
RESPAWNED_PIDS=""

spawn_worker() {
  local aid="$1" email="$2"
  local home="$WORK_ROOT/$aid"
  # CODEX_GUARD_BYPASS=1 is required when N panes spawn in the same second:
  # without it, every codex child computes the same `agent/codex/codex-task-<ts>`
  # branch slug and N-1 die racing for the same git ref. Even single-spawn
  # call sites benefit, so it's set unconditionally here.
  local pane_cmd="env CODEX_GUARD_BYPASS=1 CODEX_HOME='$home' CODEX_FLEET_AGENT_NAME='codex-$aid' CODEX_FLEET_ACCOUNT_EMAIL='$email' codex --dangerously-bypass-approvals-and-sandbox \"\$(cat '$PROMPT_FILE')\""
  if [ "$DRY_RUN" = "1" ]; then
    log "[dry-run] would spawn: aid=$aid email=$email target=$TARGET home=$home"
    return 0
  fi

  if [ "$TARGET" = "tmux" ] && tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    local pid=""
    # Prefer respawning a dead pane in the overview window.
    while IFS= read -r candidate; do
      case " $RESPAWNED_PIDS " in
        *" $candidate "*) continue ;;
      esac
      pid="$candidate"
      break
    done < <(find_dead_pane)
    if [ -n "$pid" ]; then
      tmux set-option -p -t "$pid" '@panel' "[codex-$aid]" >/dev/null 2>&1 || true
      tmux respawn-pane -k -t "$pid" "$pane_cmd" >/dev/null
      RESPAWNED_PIDS="$RESPAWNED_PIDS $pid"
      printf '%s\n' "$aid" >>"$ACTIVE_FILE"
      log "respawned dead pane $pid → codex-$aid ($email)"
      return 0
    fi
    # No dead pane — split the smallest pane in the window so the new
    # pane lands inside overview. Operator can `prefix space` to re-tile.
    local new_pid
    new_pid="$(tmux split-window -t "$TMUX_SESSION:$TMUX_WINDOW" -P -F '#{pane_id}' "$pane_cmd" 2>/dev/null || true)"
    if [ -n "$new_pid" ]; then
      tmux set-option -p -t "$new_pid" '@panel' "[codex-$aid]" >/dev/null 2>&1 || true
      tmux select-layout -t "$TMUX_SESSION:$TMUX_WINDOW" tiled >/dev/null 2>&1 || true
      printf '%s\n' "$aid" >>"$ACTIVE_FILE"
      log "split-window $new_pid → codex-$aid ($email)"
      return 0
    fi
    warn "tmux split failed for $aid; falling through to kitty"
  fi

  # Fallback: kitty window.
  if ! command -v kitty >/dev/null 2>&1; then
    warn "no tmux session '$TMUX_SESSION' and kitty not on PATH — cannot spawn $aid"
    return 1
  fi
  setsid kitty \
    --title "add-worker $aid" \
    --override window_padding_width=4 \
    bash -lc "$pane_cmd" \
    </dev/null >/dev/null 2>&1 &
  disown
  printf '%s\n' "$aid" >>"$ACTIVE_FILE"
  log "spawned kitty window → codex-$aid ($email)"
}

i=0
for row in "${PICKED[@]}"; do
  IFS=$'\t' read -r aid email <<<"$row"
  [ -n "$aid" ] && [ -n "$email" ] || continue
  spawn_worker "$aid" "$email"
  i=$((i+1))
done

log "done. spawned=$i requested=$N plan=$PLAN_SLUG"
