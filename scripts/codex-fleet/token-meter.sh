#!/usr/bin/env bash
# token-meter.sh — per-pane spend snapshot for codex-fleet tmux.
# pane /proc env + agent-auth list + tmux capture-pane → sorted table/JSON.

set -u; LC_ALL=C

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"
SESSION="codex-fleet"; USE_COLOR=1; MODE="table"; WATCH=0

usage() {
  cat <<'EOF'
token-meter.sh — codex-fleet per-pane spend snapshot
usage: bash scripts/codex-fleet/token-meter.sh [flags]
  --session <name>  tmux session (default codex-fleet)
  --no-color        disable ANSI
  --json            emit JSON array
  --watch           refresh every 5s, Ctrl-C exits
  --help            this help
cols: agent | account | 5h% | wk% | ctx% | tasks-done | status
sort asc by 5h% (lowest headroom first). red when 5h%<20 OR wk%<15 OR ctx%<15.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --session) SESSION="${2:-codex-fleet}"; shift 2 ;;
    --no-color) USE_COLOR=0; shift ;;
    --json) MODE="json"; shift ;;
    --watch) WATCH=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown flag: $1" >&2; usage >&2; exit 2 ;;
  esac
done
[ -t 1 ] || USE_COLOR=0
[ "$MODE" = "json" ] && USE_COLOR=0
c_red=""; c_dim=""; c_bold=""; c_reset=""
if [ "$USE_COLOR" = "1" ]; then
  c_red=$'\033[31m'; c_dim=$'\033[2m'; c_bold=$'\033[1m'; c_reset=$'\033[0m'
fi

# pull `agent-auth list` once; cache as email\t5h\twk per line
fetch_auth() {
  agent-auth list 2>/dev/null \
    | awk '/type=ChatGPT/ {
        email=""; fh="n/a"; wk="n/a";
        for (i=1;i<=NF;i++) {
          if ($i ~ /@/) email=$i;
          else if ($i ~ /^5h=/) { sub(/^5h=/,"",$i); fh=$i }
          else if ($i ~ /^weekly=/) { sub(/^weekly=/,"",$i); wk=$i }
        }
        if (email != "") print email"\t"fh"\t"wk
      }'
}

# tmux pane env via /proc/<pid>/environ — works even when pane is busy.
pane_env() {
  local pid="$1" key="$2"
  [ -r "/proc/$pid/environ" ] || { echo ""; return; }
  tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null \
    | awk -F= -v k="$key" '$1==k {print substr($0,length(k)+2); exit}'
}

# walk descendant pids (BFS) then resolve agent/email from /proc env.
resolve_pane() {
  local root="${1:-}" agent="" email="" q next p kids
  [ -z "$root" ] && { printf 'n/a\tn/a\n'; return; }
  q="$root"; next=""
  while [ -n "$q" ]; do
    next=""
    for p in $q; do
      if [ -z "$agent" ]; then agent=$(pane_env "$p" CODEX_FLEET_AGENT_NAME); fi
      if [ -z "$email" ]; then email=$(pane_env "$p" CODEX_FLEET_ACCOUNT_EMAIL); fi
      [ -n "$agent" ] && [ -n "$email" ] && break 2
      kids=$(pgrep -P "$p" 2>/dev/null || true)
      [ -n "$kids" ] && next="$next $kids"
    done
    q="$next"
  done
  printf '%s\t%s\n' "${agent:-n/a}" "${email:-n/a}"
}

# scrape ctx% + status from pane tail.
scrape_pane() {
  local target="$1"
  local buf ctx="n/a" status="idle"
  buf=$(tmux capture-pane -p -t "$target" -S -60 2>/dev/null || true)
  [ -z "$buf" ] && { printf '%s\t%s\n' "$ctx" "$status"; return; }
  local left
  left=$(printf '%s' "$buf" | grep -oE 'Context[[:space:]]+[0-9]+%' | tail -1 | grep -oE '[0-9]+' || true)
  [ -n "$left" ] && ctx="$((100 - left))%"
  if printf '%s' "$buf" | grep -qiE 'rate[- ]?limit|rate_limit|429'; then status="rate-limited"
  elif printf '%s' "$buf" | grep -qiE 'esc to interrupt|^[[:space:]]*Working'; then status="working"
  elif printf '%s' "$buf" | grep -qiE 'blocked|BLOCKED:'; then status="blocked"
  fi
  printf '%s\t%s\n' "$ctx" "$status"
}

pct_num() {  # "62%" → 62; "n/a" → -1
  case "$1" in n/a|"") echo -1 ;; *) echo "${1%\%}" ;; esac
}

is_hot() {
  # agent-auth's 5h% / weekly% report REMAINING quota; codex's ctx% reports
  # REMAINING context. Low values = about to wedge → red. (Earlier draft had
  # these inverted because the design brief used "spend %" semantics.)
  local fh wk ctx
  fh=$(pct_num "$1"); wk=$(pct_num "$2"); ctx=$(pct_num "$3")
  [ "$fh" -ge 0 ] && [ "$fh" -lt 20 ] 2>/dev/null && return 0
  [ "$wk" -ge 0 ] && [ "$wk" -lt 15 ] 2>/dev/null && return 0
  [ "$ctx" -ge 0 ] && [ "$ctx" -lt 15 ] 2>/dev/null && return 0
  return 1
}

collect() {  # emits TSV: agent  email  5h  wk  ctx  tasks  status
  local panes auth_tsv
  panes=$(tmux list-panes -t "$SESSION:0" -F '#{pane_id} #{pane_pid}' 2>/dev/null || true)
  if [ -z "$panes" ]; then
    echo "no panes for session '$SESSION' (try --session <name>)" >&2
    return 1
  fi
  auth_tsv=$(fetch_auth)
  while IFS=' ' read -r pid_tgt pid_pane; do
    [ -z "$pid_pane" ] && continue
    local rline agent email
    rline=$(resolve_pane "$pid_pane")
    agent=$(printf '%s' "$rline" | cut -f1)
    email=$(printf '%s' "$rline" | cut -f2)
    [ "$agent" = "n/a" ] && continue   # skip non-codex panes
    local fh="n/a" wk="n/a"
    if [ -n "$auth_tsv" ] && [ "$email" != "n/a" ]; then
      local row
      row=$(printf '%s\n' "$auth_tsv" | awk -F'\t' -v e="$email" '$1==e {print $2"\t"$3; exit}')
      [ -n "$row" ] && { fh=$(printf '%s' "$row" | cut -f1); wk=$(printf '%s' "$row" | cut -f2); }
    fi
    local sline ctx status
    sline=$(scrape_pane "$pid_tgt")
    ctx=$(printf '%s' "$sline" | cut -f1)
    status=$(printf '%s' "$sline" | cut -f2)
    local tasks="n/a"   # colony CLI exposes no per-agent count; fallback only
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$agent" "$email" "$fh" "$wk" "$ctx" "$tasks" "$status"
  done <<<"$panes" \
    | sort -t$'\t' -k3,3 -n  # sort asc by 5h% (lowest headroom first → spend-watch panes on top)
}

render_table() {
  local tsv="$1"
  local ts; ts=$(date '+%Y-%m-%d %H:%M')
  printf '%s%s%s — %s\n' "$c_bold" "codex-fleet token meter" "$c_reset" "$ts"
  printf '%-28s %-28s %-6s %-6s %-6s %-11s %s\n' \
    "agent" "account" "5h%" "wk%" "ctx%" "tasks-done" "status"
  [ -z "$tsv" ] && { printf '%s(no panes)%s\n' "$c_dim" "$c_reset"; return; }
  while IFS=$'\t' read -r agent email fh wk ctx tasks status; do
    [ -z "$agent" ] && continue
    local pre="" post=""
    if is_hot "$fh" "$wk" "$ctx"; then pre="$c_red"; post="$c_reset"; fi
    printf '%s%-28s %-28s %-6s %-6s %-6s %-11s %s%s\n' \
      "$pre" "$agent" "$email" "$fh" "$wk" "$ctx" "$tasks" "$status" "$post"
  done <<<"$tsv"
}

render_json() {
  local tsv="$1"
  local ts; ts=$(date '+%Y-%m-%dT%H:%M:%S')
  TS="$ts" SESS="$SESSION" python3 - "$tsv" <<'PY'
import json, os, sys
rows = []
for line in (sys.argv[1] or "").splitlines():
    if not line.strip():
        continue
    parts = line.split("\t")
    if len(parts) != 7:
        continue
    agent, email, fh, wk, ctx, tasks, status = parts
    rows.append({
        "agent": agent, "account": email,
        "five_hour_pct": fh, "weekly_pct": wk, "ctx_pct": ctx,
        "tasks_done": tasks, "status": status,
    })
print(json.dumps({
    "session": os.environ.get("SESS",""),
    "timestamp": os.environ.get("TS",""),
    "agents": rows,
}, indent=2))
PY
}

one_pass() {
  local tsv
  tsv=$(collect) || return $?
  if [ "$MODE" = "json" ]; then render_json "$tsv"
  else render_table "$tsv"; fi
}

if [ "$WATCH" = "1" ]; then
  trap 'printf "\n"; exit 0' INT
  while :; do
    [ "$USE_COLOR" = "1" ] && printf '\033[2J\033[H' || printf '\n'
    one_pass
    sleep 5
  done
else
  one_pass
fi
