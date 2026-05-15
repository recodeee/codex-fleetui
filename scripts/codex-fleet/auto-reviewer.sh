#!/usr/bin/env bash
# auto-reviewer.sh - review merged PRs attached to Colony plan sub-tasks.
#
# Modes:
#   --once --plan <slug>       Review every merged PR found for one plan.
#   --loop --interval=300      Poll completed local plan workspaces.
#
# Reviews are idempotent per (plan, PR) through AUTO_REVIEW_STATE_FILE
# (default /tmp/claude-viz/auto-reviewer-state.tsv).

set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
STATE_FILE="${AUTO_REVIEW_STATE_FILE:-/tmp/claude-viz/auto-reviewer-state.tsv}"
INTERVAL="${AUTO_REVIEW_INTERVAL:-300}"
MODE="once"
PLAN_SLUG=""
PR_FILTER=""
DRY_RUN=0
DESIGN_BYTES="${AUTO_REVIEW_DESIGN_BYTES:-12000}"
DIFF_BYTES="${AUTO_REVIEW_DIFF_BYTES:-180000}"

usage() {
  sed -n '1,26p' "$0"
}

log() { printf '[auto-reviewer] %s\n' "$*"; }
warn() { printf '[auto-reviewer] %s\n' "$*" >&2; }
die() { printf '[auto-reviewer] fatal: %s\n' "$*" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --once) MODE="once"; shift ;;
    --loop) MODE="loop"; shift ;;
    --plan|--slug) PLAN_SLUG="$2"; shift 2 ;;
    --plan=*|--slug=*) PLAN_SLUG="${1#*=}"; shift ;;
    --pr) PR_FILTER="$2"; shift 2 ;;
    --pr=*) PR_FILTER="${1#*=}"; shift ;;
    --interval) INTERVAL="$2"; shift 2 ;;
    --interval=*) INTERVAL="${1#*=}"; shift ;;
    --state-file) STATE_FILE="$2"; shift 2 ;;
    --state-file=*) STATE_FILE="${1#*=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown arg: $1" ;;
  esac
done

ensure_state() {
  mkdir -p "$(dirname "$STATE_FILE")"
  [[ -e "$STATE_FILE" ]] || : > "$STATE_FILE"
}

change_dir_for_plan() {
  local slug="$1"
  if [[ -d "$REPO_ROOT/openspec/changes/$slug" ]]; then
    printf '%s\n' "$REPO_ROOT/openspec/changes/$slug"
  else
    printf '%s\n' "$REPO_ROOT/openspec/changes/$slug"
  fi
}

plan_json_for() {
  local slug="$1"
  local direct="$REPO_ROOT/openspec/plans/$slug/plan.json"
  if [[ -r "$direct" ]]; then
    printf '%s\n' "$direct"
    return 0
  fi
  find "$REPO_ROOT/openspec/plans" -maxdepth 2 -name plan.json -print 2>/dev/null \
    | while IFS= read -r path; do
        if python3 - "$path" "$slug" <<'PY' >/dev/null 2>&1
import json, sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
raise SystemExit(0 if data.get("plan_slug") == sys.argv[2] else 1)
PY
        then
          printf '%s\n' "$path"
          return 0
        fi
      done | head -n 1
}

colony_db_path() {
  if [[ -n "${AUTO_REVIEW_COLONY_DB:-}" && -r "${AUTO_REVIEW_COLONY_DB:-}" ]]; then
    printf '%s\n' "$AUTO_REVIEW_COLONY_DB"
    return 0
  fi
  if command -v colony >/dev/null 2>&1; then
    colony status 2>/dev/null | sed -nE 's/^db:[[:space:]]+([^[:space:]]+).*/\1/p' | head -n 1
    return 0
  fi
  local default_db="$HOME/.colony/data.db"
  [[ -r "$default_db" ]] && printf '%s\n' "$default_db"
}

sql_quote() {
  printf "%s" "$1" | sed "s/'/''/g"
}

plan_title() {
  local slug="$1" json
  json="$(plan_json_for "$slug")"
  if [[ -n "$json" && -r "$json" ]]; then
    python3 - "$json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("title", ""))
PY
  else
    printf '%s\n' "$slug"
  fi
}

plan_spec_task_id() {
  local slug="$1" json
  json="$(plan_json_for "$slug")"
  if [[ -n "$json" && -r "$json" ]]; then
    python3 - "$json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
published = data.get("published") or {}
print(published.get("spec_task_id") or data.get("spec_task_id") or "")
PY
    return 0
  fi
  local db qslug
  db="$(colony_db_path)"
  [[ -n "$db" && -r "$db" ]] || return 0
  command -v sqlite3 >/dev/null 2>&1 || return 0
  qslug="$(sql_quote "$slug")"
  sqlite3 "$db" "select id from tasks where branch='spec/$qslug' or title='spec/$qslug' order by id desc limit 1;" 2>/dev/null || true
}

state_has() {
  local slug="$1" pr="$2"
  [[ -r "$STATE_FILE" ]] || return 1
  awk -F'\t' -v s="$slug" -v p="$pr" '$2==s && $3==p {found=1} END {exit found ? 0 : 1}' "$STATE_FILE"
}

state_mark() {
  local slug="$1" pr="$2" rank="$3" out="$4"
  ensure_state
  (
    flock -x 9
    printf '%s\t%s\t%s\t%s\t%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$slug" "$pr" "$rank" "$out" >&9
  ) 9>>"$STATE_FILE"
}

extract_pr_numbers() {
  python3 -c '
import re, sys
seen = set()
for line in sys.stdin:
    for match in re.finditer(r"(?:PR\s*#?|pull/|#)(\d+)", line, re.I):
        pr = match.group(1)
        if pr not in seen:
            seen.add(pr)
            print(pr)
'
}

collect_prs_from_plan_json() {
  local slug="$1" json
  json="$(plan_json_for "$slug")"
  [[ -n "$json" && -r "$json" ]] || return 0
  python3 - "$json" <<'PY'
import json, re, sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
tasks = data.get("tasks") or data.get("subtasks") or []
seen = set()
for task in tasks:
    if not isinstance(task, dict):
        continue
    text = "\n".join(str(task.get(k) or "") for k in (
        "completed_summary", "completion_summary", "final_summary"
    ))
    for match in re.finditer(r"(?:PR\s*#?|pull/|#)(\d+)", text, re.I):
        pr = match.group(1)
        if pr not in seen:
            seen.add(pr)
            print(pr)
PY
}

collect_prs_from_colony_db() {
  local slug="$1" db qslug ids
  command -v sqlite3 >/dev/null 2>&1 || return 0
  db="$(colony_db_path)"
  [[ -n "$db" && -r "$db" ]] || return 0
  qslug="$(sql_quote "$slug")"
  ids="$(sqlite3 "$db" "select group_concat(id, ',') from tasks where branch='spec/$qslug' or branch like 'spec/$qslug/sub-%' or title='spec/$qslug' or title like 'spec/$qslug/sub-%';" 2>/dev/null || true)"
  [[ -n "$ids" ]] || return 0
  sqlite3 "$db" "select content from observations where task_id in ($ids) and kind!='plan-subtask' and content like '%PR%' order by id;" 2>/dev/null \
    | extract_pr_numbers || true
}

collect_prs_for_plan() {
  local slug="$1"
  {
    collect_prs_from_plan_json "$slug"
    collect_prs_from_colony_db "$slug"
  } | awk 'NF && !seen[$0]++'
}

list_completed_plans() {
  command -v colony >/dev/null 2>&1 || return 0
  colony plan status 2>/dev/null | while IFS= read -r line; do
    case "$line" in
      [a-z0-9-]*)
        slug="${line%% *}"
        ;;
      "  tasks:"*)
        completed="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) completed.*/\1/p')"
        claimed="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) claimed.*/\1/p')"
        available="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) available.*/\1/p')"
        blocked="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]([0-9]+) blocked.*/\1/p')"
        if [[ -n "${slug:-}" && "${completed:-0}" -gt 0 && "${claimed:-0}" -eq 0 && "${available:-0}" -eq 0 && "${blocked:-0}" -eq 0 ]] 2>/dev/null; then
          printf '%s\n' "$slug"
        fi
        slug=""
        ;;
    esac
  done
}

trim_to_bytes() {
  local bytes="$1"
  python3 -c '
import sys
limit = int(sys.argv[1])
data = sys.stdin.buffer.read()
if len(data) > limit:
    sys.stdout.buffer.write(data[:limit])
    sys.stdout.buffer.write(b"\n[truncated]\n")
else:
    sys.stdout.buffer.write(data)
' "$bytes"
}

print_acceptance_context() {
  local slug="$1"
  local change_dir
  change_dir="$(change_dir_for_plan "$slug")"
  for file in \
    "$change_dir/CHANGE.md" \
    "$change_dir/tasks.md" \
    "$REPO_ROOT/openspec/plans/$slug/plan.json"
  do
    [[ -r "$file" ]] || continue
    printf '\n## %s\n\n' "${file#$REPO_ROOT/}"
    trim_to_bytes 40000 < "$file"
  done
}

print_design_context() {
  local slug="$1" title="$2"
  if ! printf '%s\n%s\n' "$slug" "$title" | grep -Eiq '(design|ios|bordered)'; then
    return 0
  fi
  local image_root="$REPO_ROOT/images"
  [[ -d "$image_root" ]] || return 0
  find "$image_root" -maxdepth 1 -type f -regextype posix-extended \
    -regex '.*/[A-Za-z]_.*\.html' -print | sort \
    | while IFS= read -r file; do
        printf '\n## design reference: %s\n\n' "${file#$REPO_ROOT/}"
        trim_to_bytes "$DESIGN_BYTES" < "$file"
      done
}

build_prompt() {
  local slug="$1" pr="$2" meta_file="$3" diff_file="$4"
  local title
  title="$(plan_title "$slug")"
  cat <<EOF
Review the merged PR against the Colony plan acceptance criteria.

Return Markdown. Put the first ranking line in this exact format:
RANK: N/10

Flag only concrete issues visible in the PR diff, acceptance criteria, or design reference.

Plan: $slug
Plan title: $title
PR: #$pr

EOF
  printf '## PR metadata\n\n'
  cat "$meta_file"
  printf '\n\n'
  print_acceptance_context "$slug"
  print_design_context "$slug" "$title"
  printf '\n## PR diff\n\n'
  trim_to_bytes "$DIFF_BYTES" < "$diff_file"
}

prompt_file_for_claude() {
  local fallback="${1:-}"
  local configured="$REPO_ROOT/scripts/codex-fleet/lib/auto-review-prompt.md"
  if [[ -r "$configured" ]]; then
    printf '%s\n' "$configured"
    return 0
  fi
  local tmp="$fallback"
  [[ -n "$tmp" ]] || tmp="$(mktemp)"
  cat > "$tmp" <<'EOF'
You are a strict code reviewer for completed Colony plan PRs. Score the merged PR from 1 to 10 against the stated acceptance criteria. Start with `RANK: N/10`, then list findings with file/path references when possible. Do not praise. Keep the review compact.
EOF
  printf '%s\n' "$tmp"
}

post_colony_note() {
  local task_id="$1" content="$2"
  [[ -n "$task_id" ]] || return 0
  command -v colony >/dev/null 2>&1 || return 0
  colony note --task "$task_id" "$content" >/dev/null 2>&1 || true
}

review_pr() {
  local slug="$1" pr="$2"
  ensure_state
  if state_has "$slug" "$pr"; then
    log "skip reviewed plan=$slug pr=$pr"
    return 0
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry-run plan=$slug pr=$pr"
    return 0
  fi

  command -v gh >/dev/null 2>&1 || die "gh is required"
  command -v claude >/dev/null 2>&1 || die "claude is required"

  local tmpdir meta_file diff_file prompt_file sys_prompt output_file output rank task_id rel_out
  tmpdir="$(mktemp -d)"
  meta_file="$tmpdir/pr-meta.json"
  diff_file="$tmpdir/pr.diff"
  prompt_file="$tmpdir/prompt.md"

  trap 'rm -rf "$tmpdir"' RETURN

  gh pr view "$pr" --json number,title,body,url,state,isDraft,mergedAt,headRefName,baseRefName > "$meta_file"
  gh pr diff "$pr" > "$diff_file"
  build_prompt "$slug" "$pr" "$meta_file" "$diff_file" > "$prompt_file"

  sys_prompt="$(prompt_file_for_claude "$tmpdir/system-prompt.md")"
  if ! output="$(claude -p --add-dir "$REPO_ROOT" --append-system-prompt-file "$sys_prompt" < "$prompt_file" 2>&1)"; then
    warn "claude failed plan=$slug pr=$pr"
    printf '%s\n' "$output" >&2
    return 1
  fi

  output_file="$(change_dir_for_plan "$slug")/auto-reviews/PR-$pr.md"
  mkdir -p "$(dirname "$output_file")"
  {
    printf '# Auto-review PR #%s\n\n' "$pr"
    printf '- Plan: `%s`\n' "$slug"
    printf '- Generated: `%s`\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf '\n'
    printf '%s\n' "$output"
  } > "$output_file"

  rank="$(printf '%s\n' "$output" | sed -nE 's/^RANK:[[:space:]]*([0-9]+\/10).*/\1/p' | head -n 1)"
  rank="${rank:-unranked}"
  rel_out="${output_file#$REPO_ROOT/}"
  task_id="$(plan_spec_task_id "$slug")"
  post_colony_note "$task_id" "Auto-review PR #$pr: $rank; plan=$slug; file=$rel_out"
  state_mark "$slug" "$pr" "$rank" "$rel_out"
  log "reviewed plan=$slug pr=$pr rank=$rank file=$rel_out"
}

review_plan() {
  local slug="$1"
  [[ -n "$slug" ]] || die "--plan is required in --once mode"
  local found=0 pr
  while IFS= read -r pr; do
    [[ -n "$pr" ]] || continue
    if [[ -n "$PR_FILTER" && "$pr" != "$PR_FILTER" ]]; then
      continue
    fi
    found=1
    review_pr "$slug" "$pr"
  done < <(collect_prs_for_plan "$slug")
  if [[ "$found" -eq 0 ]]; then
    log "no PRs found plan=$slug"
  fi
}

tick() {
  if [[ -n "$PLAN_SLUG" ]]; then
    review_plan "$PLAN_SLUG"
    return 0
  fi
  local slug
  while IFS= read -r slug; do
    [[ -n "$slug" ]] || continue
    review_plan "$slug"
  done < <(list_completed_plans)
}

case "$MODE" in
  once)
    tick
    ;;
  loop)
    while true; do
      tick
      sleep "$INTERVAL"
    done
    ;;
  *)
    die "invalid mode: $MODE"
    ;;
esac
