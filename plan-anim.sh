#!/usr/bin/env bash
# plan-anim — calm animated PLAN v2 view for the codex-fleet plan tab.
#
# Design rules:
#   - Calm 800ms tick, no rapid flicker
#   - iOS system palette with segmented progress rails
#   - Only claimed/in-flight segments pulse softly
#   - No marching arrows or idle color cycling
#   - In-place repaint only touches changed rows
#
# Usage:
#   bash scripts/codex-fleet/plan-anim.sh           # loop
#   bash scripts/codex-fleet/plan-anim.sh --once    # one frame
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

REPO="${PLAN_ANIM_REPO:-/home/deadpool/Documents/recodee}"
PLAN_JSON="${PLAN_ANIM_PLAN_JSON:-$REPO/openspec/plans/rust-ph13-14-15-completion-2026-05-13/plan.json}"
PLAN_SLUG="$(basename "$(dirname "$PLAN_JSON")")"

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
# Sub-task → phase (0=PH13, 1=PH14, 2=PH15)
PHASE_OF=(0 0 0 0 0 1 1 1 1 2 2 2)

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

# Load plan state
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

# Static marker; spinner only spins for `claimed`
marker() {
  local i="$1" f="$2"
  local s="${SUBST[$i]%%|*}"
  case "$s" in
    completed) printf '%s●%s' "${GRAD[6]}" "$R" ;;
    claimed)   printf '%s●%s' "$IOS_BLUE" "$R" ;;
    blocked)   printf '%s✕%s' "$RED" "$R" ;;
    *)         printf '%s◇%s' "$DIM" "$R" ;;
  esac
}

label() {
  local i="$1"
  local s="${SUBST[$i]%%|*}" a="${SUBST[$i]##*|}"
  case "$s" in
    completed) printf '%s%s%s %sdone%s' "${GRAD[6]}" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R" ;;
    claimed)   if [[ -n "$a" && "$a" != "null" ]]; then
                 printf '%s%s%s %s←%s %s%s%s' "$IOS_BLUE" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R" "$ICE" "$a" "$R"
               else
                 printf '%s%s%s %sclaimed%s' "$IOS_BLUE" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R"
               fi ;;
    blocked)   printf '%s%s%s %sblocked%s' "$RED" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R" ;;
    *)         printf '%s%s%s' "$DIM" "${SUB_TITLES[$i]}" "$R" ;;
  esac
}

# Static dependency arrow — no marching motion
arrow() {
  printf '%s───►%s' "$DIM" "$R"
}

# Fill bar for an int N out of TOTAL, width=W
fill_bar() {
  local n="$1" total="$2" width="$3"
  local pct=0; (( total > 0 )) && pct=$(( n * 100 / total ))
  local col_idx
  if   (( pct >= 90 )); then col_idx=6
  elif (( pct >= 75 )); then col_idx=5
  elif (( pct >= 60 )); then col_idx=4
  elif (( pct >= 45 )); then col_idx=3
  elif (( pct >= 30 )); then col_idx=2
  elif (( pct >= 15 )); then col_idx=1
  else                       col_idx=0
  fi
  local col="${GRAD[$col_idx]}"
  local filled=$(( n * width / total ))
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

# Segment color for the iOS rail. Only claimed segments use frame-dependent
# dimming; completed/available/blocked stay static to keep motion calm.
segment_color() {
  local s="$1" f="$2"
  case "$s" in
    completed) printf '%s' "$IOS_GREEN" ;;
    claimed)
      if (( (f / 2) % 2 == 0 )); then
        printf '%s' "$IOS_BLUE"
      else
        printf '%s%s' "$D" "$IOS_BLUE"
      fi
      ;;
    blocked) printf '%s' "$IOS_RED" ;;
    *)       printf '%s' "$IOS_GRAY2" ;;
  esac
}

phase_segment_bar() {
  local phase="$1" f="$2" label="$3"
  local out="${B}${label}${R} ${DIM}|${R}"
  local i
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    [[ "${PHASE_OF[$i]}" == "$phase" ]] || continue
    local s="${SUBST[$i]%%|*}"
    out+="$(segment_color "$s" "$f")◾${R}"
  done
  out+="${DIM}|${R}"
  printf '%s' "$out"
}

phase_count_label() {
  local phase="$1" done="$2" total="$3" claimed="$4"
  printf '%sPH%s%s %s%d/%d%s' "$DIM" "$phase" "$R" "$IOS_GREEN" "$done" "$total" "$R"
  if (( claimed > 0 )); then
    printf ' %s+%d live%s' "$IOS_BLUE" "$claimed" "$R"
  fi
}

# PLAN CLOSE label is static; the only animated element is the claimed segment
# pulse in phase_segment_bar.
close_label() {
  printf '%s✦ %sPLAN CLOSE%s ✦%s' "$DIM" "$WHITE" "$R" "$DIM"
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

  local done_n=0 claimed_n=0
  local ph_done=(0 0 0) ph_claimed=(0 0 0) ph_total=(0 0 0)
  for i in 0 1 2 3 4 5 6 7 8 9 10 11; do
    local p="${PHASE_OF[$i]}"
    ph_total[$p]=$(( ph_total[p] + 1 ))
    case "${SUBST[$i]%%|*}" in
      completed) done_n=$((done_n+1));    ph_done[$p]=$((    ph_done[p]    + 1 )) ;;
      claimed)   claimed_n=$((claimed_n+1)); ph_claimed[$p]=$(( ph_claimed[p] + 1 )) ;;
    esac
  done

  printf '%s%s╭ PLAN%s  %s%s%s   %slive%s   %s%s%s\n' \
    "$B" "$TEAL" "$R" "$WHITE" "$PLAN_SLUG" "$R" "${GRAD[6]}" "$R" "$DIM" "$ts" "$R"
  printf '\n'

  # iOS segmented progress: one calm rail, one segment per sub-task.
  printf '  %sSEGMENTS%s  %s   %s   %s\n' \
    "$B" "$R" "$(phase_segment_bar 0 "$f" PH13)" "$(phase_segment_bar 1 "$f" PH14)" "$(phase_segment_bar 2 "$f" PH15)"
  printf '  %s    %s    %s\n\n' \
    "$(phase_count_label 13 "${ph_done[0]}" "${ph_total[0]}" "${ph_claimed[0]}")" \
    "$(phase_count_label 14 "${ph_done[1]}" "${ph_total[1]}" "${ph_claimed[1]}")" \
    "$(phase_count_label 15 "${ph_done[2]}" "${ph_total[2]}" "${ph_claimed[2]}")"

  # Wave tree (static structure, only markers change with state)
  printf '  %sW1%s  ┌─ %s sub-0   %s\n' "$ICE" "$R" "$(marker 0 "$f")" "$(label 0)"
  printf '      │\n'
  printf '  %sW2%s  ├─ %s sub-1   %-26s  %s  %sW3%s %s sub-5   %s\n' \
    "$ICE" "$R" "$(marker 1 "$f")" "$(label 1)" "$(arrow)" "$ICE" "$R" "$(marker 5 "$f")" "$(label 5)"
  printf '      │                                                  │\n'
  printf '      ├─ %s sub-2   %-26s            %s  %sW4%s %s sub-6   %s\n' \
    "$(marker 2 "$f")" "$(label 2)" "$(arrow)" "$ICE" "$R" "$(marker 6 "$f")" "$(label 6)"
  printf '      │                                                  │       │\n'
  printf '      ├─ %s sub-3   %-26s            │       %s  %sW5%s %s sub-8   %s\n' \
    "$(marker 3 "$f")" "$(label 3)" "$(arrow)" "$ICE" "$R" "$(marker 8 "$f")" "$(label 8)"
  printf '      │                                                  │\n'
  printf '      └─ %s sub-4   %-26s            %s  %sW4%s %s sub-7   %s\n' \
    "$(marker 4 "$f")" "$(label 4)" "$(arrow)" "$ICE" "$R" "$(marker 7 "$f")" "$(label 7)"
  printf '                                                         │\n'
  printf '                          %s┌──────────────────────────┘%s\n' "$DIM" "$R"
  printf '                          │\n'
  printf '                  %sW6%s %s sub-9   %s\n' "$ICE" "$R" "$(marker 9 "$f")" "$(label 9)"
  printf '                          │\n'
  printf '                  %sW7%s %s sub-10  %-26s  %s◄ needs 2,3,4,9%s\n' \
    "$ICE" "$R" "$(marker 10 "$f")" "$(label 10)" "$DIM" "$R"
  printf '                          │\n'
  printf '                  %sW8%s %s sub-11  %-26s  %s◄ needs all%s\n' \
    "$ICE" "$R" "$(marker 11 "$f")" "$(label 11)" "$DIM" "$R"
  printf '                          │\n'
  printf '                          ▼ %s\n\n' "$(close_label)"

  # Total progress bar — bigger (60 cells)
  local total_bar; total_bar=$(fill_bar "$done_n" 12 60)
  local pct=$(( done_n * 100 / 12 ))
  local pct_col_idx
  if   (( pct >= 90 )); then pct_col_idx=6
  elif (( pct >= 75 )); then pct_col_idx=5
  elif (( pct >= 60 )); then pct_col_idx=4
  elif (( pct >= 45 )); then pct_col_idx=3
  elif (( pct >= 30 )); then pct_col_idx=2
  elif (( pct >= 15 )); then pct_col_idx=1
  else                       pct_col_idx=0
  fi
  printf '  %sTOTAL%s    %s   %s%d/12%s   %s%d%%%s\n\n' \
    "$B" "$R" "$total_bar" "$B" "$done_n" "$R" "${GRAD[$pct_col_idx]}" "$pct" "$R"

  # Legend
  printf '  %sLEGEND%s  %s◾%s done   %s◾%s claimed pulse   %s◾%s blocked   %s◾%s available\n' \
    "$DIM" "$R" "$IOS_GREEN" "$R" "$IOS_BLUE" "$R" "$RED" "$R" "$IOS_GRAY2" "$R"

  # Live worker mapping (only if there are any)
  if (( ${#WORKER_OF[@]} > 0 )); then
    printf '  %sWORKERS%s  ' "$DIM" "$R"
    local first=1
    for idx in "${!WORKER_OF[@]}"; do
      (( first )) || printf '  '
      first=0
      printf '%ssub-%s%s %s←%s %s%s%s' \
        "${GRAD[3]}" "$idx" "$R" "$DIM" "$R" "$ICE" "${WORKER_OF[$idx]}" "$R"
    done
    printf '\n'
  fi
  printf '  %sclaimed=%s%s%s%d%s   %sdone=%s%s%s%d%s   %srefresh=%dms%s\n' \
    "$DIM" "$R" "$B" "${GRAD[3]}" "$claimed_n" "$R" \
    "$DIM" "$R" "$B" "${GRAD[6]}" "$done_n" "$R" \
    "$DIM" "$INTERVAL_MS" "$R"
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
