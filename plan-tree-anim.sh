#!/usr/bin/env bash
# plan-tree-anim — DAG wave-tree view of the active openspec plan.
#
# Reads <repo>/openspec/plans/*/plan.json (newest by trailing YYYY-MM-DD),
# does a Kahn topological-levels sort over `depends_on`, and renders each
# level as a column of sub-task cards with ASCII edges between deps.
#
# Compared to plan-anim-generic.sh (flat rows), this exposes parallelism:
# every wave column shows what could run concurrently. Spinner pulses on
# claimed sub-tasks. Worker assignment is shown under the card title.
#
# Usage:
#   bash scripts/codex-fleet/plan-tree-anim.sh           # loop @ 1s
#   bash scripts/codex-fleet/plan-tree-anim.sh --once
#   PLAN_TREE_ANIM_PLAN_JSON=/path/plan.json ...        # pin a plan
#   PLAN_TREE_ANIM_INTERVAL_MS=800 ...                   # tick override
set -eo pipefail

REPO="${PLAN_TREE_ANIM_REPO:-/home/deadpool/Documents/recodee}"
INTERVAL_MS="${PLAN_TREE_ANIM_INTERVAL_MS:-1000}"
ONCE=0
for a in "$@"; do
  case "$a" in
    --once) ONCE=1 ;;
    --interval=*) INTERVAL_MS="${a#--interval=}" ;;
  esac
done
INTERVAL_S=$(awk -v ms="$INTERVAL_MS" 'BEGIN{printf "%.3f", ms/1000}')

_latest_plan() {
  python3 - "$REPO" <<'PYEOF'
import os, sys, re, glob
repo = sys.argv[1]
plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    return (d, os.path.getmtime(p))
plans.sort(key=key, reverse=True)
print(plans[0] if plans else "")
PYEOF
}

PLAN_JSON="${PLAN_TREE_ANIM_PLAN_JSON:-$(_latest_plan)}"
if [[ ! -f "$PLAN_JSON" ]]; then
  printf 'plan-tree-anim: no plan.json found under %s/openspec/plans/*/plan.json\n' "$REPO"
  sleep 5
  exit 1
fi
PLAN_SLUG=$(basename "$(dirname "$PLAN_JSON")")

# iOS truecolor palette (kept in lock-step with fleet-tick.sh)
E=$'\033'
R="${E}[0m"; B="${E}[1m"; D="${E}[2m"
DIM="${E}[38;2;142;142;147m"
WHITE="${E}[38;2;255;255;255m"
GREY="${E}[38;2;174;174;178m"
TEAL="${E}[38;5;73m"
ICE="${E}[38;5;117m"
G="${E}[38;2;52;199;89m"        # systemGreen
RED="${E}[38;2;255;59;48m"       # systemRed
ORANGE="${E}[38;2;255;149;0m"    # systemOrange
YELLOW="${E}[38;2;255;204;0m"    # systemYellow
BLUE="${E}[38;2;0;122;255m"      # systemBlue

SPIN=("◐" "◓" "◑" "◒")

# Kahn topological-levels — produces a wave assignment per subtask.
# Output: one line per task: "<sub_idx>\t<wave_idx>" (wave starts at 0).
levels_from_plan() {
  python3 - "$1" <<'PYEOF'
import json, sys, collections
with open(sys.argv[1]) as fh:
    plan = json.load(fh)
tasks = plan.get("tasks", []) or []
deps = {t["subtask_index"]: list(t.get("depends_on") or []) for t in tasks}
indeg = {i: len(d) for i, d in deps.items()}
revdep = collections.defaultdict(list)
for child, ds in deps.items():
    for d in ds:
        revdep[d].append(child)
# BFS by levels
level = {}
queue = collections.deque(sorted([i for i, n in indeg.items() if n == 0]))
cur = 0
while queue:
    nxt = collections.deque()
    while queue:
        i = queue.popleft()
        level[i] = cur
        for c in revdep[i]:
            indeg[c] -= 1
            if indeg[c] == 0:
                nxt.append(c)
    queue = collections.deque(sorted(nxt))
    cur += 1
# fall-back: assign any orphans (cyclic plans, malformed) to highest level
for i in deps:
    level.setdefault(i, max(level.values(), default=0))
for i in sorted(level):
    print(f"{i}\t{level[i]}")
PYEOF
}

# Per-tick task pull: idx \t status \t title \t agent
pull_tasks() {
  jq -r '.tasks | sort_by(.subtask_index) | .[] |
          "\(.subtask_index)\t\(.status // "available")\t\(.title)\t\(.claimed_by_agent // "")"' \
       "$PLAN_JSON" 2>/dev/null
}

# Compute wave assignment once (deps don't change at runtime)
declare -A LVL
while IFS=$'\t' read -r idx lvl; do
  LVL[$idx]=$lvl
done < <(levels_from_plan "$PLAN_JSON")

# How many tasks per wave (max column height)
declare -A WAVE_N
MAX_WAVE=0
for idx in "${!LVL[@]}"; do
  l=${LVL[$idx]}
  WAVE_N[$l]=$(( ${WAVE_N[$l]:-0} + 1 ))
  (( l > MAX_WAVE )) && MAX_WAVE=$l
done

# Width of each wave column. Truncate to ≥22 cells so the title is legible.
# We pick column width from pane size — fall back to 32 if not in tmux.
PANE_W=$(tmux display-message -p '#{pane_width}' 2>/dev/null || echo 140)
N_WAVES=$(( MAX_WAVE + 1 ))
# 4-char gutter between wave columns. Reserve 2 cols on each side.
COL_W=$(( (PANE_W - 4 - 4 * (N_WAVES - 1)) / N_WAVES ))
(( COL_W < 22 )) && COL_W=22
(( COL_W > 34 )) && COL_W=34

# Marker for a task status
marker_for() {
  local status="$1" f="$2"
  case "$status" in
    completed) printf '%s●%s' "$G" "$R" ;;
    claimed)   printf '%s%s%s' "$YELLOW" "${SPIN[$(( (f / 2) % 4 ))]}" "$R" ;;
    blocked)   printf '%s✕%s' "$RED" "$R" ;;
    *)         printf '%s◇%s' "$DIM" "$R" ;;
  esac
}

# Truncate s to n display cells (rough; counts bytes for ASCII titles)
trunc() {
  local s="$1" n="$2"
  if (( ${#s} > n )); then
    printf '%s…' "${s:0:n-1}"
  else
    printf '%s' "$s"
  fi
}

render() {
  local f="$1"
  local ts
  ts=$(date '+%H:%M:%S')

  # Pull task state into 4 parallel arrays indexed by sub_idx
  declare -A T_STATUS T_TITLE T_AGENT
  local total=0 done_n=0 claimed_n=0 blocked_n=0 avail_n=0
  while IFS=$'\t' read -r i s t a; do
    [[ -z "$i" ]] && continue
    T_STATUS[$i]="$s"
    T_TITLE[$i]="$t"
    T_AGENT[$i]="$a"
    total=$((total+1))
    case "$s" in
      completed) done_n=$((done_n+1)) ;;
      claimed)   claimed_n=$((claimed_n+1)) ;;
      blocked)   blocked_n=$((blocked_n+1)) ;;
      *)         avail_n=$((avail_n+1)) ;;
    esac
  done < <(pull_tasks)

  local pct=0
  (( total > 0 )) && pct=$(( done_n * 100 / total ))

  # Cursor home; we paint top-to-bottom and \033[K each line to wipe trailing chars.
  printf '%s' "${E}[H"

  # ── Header ────────────────────────────────────────────────────────────
  printf '%s%sPLAN TREE%s  %s%s%s  %s● live%s  %s%s%s%s\n' \
    "$B" "$TEAL" "$R" "$WHITE" "$PLAN_SLUG" "$R" "$G" "$R" "$DIM" "$ts" "$R" "${E}[K"
  local sep
  printf -v sep '%*s' "$PANE_W" ''
  sep="${sep// /┄}"
  printf '%s%s%s%s\n' "$GREY" "$sep" "$R" "${E}[K"

  # ── Wave column headers ──────────────────────────────────────────────
  local w
  for ((w=0; w<N_WAVES; w++)); do
    printf '  %s%sW%-2d%s' "$B" "$ICE" "$((w+1))" "$R"
    local pad=$(( COL_W - 4 ))
    (( pad > 0 )) && printf '%*s' "$pad" ""
    (( w < N_WAVES - 1 )) && printf '    '
  done
  printf '%s\n' "${E}[K"

  # Edge row directly under headers — visual cue that waves connect
  for ((w=0; w<N_WAVES; w++)); do
    printf '  %s%s%s' "$GREY" "$(printf '─%.0s' $(seq 1 $((COL_W - 2))))" "$R"
    (( w < N_WAVES - 1 )) && printf '%s ▶  %s' "$DIM" "$R"
  done
  printf '%s\n' "${E}[K"

  # ── Task cards per wave, row-major (max wave height drives row count) ─
  local max_h=0
  for ((w=0; w<N_WAVES; w++)); do
    (( ${WAVE_N[$w]:-0} > max_h )) && max_h=${WAVE_N[$w]:-0}
  done
  # Map (wave, row) → sub_idx
  declare -A CELL
  for ((w=0; w<N_WAVES; w++)); do
    local row=0
    for idx in $(printf '%s\n' "${!LVL[@]}" | sort -n); do
      if [[ "${LVL[$idx]}" == "$w" ]]; then
        CELL[$w,$row]=$idx
        row=$((row+1))
      fi
    done
  done

  local row
  for ((row=0; row<max_h; row++)); do
    # Line 1 of card: marker + sub-idx + status word
    for ((w=0; w<N_WAVES; w++)); do
      local idx="${CELL[$w,$row]:-}"
      if [[ -z "$idx" ]]; then
        printf '%*s' "$COL_W" ""
      else
        local s="${T_STATUS[$idx]}"
        local m
        m=$(marker_for "$s" "$f")
        local statword
        case "$s" in
          completed) statword="${G}done${R}" ;;
          claimed)   statword="${YELLOW}claimed${R}" ;;
          blocked)   statword="${RED}blocked${R}" ;;
          *)         statword="${DIM}available${R}" ;;
        esac
        local lead="  $m ${B}sub-${idx}${R} ${statword}"
        local lead_visible_len
        lead_visible_len=$(printf '%s' "$lead" | sed -E "s/\\$E\\[[0-9;]*m//g" | awk '{ print length }')
        local pad=$(( COL_W - lead_visible_len ))
        (( pad < 0 )) && pad=0
        printf '%s%*s' "$lead" "$pad" ""
      fi
      (( w < N_WAVES - 1 )) && {
        # Edge connector between this wave and the next, but only for rows
        # where THIS cell points to a task in the next wave (forward dep).
        if [[ -n "${CELL[$w,$row]:-}" && -n "${CELL[$((w+1)),$row]:-}" ]]; then
          printf '%s───▶%s' "$GREY" "$R"
        else
          printf '    '
        fi
      }
    done
    printf '%s\n' "${E}[K"

    # Line 2: title (truncated to col width minus indent)
    for ((w=0; w<N_WAVES; w++)); do
      local idx="${CELL[$w,$row]:-}"
      if [[ -z "$idx" ]]; then
        printf '%*s' "$COL_W" ""
      else
        local title
        title=$(trunc "${T_TITLE[$idx]}" $((COL_W - 4)))
        local label="    ${DIM}${title}${R}"
        local label_visible_len=$(( ${#title} + 4 ))
        local pad=$(( COL_W - label_visible_len ))
        (( pad < 0 )) && pad=0
        printf '%s%*s' "$label" "$pad" ""
      fi
      (( w < N_WAVES - 1 )) && printf '    '
    done
    printf '%s\n' "${E}[K"

    # Line 3: agent assignment (only if claimed)
    for ((w=0; w<N_WAVES; w++)); do
      local idx="${CELL[$w,$row]:-}"
      if [[ -z "$idx" ]]; then
        printf '%*s' "$COL_W" ""
      else
        local agent="${T_AGENT[$idx]}"
        if [[ -n "$agent" && "$agent" != "null" ]]; then
          local astr="    ${DIM}←${R} ${ICE}${agent}${R}"
          local astr_visible_len=$(( ${#agent} + 6 ))
          local pad=$(( COL_W - astr_visible_len ))
          (( pad < 0 )) && pad=0
          printf '%s%*s' "$astr" "$pad" ""
        else
          printf '%*s' "$COL_W" ""
        fi
      fi
      (( w < N_WAVES - 1 )) && printf '    '
    done
    printf '%s\n' "${E}[K"

    # Spacer row between card rows
    printf '%s\n' "${E}[K"
  done

  # ── Footer: total bar + counters ─────────────────────────────────────
  printf '%s\n' "${E}[K"
  local bar_w=44
  local filled=0
  (( total > 0 )) && filled=$(( done_n * bar_w / total ))
  local bar="${WHITE}▕${R}"
  local k
  for ((k=0;k<filled;k++)); do bar+="${G}█${R}"; done
  for ((k=filled;k<bar_w;k++)); do bar+="${DIM}░${R}"; done
  bar+="${WHITE}▏${R}"
  printf '  %sTOTAL%s  %s  %s%d/%d%s  %s%d%%%s%s\n' \
    "$B" "$R" "$bar" "$WHITE" "$done_n" "$total" "$R" "$G" "$pct" "$R" "${E}[K"

  printf '%s\n' "${E}[K"
  local cur_sp="${SPIN[$(( (f / 2) % 4 ))]}"
  printf '  %sLEGEND%s   %s●%s done   %s%s%s claimed   %s✕%s blocked   %s◇%s available%s\n' \
    "$DIM" "$R" "$G" "$R" "$YELLOW" "$cur_sp" "$R" "$RED" "$R" "$DIM" "$R" "${E}[K"
  printf '  %sclaimed=%s%d%s  %sdone=%s%d%s  %sblocked=%s%d%s  %savailable=%s%d%s%s\n' \
    "$DIM" "$YELLOW" "$claimed_n" "$R" \
    "$DIM" "$G" "$done_n" "$R" \
    "$DIM" "$RED" "$blocked_n" "$R" \
    "$DIM" "$ICE" "$avail_n" "$R" "${E}[K"

  # Wipe to end of screen so a shorter frame doesn't leave ghost rows.
  printf '%s' "${E}[J"
}

if (( ONCE == 1 )); then
  render 0
else
  printf '%s' "${E}[?25l"
  trap 'printf "%s" "${E}[?25h"; exit' INT TERM EXIT
  f=0
  while true; do
    render "$f"
    f=$((f+1))
    sleep "$INTERVAL_S"
  done
fi
