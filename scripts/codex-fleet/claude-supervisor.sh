#!/usr/bin/env bash
# claude-supervisor.sh — two-way claude-code supervisor for codex-fleet
# panes. Replaces the dumb "paste the same OVERRIDE forever" loop in
# plan-watcher.sh with a read-think-respond cycle: the supervisor reads
# what the worker said (capture-pane), feeds that to `claude -p` along
# with the active plan + Colony queue state, and pastes claude's tailored
# response back into the pane.
#
# Why this exists:
#   The operator caught plan-watcher.sh blasting an idle worker with
#   "OVERRIDE: claim sub-task 0 of plan X" while the worker was already
#   posting "Blocked. PLAN_SUBTASK_NOT_FOUND, queue empty, stale-claim
#   blocker, told me not to rescue, what now?". Pure monologue — no
#   collaboration. This daemon flips that to dialog: claude reads the
#   blocker, decides whether to rescue / switch plan / wait, and pastes
#   the right prompt back.
#
# Architecture (one tick):
#   1. tmux list-panes -> worker panes (skip header + non-codex panes)
#   2. For each pane:
#        a. capture-pane -p -S -80 -> last 80 lines
#        b. classify: working | idle-polling | blocked | stale-blocked
#        c. if state >= blocked AND outside cooldown:
#             - build context prompt (pane tail + active plan + queue snapshot)
#             - claude -p --output-format text < prompt
#             - parse ACTION:/PROMPT:/TOOL: from response
#             - apply: optional MCP/cli call, then paste-buffer + Enter
#        d. record (pane, response-hash) so we don't re-ask claude with
#           the same input ("worker said the same thing, supervisor already
#           answered, paste-once + cooldown")
#
# What this does NOT do:
#   - Run `claude -p` for every idle pane every tick. That's $50/hour.
#     It only fires when (a) the worker emitted a real blocker / stale
#     pattern, AND (b) we haven't asked claude this exact question yet,
#     AND (c) the per-pane cooldown has elapsed.
#   - Replace plan-watcher.sh. plan-watcher is the fast path for "idle
#     worker, plan with available sub-tasks" (claude doesn't need to
#     think — just paste the next one). claude-supervisor is the slow
#     path for "worker hit a wall, what now". Both run in parallel.
#   - Bypass operator approvals. tmux send-keys goes through the same
#     pane-write contract as the rest of the fleet — capture first,
#     refuse `bash` panes, dedup by content hash.
#
# Env:
#   CODEX_FLEET_SESSION             (default codex-fleet)
#   CODEX_FLEET_REPO_ROOT           (default git toplevel)
#   CLAUDE_SUPERVISOR_INTERVAL      (default 60s)
#   CLAUDE_SUPERVISOR_COOLDOWN      (default 300s — per-pane min gap between claude calls)
#   CLAUDE_SUPERVISOR_MIN_BLOCKED   (default 120s — pane must be blocked this long before asking claude)
#   CLAUDE_BIN                      (default `claude`)
#   CLAUDE_MODEL_ASKING             (default `sonnet`        — fast/cheap menu picks)
#   CLAUDE_EFFORT_ASKING            (default `medium`        — picking option 3 doesn't need xhigh)
#   CLAUDE_MODEL_BLOCKED            (default `claude-opus-4-7` — real reasoning for genuinely stuck panes)
#   CLAUDE_EFFORT_BLOCKED           (default `high`)
#   CLAUDE_MODEL                    (legacy alias for CLAUDE_MODEL_BLOCKED — kept for back-compat)
#   CLAUDE_EFFORT                   (legacy alias for CLAUDE_EFFORT_BLOCKED)
#   CLAUDE_FALLBACK_MODEL           (default `sonnet` — when the primary is overloaded)
#   CLAUDE_SUPERVISOR_STATE_DIR     (default $FLEET_STATE_DIR/claude-supervisor)
#   CLAUDE_SUPERVISOR_STRIKE_WINDOW (default 3600s — sliding window for 3-strike loop guard)
#   CLAUDE_SUPERVISOR_STRIKE_LIMIT  (default 3      — escalate after this many pastes inside the window)
#
# Usage:
#   bash claude-supervisor.sh --loop                # default 60s cadence
#   bash claude-supervisor.sh --once                # one tick + exit
#   bash claude-supervisor.sh --once --dry-run      # show decisions, don't send
#   bash claude-supervisor.sh --once --pane %298    # single pane

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WINDOW="${CODEX_FLEET_OVERVIEW_WINDOW:-overview}"
FLEET_STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
STATE_DIR="${CLAUDE_SUPERVISOR_STATE_DIR:-$FLEET_STATE_DIR/claude-supervisor}"
INTERVAL="${CLAUDE_SUPERVISOR_INTERVAL:-60}"
COOLDOWN="${CLAUDE_SUPERVISOR_COOLDOWN:-300}"
MIN_BLOCKED="${CLAUDE_SUPERVISOR_MIN_BLOCKED:-120}"
CLAUDE_BIN="${CLAUDE_BIN:-claude}"
# Back-compat: CLAUDE_MODEL / CLAUDE_EFFORT used to drive every call. They now
# default the BLOCKED tier (the path that genuinely needs reasoning).
CLAUDE_MODEL_BLOCKED="${CLAUDE_MODEL_BLOCKED:-${CLAUDE_MODEL:-claude-opus-4-7}}"
CLAUDE_EFFORT_BLOCKED="${CLAUDE_EFFORT_BLOCKED:-${CLAUDE_EFFORT:-high}}"
CLAUDE_MODEL_ASKING="${CLAUDE_MODEL_ASKING:-sonnet}"
CLAUDE_EFFORT_ASKING="${CLAUDE_EFFORT_ASKING:-medium}"
CLAUDE_FALLBACK_MODEL="${CLAUDE_FALLBACK_MODEL:-sonnet}"
STRIKE_WINDOW="${CLAUDE_SUPERVISOR_STRIKE_WINDOW:-3600}"
STRIKE_LIMIT="${CLAUDE_SUPERVISOR_STRIKE_LIMIT:-3}"

# JSON Schema enforced via `claude --json-schema`. Keeps `apply_response`'s
# parser from having to guess what claude wrote — the CLI validates against
# this schema before returning, so we either get well-formed JSON or a
# non-zero exit (which falls through to the legacy text parser).
JSON_SCHEMA='{
  "type": "object",
  "additionalProperties": false,
  "required": ["action", "plan", "prompt"],
  "properties": {
    "action": {"type": "string", "enum": ["A", "B", "C", "D"]},
    "plan":   {"type": "string"},
    "tool":   {"type": "string"},
    "prompt": {"type": "string"}
  }
}'

# Build the claude args for a given supervisor state. Each flag is
# conditional so an empty env var wipes that one flag instead of injecting
# `--model ""`. `-p` and `--json-schema` come from the caller's invocation.
claude_args_for() {
  local state="$1"
  local model effort
  case "$state" in
    asking)  model="$CLAUDE_MODEL_ASKING";  effort="$CLAUDE_EFFORT_ASKING"  ;;
    *)       model="$CLAUDE_MODEL_BLOCKED"; effort="$CLAUDE_EFFORT_BLOCKED" ;;
  esac
  local -a args=( --output-format text )
  [ -n "$model" ]                 && args+=( --model "$model" )
  [ -n "$effort" ]                && args+=( --effort "$effort" )
  [ -n "$CLAUDE_FALLBACK_MODEL" ] && args+=( --fallback-model "$CLAUDE_FALLBACK_MODEL" )
  args+=( --json-schema "$JSON_SCHEMA" )
  # Prompt cache: append our supervisor role/rules from a stable file
  # AND exclude per-machine sections (cwd, env, git status, time) from
  # the default system prompt so the prefix hashes identically across
  # calls. ~50-80% Opus discount on repeat ticks.
  if [ -f "$SYSTEM_PROMPT_FILE" ]; then
    args+=( --append-system-prompt-file "$SYSTEM_PROMPT_FILE" )
    args+=( --exclude-dynamic-system-prompt-sections )
  fi
  printf '%s\n' "${args[@]}"
}

ONCE=0
DRY_RUN=0
ONLY_PANE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --loop)     ONCE=0; shift ;;
    --once)     ONCE=1; shift ;;
    --dry-run)  DRY_RUN=1; shift ;;
    --pane)     ONLY_PANE="$2"; shift 2 ;;
    --interval) INTERVAL="$2"; shift 2 ;;
    -h|--help)  sed -n '1,40p' "$0"; exit 0 ;;
    *) echo "claude-supervisor: unknown flag $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$STATE_DIR"
LOG="$STATE_DIR/log"
ts() { date +%H:%M:%S; }
log() { printf '[%s] claude-supervisor: %s\n' "$(ts)" "$*" | tee -a "$LOG"; }

# ── prompt-cache helpers ───────────────────────────────────────────────────
# The static role + decision rules + output-format spec are byte-identical
# across calls, so we put them in an append-system-prompt file. With
# --exclude-dynamic-system-prompt-sections, Anthropic's prompt cache hits
# on the full system-prompt prefix. The per-pane variable bits (plan
# content, tail, prior answers) live in the user message and don't get
# cached, which is fine — they're small.
SYSTEM_PROMPT_FILE="$STATE_DIR/system-prompt.txt"
write_system_prompt() {
  cat > "$SYSTEM_PROMPT_FILE" <<'SYSPROMPT'
You are the supervisor of a codex-fleet worker pane. The operator just
captured the worker's recent scrollback and pinned the active plan; you
decide one next action. Your reply is pasted verbatim into the worker's
codex CLI, so write it as the operator would type it — no quotation
marks, no "tell the worker to" prefix.

# State semantics
- state=asking — codex is showing a waiting-on-input prompt: a numbered
  menu, "(recommended)"/"(default)" tag, [Y/n], "Continue?", etc. Pick
  one and move on; don't lecture.
- state=blocked — the worker emitted a genuine blocker pattern
  (PLAN_SUBTASK_NOT_FOUND, stale-claim, "less than 5% of your 5h
  limit", etc.). Decide rescue / switch-plan / wait.

# Decision rules — state=asking
- If a literal "(recommended)" or "(default)" tag appears on an
  option, pick THAT option. Only override when the recommended option
  would clearly break an acceptance criterion of the active plan.
- If no option is tagged, prefer the lowest-risk reversible choice:
  read-only > local edit > new branch > merge > destructive op. When
  in doubt, the option that keeps the active plan's branch/worktree
  intact.
- For yes/no prompts (Y/n, y/N, Continue?, Approve?, Proceed?):
  answer Y when the worker is mid-implementation on a task it
  legitimately claimed and the action is in-scope; answer n when the
  action is destructive outside its claim.
- Keep PROMPT short. A menu pick is one option number or letter.

# Decision rules — state=blocked
- "stale-claim blocker, you told me not to rescue" → either authorize
  the rescue with the exact mcp__colony__rescue_stranded_run call, OR
  abandon that lane and switch to task_ready_for_agent against another
  plan with available > 0.
- "queue is empty" but the plan registry shows available > 0 on a
  plan → name that plan + sub-task explicitly.
- "less than 5% of your 5h limit" → tell worker to post a brief
  blocker, hand off, and exit (cap-swap-daemon respawns).
- Worker is mid-task / healthy → action=C / prompt=(none).
- Never blast the same OVERRIDE the worker already complained about.
- Never claim a sub-task the registry shows as completed.
- Keep PROMPT under 600 characters.

# Reply format — JSON, schema-enforced by the CLI
{
  "action": "A" | "B" | "C" | "D",
  "plan":   "one short sentence: what you're about to make the worker do and why; '(none)' if action=C",
  "tool":   "optional MCP tool name + minimal args, or '(none)'",
  "prompt": "text to paste verbatim into the worker pane, or '(none)' if action=C"
}
A=rescue stale claim, B=switch plan, C=do nothing, D=answer codex question.
Emit the JSON object and nothing else. No markdown, no preamble.
SYSPROMPT
}
write_system_prompt

# Classifier (BUSY/ASK/BLOCKED patterns + is_busy / is_asking / is_blocked /
# last_line_is_prompt / classify_tail / tail_hash) lives in a pure-bash lib
# so the replay harness under scripts/codex-fleet/test/ can exercise it
# without spinning up the daemon. Sourcing has no side effects.
. "$SCRIPT_DIR/lib/claude-supervisor-classifier.sh"

# Per-pane state TSV: pane_id <TAB> last_ts <TAB> last_hash
STATE_FILE="$STATE_DIR/panes.tsv"
[ -f "$STATE_FILE" ] || : > "$STATE_FILE"

# Paste log: ts <TAB> pane_id <TAB> state. One row per supervisor paste,
# used by the 3-strike guard to detect "claude keeps answering and codex
# keeps re-asking" loops. Survives restarts; the strike helper only counts
# rows inside the sliding window so old entries decay automatically.
PASTE_LOG="$STATE_DIR/paste-log.tsv"
[ -f "$PASTE_LOG" ] || : > "$PASTE_LOG"

# Operator escalation feed: ts <TAB> pane_id <TAB> state <TAB> reason.
# When a pane crosses STRIKE_LIMIT we write here and skip the call; the
# operator (or another tool) reads this file to notice loops the
# supervisor refused to keep paying for.
ESCALATE_LOG="$STATE_DIR/escalate.tsv"
[ -f "$ESCALATE_LOG" ] || : > "$ESCALATE_LOG"

# Last-response per pane: pane_id <TAB> ts <TAB> state <TAB> action <TAB> prompt(escaped).
# Surfaces "you already answered this pane with X" to the next claude
# call so multi-step dialogs stay coherent ("I already picked option 3,
# this follow-up menu is the next step").
LAST_RESPONSE_FILE="$STATE_DIR/last-response.tsv"
[ -f "$LAST_RESPONSE_FILE" ] || : > "$LAST_RESPONSE_FILE"

record_last_response() {
  local pane="$1" state="$2" action="$3" prompt="$4" now; now="$(date +%s)"
  # Newlines/tabs in prompt would break TSV; collapse to single-space.
  local clean_prompt
  clean_prompt="$(printf '%s' "$prompt" | tr '\t\n' '  ' | head -c 400)"
  local tmp; tmp="$(mktemp)"
  awk -F'\t' -v p="$pane" '$1!=p' "$LAST_RESPONSE_FILE" > "$tmp"
  printf '%s\t%s\t%s\t%s\t%s\n' "$pane" "$now" "$state" "$action" "$clean_prompt" >> "$tmp"
  mv "$tmp" "$LAST_RESPONSE_FILE"
}

prior_answer_for() {
  local pane="$1"
  awk -F'\t' -v p="$pane" -v now="$(date +%s)" '
    $1==p {
      age = now - $2
      printf "(%ds ago, state=%s, action=%s)\n  prompt: %s\n", age, $3, $4, $5
      exit
    }
  ' "$LAST_RESPONSE_FILE" 2>/dev/null
}

# ── metrics: per-decision TSV with next-tick outcome backfill ───────────────
# Columns: ts | pane | panel | state | model | action | rc | outcome
# outcome starts as "?" and is rewritten by the NEXT tick for this pane:
#   resolved   → pane is now busy/quiet (the paste worked)
#   unresolved → pane is still asking/blocked (paste didn't unstick it)
# Combined with the strike guard this is the foundation for evaluating
# whether tiered models / recommended-path heuristic actually help.
METRICS_FILE="$STATE_DIR/metrics.tsv"
[ -f "$METRICS_FILE" ] || : > "$METRICS_FILE"

record_metric() {
  local pane="$1" panel="$2" state="$3" model="$4" action="$5" rc="$6"
  local now; now="$(date +%s)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t?\n' \
    "$now" "$pane" "$panel" "$state" "$model" "$action" "$rc" >> "$METRICS_FILE"
}

# For the most-recent row of `pane` whose outcome column is "?", rewrite
# it to `verdict`. Called on a pane's NEXT tick: if pane is now quiet/busy
# the previous decision is "resolved"; if still asking/blocked it's
# "unresolved". No-op when no prior row or no "?" outcome — keeps the
# script cheap on cold panes.
backfill_outcome() {
  local pane="$1" verdict="$2"
  awk -F'\t' -v OFS='\t' -v p="$pane" -v v="$verdict" '
    BEGIN { found=0 }
    {
      if (!found && $2==p && $8=="?") {
        $8 = v
        found = 1
      }
      print
    }
  ' "$METRICS_FILE" > "$METRICS_FILE.tmp" && mv "$METRICS_FILE.tmp" "$METRICS_FILE"
}

# Translate a state name into a metrics outcome label.
state_to_outcome() {
  case "$1" in
    busy|quiet)     echo resolved ;;
    asking|blocked) echo unresolved ;;
    *)              echo unknown ;;
  esac
}

# Count strikes for a pane within STRIKE_WINDOW seconds.
strike_count() {
  local pane="$1" now="$2"
  awk -F'\t' -v p="$pane" -v cut="$((now - STRIKE_WINDOW))" \
    '$2==p && $1+0 >= cut {n++} END{print n+0}' "$PASTE_LOG"
}

record_paste() {
  local pane="$1" state="$2" now; now="$(date +%s)"
  printf '%s\t%s\t%s\n' "$now" "$pane" "$state" >> "$PASTE_LOG"
}

record_escalation() {
  local pane="$1" panel="$2" state="$3" strikes="$4" now; now="$(date +%s)"
  printf '%s\t%s\t%s\t%s\tstrikes=%d in %ds\n' \
    "$now" "$pane" "$panel" "$state" "$strikes" "$STRIKE_WINDOW" >> "$ESCALATE_LOG"
}

within_cooldown() {
  local pane="$1" now="$2"
  local last; last="$(awk -F'\t' -v p="$pane" '$1==p {print $2; exit}' "$STATE_FILE")"
  [ -z "$last" ] && return 1
  [ "$((now - last))" -lt "$COOLDOWN" ]
}

already_answered() {
  local pane="$1" hash="$2"
  awk -F'\t' -v p="$pane" -v h="$hash" '$1==p && $3==h {f=1} END{exit !f}' "$STATE_FILE"
}

record() {
  local pane="$1" hash="$2" now; now="$(date +%s)"
  local tmp; tmp="$(mktemp)"
  awk -F'\t' -v p="$pane" '$1!=p' "$STATE_FILE" > "$tmp"
  printf '%s\t%s\t%s\n' "$pane" "$now" "$hash" >> "$tmp"
  mv "$tmp" "$STATE_FILE"
}

# ── plan / queue context ────────────────────────────────────────────────────
active_plan() {
  cat "$REPO_ROOT/.codex-fleet/active-plan" 2>/dev/null || true
}

plan_summary() {
  # One-line per plan: slug | done | claimed | available
  local slug="$1"
  local pj="$REPO_ROOT/openspec/plans/$slug/plan.json"
  [ -f "$pj" ] || { echo "(plan.json missing: $slug)"; return; }
  jq -r --arg s "$slug" '
    [.tasks[] | (.status // "open")] as $st |
    "\($s)\tdone=\($st | map(select(.=="complete" or .=="done" or .=="completed" or .=="merged")) | length)\tclaimed=\($st | map(select(.=="claimed")) | length)\tavailable=\($st | map(select(.=="available")) | length)"
  ' "$pj"
}

all_plans_summary() {
  for slug in "$REPO_ROOT"/openspec/plans/*/; do
    local s; s="$(basename "$slug")"
    plan_summary "$s"
  done
}

# Title + problem + acceptance criteria of the active plan. The
# build_prompt's "pick recommended unless it breaks the plan" rule has no
# teeth without this — claude can't audit a choice against criteria it
# never saw. Capped so a verbose plan can't blow out the context budget.
PLAN_CONTENT_BUDGET="${PLAN_CONTENT_BUDGET:-2400}"   # chars
active_plan_content() {
  local slug="$1"
  [ -z "$slug" ] && return 0
  local pj="$REPO_ROOT/openspec/plans/$slug/plan.json"
  [ -f "$pj" ] || return 0
  jq -r '
    "TITLE: " + (.title // "(no title)"),
    "",
    "PROBLEM:",
    (.problem // "(no problem statement)"),
    "",
    "ACCEPTANCE CRITERIA:",
    ((.acceptance_criteria // []) | to_entries | map("  " + ((.key+1)|tostring) + ". " + .value) | join("\n"))
  ' "$pj" 2>/dev/null | head -c "$PLAN_CONTENT_BUDGET"
}

# ── build the supervisor prompt (user message only) ────────────────────────
# The role / decision rules / output schema live in the cached system
# prompt ($SYSTEM_PROMPT_FILE). Only the per-call variable bits go here.
build_prompt() {
  local pane="$1" panel="$2" tail="$3" plan="$4" state="$5"
  cat <<HEAD
Pane: $panel (tmux id $pane)
State: $state

# Active plan pinned in .codex-fleet/active-plan
$plan

# Active plan content (title + problem + acceptance criteria) — audit the
# recommended option against these before picking.
$(active_plan_content "$plan" | sed 's/^/  /')

# Plan registry — counts of done / claimed / available sub-tasks
$(all_plans_summary | sed 's/^/  /')

# Previously you told this pane (most recent first, may be empty)
$(prior_answer_for "$pane" | sed 's/^/  /')

# Recent worker output (last 80 lines, ANSI stripped)
\`\`\`
$(printf '%s\n' "$tail" | sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g')
\`\`\`
HEAD
}

# ── apply claude's response ─────────────────────────────────────────────────
# Tries JSON parse first (schema-validated path). If that fails — e.g. the
# fallback model ignored the schema, or the CLI returned a JSON envelope
# with the schema'd object nested inside — falls back to the legacy line
# parser so we don't lose the response. Either way ends with the same
# (action, plan_line, tool, prompt) locals.
apply_response() {
  local pane="$1" panel="$2" response="$3" state="${4:-?}"
  local action plan_line prompt tool source="json"

  if printf '%s' "$response" | jq -e . >/dev/null 2>&1; then
    action="$(printf '%s' "$response"    | jq -r '.action  // ""')"
    plan_line="$(printf '%s' "$response" | jq -r '.plan    // ""')"
    tool="$(printf '%s' "$response"      | jq -r '.tool    // ""')"
    prompt="$(printf '%s' "$response"    | jq -r '.prompt  // ""')"
  else
    source="text-fallback"
    action="$(printf '%s\n' "$response"    | grep -m1 -iE '^(ACTION|"action")' | sed -E 's/^[^:]*: *//; s/^"//; s/",?$//' | awk '{print $1}')"
    plan_line="$(printf '%s\n' "$response" | grep -m1 -iE '^(PLAN|"plan")'     | sed -E 's/^[^:]*: *//; s/^"//; s/",?$//')"
    tool="$(printf '%s\n' "$response"      | grep -m1 -iE '^(TOOL|"tool")'     | sed -E 's/^[^:]*: *//; s/^"//; s/",?$//')"
    prompt="$(printf '%s\n' "$response"    | awk 'BEGIN{IGNORECASE=1} /^(PROMPT|"prompt")/ {flag=1; sub(/^[^:]*: */,""); sub(/^"/,""); sub(/",?$/,""); print; next} flag')"
  fi
  log "  $panel: action=${action:-?} plan=${plan_line:-(none)} tool=${tool:-(none)} via=$source"

  if [ "$action" = "C" ] || [ -z "$prompt" ] || [ "$prompt" = "(none)" ]; then
    log "  $panel: no action — letting the worker keep working"
    return 2   # caller uses rc==2 to skip strike accounting
  fi

  if [ "$DRY_RUN" = "1" ]; then
    log "  $panel: DRY-RUN — would paste $(printf '%s' "$prompt" | wc -c) chars"
    printf '    %s\n' "$(printf '%s' "$prompt" | head -c 200)" | sed 's/^/    /'
    return 0
  fi

  local buf="claude-sup-$$-${pane#%}"
  printf '%s' "$prompt" | tmux load-buffer -b "$buf" -
  tmux send-keys -t "$pane" Escape 2>/dev/null || true
  tmux paste-buffer -b "$buf" -t "$pane" -p
  tmux send-keys -t "$pane" Enter
  tmux delete-buffer -b "$buf" 2>/dev/null || true
  log "  $panel: pasted response"
  record_last_response "$pane" "$state" "${action:-?}" "$prompt"
  return 0   # rc==0 → tick() will record a strike
}

# ── one tick ────────────────────────────────────────────────────────────────
tick() {
  local now; now="$(date +%s)"
  local plan; plan="$(active_plan)"
  [ -z "$plan" ] && plan="(no plan pinned)"

  local panes
  if [ -n "$ONLY_PANE" ]; then
    panes="$ONLY_PANE|$(tmux show-option -p -t "$ONLY_PANE" '@panel' 2>/dev/null | awk '{print $2}' | tr -d '"')"
  else
    panes="$(tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{@panel}' 2>/dev/null)"
  fi

  printf '%s\n' "$panes" | while IFS='|' read -r pane panel; do
    [ -z "$panel" ] && continue
    [ "$panel" = "[codex-fleet-tab-strip]" ] && continue

    local tail; tail="$(tmux capture-pane -p -t "$pane" -S -80 2>/dev/null || true)"
    local state; state="$(classify_tail "$tail")"
    # Backfill the previous decision's outcome BEFORE we early-return —
    # even "this pane is now busy, skip" is a valid datapoint (it tells
    # us the previous paste worked).
    backfill_outcome "$pane" "$(state_to_outcome "$state")"
    case "$state" in
      busy|quiet) continue ;;
      asking|blocked) : ;;
    esac
    within_cooldown "$pane" "$now" && { log "$panel: cooldown — skip"; continue; }

    local h; h="$(tail_hash "$tail")"
    if already_answered "$pane" "$h"; then
      log "$panel: already answered this exact state (hash=$h) — skip"
      continue
    fi

    # 3-strike loop guard: if we've pasted to this pane >= STRIKE_LIMIT
    # times inside the trailing window, the worker is in a loop the
    # supervisor isn't getting out of — escalate to the operator instead
    # of paying for another claude turn.
    local strikes; strikes="$(strike_count "$pane" "$now")"
    if [ "$strikes" -ge "$STRIKE_LIMIT" ]; then
      log "$panel: $state — strike guard tripped ($strikes/$STRIKE_LIMIT in ${STRIKE_WINDOW}s) — escalating"
      record_escalation "$pane" "$panel" "$state" "$strikes"
      record "$pane" "$h"   # mark hash so we don't keep tripping every tick
      continue
    fi

    # Pick model tier by state. asking = sonnet/medium (fast, cheap),
    # blocked = opus-4-7/high (real reasoning). Both share the JSON
    # schema so apply_response sees the same shape either way.
    local -a args; mapfile -t args < <(claude_args_for "$state")
    log "$panel: $state — asking ${args[*]} (strikes=$strikes/$STRIKE_LIMIT)"
    local prompt_txt; prompt_txt="$(build_prompt "$pane" "$panel" "$tail" "$plan" "$state")"
    local response
    # Resolve the tier model for the metrics row.
    local tier_model
    case "$state" in
      asking) tier_model="${CLAUDE_MODEL_ASKING:-sonnet}" ;;
      *)      tier_model="${CLAUDE_MODEL_BLOCKED:-claude-opus-4-7}" ;;
    esac
    if response="$("$CLAUDE_BIN" -p "${args[@]}" <<<"$prompt_txt" 2>>"$STATE_DIR/claude.err")"; then
      local rc=0
      apply_response "$pane" "$panel" "$response" "$state" || rc=$?
      record "$pane" "$h"
      [ "$rc" = "0" ] && record_paste "$pane" "$state"
      # Best-effort action extraction for the metrics row. apply_response
      # already parsed it but we don't return it, so re-extract cheaply.
      local m_action
      if printf '%s' "$response" | jq -e . >/dev/null 2>&1; then
        m_action="$(printf '%s' "$response" | jq -r '.action // "?"')"
      else
        m_action="$(printf '%s\n' "$response" | grep -m1 -iE '^(ACTION|"action")' | sed -E 's/^[^:]*: *//; s/^"//; s/",?$//' | awk '{print $1}')"
      fi
      record_metric "$pane" "$panel" "$state" "$tier_model" "${m_action:-?}" "$rc"
    else
      log "  $panel: claude -p failed — see $STATE_DIR/claude.err"
      record_metric "$pane" "$panel" "$state" "$tier_model" "ERR" "1"
    fi
  done
}

# ── main ────────────────────────────────────────────────────────────────────
if (( ONCE == 1 )); then
  tick
  exit 0
fi

log "loop start (interval=${INTERVAL}s cooldown=${COOLDOWN}s min-blocked=${MIN_BLOCKED}s)"
while :; do
  tick || log "tick error — continuing"
  sleep "$INTERVAL"
done
