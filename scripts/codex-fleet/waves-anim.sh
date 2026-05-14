#!/usr/bin/env bash
# waves-anim — calm animated WAVES v2 view for the codex-fleet waves tab.
#
# Design rules (matches plan-anim sibling):
#   - Calm 800ms tick
#   - Rounded iOS-style wave cards arranged in two columns
#   - Header/body/footer inside each card
#   - Claimed/in-flight chips pulse softly; idle cards stay static
#   - Changed-row repaint to avoid flicker
#
# Usage:
#   bash scripts/codex-fleet/waves-anim.sh           # loop
#   bash scripts/codex-fleet/waves-anim.sh --once    # one frame
set -eo pipefail

ONCE=0
INTERVAL_MS=800
for a in "$@"; do
  case "$a" in
    --once) ONCE=1 ;;
    --interval=*) INTERVAL_MS="${a#--interval=}" ;;
  esac
done
INTERVAL_S=$(awk -v ms="$INTERVAL_MS" 'BEGIN{printf "%.3f", ms/1000}')

REPO="${WAVES_ANIM_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}}"
PLAN_JSON="${WAVES_ANIM_PLAN_JSON:-$REPO/openspec/plans/rust-ph13-14-15-completion-2026-05-13/plan.json}"
PLAN_SLUG="$(basename "$(dirname "$PLAN_JSON")")"
ACTIVE_FILE="${WAVES_ANIM_ACTIVE_FILE:-/tmp/claude-viz/fleet-active-accounts.txt}"

# Wave membership (matches plan dep DAG)
declare -A WAVE_MEMBERS=(
  [1]="0"
  [2]="1 2 3 4"
  [3]="5"
  [4]="6 7"
  [5]="8"
  [6]="9"
  [7]="10"
  [8]="11"
)
# Phase per sub (0=PH13, 1=PH14, 2=PH15)
PHASE_OF=(0 0 0 0 0 1 1 1 1 2 2 2)

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

# ── palette ───────────────────────────────────────────────────────────────────
B=$'\033[1m'; D=$'\033[2m'; R=$'\033[0m'
IOS_BLUE=$'\033[38;2;0;122;255m'
IOS_GREEN=$'\033[38;2;52;199;89m'
IOS_RED=$'\033[38;2;255;59;48m'
IOS_ORANGE=$'\033[38;2;255;149;0m'
IOS_YELLOW=$'\033[38;2;255;204;0m'
IOS_GRAY=$'\033[38;2;142;142;147m'
IOS_GRAY2=$'\033[38;2;174;174;178m'
IOS_GRAY6=$'\033[38;2;242;242;247m'
IOS_WHITE=$'\033[38;2;255;255;255m'
DIM="$IOS_GRAY"
WHITE="$IOS_WHITE"
TEAL="$IOS_BLUE"
ICE="$IOS_BLUE"
MAG="$IOS_ORANGE"
RED="$IOS_RED"
GRAD=(
  "$IOS_RED"
  "$IOS_RED"
  "$IOS_ORANGE"
  "$IOS_YELLOW"
  "$IOS_YELLOW"
  "$IOS_GREEN"
  "$IOS_GREEN"
)
CARD_WIDTH=42

load_state() {
  declare -gA SUBST
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    local s="available" a=""
    if [[ -f "$PLAN_JSON" ]]; then
      read -r s a < <(
        jq -r --argjson idx "$i" \
          '.tasks[] | select(.subtask_index==$idx) | "\(.status // "available")\t\(.claimed_by_agent // "")"' \
          "$PLAN_JSON" 2>/dev/null | head -1 | tr '\t' ' '
      )
      [[ -z "$s" ]] && s="available"
    fi
    [[ -e "$REPO/${SUB_EVIDENCE[$i]}" ]] && s="completed"
    SUBST[$i]="${s}|${a}"
  done
}

load_workers() {
  declare -gA WORKER_OF
  if [[ -f "$PLAN_JSON" ]]; then
    while IFS=$'\t' read -r idx agent; do
      [[ -n "$agent" && "$agent" != "null" ]] && WORKER_OF[$idx]="$agent"
    done < <(jq -r '.tasks[] | select(.claimed_by_agent != null) | "\(.subtask_index)\t\(.claimed_by_agent)"' "$PLAN_JSON" 2>/dev/null)
  fi
}

# Color the W# label by progress within the wave
wave_color() {
  local done_n="$1" total="$2"
  local pct=0; (( total > 0 )) && pct=$(( done_n * 100 / total ))
  if   (( pct >= 90 )); then printf '%s' "${GRAD[6]}"
  elif (( pct >= 75 )); then printf '%s' "${GRAD[5]}"
  elif (( pct >= 60 )); then printf '%s' "${GRAD[4]}"
  elif (( pct >= 45 )); then printf '%s' "${GRAD[3]}"
  elif (( pct >= 30 )); then printf '%s' "${GRAD[2]}"
  elif (( pct >= 15 )); then printf '%s' "${GRAD[1]}"
  else                       printf '%s' "${GRAD[0]}"
  fi
}

strip_ansi() {
  sed -E $'s/\x1B\\[[0-9;]*m//g' <<<"${1:-}"
}

visible_len() {
  local clean
  clean=$(strip_ansi "${1:-}")
  printf '%d' "${#clean}"
}

pulse_color() {
  local f="$1"
  if (( (f / 2) % 2 == 0 )); then
    printf '%s' "$IOS_BLUE"
  else
    printf '%s%s' "$D" "$IOS_BLUE"
  fi
}

status_color() {
  local s="$1" f="$2"
  case "$s" in
    completed) printf '%s' "$IOS_GREEN" ;;
    claimed)   pulse_color "$f" ;;
    blocked)   printf '%s' "$IOS_RED" ;;
    *)         printf '%s' "$IOS_GRAY2" ;;
  esac
}

# Rounded status chip for a single sub-task. Only claimed chips pulse.
chip() {
  local i="$1" f="$2"
  local s="${SUBST[$i]%%|*}"
  local col; col=$(status_color "$s" "$f")
  printf '%s◖sub-%s◗%s' "$col" "$i" "$R"
}

# Mini iOS progress rail for a wave footer.
fill_bar() {
  local n="$1" total="$2" width="$3"
  local col_idx pct=0; (( total > 0 )) && pct=$(( n * 100 / total ))
  if   (( pct >= 90 )); then col_idx=6
  elif (( pct >= 75 )); then col_idx=5
  elif (( pct >= 60 )); then col_idx=4
  elif (( pct >= 45 )); then col_idx=3
  elif (( pct >= 30 )); then col_idx=2
  elif (( pct >= 15 )); then col_idx=1
  else                       col_idx=0
  fi
  local col="${GRAD[$col_idx]}"
  local filled=0
  (( total > 0 )) && filled=$(( n * width / total ))
  (( filled < 0 )) && filled=0
  (( filled > width )) && filled=$width
  local out="${WHITE}▕${R}${col}"
  local i
  for ((i=0;i<filled;i++)); do out+="█"; done
  out+="${DIM}"
  for ((i=filled;i<width;i++)); do out+="░"; done
  out+="${WHITE}▏${R}"
  printf '%s' "$out"
}

# Strip of chips for a wave's members
wave_chips() {
  local f="$1"; shift
  local out=""
  for i in "$@"; do
    out+="$(chip "$i" "$f") "
  done
  printf '%s' "$out"
}

card_line() {
  local content="${1:-}" width="${2:-$CARD_WIDTH}"
  local content_width=$(( width - 4 ))
  local len pad
  len=$(visible_len "$content")
  pad=$(( content_width - len ))
  (( pad < 0 )) && pad=0
  printf '%s│%s %s%*s %s│%s' "$IOS_GRAY2" "$R" "$content" "$pad" "" "$IOS_GRAY2" "$R"
}

card_rule() {
  local top="${1:-1}" width="${2:-$CARD_WIDTH}"
  local fill_len=$(( width - 2 ))
  local fill
  printf -v fill '%*s' "$fill_len" ""
  fill=${fill// /─}
  if [[ "$top" == "1" ]]; then
    printf '%s╭%s╮%s' "$IOS_GRAY2" "$fill" "$R"
  else
    printf '%s╰%s╯%s' "$IOS_GRAY2" "$fill" "$R"
  fi
}

wave_status_label() {
  local done_n="$1" total="$2" claimed_n="$3" f="$4"
  if (( done_n == total )); then
    printf '%sdone%s' "$IOS_GREEN" "$R"
  elif (( claimed_n > 0 )); then
    printf '%sin flight%s' "$(pulse_color "$f")" "$R"
  elif (( done_n > 0 )); then
    printf '%spartial%s' "$IOS_ORANGE" "$R"
  else
    printf '%swaiting%s' "$IOS_GRAY2" "$R"
  fi
}

wave_card() {
  local w="$1" f="$2"
  local members="${WAVE_MEMBERS[$w]}"
  local total=0 done_n=0 claimed_n=0
  local i
  for i in $members; do
    total=$((total+1))
    case "${SUBST[$i]%%|*}" in
      completed) done_n=$((done_n+1)) ;;
      claimed)   claimed_n=$((claimed_n+1)) ;;
    esac
  done
  local wc; wc=$(wave_color "$done_n" "$total")
  local task_word="tasks"
  [[ "$total" == "1" ]] && task_word="task"
  local status; status=$(wave_status_label "$done_n" "$total" "$claimed_n" "$f")
  local chips; chips=$(wave_chips "$f" $members)
  local rail; rail=$(fill_bar "$done_n" "$total" 12)

  card_rule 1
  printf '\n'
  card_line "${B}${wc}W${w}${R} ${DIM}·${R} ${total} ${task_word} ${DIM}·${R} ${status}"
  printf '\n'
  card_line "$chips"
  printf '\n'
  card_line "${DIM}${done_n}/${total}${R} ${rail}"
  printf '\n'
  card_rule 0
  printf '\n'
}

render_card_pair() {
  local left_w="$1" right_w="$2" f="$3"
  local -a left=() right=()
  mapfile -t left < <(wave_card "$left_w" "$f")
  mapfile -t right < <(wave_card "$right_w" "$f")
  local i
  for i in "${!left[@]}"; do
    printf '  %s  %s\n' "${left[$i]}" "${right[$i]}"
  done
}

declare -a PREV_FRAME=()

paint_frame() {
  local frame="$1"
  local -a lines=()
  mapfile -t lines <<< "$frame"

  local i
  for i in "${!lines[@]}"; do
    if [[ "${PREV_FRAME[$i]-}" != "${lines[$i]}" ]]; then
      printf '\033[%d;1H%s\033[K' "$((i + 1))" "${lines[$i]}"
    fi
  done
  for ((i=${#lines[@]}; i<${#PREV_FRAME[@]}; i++)); do
    printf '\033[%d;1H\033[K' "$((i + 1))"
  done
  PREV_FRAME=("${lines[@]}")
}

render() {
  local f="$1"
  local ts; ts=$(date '+%H:%M:%S')
  load_state
  load_workers

  local total_done=0 total_claimed=0
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    case "${SUBST[$i]%%|*}" in
      completed) total_done=$((total_done+1)) ;;
      claimed)   total_claimed=$((total_claimed+1)) ;;
    esac
  done
  local total_available=$(( 12 - total_done - total_claimed ))

  printf '%s%sWAVES · parallel execution · live%s  %s%s%s  %s%s%s\n' \
    "$B" "$TEAL" "$R" "$WHITE" "$PLAN_SLUG" "$R" "$DIM" "$ts" "$R"
  printf '\n'

  render_card_pair 1 5 "$f"
  render_card_pair 2 6 "$f"
  render_card_pair 3 7 "$f"
  render_card_pair 4 8 "$f"

  # Concurrency profile + live counts
  printf '  %sMAX PARALLEL%s  %sW2 = 4%s   %s(sub-1, sub-2, sub-3, sub-4 unblocked together)%s\n' \
    "$B" "$R" "${GRAD[6]}" "$R" "$DIM" "$R"
  printf '  %sLIVE%s          %sclaimed=%s%s%s%d%s   %sdone=%s%s%s%d%s   %savailable=%s%s%s%d%s\n' \
    "$B" "$R" \
    "$DIM" "$R" "$B" "${GRAD[3]}" "$total_claimed" "$R" \
    "$DIM" "$R" "$B" "${GRAD[6]}" "$total_done" "$R" \
    "$DIM" "$R" "$B" "$DIM" "$total_available" "$R"
  printf '\n'

  # 5-worker map: which codex is on which wave-task
  printf '  %s5-WORKER MAP%s   %s(which codex on which wave-task)%s\n' "$B" "$R" "$DIM" "$R"
  local seen_workers=0
  if [[ -f "$ACTIVE_FILE" ]]; then
    while IFS= read -r aid; do
      [[ -z "$aid" ]] && continue
      seen_workers=$((seen_workers+1))
      local key="codex-$aid"
      local sub=""
      for k in "${!WORKER_OF[@]}"; do
        if [[ "${WORKER_OF[$k]}" == "$key" ]]; then sub="$k"; break; fi
      done
      if [[ -n "$sub" ]]; then
        # find wave for this sub
        local cur_w="?"
        for w in 1 2 3 4 5 6 7 8; do
          for i in ${WAVE_MEMBERS[$w]}; do
            [[ "$i" == "$sub" ]] && cur_w=$w
          done
        done
        printf '    %scodex-%-16s%s   %s→%s  %s%sW%s%s %s●%s sub-%s  %s%s%s\n' \
          "$ICE" "$aid" "$R" "$DIM" "$R" \
          "$B" "$ICE" "$cur_w" "$R" \
          "$IOS_BLUE" "$R" "$sub" \
          "$DIM" "${SUB_TITLES[$sub]}" "$R"
      else
        printf '    %scodex-%-16s%s   %s·%s  %spolling…%s\n' \
          "$ICE" "$aid" "$R" "$DIM" "$R" "$DIM" "$R"
      fi
    done < "$ACTIVE_FILE"
  fi
  if (( seen_workers == 0 )); then
    printf '    %s(no active workers — start fleet via scripts/codex-fleet/up.sh)%s\n' "$DIM" "$R"
  fi
  printf '\n'

  # Legend
  printf '  %sLEGEND%s  %s◖sub-N◗%s chip   %s◾%s rail   %sdone%s   %sclaimed pulse%s   %savailable%s   %sblocked%s\n' \
    "$DIM" "$R" "$IOS_GRAY2" "$R" "$IOS_GREEN" "$R" "$IOS_GREEN" "$R" "$IOS_BLUE" "$R" "$IOS_GRAY2" "$R" "$RED" "$R"
  printf '  %srefresh=%dms%s\n' "$DIM" "$INTERVAL_MS" "$R"
}

if (( ONCE == 1 )); then
  render 0
else
  printf '\033[?25l'
  trap 'printf "\033[?25h"; exit' INT TERM EXIT
  f=0
  while true; do
    paint_frame "$(render "$f")"
    f=$((f+1))
    sleep "$INTERVAL_S"
  done
fi
