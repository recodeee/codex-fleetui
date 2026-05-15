#!/usr/bin/env bash
# cap-swap-daemon (a.k.a. "fleet watcher") — the global supervisor for the
# codex fleet. Every INTERVAL seconds:
#   1. Scans every pane in $SESSION:overview.
#   2. If a pane's scrollback contains the cap banner, picks a fresh
#      account via cap-probe.sh (LIVE codex exec probe, not agent-auth
#      meter) and respawns the pane with that account.
#   3. Writes a human-readable status snapshot to STATUS_FILE so the
#      operator can watch what the watcher is doing.
#
# Idempotent + cooldown-protected: a pane that was just swapped won't be
# re-swapped for COOLDOWN seconds. Capped accounts are NEVER picked as
# replacements because cap-probe filters them.
#
# =============================================================================
# CONTRACT: Cap-swap hand-off (codex pane -> fallback worker)
# =============================================================================
# When a codex pane hits a 429 / approval-quota cap and we hand it off to a
# fallback worker (Kiro / Claude / a fresh codex account), the following
# fields MUST transfer to the replacement so the new worker resumes the SAME
# Colony task in the SAME agent worktree without orphaning any state:
#
#   1. CODEX_FLEET_TASK_ID        - Colony task_id the capped pane owned.
#                                   The new worker inherits this in env so it
#                                   re-claims rather than picks fresh work.
#   2. CODEX_FLEET_AGENT_BRANCH   - agent/* branch the capped pane was on.
#                                   The new worker MUST cd into the matching
#                                   worktree before running anything.
#   3. CODEX_FLEET_AGENT_WORKTREE - absolute filesystem path of the agent/*
#                                   worktree. Preserved verbatim; the daemon
#                                   never deletes/prunes it on swap.
#   4. CODEX_HOME                 - staged auth dir under /tmp/codex-fleet/<id>
#                                   for the NEW account (capped account is
#                                   abandoned, not its CODEX_HOME).
#   5. CODEX_FLEET_ACCOUNT_EMAIL  - email of the fresh, healthy account picked
#                                   by cap-probe.sh.
#   6. CODEX_FLEET_LAST_CLAIM_TS  - unix epoch of the previous worker's last
#                                   Colony claim heartbeat. Lets the new
#                                   worker decide whether to re-claim
#                                   immediately or wait for natural expiry.
#   7. CODEX_FLEET_LAST_TASK_NOTE - last task_post note the capped pane wrote
#                                   (one line), so the new worker has the
#                                   prior agent's handoff context inline.
#
# Hand-off invariants (must hold across every swap):
#   * Colony claim is NOT released by the daemon. The fallback worker
#     re-claims by inheriting CODEX_FLEET_TASK_ID. If the daemon were to
#     release first, another agent could steal the task between release and
#     re-claim, stranding the agent/* worktree.
#   * Agent worktree is NEVER pruned on swap. The daemon does no
#     `git worktree remove`, no `rm -rf`. Only `gx branch finish --cleanup`
#     prunes worktrees, and only when a task is genuinely complete.
#   * Before issuing the swap, the daemon posts a Colony note marker
#     ("swapping due to 429") via task_post / task_note_working so the
#     timeline shows the handoff explicitly.
#   * If any required field above is missing/unresolvable on the fallback
#     side, the daemon LOGS the missing field and SKIPS the swap rather
#     than fire a broken worker into an inconsistent state.
#
# =============================================================================
# SMOKE TEST (manual, commented - do not auto-run in the daemon loop):
# =============================================================================
#   # Simulate a 429 against a fixture pane and assert hand-off invariants.
#   #
#   # 1. Spin up a fixture pane with a known task_id + agent branch:
#   #      export CODEX_FLEET_TASK_ID=fixture-task-1
#   #      export CODEX_FLEET_AGENT_BRANCH=agent/claude/fixture-2026-05-15
#   #      export CODEX_FLEET_AGENT_WORKTREE=/tmp/codex-fleet-fixture-wt
#   #      tmux new-window -t codex-fleet:overview -n fixture
#   #
#   # 2. Force the daemon to treat the fixture pane as capped:
#   #      export CODEX_FLEET_SIMULATE_429=1
#   #      export CODEX_FLEET_SIMULATE_429_PANE=%<id>
#   #
#   # 3. Run exactly one sweep (no loop):
#   #      INTERVAL=999 timeout 60 bash scripts/codex-fleet/cap-swap-daemon.sh \
#   #        || true
#   #
#   # 4. Assert hand-off invariants:
#   #      # claim was NOT released
#   #      grep -q "claim_preserved=1" /tmp/claude-viz/cap-swap.log
#   #      # new worker started in same worktree
#   #      grep -q "worktree=/tmp/codex-fleet-fixture-wt" /tmp/claude-viz/cap-swap.log
#   #      # task_id was inherited into respawn env
#   #      grep -q "inherited_task_id=fixture-task-1" /tmp/claude-viz/cap-swap.log
#   #      # swap-due-to-429 marker was posted
#   #      grep -q "marker=swapping_due_to_429" /tmp/claude-viz/cap-swap.log
# =============================================================================
set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Autodetect REPO from the clone location; env override wins.
REPO="${REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}}"
SESSION="${SESSION:-codex-fleet}"
WAKE="${WAKE:-/tmp/codex-fleet-wake-prompt.md}"
LOG="${LOG:-/tmp/claude-viz/cap-swap.log}"
STATUS_FILE="${STATUS_FILE:-/tmp/claude-viz/cap-swap-status.txt}"
STATE="${STATE:-/tmp/claude-viz/cap-swap-state}"
INTERVAL="${INTERVAL:-30}"
COOLDOWN="${COOLDOWN:-180}"
MIN_5H="${MIN_5H:-40}"
MIN_WK="${MIN_WK:-25}"
CANDIDATES_PER_SWAP="${CANDIDATES_PER_SWAP:-6}"
mkdir -p "$(dirname "$LOG")" "$(dirname "$STATUS_FILE")" "$STATE"
ts() { date +%H:%M:%S; }
log() { echo "[$(ts)] $*" | tee -a "$LOG"; }

email_to_id() {
  python3 -c "
import sys
e=sys.argv[1]; part,dom=e.split('@',1); dom=dom.split('.',1)[0]
m={'magnoliavilag':'magnolia','gitguardex':'gg','pipacsclub':'pipacs'}
print(f'{part}-{m.get(dom,dom)}')
" "$1"
}

# Emails currently assigned to fleet panes (from /proc/<pid>/environ of each pane's codex)
current_emails() {
  local pane_pids tty pid env_file
  while IFS= read -r pane; do
    tty=$(tmux display-message -p -t "$pane" '#{pane_tty}' 2>/dev/null)
    [ -z "$tty" ] && continue
    # find the codex (node) child whose controlling tty is this one
    for pid in $(pgrep -f "^.*codex\b" 2>/dev/null); do
      [ -r "/proc/$pid/environ" ] 2>/dev/null || continue
      env_file="/proc/$pid/environ"
      [ -r "$env_file" ] || continue
      local proc_tty="/dev/$(awk -F'(' '/^[(]/{print $2}' "/proc/$pid/status" 2>/dev/null | head -1)"
      # quick check: just grep environ for CODEX_FLEET_ACCOUNT_EMAIL and the tty name
      local pane_env_email
      pane_env_email=$({ tr '\0' '\n' < "$env_file" 2>/dev/null || true; } | awk -F= '/^CODEX_FLEET_ACCOUNT_EMAIL=/{print $2}' 2>/dev/null)
      [ -n "$pane_env_email" ] || continue
      # Match via cgroup-tty: cross-check the pid is under the tmux server (heuristic — keep emails seen)
      echo "$pane_env_email"
    done
  done < <(tmux list-panes -t "$SESSION:overview" -F '#{pane_id}' 2>/dev/null) \
    | sort -u
}

# Rank candidates by agent-auth score, exclude those currently in fleet
rank_candidates() {
  local in_use; in_use=$(current_emails | tr '\n' '|'); in_use="${in_use%|}"
  agent-auth list 2>/dev/null \
    | MIN5="$MIN_5H" MINW="$MIN_WK" INUSE="$in_use" python3 -c '
import os, sys, re
min5=int(os.environ["MIN5"]); minw=int(os.environ["MINW"])
in_use=set(filter(None, os.environ["INUSE"].split("|")))
rows=[]
for line in sys.stdin:
    em=re.search(r"([\w.+-]+@[\w.-]+\.[a-z]+)", line)
    if not em: continue
    email=em.group(1)
    if email in in_use: continue
    h5m=re.search(r"5h=(\d+)%", line); wkm=re.search(r"weekly=(\d+)%", line)
    if not h5m or not wkm: continue
    h,w=int(h5m.group(1)),int(wkm.group(1))
    if h<min5 or w<minw: continue
    rows.append((h*w, email))
rows.sort(reverse=True)
for _, email in rows: print(email)
'
}

in_cooldown() {
  local f="$STATE/${1#%}.last"; [ -f "$f" ] || return 1
  local last=$(cat "$f") now=$(date +%s)
  [ $((now - last)) -lt "$COOLDOWN" ]
}
mark_swapped() { echo "$(date +%s)" > "$STATE/${1#%}.last"; }

stage_home() {
  local id="$1" email="$2" d="/tmp/codex-fleet/$id"
  mkdir -p "$d"
  cp "$HOME/.codex/accounts/$email.json" "$d/auth.json"
  chmod 600 "$d/auth.json"
  [ -e "$d/config.toml" ] || ln -s "$HOME/.codex/config.toml" "$d/config.toml"
}

# pane_env_value — pull a single env var from the codex (node) process
# attached to the given pane. Walks pgrep candidates and matches by the
# pane's controlling tty. Empty string on miss.
pane_env_value() {
  local pane="$1" key="$2" tty pid env_file proc_tty val
  tty=$(tmux display-message -p -t "$pane" '#{pane_tty}' 2>/dev/null) || true
  [ -n "$tty" ] || return 0
  for pid in $(pgrep -f "^.*codex\b" 2>/dev/null); do
    env_file="/proc/$pid/environ"
    [ -r "$env_file" ] || continue
    proc_tty="/dev/$(awk -F'(' '/^[(]/{print $2}' "/proc/$pid/status" 2>/dev/null | head -1)"
    [ "$proc_tty" = "$tty" ] || continue
    val=$({ tr '\0' '\n' < "$env_file" 2>/dev/null || true; } \
      | awk -F= -v k="$key" '$1==k{sub("^"k"=","");print;exit}')
    [ -n "$val" ] && { printf '%s' "$val"; return 0; }
  done
  return 0
}

# colony_post_swap_marker — best-effort note that the daemon is handing off
# the pane's Colony task due to a 429/cap. Uses the colony CLI if present;
# otherwise logs+skips (never blocks the swap). The marker exists so the
# task timeline shows the handoff explicitly and the new worker can read
# the prior context via CODEX_FLEET_LAST_TASK_NOTE.
colony_post_swap_marker() {
  local task_id="$1" pid="$2" old_email="$3" new_email="$4"
  local note="marker=swapping_due_to_429 pane=$pid old_account=${old_email:-unknown} new_account=$new_email ts=$(date +%s)"
  if [ -z "$task_id" ]; then
    log "swap-marker: no CODEX_FLEET_TASK_ID on pane $pid; skipping colony task_post (log-only)"
    return 0
  fi
  if command -v colony >/dev/null 2>&1; then
    colony task_post --task "$task_id" --kind note --content "$note" >/dev/null 2>&1 \
      || log "swap-marker: colony task_post failed for task=$task_id (continuing)"
  else
    log "swap-marker: colony CLI absent; recording locally task=$task_id note=\"$note\""
  fi
  log "POSTED swap-marker task=$task_id pane=$pid: $note"
}

swap_pane() {
  local pid="$1" email="$2"
  local id; id=$(email_to_id "$email")

  # --- Capture hand-off state from the OUTGOING (capped) pane ---
  # These mirror the CONTRACT block at the top of the file. Anything
  # missing here is logged and (for required fields) aborts the swap so
  # we never fire a broken fallback worker.
  local prev_task_id prev_branch prev_worktree prev_email prev_note prev_claim_ts
  prev_task_id=$(pane_env_value "$pid" CODEX_FLEET_TASK_ID || true)
  prev_branch=$(pane_env_value "$pid" CODEX_FLEET_AGENT_BRANCH || true)
  prev_worktree=$(pane_env_value "$pid" CODEX_FLEET_AGENT_WORKTREE || true)
  prev_email=$(pane_env_value "$pid" CODEX_FLEET_ACCOUNT_EMAIL || true)
  prev_note=$(pane_env_value "$pid" CODEX_FLEET_LAST_TASK_NOTE || true)
  prev_claim_ts=$(pane_env_value "$pid" CODEX_FLEET_LAST_CLAIM_TS || true)
  [ -n "$prev_claim_ts" ] || prev_claim_ts="$(date +%s)"

  # Defensive: if the pane is mid-task (has a task_id) but is missing the
  # worktree/branch fields, the fallback worker would have nowhere to cd
  # into and would orphan the agent/* worktree. Log+skip rather than
  # fire a broken worker (CONTRACT invariant #4).
  if [ -n "$prev_task_id" ]; then
    if [ -z "$prev_branch" ] || [ -z "$prev_worktree" ]; then
      log "SKIP swap pane=$pid task=$prev_task_id: missing CODEX_FLEET_AGENT_BRANCH/WORKTREE (broken-worker guard)"
      return 0
    fi
    # Worktree must exist on disk; otherwise the fallback `cd` will fail
    # and the new worker will start in $HOME, dirtying primary.
    if [ ! -d "$prev_worktree" ]; then
      log "SKIP swap pane=$pid task=$prev_task_id: worktree=$prev_worktree does not exist on disk"
      return 0
    fi
  fi

  # Post Colony hand-off marker BEFORE the swap. The marker is best-effort
  # (colony CLI may be absent in some environments) but the log line
  # always lands so the smoke-test can assert marker=swapping_due_to_429.
  colony_post_swap_marker "$prev_task_id" "$pid" "$prev_email" "$email"

  # NOTE: we do NOT release the Colony claim here. The new worker
  # re-claims by inheriting CODEX_FLEET_TASK_ID. Releasing first would
  # let another agent steal the task between release and re-claim,
  # stranding the agent/* worktree (CONTRACT invariant #1).
  log "claim_preserved=1 task=${prev_task_id:-none} pane=$pid"

  stage_home "$id" "$email"
  tmux set-option -p -t "$pid" '@panel' "[codex-$id]"

  # Build the respawn command preserving every hand-off field. Empty
  # fields are still exported (as empty) so the new worker sees them
  # explicitly rather than inheriting stale parent env.
  local respawn_cmd
  if [ -n "$prev_worktree" ]; then
    # Fallback worker cd's into the preserved agent/* worktree before
    # starting codex (CONTRACT invariant #2: worktree preserved verbatim).
    respawn_cmd="cd '$prev_worktree' && env CODEX_GUARD_BYPASS=1 \
CODEX_HOME=/tmp/codex-fleet/$id \
CODEX_FLEET_AGENT_NAME=codex-$id \
CODEX_FLEET_ACCOUNT_EMAIL=$email \
CODEX_FLEET_TASK_ID='${prev_task_id}' \
CODEX_FLEET_AGENT_BRANCH='${prev_branch}' \
CODEX_FLEET_AGENT_WORKTREE='${prev_worktree}' \
CODEX_FLEET_LAST_CLAIM_TS='${prev_claim_ts}' \
CODEX_FLEET_LAST_TASK_NOTE='${prev_note}' \
codex \"\$(cat $WAKE)\""
  else
    # No prior task context (fresh-fleet swap): preserve existing
    # respawn shape so the original path is unchanged when no hand-off
    # state needs to transfer.
    respawn_cmd="env CODEX_GUARD_BYPASS=1 CODEX_HOME=/tmp/codex-fleet/$id CODEX_FLEET_AGENT_NAME=codex-$id CODEX_FLEET_ACCOUNT_EMAIL=$email codex \"\$(cat $WAKE)\""
  fi
  tmux respawn-pane -k -t "$pid" "$respawn_cmd"
  mark_swapped "$pid"
  log "SWAPPED $pid -> codex-$id ($email) inherited_task_id=${prev_task_id:-none} worktree=${prev_worktree:-none} branch=${prev_branch:-none}"
}

pane_is_capped() {
  # Flatten newlines + collapse whitespace so terminal wrapping like
  #   ■ You've hit your
  #       usage limit:
  # still matches the canonical "You've hit your usage limit" phrase.
  # Without this, codex's cap banner — rendered at a narrow column and
  # wrapping mid-phrase — silently slips past every sweep and the daemon
  # never swaps a capped pane.
  tmux capture-pane -p -t "$1" -S -40 2>/dev/null \
    | tr '\n' ' ' | tr -s '[:space:]' ' ' \
    | grep -qE "You've hit your usage limit|Rate limit reached|Refusing.*usage|ERROR: You've hit|approval review failed: You've hit"
}

# pane_is_blocked_idle — worker emitted a BLOCKED handoff citing an
# approval / usage quota AND is currently idle (no `Working (…)`, no
# `Reviewing approval request`). The idle gate stops us interrupting a
# pane that recovered on its own and picked up new work.
pane_is_blocked_idle() {
  local tail
  tail=$(tmux capture-pane -p -t "$1" -S -120 2>/dev/null)
  [ -n "$tail" ] || return 1
  echo "$tail" | grep -qE "BLOCKED:" || return 1
  echo "$tail" | grep -qE "blocker=(local )?approval (quota|limit)|blocker=usage limit|hit usage/approval limit|approval quota hit" || return 1
  local last
  last=$(echo "$tail" | tail -15)
  if echo "$last" | grep -qE "Working \([0-9]+[ms]|Reviewing approval request|Calling [a-zA-Z_]+\.[a-zA-Z_]+|Ran [a-z_]+"; then
    return 1
  fi
  return 0
}

# pane_needs_swap — combined predicate: either codex-level cap (original
# detection) or a worker self-reported BLOCKED + approval/usage marker.
# When CODEX_FLEET_SIMULATE_429=1, the daemon also treats the pane
# referenced by CODEX_FLEET_SIMULATE_429_PANE (or every pane if unset)
# as capped, so the SMOKE TEST stanza at the top of the file can drive
# one synthetic hand-off without waiting for a real 429.
pane_needs_swap() {
  if [ "${CODEX_FLEET_SIMULATE_429:-0}" = "1" ]; then
    if [ -z "${CODEX_FLEET_SIMULATE_429_PANE:-}" ] \
       || [ "$1" = "${CODEX_FLEET_SIMULATE_429_PANE}" ]; then
      log "SIMULATE_429=1 forcing pane=$1 to be treated as capped"
      return 0
    fi
  fi
  pane_is_capped "$1" && return 0
  pane_is_blocked_idle "$1" && return 0
  return 1
}

write_status() {
  local capped_panes="$1" swap_count="$2" healthy_count="$3"
  cat > "$STATUS_FILE" <<EOF
fleet watcher · last sweep: $(ts) · interval=${INTERVAL}s · cooldown=${COOLDOWN}s

panes in session $SESSION:overview: $(tmux list-panes -t "$SESSION:overview" 2>/dev/null | wc -l)
capped this sweep: $capped_panes
swaps this sweep:  $swap_count
healthy candidates (ranked, not yet probed): $healthy_count

recent log (last 12 lines):
$(tail -12 "$LOG" 2>/dev/null)
EOF
}

sweep_once() {
  local capped=0 swaps=0 candidate_count=0
  local capped_list=()
  while read -r pid cmd; do
    [ "$cmd" = "node" ] || continue
    in_cooldown "$pid" && continue
    if pane_needs_swap "$pid"; then
      capped=$((capped + 1))
      capped_list+=("$pid")
    fi
  done < <(tmux list-panes -t "$SESSION:overview" -F '#{pane_id} #{pane_current_command}' 2>/dev/null)

  if [ "$capped" -gt 0 ]; then
    log "DETECTED $capped capped pane(s): ${capped_list[*]}"
    # Rank candidates once for this whole sweep
    local ranked; ranked=$(rank_candidates)
    candidate_count=$(printf '%s\n' "$ranked" | grep -c '@' || true)
    if [ "$candidate_count" -eq 0 ]; then
      log "no ranked candidates available (everyone in fleet or below floors)"
      write_status "$capped" "$swaps" "$candidate_count"
      return
    fi
    # Use cap-probe to filter to LIVE-healthy candidates (probes in parallel)
    local need; need=$capped
    [ "$need" -gt "$CANDIDATES_PER_SWAP" ] && need=$CANDIDATES_PER_SWAP
    local healthy
    healthy=$(bash "$SCRIPT_DIR/cap-probe.sh" "$need" $ranked 2>>"$LOG" || true)
    local healthy_n; healthy_n=$(printf '%s\n' "$healthy" | grep -c '@' || true)
    if [ "$healthy_n" -eq 0 ]; then
      log "cap-probe returned no healthy accounts; will retry next sweep"
      write_status "$capped" "$swaps" "$candidate_count"
      return
    fi
    log "cap-probe confirmed $healthy_n healthy account(s)"
    # Swap capped panes against confirmed healthy emails
    local i=0
    local emails=($healthy)
    for pid in "${capped_list[@]}"; do
      [ "$i" -ge "${#emails[@]}" ] && break
      swap_pane "$pid" "${emails[$i]}"
      swaps=$((swaps + 1))
      i=$((i + 1))
    done
  fi
  write_status "$capped" "$swaps" "$candidate_count"
}

log "fleet watcher started (session=$SESSION interval=${INTERVAL}s cooldown=${COOLDOWN}s min_5h=${MIN_5H}% min_wk=${MIN_WK}%)"
while :; do
  sweep_once 2>>"$LOG" || log "sweep error (see above)"
  sleep "$INTERVAL"
done
