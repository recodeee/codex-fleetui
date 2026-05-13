#!/usr/bin/env bash
# fleet-tick — live viz daemon (v2 graphical).
#   /tmp/claude-viz/live-fleet-state.txt   per-account 5h/weekly + worker→subtask
#   /tmp/claude-viz/live-plan-design.txt   wave tree of plan sub-tasks + claims
# Stop: kill $(cat /tmp/claude-viz/fleet-tick.pid)
set -eo pipefail

INTERVAL="${FLEET_TICK_INTERVAL:-5}"
TMUX_SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
REPO="${FLEET_TICK_REPO:-/home/deadpool/Documents/recodee}"
PLAN_JSON="${FLEET_TICK_PLAN_JSON:-$REPO/openspec/plans/rust-ph13-14-15-completion-2026-05-13/plan.json}"
STATE_OUT="${FLEET_TICK_STATE_OUT:-/tmp/claude-viz/live-fleet-state.txt}"
PLAN_OUT="${FLEET_TICK_PLAN_OUT:-/tmp/claude-viz/live-plan-design.txt}"
WAVES_OUT="${FLEET_TICK_WAVES_OUT:-/tmp/claude-viz/live-waves.txt}"
ACTIVE_FILE="${FLEET_TICK_ACTIVE_FILE:-/tmp/claude-viz/fleet-active-accounts.txt}"
PID_FILE="${FLEET_TICK_PID_FILE:-/tmp/claude-viz/fleet-tick.pid}"

if [[ "${FLEET_TICK_SOURCE_ONLY:-0}" != "1" ]]; then
  mkdir -p "$(dirname "$STATE_OUT")" "$(dirname "$PLAN_OUT")" "$(dirname "$WAVES_OUT")"
  echo $$ > "$PID_FILE"
fi

declare -A SHORT=(
  [koncita@pipacsclub.hu]=koncita     [mesi@lebenyse.hu]=mesi
  [matt@gitguardex.com]=matt          [recodee@mite.hu]=recodee
  [fico@magnoliavilag.hu]=fico        [ricsi@zazrifka.sk]=ricsi
  [odin@mite.hu]=odin-m               [lili@gitguardex.com]=lili
  [admin@mite.hu]=admin-m             [odin@gitguardex.com]=odin-g
  [viktor@gitguardex.com]=viktor      [zeus@magnoliavilag.hu]=zeus-m
  [admin@zazrifka.sk]=admin-z         [zeus@mite.hu]=zeus-mi
)
FLEET_EMAILS=(
  koncita@pipacsclub.hu mesi@lebenyse.hu matt@gitguardex.com recodee@mite.hu
  fico@magnoliavilag.hu ricsi@zazrifka.sk odin@mite.hu lili@gitguardex.com
  admin@mite.hu odin@gitguardex.com viktor@gitguardex.com
  zeus@magnoliavilag.hu admin@zazrifka.sk zeus@mite.hu
)
declare -A AID=(
  [koncita@pipacsclub.hu]=koncita-pipacs   [mesi@lebenyse.hu]=mesi-lebenyse
  [matt@gitguardex.com]=matt-gg            [recodee@mite.hu]=recodee-mite
  [fico@magnoliavilag.hu]=fico-magnolia    [ricsi@zazrifka.sk]=ricsi-zazrifka
  [odin@mite.hu]=odin-mite                 [lili@gitguardex.com]=lili-gg
  [admin@mite.hu]=admin-mite               [odin@gitguardex.com]=odin-gg
  [viktor@gitguardex.com]=viktor-gg        [zeus@magnoliavilag.hu]=zeus-magnolia
  [admin@zazrifka.sk]=admin-zazrifka       [zeus@mite.hu]=zeus-mite
)

# Sub-task evidence files scored by subtask_progress_pct for fractional progress.
SUB_EVIDENCE=(
  "openspec/changes/ph13-rollback-drill-inventory-2026-05-13/proposal.md"
  "rust/codex-lb-runtime/tests/rollback_drills.rs"
  "docs/rollback-drills/PLAYBOOK-TEMPLATE.md"
  "docs/rollback-drills/plans-runtime-drill.md"
  "docs/rollback-drills/workspaces-drill.md"
  "openspec/changes/ph14-observability-gates-2026-05-13/proposal.md"
  "rust/codex-lb-runtime/src/observability/metrics.rs"
  "docs/runbooks/ph14-staged-rollout.md"
  "docs/runbooks/ph14-soak-dashboard.md"
  "openspec/changes/ph15-decommission-inventory-2026-05-13/proposal.md"
  "docs/runbooks/ph15-cutover.md"
  "docs/runbooks/ph15-final-verification.md"
)
SUB_TITLES=(
  "PH13.0 inventory"        "PH13.1 drill harness"     "PH13.2 playbook"
  "PH13.3 drills A"         "PH13.4 drills B"          "PH14.0 obs-gates"
  "PH14.1 metrics"          "PH14.2 rollout runbook"   "PH14.3 dashboard"
  "PH15.0 decommission inv" "PH15.1 cutover runbook"   "PH15.2 final verify"
)

# ANSI palette (Catppuccin-ish)
B=$'\033[1m'; D=$'\033[2m'; R=$'\033[0m'
G=$'\033[38;5;114m'    # green for done/running
Y=$'\033[38;5;179m'    # yellow for claimed/warn
RED=$'\033[38;5;174m'  # red for blocked/down
C=$'\033[38;5;110m'    # cyan/blue
M=$'\033[38;5;176m'    # mauve
DIM=$'\033[38;5;240m'  # dim grey for unclaimed
TEAL=$'\033[38;5;73m'  # teal headings
# 7-stop gradient (0%→red, 50%→yellow, 100%→green)
GRAD0=$'\033[38;5;203m'   # 0–14   red
GRAD1=$'\033[38;5;209m'   # 15–29  red-orange
GRAD2=$'\033[38;5;215m'   # 30–44  orange
GRAD3=$'\033[38;5;221m'   # 45–59  yellow
GRAD4=$'\033[38;5;185m'   # 60–74  yellow-green
GRAD5=$'\033[38;5;150m'   # 75–89  lime
GRAD6=$'\033[38;5;83m'    # 90+    bright green
MAG=$'\033[38;5;176m'     # magenta (rate-limited)
ICE=$'\033[38;5;117m'     # sky (working)

# Pick gradient color by percentage int
pct_color() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || { printf '%s' "$DIM"; return; }
  if   (( n >= 90 )); then printf '%s' "$GRAD6"
  elif (( n >= 75 )); then printf '%s' "$GRAD5"
  elif (( n >= 60 )); then printf '%s' "$GRAD4"
  elif (( n >= 45 )); then printf '%s' "$GRAD3"
  elif (( n >= 30 )); then printf '%s' "$GRAD2"
  elif (( n >= 15 )); then printf '%s' "$GRAD1"
  else                     printf '%s' "$GRAD0"
  fi
}

# usage_color — INVERTED gradient for usage caps. Low % = green (lots of headroom),
# high % = red (close to cap). Use this for 5h/weekly columns; use pct_color for
# things where higher = better (progress, done%).
usage_color() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || { printf '%s' "$DIM"; return; }
  if   (( n >= 95 )); then printf '%s' "$GRAD0"
  elif (( n >= 85 )); then printf '%s' "$GRAD1"
  elif (( n >= 70 )); then printf '%s' "$GRAD2"
  elif (( n >= 55 )); then printf '%s' "$GRAD3"
  elif (( n >= 40 )); then printf '%s' "$GRAD4"
  elif (( n >= 20 )); then printf '%s' "$GRAD5"
  else                     printf '%s' "$GRAD6"
  fi
}



# Tiny block-spark for a percentage (▁..█)
pct_spark() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || { printf ' '; return; }
  if   (( n >= 88 )); then printf '█'
  elif (( n >= 75 )); then printf '▇'
  elif (( n >= 62 )); then printf '▆'
  elif (( n >= 50 )); then printf '▅'
  elif (( n >= 37 )); then printf '▄'
  elif (( n >= 25 )); then printf '▃'
  elif (( n >= 12 )); then printf '▂'
  else                     printf '▁'
  fi
}

WHITE=$'\033[38;5;253m'   # bright off-white for bar caps

# 6-cell gradient-filled horizontal bar ▕████░░▏ for 0..100 (usage semantic).
# Filled cells use usage_color (0% green / lots of room → 100% red / capped),
# empty cells dim. Each cell rendered as a colored █ on dim ░ rail.
usage_bar() {
  local n="$1" width=6
  [[ "$n" =~ ^[0-9]+$ ]] || n=0
  local filled=$(( (n * width + 50) / 100 ))
  (( filled > width )) && filled=$width
  (( filled < 0 )) && filled=0
  local col
  col=$(usage_color "$n")
  local out="${WHITE}▕${R}${col}"
  local i
  for ((i=0;i<filled;i++)); do out+="█"; done
  out+="${DIM}"
  for ((i=filled;i<width;i++)); do out+="░"; done
  out+="${WHITE}▏${R}"
  printf '%s' "$out"
}

clamp_pct() {
  local n="$1"
  [[ "$n" =~ ^[0-9]+$ ]] || { printf '0'; return; }
  (( n < 0 )) && n=0
  (( n > 100 )) && n=100
  printf '%d' "$n"
}

subtask_progress_bar() {
  local pct
  pct=$(clamp_pct "${1:-0}")
  local len="${2:-10}"
  local filled=$(( pct * len / 100 ))
  local bar=""
  local i
  for ((i=0; i<filled; i++)); do bar+="█"; done
  for ((i=filled; i<len; i++)); do bar+="░"; done
  printf '%s' "$bar"
}

# Score evidence completeness for a sub-task. This keeps the plan UI from
# treating a touched file as binary done before it has enough proof.
subtask_progress_pct() {
  local idx="$1"
  local evidence="${SUB_EVIDENCE[$idx]:-}"
  [[ -n "$evidence" ]] || { printf '0'; return; }

  local file="$REPO/$evidence"
  [[ -s "$file" ]] || { printf '0'; return; }

  local score=20
  local lines
  lines=$(wc -l < "$file" 2>/dev/null || printf '0')
  [[ "$lines" =~ ^[0-9]+$ ]] || lines=0
  local line_score=$(( lines * 30 / 60 ))
  (( line_score > 30 )) && line_score=30
  score=$(( score + line_score ))

  grep -qE '^##[[:space:]]+Why\b' "$file" 2>/dev/null && score=$(( score + 10 ))
  grep -qE '^##[[:space:]]+(What Changes|What Changed|Changes)\b' "$file" 2>/dev/null && score=$(( score + 10 ))
  grep -qE '^##[[:space:]]+(Impact|Verification|Test Plan)\b' "$file" 2>/dev/null && score=$(( score + 10 ))

  if [[ "$file" == *.rs ]]; then
    if grep -qE '#\[(tokio::|async_std::)?test\]|fn[[:space:]]+test_|fn[[:space:]]+[a-zA-Z0-9_]*test' "$file" 2>/dev/null; then
      score=$(( score + 20 ))
    fi
  elif grep -qE '(^- \[[xX]\]|```|cargo test|pytest|bun test|Verification)' "$file" 2>/dev/null; then
    score=$(( score + 20 ))
  fi

  clamp_pct "$score"
}

# Determine subtask claim from plan.json (jq fast path)
# Returns:  status|claimed_agent  (progress is scored separately from evidence)
load_subtask_state() {
  local idx="$1"
  local plan_status="" plan_agent=""
  if [[ -f "$PLAN_JSON" ]]; then
    read -r plan_status plan_agent < <(
      jq -r --argjson idx "$idx" \
        '.tasks[] | select(.subtask_index==$idx) | "\(.status // "available")\t\(.claimed_by_agent // "")"' \
        "$PLAN_JSON" 2>/dev/null | head -1 | tr '\t' ' '
    )
  fi
  local evidence="${SUB_EVIDENCE[$idx]}"
  local final_status="$plan_status"
  [[ -z "$final_status" ]] && final_status="available"
  [[ -z "$evidence" ]] && final_status="available"
  echo "${final_status}|${plan_agent}"
}

# Build worker→subtask reverse map  agent → subtask_index
declare -gA WORKER_SUB
build_worker_sub_map() {
  WORKER_SUB=()
  if [[ -f "$PLAN_JSON" ]]; then
    while IFS=$'\t' read -r idx agent; do
      [[ -n "$agent" && "$agent" != "null" ]] && WORKER_SUB[$agent]=$idx
    done < <(
      jq -r '.tasks[] | select(.claimed_by_agent != null) | "\(.subtask_index)\t\(.claimed_by_agent)"' \
        "$PLAN_JSON" 2>/dev/null
    )
  fi
}

if [[ "${FLEET_TICK_SOURCE_ONLY:-0}" == "1" ]]; then
  return 0 2>/dev/null || exit 0
fi

while true; do
  ts=$(date '+%H:%M:%S')

  # ── 1. usage from codex-auth ───────────────────────────────────────────────
  declare -A USAGE
  while IFS= read -r line; do
    email=$(grep -oP '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[A-Za-z]{2,}' <<<"$line" | head -1)
    [[ -z "$email" ]] && continue
    h5=$(grep -oP '5h=\K[0-9]+%' <<<"$line" | head -1)
    wk=$(grep -oP 'weekly=\K[0-9]+%' <<<"$line" | head -1)
    USAGE[$email]="${h5:--} ${wk:--}"
  done < <(codex-auth list 2>/dev/null || true)

  # ── 2. liveness from tmux pane cmds ────────────────────────────────────────
  declare -A ALIVE
  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    mapfile -t ACTIVE < "$ACTIVE_FILE" 2>/dev/null || ACTIVE=()
    # tmux panes 2..6 (after fleet-state + codex0 + plan-design split) are codexes
    # but pane indices may vary; query by left>0
    declare -a CODEX_PANES
    while IFS='|' read -r pid pane_idx left cmd; do
      [[ "$left" -gt 0 && "$cmd" == "node" ]] && CODEX_PANES+=("$pane_idx")
    done < <(tmux list-panes -t "$TMUX_SESSION:overview" -F '#{pane_id}|#{pane_index}|#{pane_left}|#{pane_current_command}' 2>/dev/null)
    i=0
    for aid in "${ACTIVE[@]}"; do
      [[ $i -lt ${#CODEX_PANES[@]} ]] && ALIVE[$aid]=running
      i=$((i+1))
    done
  fi


  # ── 2b. exhaustion detector: flag fully-capped agents + log to supervisor queue
  declare -A EXHAUSTED
  for email in "${FLEET_EMAILS[@]}"; do
    pair="${USAGE[$email]:-- -}"
    h5=${pair%% *}; wk=${pair##* }
    h5_n=${h5%%%}; wk_n=${wk%%%}
    [[ "$h5_n" =~ ^[0-9]+$ ]] || h5_n=0
    [[ "$wk_n" =~ ^[0-9]+$ ]] || wk_n=0
    aid=${AID[$email]}
    if (( h5_n >= 100 )) && [[ -n "${ALIVE[$aid]:-}" ]]; then
      EXHAUSTED[$aid]="5h=100%"
      # Log to supervisor queue (jsonl, append-only, dedup-by-minute)
      ts_full=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
      ts_min=$(date -u +"%Y-%m-%dT%H:%M")
      if [[ ! -f /tmp/claude-viz/supervisor-queue.jsonl ]] || ! grep -q "\"agent\":\"codex-$aid\".*\"ts_min\":\"$ts_min\"" /tmp/claude-viz/supervisor-queue.jsonl 2>/dev/null; then
        printf '{"ts":"%s","ts_min":"%s","agent":"codex-%s","email":"%s","reason":"5h=%d%% weekly=%d%%","action":"takeover_recommended"}\n' \
          "$ts_full" "$ts_min" "$aid" "$email" "$h5_n" "$wk_n" >> /tmp/claude-viz/supervisor-queue.jsonl
      fi
    fi
  done
  # ── 3. plan claim mapping ──────────────────────────────────────────────────
  build_worker_sub_map

  # ── 4. render live-fleet-state.txt — V2 ACTIVE/RESERVE sections ───────────
  # Load active worker IDs
  declare -A IS_ACTIVE
  mapfile -t _ACT < "$ACTIVE_FILE" 2>/dev/null || _ACT=()
  for _a in "${_ACT[@]}"; do IS_ACTIVE[$_a]=1; done

  # Render row function (used by both sections)
  render_row() {
    local email="$1" section="$2"
    local pair="${USAGE[$email]:-- -}"
    local h5=${pair%% *}; local wk=${pair##* }
    local aid=${AID[$email]}
    local st=${ALIVE[$aid]:-}
    local wk_num=${wk%\%}; local h5_num=${h5%\%}
    [[ "$wk_num" =~ ^[0-9]+$ ]] || wk_num=0
    [[ "$h5_num" =~ ^[0-9]+$ ]] || h5_num=0
    local wkc h5c wk_bar h5_bar live working
    wkc=$(usage_color "$wk_num"); h5c=$(usage_color "$h5_num")
    wk_bar=$(usage_bar "$wk_num"); h5_bar=$(usage_bar "$h5_num")
    if [[ -n "${EXHAUSTED[$aid]:-}" ]]; then
      live="${B}${RED}⚠ EXHAUST${R} "
    elif [[ "$st" == "running" ]]; then
      live="${B}${G}● run${R}      "
      n_alive=$((n_alive+1))
    elif [[ "$section" == "reserve" ]]; then
      live="${DIM}◌ reserve${R}  "
    else
      live="${DIM}◌ idle${R}     "
    fi
    working=""
    if [[ "$section" == "active" ]]; then
      local agent_key="codex-$aid"
      if [[ -n "${WORKER_SUB[$agent_key]:-}" ]]; then
        local sub_idx="${WORKER_SUB[$agent_key]}"
        working="${C}→ sub-$sub_idx${R} ${DIM}${SUB_TITLES[$sub_idx]}${R}"
      fi
    else
      working="${DIM}—${R}"
    fi
    printf "  ${B}%-12s${R}  ${h5c}%-4s${R} %b   ${wkc}%-5s${R} %b   %b  %b\n" \
      "${SHORT[$email]:-$email}" "$h5" "$h5_bar" "$wk" "$wk_bar" "$live" "$working"
  }

  {
    # Header banner with v2 marker
    echo -e "${B}${TEAL}╭─ CODEX-FLEET ${R}${MAG}v2${R}${TEAL} ──────────────────────────────────────────╮${R}    ${D}${ts}${R}"
    printf "  ${B}${TEAL}%-12s  %-7s  %-8s  %-11s  %s${R}\n" "ACCOUNT" "5h" "WEEKLY" "STATUS" "WORKING ON"
    echo -e "  ${TEAL}─────────────────────────────────────────────────────────────────────${R}"
    n_alive=0
    # ─── ACTIVE section ───
    n_active=${#_ACT[@]}
    echo -e "  ${B}${G}▶ ACTIVE ${n_active}/5${R}  ${DIM}running codex panes${R}"
    for email in "${FLEET_EMAILS[@]}"; do
      aid=${AID[$email]}
      [[ -z "${IS_ACTIVE[$aid]:-}" ]] && continue
      pair="${USAGE[$email]:-- -}"
      h5=${pair%% *}; wk=${pair##* }
      st=${ALIVE[$aid]:-}
      # Usage colors — gradient: 0%=red → 50%=yellow → 100%=green
      wk_num=${wk%\%}; h5_num=${h5%\%}
      [[ "$wk_num" =~ ^[0-9]+$ ]] || wk_num=0
      [[ "$h5_num" =~ ^[0-9]+$ ]] || h5_num=0
      wkc=$(usage_color "$wk_num"); h5c=$(usage_color "$h5_num")
      wk_bar=$(usage_bar "$wk_num"); h5_bar=$(usage_bar "$h5_num")
      # Worker status
      if [[ -n "${EXHAUSTED[$aid]:-}" ]]; then
        live="${B}${RED}⚠ EXHAUST${R} "
      elif [[ "$st" == "running" ]]; then
        live="${B}${G}● run${R}      "
        n_alive=$((n_alive+1))
      else
        live="${DIM}◌ idle${R}     "
      fi
      # What is this codex actually doing right now? (scrape pane content)
      working=""
      agent_key="codex-$aid"
      if [[ -n "${WORKER_SUB[$agent_key]:-}" ]]; then
        sub_idx="${WORKER_SUB[$agent_key]}"
        working="${C}→ sub-$sub_idx${R} ${DIM}${SUB_TITLES[$sub_idx]}${R}"
      elif [[ "$st" == "running" ]]; then
        # Find the tmux pane for this agent and parse its tail
        pane_for_agent=""
        i_p=0
        mapfile -t ACTIVE3 < "$ACTIVE_FILE" 2>/dev/null || ACTIVE3=()
        declare -a NP=()
        while IFS='|' read -r _pid _idx _left _cmd; do
          [[ "$_left" -gt 0 && "$_cmd" == "node" ]] && NP+=("$_idx")
        done < <(tmux list-panes -t "$TMUX_SESSION:overview" -F '#{pane_id}|#{pane_index}|#{pane_left}|#{pane_current_command}' 2>/dev/null)
        for a in "${ACTIVE3[@]}"; do
          [[ "$a" == "$aid" && $i_p -lt ${#NP[@]} ]] && pane_for_agent="${NP[$i_p]}"
          i_p=$((i_p+1))
        done
        if [[ -n "$pane_for_agent" ]]; then
          tail=$(tmux capture-pane -t "$TMUX_SESSION:overview.$pane_for_agent" -p -S -25 2>/dev/null)
          # Strip ANSI for matching
          tail_clean=$(echo "$tail" | sed 's/\[[0-9;]*m//g')
          if echo "$tail_clean" | grep -qE "usage limit|rate.?limit hit|429"; then
            working="${MAG}◍ rate-limited${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Reviewing approval request" | head -1); [[ -n "$w" ]]; then
            cmd_being_approved=$(echo "$tail_clean" | tail -8 | grep -oE "└ [^[:space:]].*" | head -1 | sed 's/└ //; s/.\{60\}.*/…/')
            working="${Y}⏸ approval: ${cmd_being_approved}${R}"
          elif w=$(echo "$tail_clean" | tail -12 | grep -oE "Working \([0-9]+[ms][^)]*\)" | tail -1); [[ -n "$w" ]]; then
            # codex draws the › prompt placeholder beneath `Working (…)` so
            # this MUST run before the idle-prompt regex below.
            secs=$(echo "$w" | grep -oE "[0-9]+[ms]" | head -1)
            working="${G}⚡ working ${secs}${R}"
          elif w=$(echo "$tail_clean" | tail -12 | grep -oE "Worked for [0-9]+m[^─]*" | tail -1); [[ -n "$w" ]]; then
            secs=$(echo "$w" | grep -oE "[0-9]+m [0-9]+s" | head -1)
            working="${G}✓ worked ${secs}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Calling [a-zA-Z_]+\.[a-zA-Z_]+" | tail -1); [[ -n "$w" ]]; then
            working="${C}● ${w}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Ran [a-z_]+" | tail -1); [[ -n "$w" ]]; then
            working="${C}● ${w}${R}"
          elif echo "$tail_clean" | tail -8 | grep -qE "^› (Find and fix|Use /skills|Run /review|Improve documentation|Implement|Summarize|Explain|Write tests)"; then
            working="${DIM}idle (default prompt)${R}"
          else
            working="${DIM}polling…${R}"
          fi
          unset NP; declare -a NP=()
        fi
      fi
      printf "  ${B}%-12s${R}  ${h5c}%-4s${R} %b   ${wkc}%-5s${R} %b   %b  %b\n" \
        "${SHORT[$email]:-$email}" "$h5" "$h5_bar" "$wk" "$wk_bar" "$live" "$working"
    done
    # ─── RESERVE section ───
    n_reserve=0
    for email in "${FLEET_EMAILS[@]}"; do
      aid=${AID[$email]}
      [[ -n "${IS_ACTIVE[$aid]:-}" ]] && continue
      n_reserve=$((n_reserve+1))
    done
    echo
    echo -e "  ${B}${C}▶ RESERVE ${n_reserve}${R}  ${DIM}promote when an active worker exhausts${R}"
    for email in "${FLEET_EMAILS[@]}"; do
      aid=${AID[$email]}
      [[ -n "${IS_ACTIVE[$aid]:-}" ]] && continue
      render_row "$email" "reserve"
    done
    echo -e "  ${TEAL}─────────────────────────────────────────────────────────────────────${R}"

    # ─── Plan summary footer ───
    # Wave chips from plan.json: green for done waves, yellow for partial, dim for pending
    plan_done=0; plan_total=12
    if [[ -f "$PLAN_JSON" ]]; then
      plan_done=$(jq -r '[.tasks[] | select(.status=="completed")] | length' "$PLAN_JSON" 2>/dev/null || echo 0)
      plan_total=$(jq -r '.tasks | length' "$PLAN_JSON" 2>/dev/null || echo 12)
    fi
    plan_pct=$(( plan_done * 100 / (plan_total > 0 ? plan_total : 1) ))
    plan_pct_color=$(pct_color "$plan_pct")
    # Wave chips W1..W8 based on actual sub-task states
    wave_chips=""
    declare -A WAVE_DEFS=([1]=0 [2]="1 2 3 4" [3]=5 [4]="6 7" [5]=8 [6]=9 [7]=10 [8]=11)
    for w in 1 2 3 4 5 6 7 8; do
      mem="${WAVE_DEFS[$w]}"
      wave_chips+=" ${B}W${w}${R}"
      for i in $mem; do
        st="${SUBST[$i]%%|*}"
        case "$st" in
          completed) wave_chips+="${G}●${R}" ;;
          claimed)   wave_chips+="${Y}◐${R}" ;;
          *)         wave_chips+="${DIM}◇${R}" ;;
        esac
      done
    done
    echo -e "  ${B}PLAN${R} ${wave_chips}  ${B}${plan_pct_color}${plan_done}/${plan_total}${R} ${DIM}(${plan_pct}%)${R}"

    # Footer counters
    if   (( n_alive >= 5 )); then awc=$GRAD6
    elif (( n_alive >= 3 )); then awc=$GRAD4
    elif (( n_alive >= 1 )); then awc=$GRAD3
    else                          awc=$DIM; fi
    n_exhausted=${#EXHAUSTED[@]}
    if (( n_exhausted > 0 )); then exhc=$RED; else exhc=$DIM; fi
    echo -e "  ${DIM}active=${R}${B}${awc}${n_alive}/${n_active}${R}  ${DIM}reserve=${R}${B}${C}${n_reserve}${R}  ${DIM}exhausted=${R}${exhc}${n_exhausted}${R}  ${DIM}refresh=${INTERVAL}s${R}"
    echo -e "${TEAL}╰──────────────────────────────────────────────────────────╯${R}"
  } > "$STATE_OUT.tmp"
  mv -f "$STATE_OUT.tmp" "$STATE_OUT"

  # ── 5. render live-plan-design.txt (graphical wave tree) ───────────────────
  # Precompute subtask state and evidence completeness independently.
  declare -A SUBST
  declare -A SUBPCT
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    SUBST[$i]=$(load_subtask_state "$i")
    SUBPCT[$i]=$(subtask_progress_pct "$i")
  done
  marker() {
    local i="$1"; local s="${SUBST[$i]%%|*}"; local pct="${SUBPCT[$i]:-0}"
    if [[ "$s" == "blocked" ]]; then
      echo -e "${RED}✕${R}"
    elif (( pct >= 100 )); then
      echo -e "${G}●${R}"
    elif [[ "$s" == "claimed" ]] || (( pct > 0 )); then
      echo -e "${Y}◐${R}"
    else
      echo -e "${DIM}◇${R}"
    fi
  }
  label() {
    local i="$1"; local s="${SUBST[$i]%%|*}"; local a="${SUBST[$i]##*|}"; local pct="${SUBPCT[$i]:-0}"
    local pcol mini suffix
    pcol=$(pct_color "$pct")
    mini=$(subtask_progress_bar "$pct" 8)
    suffix=""
    if (( pct >= 100 )); then
      suffix="${DIM}done${R}"
    elif [[ "$s" == "claimed" && -n "$a" && "$a" != "null" ]]; then
      suffix="${DIM}←${R} ${C}$a${R}"
    elif [[ "$s" == "claimed" ]]; then
      suffix="${DIM}claimed${R}"
    elif (( pct > 0 )); then
      suffix="${DIM}evidence${R}"
    fi
    echo -e "${pcol}${mini}${R} ${B}${pct}%${R} ${DIM}sub-$i${R} ${SUB_TITLES[$i]} ${suffix}"
  }
  done_count=0
  progress_sum=0
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    progress_sum=$((progress_sum + ${SUBPCT[$i]:-0}))
    (( ${SUBPCT[$i]:-0} >= 100 )) && done_count=$((done_count+1))
  done

  # ── responsive width detection ─────────────────────────────────────────────
  # Look up the consumer pane (the one watching live-plan-design.txt). The
  # pane's @panel option is set later in this loop to "[viz] plan-design" so
  # we can find it by that marker. Fall back to a wide layout if we haven't
  # tagged the pane yet (first tick after up.sh).
  plan_pane_width=$(tmux list-panes -t "$TMUX_SESSION:overview" -F '#{pane_width}|#{@panel}' 2>/dev/null | awk -F'|' '$2 ~ /plan-design/ {print $1; exit}')
  plan_pane_width=${plan_pane_width:-130}
  plan_compact=0
  # Wide tree needs ~130 chars (PH13 column + arrows + PH14 column + labels).
  # Anything narrower wraps and overlaps, so switch to compact vertical mode.
  (( plan_pane_width < 130 )) && plan_compact=1

  # ── minimal/visual plan render ───────────────────────────────────────────
  # 3 phase bars + 1 total bar + 8-wave chip grid + 1-line legend.
  # No sub-task labels, no horizontal arrows, no PLAN CLOSE arrow art —
  # just colored progress bars and chip strips so the screen reads as a
  # dashboard not a dense ascii tree.

  # Per-phase progress: PH13 = sub-0..4 (5), PH14 = sub-5..8 (4), PH15 = sub-9..11 (3)
  ph13_sum=0; ph14_sum=0; ph15_sum=0
  ph13_done=0; ph14_done=0; ph15_done=0
  for i in 0 1 2 3 4; do
    ph13_sum=$(( ph13_sum + ${SUBPCT[$i]:-0} ))
    (( ${SUBPCT[$i]:-0} >= 100 )) && ph13_done=$((ph13_done+1))
  done
  for i in 5 6 7 8; do
    ph14_sum=$(( ph14_sum + ${SUBPCT[$i]:-0} ))
    (( ${SUBPCT[$i]:-0} >= 100 )) && ph14_done=$((ph14_done+1))
  done
  for i in 9 10 11; do
    ph15_sum=$(( ph15_sum + ${SUBPCT[$i]:-0} ))
    (( ${SUBPCT[$i]:-0} >= 100 )) && ph15_done=$((ph15_done+1))
  done
  ph13_pct=$(( ph13_sum / 5 ))
  ph14_pct=$(( ph14_sum / 4 ))
  ph15_pct=$(( ph15_sum / 3 ))
  total_pct=$(( progress_sum / 12 ))

  # Bar widths scale with pane width but stay sane: 12..28 cells per phase,
  # 30..60 cells for total.
  phase_w=$(( plan_pane_width / 5 ))
  (( phase_w < 12 )) && phase_w=12
  (( phase_w > 28 )) && phase_w=28
  total_w=$(( plan_pane_width - 24 ))
  (( total_w < 30 )) && total_w=30
  (( total_w > 60 )) && total_w=60

  # chip() prints a single character for sub-task i, colored by status.
  chip() {
    local i="$1" s pct
    s="${SUBST[$i]%%|*}"
    pct="${SUBPCT[$i]:-0}"
    if [[ "$s" == "blocked" ]]; then printf '%s✕%s' "$RED" "$R"
    elif (( pct >= 100 )); then printf '%s●%s' "$G" "$R"
    elif [[ "$s" == "claimed" ]] || (( pct > 0 )); then printf '%s◐%s' "$Y" "$R"
    else printf '%s◇%s' "$DIM" "$R"
    fi
  }

  {
    echo -e "${B}${TEAL}PLAN${R}  ${D}rust-ph13-14-15-completion${R}  ${D}${ts}${R}"
    echo
    # Phase bars
    printf '  %s%-4s%s %b  %s%d/5%s  %sROLLBACK%s\n' \
      "$B" "PH13" "$R" "$(pct_color "$ph13_pct")$(subtask_progress_bar "$ph13_pct" "$phase_w")$R" \
      "$B" "$ph13_done" "$R" "$DIM" "$R"
    printf '  %s%-4s%s %b  %s%d/4%s  %sROLLOUT%s\n' \
      "$B" "PH14" "$R" "$(pct_color "$ph14_pct")$(subtask_progress_bar "$ph14_pct" "$phase_w")$R" \
      "$B" "$ph14_done" "$R" "$DIM" "$R"
    printf '  %s%-4s%s %b  %s%d/3%s  %sDECOMM%s\n' \
      "$B" "PH15" "$R" "$(pct_color "$ph15_pct")$(subtask_progress_bar "$ph15_pct" "$phase_w")$R" \
      "$B" "$ph15_done" "$R" "$DIM" "$R"
    echo
    # Total bar
    printf '  %sTOTAL%s %b  %s%d/12%s  %s%d%%%s\n' \
      "$B" "$R" "$(pct_color "$total_pct")$(subtask_progress_bar "$total_pct" "$total_w")$R" \
      "$B" "$done_count" "$R" "$(pct_color "$total_pct")" "$total_pct" "$R"
    echo
    # Wave chip grid — 2 columns (W1-W4 left, W5-W8 right) to halve vertical space
    printf '  %sW1%s %b              %sW5%s %b\n' \
      "$ICE" "$R" "$(chip 0)" \
      "$ICE" "$R" "$(chip 8)"
    printf '  %sW2%s %b %b %b %b      %sW6%s %b\n' \
      "$ICE" "$R" "$(chip 1)" "$(chip 2)" "$(chip 3)" "$(chip 4)" \
      "$ICE" "$R" "$(chip 9)"
    printf '  %sW3%s %b              %sW7%s %b\n' \
      "$ICE" "$R" "$(chip 5)" \
      "$ICE" "$R" "$(chip 10)"
    printf '  %sW4%s %b %b            %sW8%s %b\n' \
      "$ICE" "$R" "$(chip 6)" "$(chip 7)" \
      "$ICE" "$R" "$(chip 11)"
    echo
    # Minimal legend — one line
    echo -e "  ${DIM}${G}●${R}${DIM} done  ${Y}◐${R}${DIM} active  ◇ pending  ${RED}✕${R}${DIM} blocked${R}"
  } > "$PLAN_OUT.tmp"
  mv -f "$PLAN_OUT.tmp" "$PLAN_OUT"

  unset USAGE ALIVE CODEX_PANES
  declare -A USAGE ALIVE
  declare -a CODEX_PANES


  # ── 5b. render live-waves.txt — parallel execution model ───────────────────
  # Wave membership (matches plan dep DAG)
  declare -A WAVE_MEMBERS
  WAVE_MEMBERS[1]="0"
  WAVE_MEMBERS[2]="1 2 3 4"
  WAVE_MEMBERS[3]="5"
  WAVE_MEMBERS[4]="6 7"
  WAVE_MEMBERS[5]="8"
  WAVE_MEMBERS[6]="9"
  WAVE_MEMBERS[7]="10"
  WAVE_MEMBERS[8]="11"

  wave_marker_line() {
    local sub_csv="$1"
    local out=""
    for i in $sub_csv; do
      local s="${SUBST[$i]%%|*}"
      local pct="${SUBPCT[$i]:-0}"
      if [[ "$s" == "blocked" ]]; then
        out+="${RED}✕${R}"
      elif (( pct >= 100 )); then
        out+="${G}●${R}"
      elif [[ "$s" == "claimed" ]] || (( pct > 0 )); then
        out+="${Y}◐${R}"
      else
        out+="${DIM}◇${R}"
      fi
      out+=" "
    done
    echo -e "$out"
  }
  {
    echo -e "${B}${TEAL}WAVES${R}  ${D}parallel execution model · rust-ph13-14-15-completion${R}    ${D}${ts}${R}"
    echo
    printf '  %-3s %-7s %-30s %s\n' "" "tasks" "members" "status"
    echo "  ────────────────────────────────────────────────────────────────────────"
    for w in 1 2 3 4 5 6 7 8; do
      members="${WAVE_MEMBERS[$w]}"
      n=$(echo "$members" | wc -w)
      bar=""
      for ((i=0;i<n;i++)); do bar+="████"; done
      # padding bar so all rows align
      padlen=$(( 20 - n*4 ))
      pad=$(printf '%*s' "$padlen" '')
      # subtask list "sub-X,sub-Y,..."
      sub_list=""
      for i in $members; do sub_list+="sub-$i,"; done
      sub_list=${sub_list%,}
      # status chips
      chips=$(wave_marker_line "$members")
      # count completed in this wave
      wave_done=0
      for i in $members; do
        (( ${SUBPCT[$i]:-0} >= 100 )) && wave_done=$((wave_done+1))
      done
      if (( wave_done == n )); then
        wcol=$G
      elif (( wave_done > 0 )); then
        wcol=$Y
      else
        wcol=$DIM
      fi
      printf "  ${B}W%d${R}  ${wcol}%-4s${R}  %-30s  %b  %s${pad}\n" \
        "$w" "${n}t" "$sub_list" "$chips" "${wcol}${bar}${R}"
    done
    echo
    # Concurrency profile
    echo -e "  ${B}MAX PARALLEL${R}  ${G}W2 = 4${R}  ${DIM}(sub-1, sub-2, sub-3, sub-4 unblocked together)${R}"
    echo
    # Live claim count
    claimed_now=0
    for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
      [[ "${SUBST[$i]%%|*}" == "claimed" ]] && claimed_now=$((claimed_now+1))
    done
    completed_now=$done_count
    in_flight=$((claimed_now + completed_now))
    echo -e "  ${B}LIVE${R}      ${Y}claimed=${claimed_now}${R}  ${G}done=${completed_now}${R}  ${DIM}available=$((12 - in_flight))${R}"
    echo
    echo "  ────────────────────────────────────────────────────────────────────────"
    echo -e "  ${B}5-WORKER MAP${R}    ${DIM}(which codex on which wave-task)${R}"
    for aid in mesi-lebenyse matt-gg recodee-mite koncita-pipacs fico-magnolia; do
      key="codex-$aid"
      sub="${WORKER_SUB[$key]:-}"
      if [[ -n "$sub" ]]; then
        # Find wave for this sub
        cur_w="?"
        for w in 1 2 3 4 5 6 7 8; do
          for i in ${WAVE_MEMBERS[$w]}; do
            [[ "$i" == "$sub" ]] && cur_w=$w
          done
        done
        printf "    ${C}codex-%-15s${R} → ${B}W%s${R} ${Y}sub-%s${R}  ${DIM}%s${R}\n" \
          "$aid" "$cur_w" "$sub" "${SUB_TITLES[$sub]}"
      else
        printf "    ${C}codex-%-15s${R} ${DIM}polling…${R}\n" "$aid"
      fi
    done
    echo
    echo -e "  ${DIM}LEGEND  W#=wave  bar length ∝ parallelism  ●done  ◐claimed  ◇available  ✕blocked${R}"
    echo -e "  ${DIM}refresh=${INTERVAL}s${R}"
  } > "$WAVES_OUT.tmp"
  mv -f "$WAVES_OUT.tmp" "$WAVES_OUT"

  # ── 6. refresh tmux pane titles (codex auto-rename keeps overwriting) ──────
  if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    mapfile -t ACTIVE2 < "$ACTIVE_FILE" 2>/dev/null || ACTIVE2=()
    pane_i=0
    declare -a NODE_PANES
    while IFS='|' read -r pid pane_idx left cmd; do
      if [[ "$left" -gt 0 && "$cmd" == "node" ]]; then
        NODE_PANES+=("$pane_idx")
      fi
    done < <(tmux list-panes -t "$TMUX_SESSION:overview" -F '#{pane_id}|#{pane_index}|#{pane_left}|#{pane_current_command}' 2>/dev/null)
    for aid in "${ACTIVE2[@]}"; do
      [[ $pane_i -lt ${#NODE_PANES[@]} ]] || break
      pidx=${NODE_PANES[$pane_i]}
      agent_key="codex-$aid"
      sub="${WORKER_SUB[$agent_key]:-}"
      # Scrape pane scrollback for branch + PR URL + PR state
      ptail=$(tmux capture-pane -t "$TMUX_SESSION:overview.${pidx}" -p -S -300 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g')
      # Find last branch reference: either "branch=foo" or "agent/codex/<slug>" or "spec/<slug>/sub-N"
      branch=$(echo "$ptail" | grep -oE "(branch=|on )(agent/[^[:space:]]+|spec/[^[:space:]]+)" | tail -1 | sed -E 's/^(branch=|on )//')
      if [[ -z "$branch" ]]; then
        branch=$(echo "$ptail" | grep -oE "(agent/codex/[a-zA-Z0-9_-]+|spec/[a-zA-Z0-9_-]+/sub-[0-9]+|auto-plan-[a-zA-Z0-9-]+)" | tail -1)
      fi
      # Trim branch for display
      branch_short=$(echo "$branch" | sed -E 's|^agent/codex/||; s|^spec/||; s|(.{32}).*|\1…|')
      # Find last PR URL + nearby state word
      pr_url=$(echo "$ptail" | grep -oE "https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_-]+/pull/[0-9]+" | tail -1)
      pr_num=""
      pr_state=""
      if [[ -n "$pr_url" ]]; then
        pr_num=$(echo "$pr_url" | grep -oE "[0-9]+$")
        # State word in last 20 lines before the PR mention
        pr_ctx=$(echo "$ptail" | grep -B0 -A0 -E "pull/$pr_num|PR #?$pr_num|PR (merged|opened|closed|open|review)" | tail -20)
        if   echo "$pr_ctx" | grep -qiE "merged"; then pr_state="merged"
        elif echo "$pr_ctx" | grep -qiE "PR (opened|open)|opened PR|pr-open"; then pr_state="open"
        elif echo "$pr_ctx" | grep -qiE "closed"; then pr_state="closed"
        else pr_state="?"
        fi
      fi
      # Compose title
      if [[ -n "$sub" ]]; then
        title="[codex-${aid}] → sub-${sub}"
      else
        title="[codex-${aid}]"
      fi
      if [[ -n "$branch_short" ]]; then
        title="${title} · ${branch_short}"
      fi
      if [[ -n "$pr_num" ]]; then
        title="${title} · PR#${pr_num} ${pr_state}"
      fi
      # Fallback hint when nothing scraped
      if [[ -z "$sub" && -z "$branch_short" && -z "$pr_num" ]]; then
        title="${title} polling"
      fi
      tmux set-option -t "$TMUX_SESSION:overview.${pidx}" -p @panel "$title" 2>/dev/null || true
      pane_i=$((pane_i+1))
    done
    # Also re-pin the two viz panes by content sniff
    for pidx in $(tmux list-panes -t "$TMUX_SESSION:overview" -F '#{pane_index}'); do
      cmd=$(tmux display-message -t "$TMUX_SESSION:overview.$pidx" -p '#{pane_current_command}' 2>/dev/null)
      if [[ "$cmd" == "watch" ]]; then
        sample=$(tmux capture-pane -t "$TMUX_SESSION:overview.$pidx" -p -S -100 2>/dev/null | grep -m1 -oE 'CODEX-FLEET|rust-ph13-14-15' || echo "")
        case "$sample" in
          rust-ph13-14-15) tmux set-option -t "$TMUX_SESSION:overview.$pidx" -p @panel '[viz] plan-design' 2>/dev/null ;;
          CODEX-FLEET)     tmux set-option -t "$TMUX_SESSION:overview.$pidx" -p @panel '[viz] fleet-state' 2>/dev/null ;;
        esac
      fi
    done
    unset NODE_PANES; declare -a NODE_PANES
  fi

  sleep "$INTERVAL"
done
