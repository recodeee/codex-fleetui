#!/usr/bin/env bash
# plan-anim — calm animated PLAN v2 view for the codex-fleet plan tab.
#
# Design rules (after eye-strain feedback):
#   - Slow tick (~600ms), no rapid flicker
#   - Spinner only renders when a sub-task is actually claimed
#   - No marching arrows — static colored arrows
#   - Title bar / PLAN CLOSE color cycle at ~3s cadence, not per frame
#   - Per-phase progress bars (PH13 / PH14 / PH15) so progress reads at-a-glance
#
# Usage:
#   bash scripts/codex-fleet/plan-anim.sh           # loop
#   bash scripts/codex-fleet/plan-anim.sh --once    # one frame
set -eo pipefail

ONCE=0
INTERVAL_MS=1000
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
DIM=$'\033[38;5;240m'
WHITE=$'\033[38;5;253m'
TEAL=$'\033[38;5;73m'
ICE=$'\033[38;5;117m'
MAG=$'\033[38;5;176m'
RED=$'\033[38;5;203m'
GRAD=(
  $'\033[38;5;203m'  # red
  $'\033[38;5;209m'  # red-orange
  $'\033[38;5;215m'  # orange
  $'\033[38;5;221m'  # yellow
  $'\033[38;5;185m'  # yellow-green
  $'\033[38;5;150m'  # lime
  $'\033[38;5;83m'   # bright green
)
BG_PH13=$'\033[48;5;94m'
BG_PH14=$'\033[48;5;24m'
BG_PH15=$'\033[48;5;52m'

# Calm spinner (only used on claimed tasks). Slower cycle (advances every 2 ticks).
SPINNER=(◐ ◓ ◑ ◒)

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
    claimed)   printf '%s%s%s' "${GRAD[3]}" "${SPINNER[$(( (f / 2) % 4 ))]}" "$R" ;;
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
                 printf '%s%s%s %s←%s %s%s%s' "${GRAD[3]}" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R" "$ICE" "$a" "$R"
               else
                 printf '%s%s%s %sclaimed%s' "${GRAD[3]}" "${SUB_TITLES[$i]}" "$R" "$DIM" "$R"
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

# PLAN CLOSE label — static dim when nothing's happening, only colors when
# the plan has motion (any claimed or done). Removes the idle color cycle.
close_label() {
  local f="$1" active="$2"
  if (( active > 0 )); then
    local idx=$(( (f / 8) % 7 ))   # slow cycle: ~8s per stop
    printf '%s✦%s %s%sPLAN CLOSE%s %s✦%s' "${GRAD[$idx]}" "$R" "$B" "${GRAD[6]}" "$R" "${GRAD[$idx]}" "$R"
  else
    printf '%s✦ %sPLAN CLOSE%s ✦%s' "$DIM" "$WHITE" "$R" "$DIM"
  fi
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

  # Cursor home only (no full clear) → in-place repaint, no flicker.
  # Each line ends with \033[K to wipe any leftover characters from prior frame.
  printf '\033[H'
  printf '%s%s╭ PLAN%s  %s%s%s   %slive%s   %s%s%s\n' \
    "$B" "$TEAL" "$R" "$WHITE" "$PLAN_SLUG" "$R" "${GRAD[6]}" "$R" "$DIM" "$ts" "$R"
  printf '\n'

  # Per-phase progress strip — the load-bearing visibility upgrade
  printf '  %s%sPH13 ROLLBACK DRILLS%s  %s  %s%d/%d%s   ' \
    "$B" "$BG_PH13" "$R" "$(fill_bar "${ph_done[0]}" "${ph_total[0]}" 10)" "$B" "${ph_done[0]}" "${ph_total[0]}" "$R"
  if (( ph_claimed[0] > 0 )); then printf '%s+%d in flight%s' "${GRAD[3]}" "${ph_claimed[0]}" "$R"; fi
  printf '\n'
  printf '  %s%sPH14 ROLLOUT GATES  %s  %s  %s%d/%d%s   ' \
    "$B" "$BG_PH14" "$R" "$(fill_bar "${ph_done[1]}" "${ph_total[1]}" 10)" "$B" "${ph_done[1]}" "${ph_total[1]}" "$R"
  if (( ph_claimed[1] > 0 )); then printf '%s+%d in flight%s' "${GRAD[3]}" "${ph_claimed[1]}" "$R"; fi
  printf '\n'
  printf '  %s%sPH15 DECOMM         %s  %s  %s%d/%d%s   ' \
    "$B" "$BG_PH15" "$R" "$(fill_bar "${ph_done[2]}" "${ph_total[2]}" 10)" "$B" "${ph_done[2]}" "${ph_total[2]}" "$R"
  if (( ph_claimed[2] > 0 )); then printf '%s+%d in flight%s' "${GRAD[3]}" "${ph_claimed[2]}" "$R"; fi
  printf '\n\n'

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
  printf '                          ▼ %s\n\n' "$(close_label "$f" "$(( claimed_n + done_n ))")"

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
  printf '  %sLEGEND%s  %s●%s done   %s%s%s claimed (slow spin)   %s✕%s blocked   %s◇%s available\n' \
    "$DIM" "$R" "${GRAD[6]}" "$R" "${GRAD[3]}" "${SPINNER[$(( (f / 2) % 4 ))]}" "$R" "$RED" "$R" "$DIM" "$R"

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
  # Clear from here to end of screen so shrinking content doesn't leave ghosts.
  printf '\033[J'
}

if (( ONCE == 1 )); then
  render 0
else
  printf '\033[?25l'
  trap 'printf "\033[?25h"; exit' INT TERM EXIT
  f=0
  while true; do
    render "$f"
    f=$((f+1))
    sleep "$INTERVAL_S"
  done
fi
