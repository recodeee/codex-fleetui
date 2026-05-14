#!/usr/bin/env bash
#
# add-workers — spawn N additional codex panes into an existing fleet to
# pick up ready sub-tasks. Use when the plan board has `available` subs but
# the existing panes are dead, capped, or stuck idle.
#
# Picks N healthy accounts (cap-probe if present, else codex-auth list with
# the canonical 5h<100% / weekly<90% / not-already-active filter), spawns
# each as a new kitty window running `codex` with the standard worker prompt
# pre-pinned to the target plan. Records the new accounts in the active
# accounts file so cap-swap-daemon and supervisor don't re-spawn them.
#
# Usage:
#   bash scripts/codex-fleet/add-workers.sh 4
#   bash scripts/codex-fleet/add-workers.sh 4 --plan-slug rust-ph13-14-15-completion-2026-05-13
#   bash scripts/codex-fleet/add-workers.sh 4 --fleet-id 2
#   bash scripts/codex-fleet/add-workers.sh --auto         # spawn as many as there are ready subs
#   bash scripts/codex-fleet/add-workers.sh 2 --dry-run    # show plan + picked accounts, don't spawn

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

N=""
PLAN_SLUG=""
FLEET_ID="${FLEET_ID:-}"
AUTO=0
DRY_RUN=0
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
PROMPT_TEMPLATE="${CODEX_FLEET_WORKER_PROMPT:-$SCRIPT_DIR/worker-prompt.md}"

while [ $# -gt 0 ]; do
  case "$1" in
    --plan-slug) PLAN_SLUG="$2"; shift 2 ;;
    --fleet-id) FLEET_ID="$2"; shift 2 ;;
    --auto) AUTO=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
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
# Prefers cap-probe.sh when present (it caches 5h-cap reset times); falls
# back to parsing `codex-auth list` directly with the standard filter.
pick_accounts() {
  local need="$1"
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
  # Fallback: parse `codex-auth list` directly.
  if ! command -v codex-auth >/dev/null 2>&1; then
    return
  fi
  codex-auth list 2>/dev/null | python3 - "$need" "$ACTIVE_FILE" "$SCRIPT_DIR/accounts.yml" <<'PY'
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
for line in sys.stdin:
    em = re.search(r'(\S+@\S+)', line)
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
[ "$PICKED_N" -gt 0 ] || die "no healthy accounts available — check codex-auth list / accounts.yml"

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
  printf '---\n\n'
  cat "$PROMPT_TEMPLATE"
} > "$PROMPT_FILE"

log "wrote prompt: $PROMPT_FILE"

# Spawn each worker as a kitty window — same pattern supervisor.sh uses,
# avoids squeezing the existing tmux panes in the codex-fleet session.
spawn_worker() {
  local aid="$1" email="$2"
  local home="$WORK_ROOT/$aid"
  if [ "$DRY_RUN" = "1" ]; then
    log "[dry-run] would spawn: aid=$aid email=$email home=$home"
    return 0
  fi
  if ! command -v kitty >/dev/null 2>&1; then
    warn "kitty not on PATH — cannot spawn windowed worker for $aid"
    return 1
  fi
  setsid kitty \
    --title "add-worker $aid" \
    --override window_padding_width=4 \
    bash -lc "env CODEX_HOME='$home' CODEX_FLEET_AGENT_NAME='codex-$aid' CODEX_FLEET_ACCOUNT_EMAIL='$email' codex \"\$(cat '$PROMPT_FILE')\"" \
    </dev/null >/dev/null 2>&1 &
  disown
  printf '%s\n' "$aid" >>"$ACTIVE_FILE"
  log "spawned: codex-$aid ($email)"
}

i=0
for row in "${PICKED[@]}"; do
  IFS=$'\t' read -r aid email <<<"$row"
  [ -n "$aid" ] && [ -n "$email" ] || continue
  spawn_worker "$aid" "$email"
  i=$((i+1))
done

log "done. spawned=$i requested=$N plan=$PLAN_SLUG"
