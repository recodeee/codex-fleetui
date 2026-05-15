#!/usr/bin/env bash
# auto-reviewer.sh - end-of-plan auto-reviewer for codex-fleet.
#
# Resolves the active plan slug, gathers PRs linked to that plan, builds a
# review prompt (rubric + prepass + truncated diffs), and invokes
# `claude --print --permission-mode bypassPermissions` to produce a single
# Markdown review at /tmp/claude-viz/plan-review-<slug>.md.
#
# Usage:
#   scripts/codex-fleet/auto-reviewer.sh [--plan-slug <slug>] [--dry-run]
#
# Defaults:
#   --plan-slug: contents of .codex-fleet/active-plan (relative to repo root).
#
# Companion files (owned by Lane 7; optional, fallback to built-ins):
#   scripts/codex-fleet/lib/review-rubric.md
#   scripts/codex-fleet/lib/review-prepass.sh
#
# Output:
#   /tmp/claude-viz/plan-review-<slug>.md   (review artifact)
#
# Exit codes:
#   0  success (review written, or --dry-run prompt printed)
#   2  fatal (missing tools, no plan slug resolvable)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

PLAN_SLUG=""
DRY_RUN=0
LOOP=0
INTERVAL=300
ONCE=0
DIFF_LINE_LIMIT="${AUTO_REVIEW_DIFF_LINES:-200}"
OUTPUT_DIR="${AUTO_REVIEW_OUTPUT_DIR:-/tmp/claude-viz}"

RUBRIC_PATH="$REPO_ROOT/scripts/codex-fleet/lib/review-rubric.md"
PREPASS_PATH="$REPO_ROOT/scripts/codex-fleet/lib/review-prepass.sh"
ACTIVE_PLAN_FILE="$REPO_ROOT/.codex-fleet/active-plan"

log()  { printf '[auto-reviewer] %s\n' "$*"; }
warn() { printf '[auto-reviewer] %s\n' "$*" >&2; }
die()  { printf '[auto-reviewer] fatal: %s\n' "$*" >&2; exit 2; }

usage() {
  cat <<'USAGE'
auto-reviewer.sh - end-of-plan auto-reviewer for codex-fleet.

Usage:
  auto-reviewer.sh [--plan-slug <slug>] [--dry-run] [--once|--loop [--interval=<sec>]] [-h|--help]

Options:
  --plan-slug <slug>   Plan slug to review. Defaults to contents of
                       .codex-fleet/active-plan.
  --dry-run            Build and print the prompt to stdout; do not invoke
                       claude and do not write the review artifact.
  --once               Run main() once and exit (the default). Mutually
                       exclusive with --loop.
  --loop               Run main() repeatedly with --interval between runs.
                       Used by full-bringup.sh's ticker window.
  --interval=<sec>     Seconds between iterations in --loop mode (default 300).
  -h, --help           Show this help and exit.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --plan-slug) PLAN_SLUG="${2:-}"; shift 2 ;;
    --plan-slug=*) PLAN_SLUG="${1#*=}"; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --once) ONCE=1; LOOP=0; shift ;;
    --loop) LOOP=1; ONCE=0; shift ;;
    --interval) INTERVAL="${2:-300}"; shift 2 ;;
    --interval=*) INTERVAL="${1#*=}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown arg: $1" ;;
  esac
done

resolve_plan_slug() {
  if [[ -n "$PLAN_SLUG" ]]; then
    printf '%s\n' "$PLAN_SLUG"
    return 0
  fi
  if [[ -r "$ACTIVE_PLAN_FILE" ]]; then
    local slug
    slug="$(tr -d '[:space:]' < "$ACTIVE_PLAN_FILE")"
    if [[ -n "$slug" ]]; then
      printf '%s\n' "$slug"
      return 0
    fi
  fi
  return 1
}

# Collect PR numbers via `gh pr list --search 'head:agent/* <slug>'`.
# Emits one PR number per line. Returns 0 on success (even when empty).
collect_prs_via_gh() {
  local slug="$1"
  command -v gh >/dev/null 2>&1 || return 0
  # head:agent/* narrows to agent-branch PRs; the slug is matched as free text.
  gh pr list \
    --state all \
    --limit 100 \
    --search "head:agent/* $slug" \
    --json number \
    --jq '.[].number' 2>/dev/null || true
}

# Collect PR numbers by harvesting #<num> / pull/<num> / PR #<num> references
# from completed_summary / completion_summary / final_summary fields in the
# plan.json for the slug. Emits one PR number per line.
collect_prs_from_plan_json() {
  local slug="$1"
  local plan_json="$REPO_ROOT/openspec/plans/$slug/plan.json"
  [[ -r "$plan_json" ]] || return 0
  command -v python3 >/dev/null 2>&1 || return 0
  python3 - "$plan_json" <<'PY'
import json
import re
import sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
except (OSError, ValueError):
    raise SystemExit(0)

tasks = data.get("tasks") or data.get("subtasks") or []
seen = set()
pattern = re.compile(r"(?:PR\s*#?|pull/|#)(\d+)", re.IGNORECASE)
for task in tasks:
    if not isinstance(task, dict):
        continue
    blob = "\n".join(
        str(task.get(k) or "")
        for k in ("completed_summary", "completion_summary", "final_summary")
    )
    for match in pattern.finditer(blob):
        pr = match.group(1)
        if pr not in seen:
            seen.add(pr)
            print(pr)
PY
}

collect_prs_for_plan() {
  local slug="$1"
  local prs
  prs="$(collect_prs_via_gh "$slug" | awk 'NF && !seen[$0]++')"
  if [[ -n "$prs" ]]; then
    printf '%s\n' "$prs"
    return 0
  fi
  warn "no PRs from gh pr list; falling back to plan.json completed_summary harvest"
  collect_prs_from_plan_json "$slug" | awk 'NF && !seen[$0]++'
}

# Truncate the contents of stdin to at most $DIFF_LINE_LIMIT lines, appending
# a "[truncated to N lines]" marker when the input is longer.
truncate_diff() {
  local limit="$DIFF_LINE_LIMIT"
  awk -v limit="$limit" '
    { lines[NR] = $0 }
    END {
      total = NR
      cap = (total < limit) ? total : limit
      for (i = 1; i <= cap; i++) print lines[i]
      if (total > limit) {
        printf("\n[truncated to %d of %d lines]\n", limit, total)
      }
    }
  '
}

print_rubric() {
  if [[ -r "$RUBRIC_PATH" ]]; then
    cat "$RUBRIC_PATH"
    return 0
  fi
  warn "review rubric missing at $RUBRIC_PATH; using built-in minimal rubric"
  cat <<'EOF'
# Review rubric (built-in fallback)

You are a strict, terse technical reviewer for completed Colony plan PRs.

Return Markdown with these sections, in order:

## SUMMARY
One or two sentences on plan match and main risk.

## WHAT MATCHED
Concrete acceptance criteria the diffs satisfy. Evidence-based.

## WHAT DRIFTED
Concrete deviations from plan / verification gate / repo conventions, with
file/path references. Write `- None found.` if none.

## WHAT TO FIX NEXT
Smallest follow-up fixes. Write `- Nothing required.` if ready.

RANK: N/10
Replace `N` with an integer 1..10. Final line must match exactly
`RANK: <integer>/10`.
EOF
}

print_prepass() {
  local slug="$1"
  if [[ -x "$PREPASS_PATH" ]]; then
    if ! "$PREPASS_PATH" --plan-slug "$slug" 2>/dev/null; then
      warn "review-prepass.sh failed; continuing with empty prepass"
    fi
    return 0
  fi
  if [[ -r "$PREPASS_PATH" ]]; then
    warn "review-prepass.sh found but not executable; printing contents"
    cat "$PREPASS_PATH"
    return 0
  fi
  warn "review-prepass.sh missing at $PREPASS_PATH; using built-in minimal prepass"
  cat <<EOF
# Prepass (built-in fallback)

Plan slug: $slug
Repo root: $REPO_ROOT
Generated: $(date -u '+%Y-%m-%dT%H:%M:%SZ')

No structured prepass available; rely on PR diffs below.
EOF
}

print_pr_block() {
  local pr="$1"
  printf '\n---\n\n## PR #%s\n\n' "$pr"
  if command -v gh >/dev/null 2>&1; then
    if ! gh pr view "$pr" --json number,title,url,state,headRefName,baseRefName,mergedAt 2>/dev/null; then
      printf '(gh pr view #%s failed)\n' "$pr"
    fi
    printf '\n### diff (truncated to %s lines)\n\n```diff\n' "$DIFF_LINE_LIMIT"
    if ! gh pr diff "$pr" 2>/dev/null | truncate_diff; then
      printf '(gh pr diff #%s failed)\n' "$pr"
    fi
    printf '```\n'
  else
    printf '(gh CLI unavailable; cannot fetch diff)\n'
  fi
}

build_prompt() {
  local slug="$1"
  shift
  local prs=("$@")

  printf '# Auto-review for plan: %s\n\n' "$slug"
  printf 'Generated: %s\n\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

  printf '## Rubric\n\n'
  print_rubric

  printf '\n## Prepass\n\n'
  print_prepass "$slug"

  if (( ${#prs[@]} == 0 )); then
    printf '\n## PRs\n\n(no PRs found for plan %s)\n' "$slug"
    return 0
  fi

  printf '\n## PRs (%d total, diffs truncated to %s lines each)\n' \
    "${#prs[@]}" "$DIFF_LINE_LIMIT"
  local pr
  for pr in "${prs[@]}"; do
    print_pr_block "$pr"
  done
}

main() {
  local slug
  if ! slug="$(resolve_plan_slug)"; then
    die "no plan slug; pass --plan-slug or populate .codex-fleet/active-plan"
  fi
  log "plan=$slug dry_run=$DRY_RUN"

  local prs=()
  local pr
  while IFS= read -r pr; do
    [[ -n "$pr" ]] || continue
    prs+=("$pr")
  done < <(collect_prs_for_plan "$slug")
  log "prs=${#prs[@]}"

  local tmpdir
  tmpdir="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmpdir'" EXIT

  local prompt_file="$tmpdir/prompt.md"
  build_prompt "$slug" "${prs[@]}" > "$prompt_file"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry-run: printing prompt to stdout (no claude invocation)"
    cat "$prompt_file"
    return 0
  fi

  command -v claude >/dev/null 2>&1 || die "claude CLI required for non-dry-run mode"

  mkdir -p "$OUTPUT_DIR"
  local output_file="$OUTPUT_DIR/plan-review-$slug.md"

  log "invoking claude (output=$output_file)"
  if ! claude --print --permission-mode bypassPermissions < "$prompt_file" > "$output_file"; then
    warn "claude invocation failed for plan=$slug"
    rm -f "$output_file"
    return 2
  fi

  log "review written plan=$slug file=$output_file"
}

if [[ "$LOOP" -eq 1 ]]; then
  log "loop mode interval=${INTERVAL}s"
  while true; do
    main || warn "iteration failed rc=$?; continuing"
    sleep "$INTERVAL"
  done
else
  main
fi
