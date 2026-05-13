#!/usr/bin/env bash
# fleet-tick — live viz daemon (v2 graphical).
#   /tmp/claude-viz/live-fleet-state.txt   per-account 5h/weekly + worker→subtask
#   /tmp/claude-viz/live-plan-design.txt   wave tree of plan sub-tasks + claims
# Stop: kill $(cat /tmp/claude-viz/fleet-tick.pid)
set -eo pipefail

INTERVAL="${FLEET_TICK_INTERVAL:-5}"
TMUX_SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
REPO=/home/deadpool/Documents/recodee
PLAN_JSON="$REPO/openspec/plans/rust-ph13-14-15-completion-2026-05-13/plan.json"
STATE_OUT=/tmp/claude-viz/live-fleet-state.txt
PLAN_OUT=/tmp/claude-viz/live-plan-design.txt
WAVES_OUT=/tmp/claude-viz/live-waves.txt
ACTIVE_FILE=/tmp/claude-viz/fleet-active-accounts.txt
PID_FILE=/tmp/claude-viz/fleet-tick.pid

mkdir -p /tmp/claude-viz
echo $$ > "$PID_FILE"

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

# Sub-task evidence files (anyOf semantics: file exists ⇒ done)
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

# Determine subtask claim from plan.json (jq fast path)
# Returns:  status|claimed_agent  (status=done if evidence exists, else from plan.json)
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
  if [[ -e "$REPO/$evidence" ]]; then
    final_status="completed"
  fi
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

  # ── 4. render live-fleet-state.txt ─────────────────────────────────────────
  {
    echo -e "${B}${TEAL}╭─ CODEX-FLEET LIVE STATE ─────────────────────────────────╮${R}    ${D}${ts}${R}"
    printf "  ${B}${TEAL}%-12s  %-7s  %-8s  %-11s  %s${R}\n" "ACCOUNT" "5h" "WEEKLY" "WORKER" "WORKING ON"
    echo -e "  ${TEAL}─────────────────────────────────────────────────────────────────────${R}"
    n_alive=0
    for email in "${FLEET_EMAILS[@]}"; do
      pair="${USAGE[$email]:-- -}"
      h5=${pair%% *}; wk=${pair##* }
      aid=${AID[$email]}
      st=${ALIVE[$aid]:-}
      # Usage colors — gradient: 0%=red → 50%=yellow → 100%=green
      wk_num=${wk%\%}; h5_num=${h5%\%}
      [[ "$wk_num" =~ ^[0-9]+$ ]] || wk_num=0
      [[ "$h5_num" =~ ^[0-9]+$ ]] || h5_num=0
      wkc=$(pct_color "$wk_num"); h5c=$(pct_color "$h5_num")
      wk_bar=$(pct_spark "$wk_num"); h5_bar=$(pct_spark "$h5_num")
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
          elif echo "$tail_clean" | tail -8 | grep -qE "^› (Find and fix|Use /skills|Run /review|Improve documentation|Implement|Summarize|Explain|Write tests)"; then
            working="${DIM}idle (default prompt)${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Reviewing approval request" | head -1); [[ -n "$w" ]]; then
            cmd_being_approved=$(echo "$tail_clean" | tail -8 | grep -oE "└ [^[:space:]].*" | head -1 | sed 's/└ //; s/.\{60\}.*/…/')
            working="${Y}⏸ approval: ${cmd_being_approved}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Working \([0-9]+[ms][^)]*\)" | tail -1); [[ -n "$w" ]]; then
            secs=$(echo "$w" | grep -oE "[0-9]+[ms]" | head -1)
            working="${G}⚡ working ${secs}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Worked for [0-9]+m[^─]*" | tail -1); [[ -n "$w" ]]; then
            secs=$(echo "$w" | grep -oE "[0-9]+m [0-9]+s" | head -1)
            working="${G}✓ worked ${secs}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Calling [a-zA-Z_]+\.[a-zA-Z_]+" | tail -1); [[ -n "$w" ]]; then
            working="${C}● ${w}${R}"
          elif w=$(echo "$tail_clean" | tail -10 | grep -oE "Ran [a-z_]+" | tail -1); [[ -n "$w" ]]; then
            working="${C}● ${w}${R}"
          else
            working="${DIM}polling…${R}"
          fi
          unset NP; declare -a NP=()
        fi
      fi
      printf "  ${B}%-12s${R}  ${h5c}%-4s %s${R}  ${wkc}%-5s %s${R}  %b  %b\n" \
        "${SHORT[$email]:-$email}" "$h5" "$h5_bar" "$wk" "$wk_bar" "$live" "$working"
    done
    echo -e "  ${TEAL}─────────────────────────────────────────────────────────────────────${R}"
    # color the active-workers count by saturation (5/5 green, 0/5 dim)
    if   (( n_alive >= 5 )); then awc=$GRAD6
    elif (( n_alive >= 3 )); then awc=$GRAD4
    elif (( n_alive >= 1 )); then awc=$GRAD3
    else                          awc=$DIM; fi
    echo -e "  ${DIM}active workers=${R}${B}${awc}${n_alive}/5${R}   ${DIM}refresh=${INTERVAL}s   tick=$$${R}"
    echo -e "${TEAL}╰──────────────────────────────────────────────────────────╯${R}"
  } > "$STATE_OUT.tmp"
  mv -f "$STATE_OUT.tmp" "$STATE_OUT"

  # ── 5. render live-plan-design.txt (graphical wave tree) ───────────────────
  # Precompute subtask state[i] = "status|agent"
  declare -A SUBST
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    SUBST[$i]=$(load_subtask_state "$i")
  done
  marker() {
    local i="$1"; local s="${SUBST[$i]%%|*}"
    case "$s" in
      completed) echo -e "${G}●${R}" ;;
      claimed)   echo -e "${Y}◐${R}" ;;
      blocked)   echo -e "${RED}✕${R}" ;;
      *)         echo -e "${DIM}◇${R}" ;;
    esac
  }
  label() {
    local i="$1"; local s="${SUBST[$i]%%|*}"; local a="${SUBST[$i]##*|}"
    case "$s" in
      completed) echo -e "${G}${SUB_TITLES[$i]}${R} ${DIM}done${R}" ;;
      claimed)   if [[ -n "$a" && "$a" != "null" ]]; then echo -e "${Y}${SUB_TITLES[$i]}${R} ${DIM}←${R} ${C}$a${R}";
                 else echo -e "${Y}${SUB_TITLES[$i]}${R} ${DIM}claimed${R}"; fi ;;
      *)         echo -e "${DIM}${SUB_TITLES[$i]}${R}" ;;
    esac
  }
  done_count=0
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    [[ "${SUBST[$i]%%|*}" == "completed" ]] && done_count=$((done_count+1))
  done

  {
    echo -e "${B}${TEAL}PLAN${R}  ${D}rust-ph13-14-15-completion-2026-05-13${R}            ${D}${ts}${R}"
    echo
    printf '       ${M}%-26s${R}   ${M}%-22s${R}   ${M}%-14s${R}\n' \
      "PH13  ROLLBACK DRILLS" "PH14  ROLLOUT GATES" "PH15  DECOMM" \
      | sed 's/\${M}/'"$M"'/g; s/\${R}/'"$R"'/g'
    echo "       ─────────────────────       ──────────────────       ────────────"
    echo

    # Row W1
    echo -e "  W1  ┌─ $(marker 0) sub-0  $(label 0)"
    echo -e "      │"
    # Row W2 with arrows into PH14
    echo -e "  W2  ├─ $(marker 1) sub-1  $(label 1)  ${DIM}──►${R}  W3 $(marker 5) sub-5  $(label 5)"
    echo -e "      │                                                  │"
    echo -e "      ├─ $(marker 2) sub-2  $(label 2)                       ${DIM}├─►${R} W4 $(marker 6) sub-6  $(label 6)"
    echo -e "      │                                                  │       │"
    echo -e "      ├─ $(marker 3) sub-3  $(label 3)                       │       ${DIM}└─►${R} W5 $(marker 8) sub-8  $(label 8)"
    echo -e "      │                                                  │"
    echo -e "      └─ $(marker 4) sub-4  $(label 4)                       ${DIM}└─►${R} W4 $(marker 7) sub-7  $(label 7)"
    echo -e "                                                                  │"
    echo -e "                                  ${DIM}┌─────────────────────────────────┘${R}"
    echo -e "                                  │"
    echo -e "                          W6 $(marker 9) sub-9  $(label 9)"
    echo -e "                                  │"
    echo -e "                          W7 $(marker 10) sub-10 $(label 10) ${DIM}◄ needs 2,3,4,9${R}"
    echo -e "                                  │"
    echo -e "                          W8 $(marker 11) sub-11 $(label 11) ${DIM}◄ needs all${R}"
    echo -e "                                  │"
    echo -e "                                  ▼ ${B}${G}PLAN CLOSE${R}"
    echo
    # progress bar
    bar_len=50
    filled=$(( done_count * bar_len / 12 ))
    bar=""
    for ((i=0;i<filled;i++)); do bar+="█"; done
    for ((i=filled;i<bar_len;i++)); do bar+="░"; done
    pct=$(( done_count * 100 / 12 ))
    echo -e "  progress  ${G}${bar}${R}  ${B}${done_count}/12${R}  ${DIM}(${pct}%)${R}"
    echo
    echo -e "  ${DIM}LEGEND ${G}●${R}${DIM} done   ${Y}◐${R}${DIM} claimed   ${RED}✕${R}${DIM} blocked   ◇ available   ◆ finalizer${R}"
    echo -e "  ${DIM}GATE   PH12 still [>] — checkbox flips need PH12 green${R}"
    echo -e "  ${DIM}refresh=${INTERVAL}s${R}"
  } > "$PLAN_OUT.tmp"
  mv -f "$PLAN_OUT.tmp" "$PLAN_OUT"

  unset USAGE ALIVE SUBST CODEX_PANES
  declare -A USAGE ALIVE SUBST
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
      case "$s" in
        completed) out+="${G}●${R}" ;;
        claimed)   out+="${Y}◐${R}" ;;
        blocked)   out+="${RED}✕${R}" ;;
        *)         out+="${DIM}◇${R}" ;;
      esac
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
        [[ "${SUBST[$i]%%|*}" == "completed" ]] && wave_done=$((wave_done+1))
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
