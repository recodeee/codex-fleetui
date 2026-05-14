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

REPO="${PLAN_TREE_ANIM_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}}"
# 500ms keeps the spinner pulse feeling alive without burning CPU — each
# render is ~60ms (jq + tmux display-message + awk clamp) so duty-cycle ≈ 12%.
INTERVAL_MS="${PLAN_TREE_ANIM_INTERVAL_MS:-500}"
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
import os, sys, re, glob, json
repo = sys.argv[1]
plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    return (d, os.path.getmtime(p))
plans.sort(key=key, reverse=True)
# Skip empty plans (.tasks == []) — picking one renders an empty pane.
for p in plans:
    try:
        with open(p) as fh:
            data = json.load(fh)
        if data.get("tasks"):
            print(p)
            sys.exit(0)
    except Exception:
        continue
print(plans[0] if plans else "")
PYEOF
}

# Plan selection order:
#   1) PLAN_TREE_ANIM_PLAN_JSON env var (one-shot pin)
#   2) Pin file at PLAN_TREE_ANIM_PIN_FILE (sticky across respawns)
#   3) Newest non-empty plan from openspec/plans/
PIN_FILE="${PLAN_TREE_ANIM_PIN_FILE:-/tmp/claude-viz/plan-tree-pin.txt}"
PLAN_JSON="${PLAN_TREE_ANIM_PLAN_JSON:-}"
if [[ -z "$PLAN_JSON" && -f "$PIN_FILE" ]]; then
  pin_path="$(head -1 "$PIN_FILE" 2>/dev/null)"
  [[ -f "$pin_path" ]] && PLAN_JSON="$pin_path"
fi
[[ -z "$PLAN_JSON" ]] && PLAN_JSON="$(_latest_plan)"
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

# SGR-aware line clamp.
#
# Why: pane-width visible cells are not the same as Bash string length —
# emojis (📂 👤) are width-2, SGR escapes are zero-width. When the body
# emitted a line whose visible width exceeded $PANE_W the terminal wrapped it
# onto the next row, and the next frame's cursor-home repaint left wrap
# fragments visible. That is the "re-rendered again and again" flicker the
# user sees.
#
# This awk filter walks each line cell-by-cell, copies CSI escapes verbatim
# (zero visible cells), and stops counting visible cells at $PANE_W − 1.
# Combined with the single-write at the end of render() this gives an atomic
# redraw with no wrap.
clamp_lines_to_pane() {
  local w="$1"
  awk -v W="$w" '
    BEGIN {
      E = sprintf("%c", 27)
      for (k = 0; k < 256; k++) ORD[sprintf("%c", k)] = k
    }
    {
      n = length($0); i = 1; vis = 0; out = ""
      while (i <= n) {
        c = substr($0, i, 1); b = ORD[c]
        if (b == 27) {
          # CSI: copy verbatim until a letter (final byte). Zero visible cells.
          j = i + 1
          while (j <= n && substr($0, j, 1) !~ /[A-Za-z]/) j++
          out = out substr($0, i, j - i + 1)
          i = j + 1
        } else if (b < 128) {
          if (vis + 1 > W) break
          out = out c; vis++; i++
        } else {
          # UTF-8 leading byte → determine sequence length and visible width.
          # 4-byte (0xF0-0xF7) is supplementary plane (emoji) → width 2.
          # 2/3-byte covers box-drawing, dingbats, CJK BMP — treat as width 1
          # (BMP CJK is technically 2, but the script does not emit it).
          if      (b >= 240) { len = 4; v = 2 }
          else if (b >= 224) { len = 3; v = 1 }
          else if (b >= 192) { len = 2; v = 1 }
          else               { len = 1; v = 1 }  # stray continuation byte
          if (vis + v > W) break
          out = out substr($0, i, len); vis += v; i += len
        }
      }
      # Always tail-clear to EOL. The PROPOSAL-card printfs in _render_body
      # do not emit \e[K themselves, so without this enforcement, residue
      # from a longer previous frame (e.g. box-border fragments) stays
      # visible at the right edge of card rows.
      print out E "[K"
    }
  '
}

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

# ── PR enrichment cache ──────────────────────────────────────────────
# plan.json often lacks claimed_by_agent / completed_summary for tasks whose
# workers never wrote back. We backfill from `gh pr list` matched against the
# task title. Cached to disk for PLAN_TREE_PR_CACHE_TTL seconds (default 60s).
PR_CACHE_FILE="${PLAN_TREE_PR_CACHE_FILE:-/tmp/claude-viz/plan-tree-prs.json}"
PR_CACHE_TTL="${PLAN_TREE_PR_CACHE_TTL:-60}"

refresh_pr_cache() {
  local now age
  now=$(date +%s)
  if [[ -f "$PR_CACHE_FILE" ]]; then
    local mt; mt=$(stat -c '%Y' "$PR_CACHE_FILE" 2>/dev/null || echo 0)
    age=$(( now - mt ))
    (( age < PR_CACHE_TTL )) && return 0
  fi
  # `gh pr list` runs once per ~60s. Tolerate failure (offline, etc.).
  mkdir -p "$(dirname "$PR_CACHE_FILE")"
  gh pr list --state all --limit 80 \
      --json number,title,headRefName,state,mergedAt \
      > "$PR_CACHE_FILE.tmp" 2>/dev/null \
    && mv -f "$PR_CACHE_FILE.tmp" "$PR_CACHE_FILE" \
    || rm -f "$PR_CACHE_FILE.tmp"
}

# Find best-matching PR for a task title via word overlap.
# Returns "<pr_num>\t<agent_slug>" or empty.
lookup_pr_for_title() {
  local title="$1"
  [[ -f "$PR_CACHE_FILE" ]] || return 0
  python3 - "$PR_CACHE_FILE" "$title" <<'PYEOF' 2>/dev/null
import json, sys, re
cache, title = sys.argv[1], sys.argv[2]
try:
    prs = json.load(open(cache))
except Exception:
    sys.exit(0)
def tokens(s):
    s = s.lower()
    return set(t for t in re.split(r'[^a-z0-9]+', s) if len(t) >= 3)
target = tokens(title)
if not target:
    sys.exit(0)
best = None
best_score = 0
for pr in prs:
    pt = tokens(pr.get("title", ""))
    if not pt:
        continue
    score = len(target & pt)
    # Prefer merged PRs at equal score
    if pr.get("mergedAt"):
        score += 0.1
    if score > best_score:
        best_score = score; best = pr
# Require at least 2 shared meaningful tokens
if not best or best_score < 2:
    sys.exit(0)
ref = best.get("headRefName", "") or ""
# Branch slugs look like agent/<who>/<work-slug>-DATE; pick segment 2
parts = ref.split("/")
agent = parts[1] if len(parts) >= 2 and parts[0] == "agent" else ""
print(f"{best.get('number','')}\t{agent}")
PYEOF
}

# Per-tick task pull: idx \t status \t title \t agent
pull_tasks() {
  jq -r '.tasks | sort_by(.subtask_index) | .[] |
          "\(.subtask_index)\t\(.status // "available")\t\(.title)\t\(.claimed_by_agent // "")"' \
       "$PLAN_JSON" 2>/dev/null
}

# Per-tick rich pull for the proposal cards. Uses ASCII unit separator ()
# between fields so titles/descriptions with tabs/spaces survive intact.
# Newlines inside .description are flattened to a single space.
# Output schema (per line):
#   idx | status | title | agent | description | files_csv | deps_csv | summary
pull_tasks_full() {
  jq -r --arg US $'\x1f' '
    .tasks | sort_by(.subtask_index) | .[] |
    [
      (.subtask_index|tostring),
      (.status // "available"),
      .title,
      (.claimed_by_agent // ""),
      ((.description // "") | gsub("\n"; " ") | gsub("\\s+"; " ")),
      ((.file_scope // []) | join(",")),
      ((.depends_on // []) | map(tostring) | join(",")),
      (.completed_summary // "")
    ] | join($US)
  ' "$PLAN_JSON" 2>/dev/null
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

# Word-wrap "$1" to width "$2", emitting each wrapped line on stdout.
wrap_text() {
  local s="$1" w="$2"
  local line="" word
  for word in $s; do
    if (( ${#line} == 0 )); then
      line="$word"
    elif (( ${#line} + 1 + ${#word} <= w )); then
      line="$line $word"
    else
      printf '%s\n' "$line"
      line="$word"
    fi
  done
  [[ -n "$line" ]] && printf '%s\n' "$line"
}

# Card border helpers (PANE_W minus 4 outer margin)
card_top()    { local title="$1" w="$2" pad; pad=$(( w - ${#title} - 6 )); (( pad < 1 )) && pad=1; printf '%s┌─ %s%b ─%s─┐%s\n' "$GREY" "$WHITE" "$title" "$(printf '─%.0s' $(seq 1 "$pad"))" "$R"; }
card_bottom() { local w="$1" line; line=$(printf '─%.0s' $(seq 1 $((w-2)))); printf '%s└%s┘%s\n' "$GREY" "$line" "$R"; }
card_blank()  { local w="$1"; printf '%s│%*s│%s\n' "$GREY" $(( w - 2 )) "" "$R"; }
# Print one card row, given already-styled inner content + visible-len of that content.
# Args: width, visible_len, inner_content
card_row() {
  local w="$1" vis="$2" inner="$3"
  local pad=$(( w - 4 - vis ))
  (( pad < 0 )) && pad=0
  printf '%s│%s  %b%*s%s│%s\n' "$GREY" "$R" "$inner" "$pad" "" "$GREY" "$R"
}
# Plain-text row (no SGR in content) — auto-compute visible length.
card_row_plain() {
  local w="$1" text="$2"
  card_row "$w" "${#text}" "$text"
}

# Status chip — colored pill `◖ ● done ◗` etc.
status_chip() {
  local status="$1"
  case "$status" in
    completed) printf '%s◖ ● done    ◗%s' "$G" "$R" ;;
    claimed)   printf '%s◖ ◐ claimed ◗%s' "$YELLOW" "$R" ;;
    blocked)   printf '%s◖ ✕ blocked ◗%s' "$RED" "$R" ;;
    *)         printf '%s◖ ◇ ready   ◗%s' "$DIM" "$R" ;;
  esac
}
# Visible length of the chip (constant) — used for column padding math
STATUS_CHIP_VIS=13

# Body of one render frame — writes the frame to stdout. The outer `render`
# wrapper captures stdout into a buffer, clamps lines to $PANE_W, and emits
# the whole frame in a single write so the user does not see top-to-bottom
# repaint flicker.
_render_body() {
  local f="$1"
  local ts
  ts=$(date '+%H:%M:%S')

  # Keep the gh-pr lookup cache warm — no-op if file is < TTL old.
  refresh_pr_cache

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

  # ── Proposal cards — compact strip per sub-task ──────────────────────
  # Two inner rows per card:
  #   row 1: 📂 files-csv (truncated)        deps chips           progress rail
  #   row 2: 👤 worker chip                  ✓ PR#N if done       —
  printf '%s\n' "${E}[K"
  printf '%s%sPROPOSALS%s  %sfile · deps · worker · pr%s%s\n' \
    "$B" "$TEAL" "$R" "$DIM" "$R" "${E}[K"
  local card_w=$(( PANE_W - 4 ))
  (( card_w < 70 )) && card_w=70
  (( card_w > 200 )) && card_w=200

  # Helper: extract PR number from completed_summary text.
  pr_from_summary() {
    local s="$1"
    # Try `PR 1839`, `PR #1839`, `pull/1839`, `pull-1839`, `#1839`
    local n
    n=$(printf '%s' "$s" | grep -oE 'pull/[0-9]+|PR #?[0-9]+|#[0-9]+' | head -1 | grep -oE '[0-9]+' | head -1)
    printf '%s' "$n"
  }

  # Helper: a 12-cell horizontal rail colored by status.
  task_rail() {
    local status="$1" w=12 col fill
    case "$status" in
      completed) col="$G";   fill="$w" ;;
      claimed)   col="$YELLOW"; fill=$(( w / 2 )) ;;
      blocked)   col="$RED"; fill=2 ;;
      *)         col="$DIM"; fill=0 ;;
    esac
    local out="${GREY}▕${R}${col}"
    local k
    for ((k=0;k<fill;k++)); do out+="█"; done
    out+="${DIM}"
    for ((k=fill;k<w;k++)); do out+="░"; done
    out+="${GREY}▏${R}"
    printf '%s' "$out"
  }

  local IFS_OLD="$IFS"
  while IFS=$'\x1f' read -r idx status title agent description files deps summary; do
    [[ -z "$idx" ]] && continue
    local wave="${LVL[$idx]:-?}"

    # ── Card header: W{n} · sub-{i} · title  + status pill, no chip-strip clutter
    local status_word col
    case "$status" in
      completed) status_word="● done";     col="$G" ;;
      claimed)   status_word="◐ claimed";  col="$YELLOW" ;;
      blocked)   status_word="✕ blocked";  col="$RED" ;;
      *)         status_word="◇ ready";    col="$DIM" ;;
    esac
    local title_trim
    title_trim=$(trunc "$title" $(( card_w - 30 )))
    local hdr_inner="${col}${status_word}${R}  ${B}W$((wave+1))·sub-${idx}${R}  ${WHITE}${title_trim}${R}"
    local hdr_vis=$(( ${#status_word} + 2 + 9 + ${#idx} + 2 + ${#title_trim} ))
    printf '%s' "${E}[K"
    # Top border
    local fill_w=$(( card_w - 2 ))
    local top
    printf -v top '%*s' "$fill_w" ''; top="${top// /─}"
    printf '%s┌%s┐%s\n' "$GREY" "$top" "$R"
    # Header row (overlaid into card body)
    local pad=$(( card_w - 4 - hdr_vis ))
    (( pad < 0 )) && pad=0
    printf '%s│%s  %b%*s%s│%s\n' "$GREY" "$R" "$hdr_inner" "$pad" "" "$GREY" "$R"

    # ── Row A: files (compact, comma-separated, truncated)
    local files_disp="${files//,/${DIM}·${R} ${ICE}}"
    # Strip leading separators if any
    files_disp="${ICE}${files_disp}${R}"
    local files_plain="${files//,/ · }"
    local files_max=$(( card_w - 30 ))
    if (( ${#files_plain} > files_max )); then
      files_plain=$(trunc "$files_plain" "$files_max")
      files_disp="${ICE}${files_plain}${R}"
    fi
    local row_a="${DIM}📂${R} ${files_disp}"
    local row_a_vis=$(( 3 + ${#files_plain} ))
    local pad_a=$(( card_w - 4 - row_a_vis ))
    (( pad_a < 0 )) && pad_a=0
    printf '%s│%s  %b%*s%s│%s\n' "$GREY" "$R" "$row_a" "$pad_a" "" "$GREY" "$R"

    # ── Row B: deps · worker · PR · rail   (graphical row)
    local row_b="" row_b_vis=0
    if [[ -n "$deps" ]]; then
      IFS=',' read -ra DEP_ARR <<<"$deps"
      local d_idx d_stat
      for d_idx in "${DEP_ARR[@]}"; do
        d_stat="${T_STATUS[$d_idx]:-?}"
        local d_dot
        case "$d_stat" in
          completed) d_dot="${G}●${R}" ;;
          claimed)   d_dot="${YELLOW}◐${R}" ;;
          blocked)   d_dot="${RED}✕${R}" ;;
          *)         d_dot="${DIM}◇${R}" ;;
        esac
        row_b+="${d_dot}${DIM}sub-${d_idx}${R} "
        row_b_vis=$(( row_b_vis + 1 + 5 + ${#d_idx} ))
      done
      row_b+="${DIM}·${R} "
      row_b_vis=$(( row_b_vis + 2 ))
    fi
    # PR number + branch-derived agent.
    # Source priority:
    #   1. PR number from completed_summary (fast, authoritative)
    #   2. GitHub PR list lookup by title keywords (covers null summary)
    # We always do the gh lookup when claimed_by_agent is null so the worker
    # chip can show the branch-derived agent slug even when summary had the PR.
    local pr_num="" gh_agent=""
    if [[ "$status" == "completed" ]]; then
      pr_num=$(pr_from_summary "$summary")
      local need_lookup=0
      [[ -z "$pr_num" ]] && need_lookup=1
      [[ -z "$agent" || "$agent" == "null" ]] && need_lookup=1
      if (( need_lookup )); then
        local _lookup _lookup_pr _lookup_agent
        _lookup=$(lookup_pr_for_title "$title")
        if [[ -n "$_lookup" ]]; then
          _lookup_pr="${_lookup%%$'\t'*}"
          _lookup_agent="${_lookup##*$'\t'}"
          [[ -z "$pr_num"   && -n "$_lookup_pr"    ]] && pr_num="$_lookup_pr"
          [[ -z "$gh_agent" && -n "$_lookup_agent" ]] && gh_agent="$_lookup_agent"
        fi
      fi
    fi

    # Worker chip — plan.json agent wins; fall back to gh-derived branch slug.
    local worker_disp=""
    if [[ -n "$agent" && "$agent" != "null" ]]; then
      worker_disp="$agent"
    elif [[ -n "$gh_agent" ]]; then
      worker_disp="$gh_agent"
    fi
    if [[ -n "$worker_disp" ]]; then
      row_b+="${DIM}👤${R} ${ICE}${worker_disp}${R} "
      row_b_vis=$(( row_b_vis + 3 + ${#worker_disp} + 1 ))
    else
      row_b+="${DIM}👤 —${R} "
      row_b_vis=$(( row_b_vis + 5 ))
    fi
    # PR chip
    if [[ -n "$pr_num" ]]; then
      row_b+="${DIM}·${R} ${G}✓${R} ${ICE}PR#${pr_num}${R} "
      row_b_vis=$(( row_b_vis + 4 + 4 + ${#pr_num} + 1 ))
    fi
    # Rail pinned to right side — compute pad so rail lands flush right
    local rail; rail=$(task_rail "$status")
    local rail_vis=14
    local pad_b=$(( card_w - 4 - row_b_vis - rail_vis ))
    (( pad_b < 1 )) && pad_b=1
    printf '%s│%s  %b%*s%b%s│%s\n' "$GREY" "$R" "$row_b" "$pad_b" "" "$rail" "$GREY" "$R"

    # Bottom border
    printf '%s└%s┘%s\n' "$GREY" "$top" "$R"
    printf '%s\n' "${E}[K"
  done < <(pull_tasks_full)
  IFS="$IFS_OLD"

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

# Atomic-write wrapper around `_render_body`. The subshell capture also gives
# us a stable buffer to run the SGR-aware line clamp over before any byte
# reaches the terminal. One write syscall ≈ one frame.
render() {
  local f="$1" frame
  frame=$(_render_body "$f")
  # Re-detect pane width each frame so resizes don't keep wrapping.
  local pw
  pw=$(tmux display-message -p '#{pane_width}' 2>/dev/null || echo "$PANE_W")
  [[ -n "$pw" && "$pw" -gt 0 ]] && PANE_W="$pw"
  frame=$(printf '%s' "$frame" | clamp_lines_to_pane "$PANE_W")
  # Cursor-home and wipe to end-of-screen wrap the buffered frame so any
  # leftover rows from a taller previous frame disappear cleanly.
  printf '%s%s%s' "${E}[H" "$frame" "${E}[J"
}

if (( ONCE == 1 )); then
  render 0
else
  # Alternate screen buffer isolates the live render from the terminal
  # scrollback; combined with hidden cursor it gives a clean canvas. Both
  # are restored on any clean or signal-driven exit.
  printf '%s' "${E}[?1049h${E}[?25l"
  trap 'printf "%s" "${E}[?25h${E}[?1049l"; exit' INT TERM EXIT
  f=0
  while true; do
    render "$f"
    f=$((f+1))
    sleep "$INTERVAL_S"
  done
fi
