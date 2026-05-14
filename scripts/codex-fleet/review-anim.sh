#!/usr/bin/env bash
# review-anim — iOS-style approval-queue view (screen 4/4 of the live viz).
#
# Renders the auto-reviewer pending card on the left and a Recent Decisions
# rail on the right. Sibling of fleet-tick / plan-anim / waves-anim and reuses
# the same iOS palette + rounded-card primitives.
#
# Data source: $REVIEW_ANIM_QUEUE_JSON (default /tmp/claude-viz/live-review-
# queue.json). If absent or unreadable, falls back to a built-in demo fixture
# that matches the design comp. Each tick re-reads the file so an external
# producer can drive the screen.
#
# Output: ANSI text on stdout. Use --once for one frame or omit for the
# default 800ms diff-painted loop.
#
# Queue JSON shape:
#   {
#     "approved_today": 124,
#     "pending": [
#       {
#         "id": "REV-014",
#         "age_seconds": 557,
#         "title": "apply_patch touching 3 files",
#         "agent": "codex-ricsi-zazrifka",
#         "pane": 4,
#         "risk": "medium",        // low | medium | high
#         "auth": "high",          // low | medium | high
#         "rationale": "Bounded local edits within the claimed task ...",
#         "files": [
#           "scripts/codex-fleet/lib/_env.sh",
#           "scripts/codex-fleet/down-kitty.sh",
#           "docs/cockpit.md"
#         ]
#       }
#     ],
#     "decisions": [
#       { "cmd": "sleep 60", "agent": "codex-admin-kollarrobert",
#         "age_minutes": 3, "risk": "low", "outcome": "approved" },
#       ...
#     ]
#   }

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

QUEUE_JSON="${REVIEW_ANIM_QUEUE_JSON:-/tmp/claude-viz/live-review-queue.json}"

# ── palette ───────────────────────────────────────────────────────────────────
B=$'\033[1m'; D=$'\033[2m'; R=$'\033[0m'
IOS_BLUE=$'\033[38;2;0;122;255m'
IOS_GREEN=$'\033[38;2;52;199;89m'
IOS_RED=$'\033[38;2;255;59;48m'
IOS_ORANGE=$'\033[38;2;255;149;0m'
IOS_YELLOW=$'\033[38;2;255;204;0m'
IOS_GRAY=$'\033[38;2;142;142;147m'
IOS_GRAY2=$'\033[38;2;174;174;178m'
IOS_GRAY3=$'\033[38;2;99;99;102m'
IOS_WHITE=$'\033[38;2;255;255;255m'
IOS_BG_BLUE=$'\033[48;2;0;122;255m'
IOS_BG_GREEN=$'\033[48;2;52;199;89m'
IOS_BG_RED=$'\033[48;2;255;59;48m'
IOS_BG_ORANGE=$'\033[48;2;255;149;0m'
IOS_BG_GRAY=$'\033[48;2;58;58;60m'
DIM="$IOS_GRAY"
WHITE="$IOS_WHITE"
TEAL="$IOS_BLUE"
ACCENT="$IOS_BLUE"

LEFT_WIDTH=52
RIGHT_WIDTH=42
GAP="  "

# ── ANSI helpers ──────────────────────────────────────────────────────────────
strip_ansi() {
  sed -E $'s/\x1B\\[[0-9;]*m//g' <<<"${1:-}"
}

visible_len() {
  local clean
  clean=$(strip_ansi "${1:-}")
  printf '%d' "${#clean}"
}

pad_to() {
  local content="${1:-}" width="${2:-0}"
  local len pad
  len=$(visible_len "$content")
  pad=$(( width - len ))
  (( pad < 0 )) && pad=0
  printf '%s%*s' "$content" "$pad" ""
}

# ── chip helpers (rounded pill with bg+fg block) ──────────────────────────────
pill() {
  local bg="$1" fg="$2" text="$3"
  printf '%s%s %s %s' "$bg" "$fg" "$text" "$R"
}

risk_pill() {
  case "${1:-low}" in
    high)   pill "$IOS_BG_RED"    "$WHITE" "${B}risk high${R}" ;;
    medium) pill "$IOS_BG_ORANGE" "$WHITE" "${B}risk medium${R}" ;;
    *)      pill "$IOS_BG_GRAY"   "$WHITE" "${B}risk low${R}" ;;
  esac
}

auth_pill() {
  case "${1:-low}" in
    high)   pill "$IOS_BG_RED"    "$WHITE" "${B}auth high${R}" ;;
    medium) pill "$IOS_BG_ORANGE" "$WHITE" "${B}auth medium${R}" ;;
    *)      pill "$IOS_BG_GRAY"   "$WHITE" "${B}auth low${R}" ;;
  esac
}

outcome_pill() {
  case "${1:-approved}" in
    approved)  pill "$IOS_BG_GREEN"  "$WHITE" "${B}● approved${R}" ;;
    escalated) pill "$IOS_BG_ORANGE" "$WHITE" "${B}● escalated${R}" ;;
    denied)    pill "$IOS_BG_RED"    "$WHITE" "${B}● denied${R}" ;;
    *)         pill "$IOS_BG_GRAY"   "$WHITE" "${B}● ${1}${R}" ;;
  esac
}

risk_inline() {
  local r="${1:-low}"
  case "$r" in
    high)   printf '%srisk · high%s'   "$IOS_RED"    "$R" ;;
    medium) printf '%srisk · medium%s' "$IOS_ORANGE" "$R" ;;
    *)      printf '%srisk · low%s'    "$IOS_GRAY2"  "$R" ;;
  esac
}

# ── box-drawing primitives ────────────────────────────────────────────────────
card_top() {
  local width="${1:-$LEFT_WIDTH}" color="${2:-$IOS_GRAY3}"
  local fill_len=$(( width - 2 ))
  local fill; printf -v fill '%*s' "$fill_len" ""; fill=${fill// /─}
  printf '%s╭%s╮%s' "$color" "$fill" "$R"
}

card_bottom() {
  local width="${1:-$LEFT_WIDTH}" color="${2:-$IOS_GRAY3}"
  local fill_len=$(( width - 2 ))
  local fill; printf -v fill '%*s' "$fill_len" ""; fill=${fill// /─}
  printf '%s╰%s╯%s' "$color" "$fill" "$R"
}

card_line() {
  local content="${1:-}" width="${2:-$LEFT_WIDTH}" color="${3:-$IOS_GRAY3}"
  local inner=$(( width - 4 ))
  local len pad
  len=$(visible_len "$content")
  pad=$(( inner - len ))
  (( pad < 0 )) && pad=0
  printf '%s│%s %s%*s %s│%s' "$color" "$R" "$content" "$pad" "" "$color" "$R"
}

card_blank() {
  card_line "" "${1:-$LEFT_WIDTH}" "${2:-$IOS_GRAY3}"
}

# ── elapsed formatter ─────────────────────────────────────────────────────────
fmt_age() {
  local secs="${1:-0}"
  if (( secs < 60 )); then
    printf '%ds' "$secs"
  elif (( secs < 3600 )); then
    printf '%dm %ds' $(( secs / 60 )) $(( secs % 60 ))
  else
    printf '%dh %dm' $(( secs / 3600 )) $(( (secs % 3600) / 60 ))
  fi
}

fmt_minutes_ago() {
  local m="${1:-0}"
  if (( m < 60 )); then
    printf '%dm ago' "$m"
  else
    printf '%dh ago' $(( m / 60 ))
  fi
}

# ── data loading ──────────────────────────────────────────────────────────────
declare -a DEMO_PENDING=(
  '{"id":"REV-014","age_seconds":557,"title":"apply_patch touching 3 files","agent":"codex-ricsi-zazrifka","pane":4,"risk":"medium","auth":"high","rationale":"Bounded local edits within the claimed task file scope on an isolated agent worktree, a reversible change explicitly authorized by the user'"'"'s worker and repo workflow.","files":["scripts/codex-fleet/lib/_env.sh","scripts/codex-fleet/down-kitty.sh","docs/cockpit.md"]}'
)

declare -a DEMO_DECISIONS=(
  '{"cmd":"sleep 60","agent":"codex-admin-kollarrobert","age_minutes":3,"risk":"low","outcome":"approved"}'
  '{"cmd":"openspec validate --spec","agent":"codex-admin-magnolia","age_minutes":7,"risk":"low","outcome":"approved"}'
  '{"cmd":"bash -lc '"'"'ls scripts/'"'"'","agent":"codex-matt-gg","age_minutes":12,"risk":"low","outcome":"approved"}'
  '{"cmd":"git diff --no-index","agent":"codex-fico-magnolia","age_minutes":18,"risk":"low","outcome":"approved"}'
  '{"cmd":"rm -rf .cap-probe-cache","agent":"codex-recodee-mite","age_minutes":24,"risk":"medium","outcome":"escalated"}'
  '{"cmd":"curl https://api.colony…","agent":"codex-ricsi-zazrifka","age_minutes":31,"risk":"medium","outcome":"denied"}'
)
DEMO_APPROVED_TODAY=124

QUEUE_RAW=""
load_queue() {
  if [[ -r "$QUEUE_JSON" ]] && command -v jq >/dev/null 2>&1; then
    if jq -e . "$QUEUE_JSON" >/dev/null 2>&1; then
      QUEUE_RAW=$(cat "$QUEUE_JSON")
      return 0
    fi
  fi
  QUEUE_RAW=""
}

queue_field() {
  local path="$1" default="${2:-}"
  if [[ -n "$QUEUE_RAW" ]] && command -v jq >/dev/null 2>&1; then
    jq -r "$path // empty" <<<"$QUEUE_RAW" 2>/dev/null || printf '%s' "$default"
  else
    printf '%s' "$default"
  fi
}

queue_array() {
  local path="$1"
  if [[ -n "$QUEUE_RAW" ]] && command -v jq >/dev/null 2>&1; then
    jq -c "$path[]?" <<<"$QUEUE_RAW" 2>/dev/null
  fi
}

pending_items() {
  if [[ -n "$QUEUE_RAW" ]]; then
    queue_array '.pending'
  else
    printf '%s\n' "${DEMO_PENDING[@]}"
  fi
}

decision_items() {
  if [[ -n "$QUEUE_RAW" ]]; then
    queue_array '.decisions'
  else
    printf '%s\n' "${DEMO_DECISIONS[@]}"
  fi
}

approved_today() {
  local v
  v=$(queue_field '.approved_today')
  [[ -z "$v" ]] && v="$DEMO_APPROVED_TODAY"
  printf '%s' "$v"
}

# ── word-wrap rationale to a column ───────────────────────────────────────────
wrap_text() {
  local text="$1" width="$2"
  awk -v w="$width" '
    {
      n = split($0, words, " ")
      line = ""
      for (i = 1; i <= n; i++) {
        candidate = (line == "") ? words[i] : line " " words[i]
        if (length(candidate) > w && line != "") {
          print line
          line = words[i]
        } else {
          line = candidate
        }
      }
      if (line != "") print line
    }' <<<"$text"
}

# ── left card: pending review ─────────────────────────────────────────────────
render_pending_card() {
  local item="$1" frame="$2"
  local id title agent pane risk auth rationale age_s
  id=$(jq -r '.id // "REV-???"' <<<"$item")
  title=$(jq -r '.title // ""' <<<"$item")
  agent=$(jq -r '.agent // ""' <<<"$item")
  pane=$(jq -r '.pane // ""' <<<"$item")
  risk=$(jq -r '.risk // "low"' <<<"$item")
  auth=$(jq -r '.auth // "low"' <<<"$item")
  rationale=$(jq -r '.rationale // ""' <<<"$item")
  age_s=$(jq -r '.age_seconds // 0' <<<"$item")
  local age; age=$(fmt_age "$age_s")

  local card_color="$ACCENT"

  card_top "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Header row: ⚡ id · age   risk pill
  local hdr_left hdr_right
  hdr_left=$(printf '%s⚡%s %s%s%s %s● %s%s' "$IOS_ORANGE" "$R" "$DIM" "$id" "$R" "$DIM" "$age" "$R")
  hdr_right=$(risk_pill "$risk")
  local inner=$(( LEFT_WIDTH - 4 ))
  local lhs_len rhs_len gap_n
  lhs_len=$(visible_len "$hdr_left")
  rhs_len=$(visible_len "$hdr_right")
  gap_n=$(( inner - lhs_len - rhs_len ))
  (( gap_n < 1 )) && gap_n=1
  card_line "$(printf '%s%*s%s' "$hdr_left" "$gap_n" "" "$hdr_right")" "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Title row
  card_line "$(printf '%s%s%s%s' "$B" "$WHITE" "$title" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Agent · pane           auth pill
  local meta_left meta_right
  if [[ -n "$pane" ]]; then
    meta_left=$(printf '%s%s%s %s·%s %spane %s%s' "$DIM" "$agent" "$R" "$DIM" "$R" "$DIM" "$pane" "$R")
  else
    meta_left=$(printf '%s%s%s' "$DIM" "$agent" "$R")
  fi
  meta_right=$(auth_pill "$auth")
  lhs_len=$(visible_len "$meta_left")
  rhs_len=$(visible_len "$meta_right")
  gap_n=$(( inner - lhs_len - rhs_len ))
  (( gap_n < 1 )) && gap_n=1
  card_line "$(printf '%s%*s%s' "$meta_left" "$gap_n" "" "$meta_right")" "$LEFT_WIDTH" "$card_color"; printf '\n'

  card_blank "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Rationale block — yellow left accent, label + wrapped body
  card_line "$(printf '%s│%s %sAUTO-REVIEWER RATIONALE%s' "$IOS_YELLOW" "$R" "$B" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'
  local rat_width=$(( inner - 3 ))
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    card_line "$(printf '%s│%s %s%s%s' "$IOS_YELLOW" "$R" "$IOS_GRAY2" "$line" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'
  done < <(wrap_text "$rationale" "$rat_width")

  card_blank "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Files
  local file_count
  file_count=$(jq -r '.files | length' <<<"$item")
  card_line "$(printf '%s%s FILES TOUCHED%s' "$B" "$file_count" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    card_line "$(printf ' %s↕%s  %s%s%s' "$IOS_BLUE" "$R" "$WHITE" "$f" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'
  done < <(jq -r '.files[]?' <<<"$item")

  card_blank "$LEFT_WIDTH" "$card_color"; printf '\n'

  # Action buttons
  local btn_a btn_v btn_d btn_row
  btn_a=$(pill "$IOS_BG_BLUE"   "$WHITE" "${B}A · Approve${R}")
  btn_v=$(pill "$IOS_BG_GRAY"   "$WHITE" "${B}V · View diff${R}")
  btn_d=$(pill "$IOS_BG_RED"    "$WHITE" "${B}D · Deny${R}")
  btn_row=$(printf '%s  %s  %s' "$btn_a" "$btn_v" "$btn_d")
  card_line "$btn_row" "$LEFT_WIDTH" "$card_color"; printf '\n'

  card_bottom "$LEFT_WIDTH" "$card_color"; printf '\n'
}

# When there is no pending item, the card collapses to a calm empty state.
render_empty_pending_card() {
  local card_color="$IOS_GRAY3"
  card_top "$LEFT_WIDTH" "$card_color"; printf '\n'
  card_blank "$LEFT_WIDTH" "$card_color"; printf '\n'
  card_line "$(printf '%s%s✓%s  %squeue clear · auto-reviewer caught up%s' "$B" "$IOS_GREEN" "$R" "$IOS_GRAY2" "$R")" "$LEFT_WIDTH" "$card_color"; printf '\n'
  card_blank "$LEFT_WIDTH" "$card_color"; printf '\n'
  card_bottom "$LEFT_WIDTH" "$card_color"; printf '\n'
}

# ── right card: recent decisions ──────────────────────────────────────────────
render_decisions_card() {
  local card_color="$IOS_GRAY3"
  card_top "$RIGHT_WIDTH" "$card_color"; printf '\n'

  # Header: "Recent decisions   last 30m"
  local inner=$(( RIGHT_WIDTH - 4 ))
  local hdr_l hdr_r gap_n lhs_len rhs_len
  hdr_l=$(printf '%s%sRecent decisions%s' "$B" "$WHITE" "$R")
  hdr_r=$(printf '%slast 30m%s' "$DIM" "$R")
  lhs_len=$(visible_len "$hdr_l")
  rhs_len=$(visible_len "$hdr_r")
  gap_n=$(( inner - lhs_len - rhs_len ))
  (( gap_n < 1 )) && gap_n=1
  card_line "$(printf '%s%*s%s' "$hdr_l" "$gap_n" "" "$hdr_r")" "$RIGHT_WIDTH" "$card_color"; printf '\n'

  card_blank "$RIGHT_WIDTH" "$card_color"; printf '\n'

  local any=0
  while IFS= read -r entry; do
    [[ -z "$entry" ]] && continue
    any=1
    local cmd agent age_m risk outcome
    cmd=$(jq -r '.cmd // ""' <<<"$entry")
    agent=$(jq -r '.agent // ""' <<<"$entry")
    age_m=$(jq -r '.age_minutes // 0' <<<"$entry")
    risk=$(jq -r '.risk // "low"' <<<"$entry")
    outcome=$(jq -r '.outcome // "approved"' <<<"$entry")

    # Row 1: cmd ……………………… outcome_pill (right-aligned)
    local left right
    local cmd_max=$(( inner - $(visible_len "$(outcome_pill "$outcome")") - 2 ))
    (( cmd_max < 8 )) && cmd_max=8
    local cmd_disp="$cmd"
    if (( ${#cmd} > cmd_max )); then
      cmd_disp="${cmd:0:$((cmd_max - 1))}…"
    fi
    left=$(printf '%s%s%s%s' "$B" "$WHITE" "$cmd_disp" "$R")
    right=$(outcome_pill "$outcome")
    lhs_len=$(visible_len "$left")
    rhs_len=$(visible_len "$right")
    gap_n=$(( inner - lhs_len - rhs_len ))
    (( gap_n < 1 )) && gap_n=1
    card_line "$(printf '%s%*s%s' "$left" "$gap_n" "" "$right")" "$RIGHT_WIDTH" "$card_color"; printf '\n'

    # Row 2: agent · age · risk
    local meta
    meta=$(printf '%s%s%s  %s%s%s  %s' \
      "$DIM" "$agent" "$R" \
      "$DIM" "$(fmt_minutes_ago "$age_m")" "$R" \
      "$(risk_inline "$risk")")
    card_line "$meta" "$RIGHT_WIDTH" "$card_color"; printf '\n'

    card_blank "$RIGHT_WIDTH" "$card_color"; printf '\n'
  done < <(decision_items)

  if (( any == 0 )); then
    card_line "$(printf '%s(no decisions yet)%s' "$DIM" "$R")" "$RIGHT_WIDTH" "$card_color"; printf '\n'
    card_blank "$RIGHT_WIDTH" "$card_color"; printf '\n'
  fi

  card_bottom "$RIGHT_WIDTH" "$card_color"; printf '\n'
}

# ── two-column join ───────────────────────────────────────────────────────────
join_columns() {
  local left="$1" right="$2"
  local -a L=() Rr=()
  mapfile -t L <<< "$left"
  mapfile -t Rr <<< "$right"
  local n=${#L[@]}
  (( ${#Rr[@]} > n )) && n=${#Rr[@]}
  local i
  for ((i=0; i<n; i++)); do
    local l="${L[$i]-}" r="${Rr[$i]-}"
    local lpad
    lpad=$(pad_to "$l" "$LEFT_WIDTH")
    printf '%s%s%s\n' "$lpad" "$GAP" "$r"
  done
}

# ── flicker-free repaint ──────────────────────────────────────────────────────
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

# ── top-of-page header (above the two-column body) ────────────────────────────
render_header() {
  local pending_n="$1" approved_n="$2" ts="$3"
  printf '%s%sREVIEW%s %s·%s %s%d pending%s %s·%s %sauto-reviewer on%s   %s%s%s\n' \
    "$B" "$TEAL" "$R" \
    "$DIM" "$R" \
    "$B" "$pending_n" "$R" \
    "$DIM" "$R" \
    "$IOS_GREEN" "$R" \
    "$DIM" "$ts" "$R"
  printf '\n'

  local awaiting_word="awaiting"
  (( pending_n == 1 )) || awaiting_word="awaiting"
  printf '%s%s%d %s%s %s·%s %s%d approved today%s\n' \
    "$B" "$WHITE" "$pending_n" "$awaiting_word" "$R" \
    "$DIM" "$R" \
    "$WHITE" "$approved_n" "$R"
  printf '\n'
}

render_footer() {
  printf '%sJ%s %s·%s %sReview — approval queue%s   %s4 / 4%s\n' \
    "$B" "$R" \
    "$DIM" "$R" \
    "$IOS_GRAY2" "$R" \
    "$DIM" "$R"
}

render() {
  local f="$1"
  local ts; ts=$(date '+%H:%M:%S')
  load_queue

  local pending_count=0
  while IFS= read -r _; do pending_count=$((pending_count+1)); done < <(pending_items)
  local approved
  approved=$(approved_today)

  render_header "$pending_count" "$approved" "$ts"

  local first_pending
  first_pending=$(pending_items | head -1)

  local left_card right_card
  if [[ -n "$first_pending" ]]; then
    left_card=$(render_pending_card "$first_pending" "$f")
  else
    left_card=$(render_empty_pending_card)
  fi
  right_card=$(render_decisions_card)

  join_columns "$left_card" "$right_card"

  printf '\n'
  render_footer
}

if ! command -v jq >/dev/null 2>&1; then
  printf '%sreview-anim: jq is required%s\n' "$IOS_RED" "$R" >&2
  exit 2
fi

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
