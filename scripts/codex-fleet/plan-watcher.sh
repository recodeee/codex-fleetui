#!/usr/bin/env bash
# plan-watcher.sh — auto-prompt codex-fleet workers when Colony plans have
# unclaimed work and one or more worker panes are idle. Closes the gap
# between `colony plan publish ...` and workers actually picking up
# sub-tasks (previously required a manual tmux send-keys per pane).
#
# Architecture:
#   1. Every INTERVAL seconds, list Colony plans via `colony plan status`
#      and keep the ones with available > 0.
#   2. For each idle worker pane (detected by recent stdout — "no claimable
#      work" / "polls returned" / "stale rescue" — or no edits in N minutes),
#      pair it with one of the available plans. The pairing prefers plans
#      that don't yet have a claimed sub-task so we spread workers across
#      plans rather than piling onto one.
#   3. Send an OVERRIDE prompt that pins the worker to the next available
#      sub-task of its assigned plan. Idempotent: per-(pane, plan) cooldown
#      stored in $STATE_DIR/plan-watcher-state.tsv prevents re-prompting
#      the same worker on the same plan within COOLDOWN_SECONDS.
#
# What "idle" means here:
#   - The pane's tail (last ~20 captured lines) matches one of the
#     IDLE_PATTERNS (worker reports it has no claimable work).
#   - OR the pane has not produced new output in IDLE_AFTER_SECONDS
#     (default 120s) — a worker silently polling counts as idle.
#
# What this script does NOT do:
#   - It does not handle codex CLI auth caps — that's cap-swap-daemon.sh.
#   - It does not respawn dead panes — that's add-workers.sh + supervisor.sh.
#   - It does not decompose sequential plans into parallel sub-tasks; it
#     only fans workers across the work already available.
#
# Usage:
#   bash plan-watcher.sh --loop --interval=30
#   bash plan-watcher.sh --once                                       # one tick + exit
#   bash plan-watcher.sh --once --dry-run                              # print decisions, no send
#   CODEX_FLEET_SESSION=codex-fleet-2 bash plan-watcher.sh --loop      # second fleet

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WINDOW="${CODEX_FLEET_OVERVIEW_WINDOW:-overview}"
FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
STATE_DIR="${PLAN_WATCHER_STATE_DIR:-$FLEET_STATE_DIR/plan-watcher}"
STATE_FILE="$STATE_DIR/state.tsv"
INTERVAL="${PLAN_WATCHER_INTERVAL:-30}"
COOLDOWN_SECONDS="${PLAN_WATCHER_COOLDOWN:-600}"   # 10 min — long enough for a sub-task to start
IDLE_AFTER_SECONDS="${PLAN_WATCHER_IDLE_AFTER:-120}"
DRY_RUN=0
ONCE=0

mkdir -p "$STATE_DIR"
touch "$STATE_FILE"

while [ $# -gt 0 ]; do
  case "$1" in
    --loop) ONCE=0; shift ;;
    --once) ONCE=1; shift ;;
    --interval=*) INTERVAL="${1#*=}"; shift ;;
    --interval) INTERVAL="$2"; shift 2 ;;
    --cooldown=*) COOLDOWN_SECONDS="${1#*=}"; shift ;;
    --idle-after=*) IDLE_AFTER_SECONDS="${1#*=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) sed -n '1,42p' "$0"; exit 0 ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

log() { printf '\033[36m[plan-watcher]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[plan-watcher]\033[0m %s\n' "$*" >&2; }
dryrun() { (( DRY_RUN == 1 )) && printf '\033[35m[plan-watcher][dry-run]\033[0m %s\n' "$*" || true; }

# Telltale phrases the worker prints when task_ready_for_agent has no work
# for its current pin. These are the lines we see in the overview screenshot
# when a plan is exhausted or stuck behind a stale blocker.
IDLE_PATTERNS=(
  "No claimable subtask"
  "no claimable work"
  "polls returned no claimable work"
  "task_ready_for_agent reports the queue is blocked"
  "task_ready_for_agent has no claimable work"
  "task_ready_for_agent keeps returning a stale blocker"
  "repeated 60s polls"
)

# ── colony plan status parser ───────────────────────────────────────────────
# `colony plan status` prints one plan per stanza:
#   <slug>  <title>
#     tasks: A completed, B claimed, C available, D blocked
#     path:  …
# We emit TSV: slug \t completed \t claimed \t available \t blocked.
# Parsed in bash + sed because mawk (the default on Debian/Ubuntu) doesn't
# support gawk's array-capturing `match()` form.
list_plans_with_available_work() {
  local slug=""
  colony plan status 2>/dev/null | while IFS= read -r line; do
    case "$line" in
      "  tasks:"*)
        local completed claimed available blocked
        completed="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) completed.*/\1/p')"
        claimed="$(printf '%s' "$line"   | sed -nE 's/.*[[:space:]]([0-9]+) claimed.*/\1/p')"
        available="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) available.*/\1/p')"
        blocked="$(printf '%s' "$line"   | sed -nE 's/.*[[:space:]]([0-9]+) blocked.*/\1/p')"
        if [ -n "$slug" ] && [ -n "$available" ] && [ "$available" -gt 0 ] 2>/dev/null; then
          printf '%s\t%s\t%s\t%s\t%s\n' "$slug" "${completed:-0}" "${claimed:-0}" "$available" "${blocked:-0}"
        fi
        slug=""
        ;;
      "  path:"*) ;;   # noise
      "")        ;;   # noise
      *)
        # The slug stanza header starts in column 0 and the slug is the
        # first whitespace-separated token. Everything that doesn't match
        # the indented `  tasks:` / `  path:` rows is treated as a header.
        case "$line" in
          [a-z0-9-]*) slug="${line%% *}" ;;
          *) ;;
        esac
        ;;
    esac
  done
}

# ── idle-worker detection ───────────────────────────────────────────────────
# Returns one pane_id per line for panes that look idle. Skips:
#   - the fleet-tab-strip header pane (panel == [codex-fleet-tab-strip])
#   - panes without an @panel option (uninitialised / shell)
#   - panes whose tail does not match any IDLE_PATTERN
list_idle_workers() {
  tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{@panel}' 2>/dev/null \
    | while IFS='|' read -r pid panel; do
        [ -z "$panel" ] && continue
        [ "$panel" = "[codex-fleet-tab-strip]" ] && continue
        # Capture the last ~25 lines of the pane and grep for idle markers.
        # `tail -25` keeps the check cheap — workers print the same idle
        # status repeatedly so the signal stays in the recent window.
        tail="$(tmux capture-pane -p -t "$pid" -S -25 2>/dev/null || true)"
        for pat in "${IDLE_PATTERNS[@]}"; do
          if printf '%s\n' "$tail" | grep -qF "$pat"; then
            printf '%s|%s\n' "$pid" "$panel"
            break
          fi
        done
      done
}

# ── per-(pane, plan) cooldown ───────────────────────────────────────────────
# State file is plain TSV: pane_id \t plan_slug \t unix_ts_last_prompted.
# Avoids re-prompting the same pane on the same plan within
# COOLDOWN_SECONDS — when a sub-task starts, the worker takes a few
# minutes to spawn a worktree + build + edit + PR, and we don't want to
# stomp on that with a fresh OVERRIDE every 30s.
last_prompted() {
  local pid="$1" slug="$2"
  awk -F'\t' -v p="$pid" -v s="$slug" '$1==p && $2==s {print $3; exit}' "$STATE_FILE"
}

record_prompt() {
  local pid="$1" slug="$2" ts
  ts="$(date +%s)"
  # Strip any prior row for this (pane, plan) pair, then append the new one.
  local tmp; tmp="$(mktemp)"
  awk -F'\t' -v p="$pid" -v s="$slug" '!($1==p && $2==s)' "$STATE_FILE" > "$tmp"
  printf '%s\t%s\t%s\n' "$pid" "$slug" "$ts" >> "$tmp"
  mv "$tmp" "$STATE_FILE"
}

within_cooldown() {
  local pid="$1" slug="$2" last_ts now
  last_ts="$(last_prompted "$pid" "$slug")"
  [ -z "$last_ts" ] && return 1     # never prompted → not cooling down
  now="$(date +%s)"
  (( now - last_ts < COOLDOWN_SECONDS ))
}

# ── pick next available sub-task index for a plan ────────────────────────────
# `colony plan status <slug>` (singular) emits a per-task table; we grep the
# first row marked `available` and return its index + title.
next_available_subtask() {
  local slug="$1"
  # `|| true` absorbs the non-zero grep emits when the per-slug status
  # output doesn't include the per-task rows the older Colony CLI did —
  # the caller's prompt builder handles the empty-result case.
  colony plan status "$slug" 2>/dev/null \
    | grep -E '^[[:space:]]*[0-9]+\.?[[:space:]]+available' \
    | head -1 || true
}

# ── build OVERRIDE prompt ───────────────────────────────────────────────────
# Mirrors the manual prompts we'd been hand-typing into the panes. Pinning
# is explicit — the worker is expected to call task_plan_claim_subtask with
# (slug, sub_idx) instead of polling task_ready_for_agent against its
# pre-existing pin.
build_prompt() {
  local slug="$1" sub_idx="$2" sub_title="$3"
  cat <<EOF
OVERRIDE current plan pinning. Claim sub-task ${sub_idx} of plan ${slug} via Colony task_plan_claim_subtask (force the agent slug to your CODEX_FLEET_AGENT_NAME). Title: ${sub_title}. Implement on a fresh agent worktree per AGENTS.md, run the narrowest verification, open + merge a PR via \`gx branch finish --branch <your-branch> --via-pr --wait-for-merge --cleanup\`, post a Colony task_post note on this sub-task with evidence (branch, PR URL, MERGED state), then mark the sub-task completed. If this plan's queue is exhausted, fall back to task_ready_for_agent against any other available plan rather than rescuing stale claims.
EOF
}

# ── send a prompt to a pane via the canonical fleet pattern ─────────────────
send_prompt() {
  local pane="$1" prompt="$2"
  if (( DRY_RUN == 1 )); then
    dryrun "send-keys → $pane: $(printf '%s' "$prompt" | head -c 120)…"
    return 0
  fi
  # load-buffer + paste-buffer survives the multi-line OVERRIDE text cleaner
  # than `send-keys -l` which has historically tripped on tmux's "not in a
  # mode" handling around embedded newlines.
  local buf="plan-watcher-$(date +%s)-$$"
  printf '%s' "$prompt" | tmux load-buffer -b "$buf" -
  tmux paste-buffer -b "$buf" -t "$pane" -p
  tmux send-keys -t "$pane" Enter
  tmux delete-buffer -b "$buf" 2>/dev/null || true
}

# ── one iteration ───────────────────────────────────────────────────────────
tick() {
  local now; now="$(date +%s)"
  log "tick @ $(date +%H:%M:%S) — session=$SESSION"

  # Snapshot live state in two reads, then iterate in-memory.
  local plans idle_workers
  plans="$(list_plans_with_available_work)"
  idle_workers="$(list_idle_workers)"

  if [ -z "$plans" ]; then
    log "no plans with available work; sleeping"
    return 0
  fi
  if [ -z "$idle_workers" ]; then
    log "no idle workers; sleeping"
    return 0
  fi

  # Convert to arrays.
  local -a plan_lines worker_lines
  mapfile -t plan_lines <<<"$plans"
  mapfile -t worker_lines <<<"$idle_workers"

  log "candidates: ${#plan_lines[@]} plan(s) with available work, ${#worker_lines[@]} idle worker(s)"

  # Round-robin: walk workers, assign each to the plan with the most
  # available + the fewest already-claimed slots (best fanning candidate).
  # Cooldown skips already-prompted (pane, plan) pairs.
  local plan_idx=0
  for w in "${worker_lines[@]}"; do
    local pid panel
    pid="${w%%|*}"
    panel="${w#*|}"

    local assigned=""
    local tries=0
    while (( tries < ${#plan_lines[@]} )); do
      local plan_line="${plan_lines[$plan_idx]}"
      local slug="${plan_line%%	*}"
      if ! within_cooldown "$pid" "$slug"; then
        assigned="$plan_line"
        break
      fi
      plan_idx=$(( (plan_idx + 1) % ${#plan_lines[@]} ))
      tries=$((tries + 1))
    done

    if [ -z "$assigned" ]; then
      log "skip $panel ($pid): all available plans cooling down for this pane"
      continue
    fi

    local slug; slug="${assigned%%	*}"
    local sub_row; sub_row="$(next_available_subtask "$slug")"
    local sub_idx sub_title
    if [ -n "$sub_row" ]; then
      sub_idx="$(printf '%s' "$sub_row" | awk '{print $1}' | tr -d '.')"
      sub_title="$(printf '%s' "$sub_row" | sed -E 's/^[[:space:]]*[0-9]+\.?[[:space:]]+available[[:space:]]+//')"
    else
      # Older colony versions print sub-tasks differently — fall back to
      # "sub-task 0 of <slug>" which the worker prompt is permissive about.
      sub_idx="0"
      sub_title="(see plan ${slug})"
    fi
    [ -z "$sub_title" ] && sub_title="(see plan ${slug})"

    log "→ prompt $panel ($pid) with $slug / sub-${sub_idx}: ${sub_title:0:60}"
    send_prompt "$pid" "$(build_prompt "$slug" "$sub_idx" "$sub_title")"
    record_prompt "$pid" "$slug"

    # Move to next plan so back-to-back workers spread across plans.
    plan_idx=$(( (plan_idx + 1) % ${#plan_lines[@]} ))
  done
}

# ── main ────────────────────────────────────────────────────────────────────
if (( ONCE == 1 )); then
  tick
  exit 0
fi

log "starting loop (interval=${INTERVAL}s, cooldown=${COOLDOWN_SECONDS}s, session=$SESSION)"
while :; do
  tick || warn "tick failed; continuing"
  sleep "$INTERVAL"
done
