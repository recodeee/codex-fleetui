#!/usr/bin/env bash
# review-pane-scanner — turns Codex auto-reviewer output into review-queue events.
#
# Walks the panes in the codex-fleet worker window, captures their visible
# content, and looks for the Codex auto-reviewer block:
#
#     ⚠ Automatic approval review <approved|denied|escalated>
#     (risk: <low|medium|high>, authorization: <low|medium|high>)
#     ✓ Request approved for <command>            ← optional outcome line
#       <trailing-detail>                          ← optional trailing line
#
# When a new block appears, the scanner emits a `pending` event via
# review-queue.sh and (when an outcome is visible) immediately emits the
# matching `decided` event so the Recent Decisions rail in review-anim.sh
# reflects it. Already-seen blocks are kept in $REVIEW_SCANNER_STATE so the
# scanner is safe to run on a tight tick.
#
# Discovery:
#   $REVIEW_SCANNER_SESSION  tmux session (default codex-fleet)
#   $REVIEW_SCANNER_WINDOW   tmux window  (default overview — the codex worker pane window)
#   $REVIEW_SCANNER_AGENT_FMT format string for the agent name. Receives the
#                            pane id as %s; default "codex-pane-%s" so naive
#                            installs still emit a stable agent slug even when
#                            the pane title is not yet a codex-* name. If the
#                            pane title looks like a codex-* agent it is used
#                            verbatim and this fallback is ignored.
#
# Modes:
#   default          loop: capture → match → emit → sleep $INTERVAL_S
#   --once           single pass for tests / cron
#   --dry-run        match + log to stderr, do not emit events
#   --fixture FILE   read a single pane's captured text from FILE instead of
#                    calling tmux. Pairs with --once / --dry-run for tests.
#
# Tunables:
#   $REVIEW_SCANNER_INTERVAL_S   tick (default 2)
#   $REVIEW_SCANNER_TAIL_LINES   capture-pane -p -S -<N> tail size (default 200)
#   $REVIEW_SCANNER_STATE        seen-id state file (default /tmp/claude-viz/review-scanner-state.txt)

set -eo pipefail

ONCE=0
DRY=0
FIXTURE=""
FIXTURE_AGENT="codex-fixture"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --once) ONCE=1; shift ;;
    --dry-run) DRY=1; shift ;;
    --fixture) FIXTURE="$2"; ONCE=1; shift 2 ;;
    --fixture-agent) FIXTURE_AGENT="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,40p' "$0"
      exit 0
      ;;
    *) printf 'review-pane-scanner: unknown flag: %s\n' "$1" >&2; exit 2 ;;
  esac
done

REVIEW_SCANNER_SESSION="${REVIEW_SCANNER_SESSION:-codex-fleet}"
REVIEW_SCANNER_WINDOW="${REVIEW_SCANNER_WINDOW:-overview}"
REVIEW_SCANNER_AGENT_FMT="${REVIEW_SCANNER_AGENT_FMT:-codex-pane-%s}"
REVIEW_SCANNER_INTERVAL_S="${REVIEW_SCANNER_INTERVAL_S:-2}"
REVIEW_SCANNER_TAIL_LINES="${REVIEW_SCANNER_TAIL_LINES:-200}"
REVIEW_SCANNER_STATE="${REVIEW_SCANNER_STATE:-/tmp/claude-viz/review-scanner-state.txt}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
QUEUE_SH="${REVIEW_QUEUE_SH:-$SCRIPT_DIR/review-queue.sh}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { printf 'review-pane-scanner: missing required cmd: %s\n' "$1" >&2; exit 2; }
}
require_cmd jq

ensure_state() {
  mkdir -p "$(dirname "$REVIEW_SCANNER_STATE")"
  [[ -e "$REVIEW_SCANNER_STATE" ]] || : > "$REVIEW_SCANNER_STATE"
}

seen_id() {
  local id="$1"
  grep -Fxq "$id" "$REVIEW_SCANNER_STATE" 2>/dev/null
}

mark_seen() {
  printf '%s\n' "$1" >> "$REVIEW_SCANNER_STATE"
}

# Deterministic short id from agent + the canonical match payload. Uses sha1
# because md5sum is not always installed (busybox / minimal containers).
make_id() {
  local agent="$1" payload="$2"
  local h
  h=$(printf '%s\0%s' "$agent" "$payload" | sha1sum | cut -c1-6 | tr 'a-z' 'A-Z')
  printf 'REV-%s' "$h"
}

# `tmux list-panes` for the target window. Each line is "<pane_id>\t<pane_title>".
list_codex_panes() {
  if [[ -n "$FIXTURE" ]]; then
    printf '%%fixture\t%s\n' "$FIXTURE_AGENT"
    return
  fi
  command -v tmux >/dev/null 2>&1 || return 0
  tmux list-panes -t "$REVIEW_SCANNER_SESSION:$REVIEW_SCANNER_WINDOW" \
    -F '#{pane_id}'$'\t''#{pane_title}' 2>/dev/null || true
}

capture_pane_text() {
  local pane_id="$1"
  if [[ -n "$FIXTURE" ]]; then
    cat "$FIXTURE"
    return
  fi
  tmux capture-pane -p -J -t "$pane_id" -S "-$REVIEW_SCANNER_TAIL_LINES" 2>/dev/null || true
}

resolve_agent() {
  local pane_id="$1" pane_title="$2"
  # If the pane title already looks like a codex-* agent, trust it.
  if [[ "$pane_title" == codex-* ]]; then
    printf '%s' "$pane_title"
    return
  fi
  # Else derive a stable slug from the pane id.
  # shellcheck disable=SC2059
  printf "$REVIEW_SCANNER_AGENT_FMT" "${pane_id#%}"
}

emit() {
  local subcmd="$1"; shift
  if (( DRY == 1 )); then
    printf '[dry-run] %s' "$subcmd" >&2
    for arg in "$@"; do printf ' %q' "$arg" >&2; done
    printf '\n' >&2
    return 0
  fi
  bash "$QUEUE_SH" "$subcmd" "$@" >/dev/null
}

# Parse one block starting at $1 (line number, 1-based) of the captured text
# stored in array $TEXT. Writes the parsed fields to globals:
#   PARSED_OUTCOME  approved|denied|escalated|pending
#   PARSED_RISK     low|medium|high
#   PARSED_AUTH     low|medium|high
#   PARSED_TITLE    cmd / change description
#   PARSED_PAYLOAD  the raw 4-line block used as the dedup payload
#
# Returns 0 on a valid parse, 1 otherwise.
parse_block() {
  local idx="$1"
  PARSED_OUTCOME=""
  PARSED_RISK="low"
  PARSED_AUTH="low"
  PARSED_TITLE=""
  PARSED_PAYLOAD=""

  local trigger="${TEXT[$idx]}"
  case "$trigger" in
    *"Automatic approval review approved"*)   PARSED_OUTCOME="approved" ;;
    *"Automatic approval review denied"*)     PARSED_OUTCOME="denied" ;;
    *"Automatic approval review escalated"*)  PARSED_OUTCOME="escalated" ;;
    *"Automatic approval review pending"*)    PARSED_OUTCOME="pending" ;;
    *"Approval required"*)                    PARSED_OUTCOME="pending" ;;
    *) return 1 ;;
  esac

  # Risk/auth tuple is the next non-blank line within the next 3 rows.
  local i j riskline=""
  for (( j=idx+1; j<idx+5 && j<${#TEXT[@]}; j++ )); do
    if [[ "${TEXT[$j]}" =~ \(risk:[[:space:]]*([a-z]+),[[:space:]]*authorization:[[:space:]]*([a-z]+)\) ]]; then
      PARSED_RISK="${BASH_REMATCH[1]}"
      PARSED_AUTH="${BASH_REMATCH[2]}"
      riskline="${TEXT[$j]}"
      break
    fi
  done
  [[ -z "$riskline" ]] && return 1

  # Title comes from the next "✓ Request approved for X" / "✗ Request denied for X"
  # line, plus an optional trailing detail line. When the outcome is "pending"
  # there may be no ✓/✗ — fall back to the trigger line.
  local title_main="" title_tail=""
  for (( i=j+1; i<j+5 && i<${#TEXT[@]}; i++ )); do
    local L="${TEXT[$i]}"
    if [[ "$L" == *"Request approved for "* ]]; then
      title_main="${L#*Request approved for }"
      title_main="${title_main## }"
      # Trailing detail = the next non-empty line that is not another section.
      if (( i+1 < ${#TEXT[@]} )); then
        local nxt="${TEXT[$((i+1))]}"
        nxt="${nxt## }"; nxt="${nxt%% }"
        if [[ -n "$nxt" && "$nxt" != "●"* && "$nxt" != "⚠"* && "$nxt" != "✓"* ]]; then
          title_tail="$nxt"
        fi
      fi
      break
    elif [[ "$L" == *"Request denied for "* ]]; then
      title_main="${L#*Request denied for }"
      break
    fi
  done
  if [[ -z "$title_main" ]]; then
    title_main="$trigger"
  fi
  if [[ -n "$title_tail" ]]; then
    PARSED_TITLE="$title_main $title_tail"
  else
    PARSED_TITLE="$title_main"
  fi
  # Trim. Title is what shows in the rail / pending card.
  PARSED_TITLE="${PARSED_TITLE## }"
  PARSED_TITLE="${PARSED_TITLE%% }"

  # Dedup payload combines the trigger + risk/auth + title so two different
  # commands at the same risk level get distinct ids.
  PARSED_PAYLOAD="$trigger|$riskline|$PARSED_TITLE"
  return 0
}

scan_one_pane() {
  local pane_id="$1" agent="$2"
  local content
  content=$(capture_pane_text "$pane_id")
  [[ -z "$content" ]] && return 0

  # Load into array; strip trailing whitespace from each line for stable matching.
  TEXT=()
  while IFS= read -r line; do
    TEXT+=("${line%% }")
  done <<< "$content"

  local i
  for (( i=0; i<${#TEXT[@]}; i++ )); do
    if [[ "${TEXT[$i]}" == *"Automatic approval review"* || "${TEXT[$i]}" == *"Approval required"* ]]; then
      if parse_block "$i"; then
        local id
        id=$(make_id "$agent" "$PARSED_PAYLOAD")
        if seen_id "$id"; then
          continue
        fi
        # emit-pending first so the renderer briefly shows the awaiting card.
        emit emit-pending \
          --id "$id" \
          --title "$PARSED_TITLE" \
          --agent "$agent" \
          --pane "${pane_id#%}" \
          --risk "$PARSED_RISK" \
          --auth "$PARSED_AUTH" \
          --rationale "Captured from Codex auto-reviewer output on $agent."
        # When the outcome is already known (the visible line says approved/
        # denied/escalated), emit the decided event immediately so the rail
        # reflects it.
        case "$PARSED_OUTCOME" in
          approved|denied|escalated)
            emit emit-decided --id "$id" --outcome "$PARSED_OUTCOME" --reviewer auto
            ;;
        esac
        mark_seen "$id"
        # Rebuild snapshot once per detected block so the renderer picks up
        # the change without waiting for the review-queue daemon tick.
        if (( DRY == 0 )); then
          bash "$QUEUE_SH" build >/dev/null 2>&1 || true
        fi
      fi
    fi
  done
}

scan_once() {
  ensure_state
  local line
  while IFS=$'\t' read -r pane_id pane_title; do
    [[ -z "$pane_id" ]] && continue
    local agent
    agent=$(resolve_agent "$pane_id" "$pane_title")
    scan_one_pane "$pane_id" "$agent"
  done < <(list_codex_panes)
}

if (( ONCE == 1 )); then
  scan_once
else
  trap 'exit 0' INT TERM
  while true; do
    scan_once || printf 'review-pane-scanner: scan failed at %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" >&2
    sleep "$REVIEW_SCANNER_INTERVAL_S"
  done
fi
