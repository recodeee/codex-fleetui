#!/usr/bin/env bash
# cap-swap-daemon (a.k.a. "fleet watcher") — the global supervisor for the
# codex fleet. Every INTERVAL seconds:
#   1. Scans every pane in $SESSION:overview.
#   2. If a pane's scrollback contains the cap banner, picks a fresh
#      account via cap-probe.sh (LIVE codex exec probe, not codex-auth
#      meter) and respawns the pane with that account.
#   3. Writes a human-readable status snapshot to STATUS_FILE so the
#      operator can watch what the watcher is doing.
#
# Idempotent + cooldown-protected: a pane that was just swapped won't be
# re-swapped for COOLDOWN seconds. Capped accounts are NEVER picked as
# replacements because cap-probe filters them.
set -eo pipefail

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

# Rank candidates by codex-auth score, exclude those currently in fleet
rank_candidates() {
  local in_use; in_use=$(current_emails | tr '\n' '|'); in_use="${in_use%|}"
  codex-auth list 2>/dev/null \
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

swap_pane() {
  local pid="$1" email="$2"
  local id; id=$(email_to_id "$email")
  stage_home "$id" "$email"
  tmux set-option -p -t "$pid" '@panel' "[codex-$id]"
  tmux respawn-pane -k -t "$pid" \
    "env CODEX_GUARD_BYPASS=1 CODEX_HOME=/tmp/codex-fleet/$id CODEX_FLEET_AGENT_NAME=codex-$id CODEX_FLEET_ACCOUNT_EMAIL=$email codex \"\$(cat $WAKE)\""
  mark_swapped "$pid"
  log "SWAPPED $pid → codex-$id ($email)"
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
pane_needs_swap() {
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
