#!/usr/bin/env bash
# review-queue — event-sourced producer for the Review tab (review-anim.sh).
#
# Event log:   $REVIEW_EVENTS_LOG (default /tmp/claude-viz/review-events.jsonl)
#                One JSON event per line. Append-only; never edited in place.
# Snapshot:    $REVIEW_QUEUE_JSON (default /tmp/claude-viz/live-review-queue.json)
#                Collapsed current view that review-anim.sh consumes.
#
# Subcommands:
#   emit-pending  --id ID --title T --agent A [--pane P] [--risk low|medium|high]
#                 [--auth low|medium|high] [--rationale R] [--file F ...]
#   emit-decided  --id ID --outcome approved|escalated|denied
#                 [--reviewer R]
#   build         Read the event log and write the current queue snapshot.
#                 The snapshot's `decisions` rail is windowed to the last
#                 $REVIEW_DECISIONS_WINDOW_MIN minutes (default 30).
#   daemon        Run `build` every $REVIEW_BUILD_INTERVAL_S seconds (default 5).
#                 PID file at $REVIEW_BUILD_PID_FILE for clean shutdown.
#   show          Print the current snapshot to stdout (for debugging).
#   clear         Truncate the event log (testing only; refuses on a non-tmp path).
#
# Design notes:
# - Event-sourced so any number of producers (codex hooks, manual scripts,
#   a future pane scanner) can append without coordination beyond flock-on-
#   append. `build` is idempotent; running it twice produces the same JSON.
# - The renderer only ever reads `live-review-queue.json`. It never reads the
#   raw event log. This keeps the renderer fast and the contract narrow.
# - "Approved today" counts approvals with `at >= today 00:00 local`. The
#   "decisions" rail is windowed to the recent past because older decisions
#   move off-screen in the UI.

set -eo pipefail

REVIEW_EVENTS_LOG="${REVIEW_EVENTS_LOG:-/tmp/claude-viz/review-events.jsonl}"
REVIEW_QUEUE_JSON="${REVIEW_QUEUE_JSON:-/tmp/claude-viz/live-review-queue.json}"
REVIEW_DECISIONS_WINDOW_MIN="${REVIEW_DECISIONS_WINDOW_MIN:-30}"
REVIEW_BUILD_INTERVAL_S="${REVIEW_BUILD_INTERVAL_S:-5}"
REVIEW_BUILD_PID_FILE="${REVIEW_BUILD_PID_FILE:-/tmp/claude-viz/review-queue.pid}"

die() { printf 'review-queue: %s\n' "$*" >&2; exit 2; }

require_jq() {
  command -v jq >/dev/null 2>&1 || die "jq is required"
}

ensure_log_dir() {
  mkdir -p "$(dirname "$REVIEW_EVENTS_LOG")" "$(dirname "$REVIEW_QUEUE_JSON")"
  [[ -e "$REVIEW_EVENTS_LOG" ]] || : > "$REVIEW_EVENTS_LOG"
}

now_iso() { date -u '+%Y-%m-%dT%H:%M:%SZ'; }
now_epoch() { date '+%s'; }

# Single-writer append using flock so concurrent producers can't interleave a
# line. flock holds the lock across the redirection block.
append_event() {
  local payload="$1"
  ensure_log_dir
  (
    flock -x 9
    printf '%s\n' "$payload" >&9
  ) 9>>"$REVIEW_EVENTS_LOG"
}

# ── emit-pending ──────────────────────────────────────────────────────────────
cmd_emit_pending() {
  require_jq
  local id="" title="" agent="" pane=""
  local risk="low" auth="low" rationale=""
  local -a files=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --id)        id="$2"; shift 2 ;;
      --title)     title="$2"; shift 2 ;;
      --agent)     agent="$2"; shift 2 ;;
      --pane)      pane="$2"; shift 2 ;;
      --risk)      risk="$2"; shift 2 ;;
      --auth)      auth="$2"; shift 2 ;;
      --rationale) rationale="$2"; shift 2 ;;
      --file)      files+=("$2"); shift 2 ;;
      *) die "emit-pending: unknown flag: $1" ;;
    esac
  done
  [[ -n "$id"    ]] || die "emit-pending: --id required"
  [[ -n "$title" ]] || die "emit-pending: --title required"
  [[ -n "$agent" ]] || die "emit-pending: --agent required"

  local files_json
  files_json=$(printf '%s\n' "${files[@]}" | jq -R . | jq -s .)
  local pane_arg
  if [[ -n "$pane" ]]; then pane_arg="$pane"; else pane_arg=null; fi
  local event
  event=$(jq -nc \
    --arg kind "pending" \
    --arg id "$id" \
    --arg at "$(now_iso)" \
    --argjson epoch "$(now_epoch)" \
    --arg title "$title" \
    --arg agent "$agent" \
    --argjson pane "$pane_arg" \
    --arg risk "$risk" \
    --arg auth "$auth" \
    --arg rationale "$rationale" \
    --argjson files "$files_json" \
    '{kind:$kind, id:$id, at:$at, epoch:$epoch, title:$title, agent:$agent,
      pane:$pane, risk:$risk, auth:$auth, rationale:$rationale, files:$files}')
  append_event "$event"
  printf '%s\n' "$event"
}

# ── emit-decided ──────────────────────────────────────────────────────────────
cmd_emit_decided() {
  require_jq
  local id="" outcome="" reviewer=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --id)       id="$2"; shift 2 ;;
      --outcome)  outcome="$2"; shift 2 ;;
      --reviewer) reviewer="$2"; shift 2 ;;
      *) die "emit-decided: unknown flag: $1" ;;
    esac
  done
  [[ -n "$id"      ]] || die "emit-decided: --id required"
  [[ -n "$outcome" ]] || die "emit-decided: --outcome required"
  case "$outcome" in
    approved|escalated|denied) ;;
    *) die "emit-decided: --outcome must be approved|escalated|denied" ;;
  esac

  local event
  event=$(jq -nc \
    --arg kind "decided" \
    --arg id "$id" \
    --arg at "$(now_iso)" \
    --argjson epoch "$(now_epoch)" \
    --arg outcome "$outcome" \
    --arg reviewer "$reviewer" \
    '{kind:$kind, id:$id, at:$at, epoch:$epoch, outcome:$outcome,
      reviewer:$reviewer}')
  append_event "$event"
  printf '%s\n' "$event"
}

# ── build (event log → snapshot) ──────────────────────────────────────────────
cmd_build() {
  require_jq
  ensure_log_dir

  local now today_epoch decisions_cutoff_epoch
  now=$(now_epoch)
  today_epoch=$(date -d "$(date '+%Y-%m-%d') 00:00:00" '+%s' 2>/dev/null \
    || python3 -c 'import datetime as d; t=d.datetime.now().replace(hour=0,minute=0,second=0,microsecond=0); print(int(t.timestamp()))')
  decisions_cutoff_epoch=$(( now - REVIEW_DECISIONS_WINDOW_MIN * 60 ))

  local snapshot
  snapshot=$(jq -sc \
    --argjson now "$now" \
    --argjson today "$today_epoch" \
    --argjson cutoff "$decisions_cutoff_epoch" \
    --argjson window_min "$REVIEW_DECISIONS_WINDOW_MIN" '
    # Each event must be an object with .id.
    map(select(type == "object" and .id != null))
    | . as $events
    # Latest event per id (events are append-ordered, so last wins).
    | reduce $events[] as $e ({}; .[$e.id] = $e)
    | to_entries
    | map(.value) as $latest
    | {
        approved_today:
          ($events
            | map(select(.kind == "decided" and .outcome == "approved" and (.epoch // 0) >= $today))
            | length),
        pending:
          ($latest
            | map(select(.kind == "pending"))
            | sort_by(.epoch // 0)
            | map(. + {age_seconds: ($now - (.epoch // $now))})),
        decisions:
          ($events
            | map(select(.kind == "decided" and (.epoch // 0) >= $cutoff))
            # Hydrate each decision with its matching pending row for context.
            | map(. as $d
                | ($events
                    | map(select(.id == $d.id and .kind == "pending"))
                    | last) as $p
                | {
                    cmd:     ($p.title // $d.id),
                    agent:   ($p.agent // ""),
                    age_minutes: (($now - (.epoch // $now)) / 60 | floor),
                    risk:    ($p.risk // "low"),
                    outcome: .outcome
                  })
            | sort_by(.age_minutes)
            | .[0:12]),
        decisions_window_minutes: $window_min
      }
  ' "$REVIEW_EVENTS_LOG")

  # Atomic write so a half-written file never lands in the renderer's read path.
  local tmp="${REVIEW_QUEUE_JSON}.tmp.$$"
  printf '%s\n' "$snapshot" > "$tmp"
  mv "$tmp" "$REVIEW_QUEUE_JSON"
}

cmd_show() {
  require_jq
  if [[ ! -r "$REVIEW_QUEUE_JSON" ]]; then
    cmd_build
  fi
  jq . "$REVIEW_QUEUE_JSON"
}

cmd_daemon() {
  ensure_log_dir
  echo $$ > "$REVIEW_BUILD_PID_FILE"
  trap 'rm -f "$REVIEW_BUILD_PID_FILE"; exit 0' INT TERM EXIT
  while true; do
    cmd_build || printf 'review-queue daemon: build failed at %s\n' "$(now_iso)" >&2
    sleep "$REVIEW_BUILD_INTERVAL_S"
  done
}

cmd_clear() {
  # Only allow clearing inside a tmp-style directory so a fat-fingered call
  # cannot delete events from a non-test path.
  case "$REVIEW_EVENTS_LOG" in
    /tmp/*|"${TMPDIR:-/tmp}"/*) : > "$REVIEW_EVENTS_LOG" ;;
    *) die "clear: refusing to truncate non-tmp log: $REVIEW_EVENTS_LOG" ;;
  esac
}

main() {
  local sub="${1:-}"
  shift || true
  case "$sub" in
    emit-pending) cmd_emit_pending "$@" ;;
    emit-decided) cmd_emit_decided "$@" ;;
    build)        cmd_build ;;
    show)         cmd_show ;;
    daemon)       cmd_daemon ;;
    clear)        cmd_clear ;;
    ""|-h|--help|help)
      sed -n '2,30p' "$0"
      ;;
    *) die "unknown subcommand: $sub" ;;
  esac
}

main "$@"
