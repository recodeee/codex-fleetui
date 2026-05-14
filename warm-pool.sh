#!/usr/bin/env bash
#
# warm-pool — keep a hidden tmux window of pre-booted codex panes ready
# for fast fleet account swaps.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WINDOW="${WARM_POOL_WINDOW:-warm}"
POOL_SIZE="${WARM_POOL_SIZE:-3}"
WORK_ROOT="${CODEX_FLEET_WORK_ROOT:-/tmp/codex-fleet}"
STATE_DIR="${FLEET_STATE_DIR:-/tmp/claude-viz}"
HEALTHY_POOL="${WARM_POOL_HEALTHY_POOL:-$STATE_DIR/healthy-pool.txt}"
ACTIVE_FILE="${CODEX_FLEET_ACTIVE_FILE:-$STATE_DIR/fleet-active-accounts.txt}"
ACCOUNTS_FILE="${CODEX_FLEET_ACCOUNTS:-$SCRIPT_DIR/accounts.yml}"
PROMPT_FILE="${WARM_POOL_PROMPT:-$WORK_ROOT/warm-noop-prompt.md}"
SMOKE_TIMEOUT="${WARM_POOL_SMOKE_TIMEOUT:-120}"

usage() {
  cat <<'EOF'
Usage: warm-pool.sh <command>

Commands:
  init                         create/replenish the warm pool
  replenish                    maintain exactly WARM_POOL_SIZE warm panes
  steal <aid> <email> <prompt> move one warm pane into active service
  status                       print warm pane status
  shutdown                     kill the warm tmux window
  smoke                        boot one warm pane and wait for "tokens used"
EOF
}

log() { printf '[warm-pool] %s\n' "$*"; }
warn() { printf '[warm-pool] WARN: %s\n' "$*" >&2; }
die() { printf '[warm-pool] FATAL: %s\n' "$*" >&2; exit 1; }

require_bin() {
  command -v "$1" >/dev/null 2>&1 || die "$1 not on PATH"
}

is_positive_int() {
  [[ "${1:-}" =~ ^[1-9][0-9]*$ ]]
}

require_pool_size() {
  is_positive_int "$POOL_SIZE" || die "WARM_POOL_SIZE must be a positive integer"
}

require_tmux_session() {
  require_bin tmux
  tmux has-session -t "$SESSION" 2>/dev/null || die "tmux session not found: $SESSION"
}

window_exists() {
  tmux list-windows -t "$SESSION" -F '#{window_name}' 2>/dev/null | grep -Fxq "$WINDOW"
}

ensure_prompt_file() {
  mkdir -p "$(dirname "$PROMPT_FILE")"
  if [[ ! -f "$PROMPT_FILE" ]]; then
    cat >"$PROMPT_FILE" <<'EOF'
You are a pre-booted warm codex pane. Do not claim Colony work yet.
Wait for a wake prompt sent by scripts/codex-fleet/warm-pool.sh steal.
EOF
  fi
}

account_rows() {
  [[ -f "$ACCOUNTS_FILE" ]] || die "accounts file not found: $ACCOUNTS_FILE"
  python3 - "$ACCOUNTS_FILE" <<'PY'
import re
import sys

path = sys.argv[1]
rows = []
cur = {}
with open(path, "r", encoding="utf-8") as fh:
    for raw in fh:
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("- id:"):
            if cur.get("id") and cur.get("email"):
                rows.append(cur)
            cur = {"id": stripped.split(":", 1)[1].strip().strip("\"'")}
            continue
        if not cur:
            continue
        match = re.match(r"^([a-zA-Z_][a-zA-Z0-9_]*)\s*:\s*(.*?)$", stripped)
        if not match:
            continue
        key, value = match.group(1), match.group(2).strip().strip("\"'")
        if key in {"id", "email"}:
            cur[key] = value
if cur.get("id") and cur.get("email"):
    rows.append(cur)
for row in rows:
    print(f"{row['id']}\t{row['email']}")
PY
}

account_id_for_email() {
  local needle="$1"
  account_rows | awk -F '\t' -v email="$needle" '$2 == email { print $1; exit }'
}

active_account_ids() {
  [[ -f "$ACTIVE_FILE" ]] || return 0
  awk 'NF && $1 !~ /^#/ { print $1 }' "$ACTIVE_FILE"
}

mark_active_account() {
  local aid="$1"
  mkdir -p "$(dirname "$ACTIVE_FILE")"
  touch "$ACTIVE_FILE"
  grep -Fxq "$aid" "$ACTIVE_FILE" || printf '%s\n' "$aid" >>"$ACTIVE_FILE"
}

healthy_pool_emails() {
  [[ -f "$HEALTHY_POOL" ]] || return 0
  python3 - "$HEALTHY_POOL" <<'PY'
import re
import sys

seen = set()
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    for raw in fh:
        if raw.lstrip().startswith("#"):
            continue
        match = re.search(r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}", raw)
        if match and match.group(0) not in seen:
            seen.add(match.group(0))
            print(match.group(0))
PY
}

candidate_emails() {
  local active_tmp
  active_tmp="$(mktemp)"
  active_account_ids >"$active_tmp"
  account_rows | awk -F '\t' 'NR == FNR { active[$1] = 1; next } !($1 in active) { print $2 }' "$active_tmp" -
  rm -f "$active_tmp"
}

pick_emails() {
  local need="$1"
  local selected_tmp candidates_tmp active_tmp
  selected_tmp="$(mktemp)"
  candidates_tmp="$(mktemp)"
  active_tmp="$(mktemp)"
  active_account_ids >"$active_tmp"
  candidate_emails >"$candidates_tmp"

  while IFS= read -r email; do
    [[ -n "$email" ]] || continue
    local aid
    aid="$(account_id_for_email "$email")"
    [[ -n "$aid" ]] || continue
    grep -Fxq "$aid" "$active_tmp" && continue
    grep -Fxq "$email" "$selected_tmp" && continue
    printf '%s\n' "$email" >>"$selected_tmp"
    [[ "$(wc -l <"$selected_tmp")" -ge "$need" ]] && break
  done < <(healthy_pool_emails)

  local have
  have="$(wc -l <"$selected_tmp")"
  if [[ "$have" -lt "$need" && -x "$SCRIPT_DIR/cap-probe.sh" ]]; then
    local remaining=$((need - have))
    mapfile -t probe_candidates < <(awk 'NR == FNR { seen[$1] = 1; next } !($1 in seen) { print $1 }' "$selected_tmp" "$candidates_tmp")
    if [[ "${#probe_candidates[@]}" -gt 0 ]]; then
      bash "$SCRIPT_DIR/cap-probe.sh" "$remaining" "${probe_candidates[@]}" 2>/dev/null \
        | while IFS= read -r email; do
            [[ -n "$email" ]] || continue
            grep -Fxq "$email" "$selected_tmp" || printf '%s\n' "$email" >>"$selected_tmp"
          done
    fi
  fi

  cat "$selected_tmp"
  rm -f "$selected_tmp" "$candidates_tmp" "$active_tmp"
}

stage_home() {
  local slot="$1" email="$2"
  local home="$WORK_ROOT/warm-$slot"
  local auth="$HOME/.codex/accounts/$email.json"
  [[ -f "$auth" ]] || die "account auth file not found: $auth"
  mkdir -p "$home"
  cp -f "$auth" "$home/auth.json"
  chmod 600 "$home/auth.json"
  if [[ -f "$HOME/.codex/config.toml" ]]; then
    ln -sf "$HOME/.codex/config.toml" "$home/config.toml"
  fi
}

pane_for_slot() {
  local slot="$1"
  window_exists || return 1
  tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{@panel}' 2>/dev/null \
    | awk -F '|' -v panel="[warm-$slot]" '$2 == panel { print $1; exit }'
}

warm_panes() {
  window_exists || return 0
  tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{@panel}' 2>/dev/null \
    | awk -F '|' '$2 ~ /^\[warm-[0-9]+\]$/ { print $0 }'
}

slot_from_panel() {
  sed -n 's/^\[warm-\([0-9][0-9]*\)\]$/\1/p' <<<"$1"
}

is_pane_alive() {
  local pane_id="$1"
  local dead
  dead="$(tmux capture-pane -p -t "$pane_id" -S -40 2>/dev/null || true)"
  case "$dead" in
    *"[Process completed]"*|*"[Process exited"*|*"[exited]"*|*"session has ended"*)
      return 1 ;;
  esac
  return 0
}

spawn_slot() {
  local slot="$1" email="$2"
  stage_home "$slot" "$email"
  ensure_prompt_file

  local home="$WORK_ROOT/warm-$slot"
  local pane_cmd
  pane_cmd="env CODEX_GUARD_BYPASS=1 CODEX_HOME='$home' CODEX_FLEET_AGENT_NAME='codex-warm-$slot' CODEX_FLEET_ACCOUNT_EMAIL='$email' codex \"\$(cat '$PROMPT_FILE')\""

  local pane_id
  if window_exists; then
    pane_id="$(tmux split-window -t "$SESSION:$WINDOW" -P -F '#{pane_id}' "$pane_cmd")"
    tmux select-layout -t "$SESSION:$WINDOW" tiled >/dev/null 2>&1 || true
  else
    pane_id="$(tmux new-window -d -P -F '#{pane_id}' -t "$SESSION:" -n "$WINDOW" "$pane_cmd")"
    tmux set-option -w -t "$SESSION:$WINDOW" '@codex_fleet_warm_pool' '1' >/dev/null 2>&1 || true
  fi

  tmux set-option -p -t "$pane_id" '@panel' "[warm-$slot]" >/dev/null 2>&1 || true
  tmux set-option -p -t "$pane_id" '@warm_slot' "$slot" >/dev/null 2>&1 || true
  tmux set-option -p -t "$pane_id" '@warm_email' "$email" >/dev/null 2>&1 || true
  log "spawned slot=$slot pane=$pane_id email=$email"
}

prune_extra_panes() {
  local row pane_id panel slot seen_slots=" "
  while IFS='|' read -r pane_id panel; do
    [[ -n "$pane_id" ]] || continue
    slot="$(slot_from_panel "$panel")"
    if [[ -z "$slot" || "$slot" -gt "$POOL_SIZE" || "$seen_slots" == *" $slot "* ]]; then
      tmux kill-pane -t "$pane_id" >/dev/null 2>&1 || true
      log "removed extra warm pane=$pane_id panel=$panel"
    else
      seen_slots="$seen_slots$slot "
    fi
  done < <(warm_panes)
}

replenish() {
  require_pool_size
  require_tmux_session
  prune_extra_panes

  local missing=()
  local slot pane_id
  for ((slot = 1; slot <= POOL_SIZE; slot++)); do
    pane_id="$(pane_for_slot "$slot" || true)"
    if [[ -z "$pane_id" ]] || ! is_pane_alive "$pane_id"; then
      [[ -n "$pane_id" ]] && tmux kill-pane -t "$pane_id" >/dev/null 2>&1 || true
      missing+=("$slot")
    fi
  done

  if [[ "${#missing[@]}" -eq 0 ]]; then
    log "pool already full: size=$POOL_SIZE"
    return 0
  fi

  mapfile -t emails < <(pick_emails "${#missing[@]}")
  if [[ "${#emails[@]}" -lt "${#missing[@]}" ]]; then
    die "only ${#emails[@]}/${#missing[@]} healthy accounts available for warm pool"
  fi

  local i
  for i in "${!missing[@]}"; do
    spawn_slot "${missing[$i]}" "${emails[$i]}"
  done
}

status() {
  require_pool_size
  require_bin tmux
  if ! tmux has-session -t "$SESSION" 2>/dev/null || ! window_exists; then
    printf 'session=%s window=%s status=absent size=%s\n' "$SESSION" "$WINDOW" "$POOL_SIZE"
    return 0
  fi

  printf 'session=%s window=%s size=%s\n' "$SESSION" "$WINDOW" "$POOL_SIZE"
  tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{pane_index}|#{pane_current_command}|#{@panel}|#{@warm_email}' \
    | while IFS='|' read -r pane_id pane_index command panel email; do
        case "$panel" in
          "[warm-"*"]") printf 'pane=%s index=%s panel=%s email=%s command=%s\n' "$pane_id" "$pane_index" "$panel" "${email:-unknown}" "$command" ;;
        esac
      done
}

steal() {
  local new_aid="${1:-}" new_email="${2:-}" wake_prompt="${3:-}"
  [[ -n "$new_aid" && -n "$new_email" && -n "$wake_prompt" ]] || die "usage: warm-pool.sh steal <new-aid> <new-email> <wake-prompt-path>"
  [[ -f "$wake_prompt" ]] || die "wake prompt not found: $wake_prompt"
  require_tmux_session
  window_exists || die "warm window not found: $SESSION:$WINDOW"

  local pane_id="" row_pane row_panel row_email
  while IFS='|' read -r row_pane row_panel row_email; do
    [[ -n "$row_pane" ]] || continue
    if [[ "$row_email" == "$new_email" ]]; then
      pane_id="$row_pane"
      break
    fi
    [[ -z "$pane_id" ]] && pane_id="$row_pane"
  done < <(tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}|#{@panel}|#{@warm_email}' 2>/dev/null | awk -F '|' '$2 ~ /^\[warm-[0-9]+\]$/')

  [[ -n "$pane_id" ]] || die "no warm pane available to steal"
  local warm_slot
  warm_slot="$(tmux show-option -pqv -t "$pane_id" '@warm_slot' 2>/dev/null || true)"
  row_email="$(tmux show-option -pqv -t "$pane_id" '@warm_email' 2>/dev/null || true)"
  if [[ -n "$row_email" && "$row_email" != "$new_email" ]]; then
    warn "stealing pane with email=$row_email for requested email=$new_email"
  fi

  tmux set-option -p -t "$pane_id" '@panel' "[codex-$new_aid]" >/dev/null 2>&1 || true
  tmux set-option -p -t "$pane_id" '@warm_stolen' '1' >/dev/null 2>&1 || true
  mark_active_account "$new_aid"
  tmux send-keys -t "$pane_id" -l "You are now codex-$new_aid. Use agent=codex-$new_aid and account=$new_email for all Colony calls."
  tmux send-keys -t "$pane_id" Enter
  tmux send-keys -t "$pane_id" -l "$(cat "$wake_prompt")"
  tmux send-keys -t "$pane_id" Enter
  log "stole pane=$pane_id warm_slot=${warm_slot:-unknown} -> codex-$new_aid"

  replenish
}

shutdown() {
  require_bin tmux
  if tmux has-session -t "$SESSION" 2>/dev/null && window_exists; then
    tmux kill-window -t "$SESSION:$WINDOW"
    log "shutdown $SESSION:$WINDOW"
  else
    log "warm pool already absent"
  fi
}

smoke() {
  require_bin tmux
  require_pool_size
  local smoke_session="${SESSION}-warm-smoke-$$"
  SESSION="$smoke_session"
  POOL_SIZE=1

  tmux new-session -d -s "$SESSION" -n seed "sleep 3600"
  trap 'tmux kill-session -t "$SESSION" >/dev/null 2>&1 || true' EXIT
  replenish

  local deadline=$((SECONDS + SMOKE_TIMEOUT))
  local pane_id text
  pane_id="$(pane_for_slot 1 || true)"
  [[ -n "$pane_id" ]] || die "smoke failed to create warm pane"
  while [[ "$SECONDS" -lt "$deadline" ]]; do
    text="$(tmux capture-pane -p -t "$pane_id" -S -120 2>/dev/null || true)"
    if grep -qi 'tokens used' <<<"$text"; then
      log "smoke ok: pane=$pane_id reached tokens used"
      return 0
    fi
    sleep 2
  done
  die "smoke timed out after ${SMOKE_TIMEOUT}s waiting for tokens used"
}

main() {
  local cmd="${1:-}"
  case "$cmd" in
    init|replenish) replenish ;;
    steal) shift; steal "$@" ;;
    status) status ;;
    shutdown) shutdown ;;
    smoke) smoke ;;
    -h|--help|help|"") usage ;;
    *) usage >&2; die "unknown command: $cmd" ;;
  esac
}

main "$@"
