#!/usr/bin/env bash
# score-merged-pr.sh — improvement #1's post-merge half: take a merged PR
# number, find its plan's acceptance criteria, score the diff against them
# via lib/score-diff.py, and merge the result into
# /tmp/claude-viz/fleet-quality-scores.json keyed by agent-id.
#
# The scorer is intentionally advisory: it writes to a local JSON file
# read by the dashboards, never to any state that drives routing /
# claims. fleet-state's third rail surfaces the score; a low number is a
# "look at this" prompt for the operator, not an automated penalty.
#
# Usage:
#   ./scripts/codex-fleet/score-merged-pr.sh <PR-number>
#
# Env:
#   ANTHROPIC_API_KEY  required (consumed by lib/score-diff.py)
#   CODEX_FLEET_SCORES_PATH  override sink path (default: /tmp/claude-viz/fleet-quality-scores.json)
#   CODEX_FLEET_REPO_ROOT  override repo root for plan.md lookup (default: git toplevel)

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <PR-number>" >&2
  exit 2
fi

PR_NUMBER="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCORER="$SCRIPT_DIR/lib/score-diff.py"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(git rev-parse --show-toplevel)}"
SCORES_PATH="${CODEX_FLEET_SCORES_PATH:-/tmp/claude-viz/fleet-quality-scores.json}"

mkdir -p "$(dirname "$SCORES_PATH")"

# 1. Fetch PR metadata via gh. We need: branch name (to derive agent-id +
# locate the plan), PR title, full unified diff. Bail clearly on auth /
# missing-PR errors so the wrapper doesn't write a half-baked row.
PR_JSON=$(gh pr view "$PR_NUMBER" --json number,title,headRefName,baseRefName,mergeCommit,state 2>&1) || {
  echo "score-merged-pr.sh: gh pr view #$PR_NUMBER failed:" >&2
  echo "$PR_JSON" >&2
  exit 3
}

PR_TITLE=$(echo "$PR_JSON" | jq -r '.title')
HEAD_REF=$(echo "$PR_JSON" | jq -r '.headRefName')
STATE=$(echo "$PR_JSON" | jq -r '.state')

if [[ "$STATE" != "MERGED" ]]; then
  echo "score-merged-pr.sh: PR #$PR_NUMBER is not MERGED (state=$STATE); refusing to score." >&2
  exit 4
fi

# 2. Derive agent-id from the branch. Convention:
#    agent/<owner>/<task>-YYYY-MM-DD-HH-MM  →  agent-id = <owner>
# Falls back to the full head ref when the convention doesn't match.
AGENT_ID=$(echo "$HEAD_REF" | awk -F/ '/^agent\// {print $2; exit} {print $0}')

# 3. Pull the diff. `gh pr diff` returns the unified diff against the PR's
# base — exactly what we want to grade.
DIFF=$(gh pr diff "$PR_NUMBER")

# 4. Locate the plan's acceptance criteria. Convention in this repo:
#    openspec/plans/<slug>/plan.md  with an "## Acceptance Criteria" section.
# Search openspec/plans/*/plan.md for any whose slug appears in the branch
# name or PR title. Multiple candidates → take the first; none → empty
# criteria (the scorer will return score: null).
CRITERIA=""
PLAN_SLUG="null"
while IFS= read -r plan_md; do
  slug=$(basename "$(dirname "$plan_md")")
  if echo "$HEAD_REF $PR_TITLE" | grep -qF "$slug"; then
    # Extract the "## Acceptance Criteria" section up to the next "## ".
    CRITERIA=$(awk '
      /^## Acceptance Criteria/ { capture=1; next }
      capture && /^## / { exit }
      capture { print }
    ' "$plan_md")
    PLAN_SLUG="\"$slug\""
    break
  fi
done < <(find "$REPO_ROOT/openspec/plans" -maxdepth 2 -name plan.md 2>/dev/null)

# 5. Build the scorer payload and invoke. The Python script reads JSON on
# stdin and writes the verdict JSON on stdout.
VERDICT=$(
  jq -n \
    --arg diff "$DIFF" \
    --arg criteria "$CRITERIA" \
    --arg title "$PR_TITLE" \
    --arg mode "merged" \
    '{diff: $diff, criteria: $criteria, pr_title: $title, mode: $mode}' \
  | python3 "$SCORER"
) || {
  echo "score-merged-pr.sh: scorer failed for PR #$PR_NUMBER; not updating $SCORES_PATH." >&2
  exit 5
}

# 6. Merge into the scores file. Atomic-ish: write to a temp file, then mv.
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EXISTING="{}"
if [[ -f "$SCORES_PATH" ]]; then
  EXISTING=$(cat "$SCORES_PATH")
fi

UPDATED=$(
  jq -n \
    --argjson existing "$EXISTING" \
    --argjson verdict "$VERDICT" \
    --arg agent_id "$AGENT_ID" \
    --arg now "$NOW" \
    --arg pr_title "$PR_TITLE" \
    --arg branch "$HEAD_REF" \
    --argjson pr_number "$PR_NUMBER" \
    --argjson plan_slug "$PLAN_SLUG" \
    '
    ($existing | if has("scores") then . else . + {scores: {}} end)
    | .generated_at = $now
    | .scores[$agent_id] = ($verdict + {
        agent_id: $agent_id,
        pr_number: $pr_number,
        pr_title: $pr_title,
        branch: $branch,
        plan_slug: $plan_slug,
        scored_at: $now
      })
    '
)

TMP=$(mktemp)
echo "$UPDATED" > "$TMP"
mv "$TMP" "$SCORES_PATH"

SCORE=$(echo "$VERDICT" | jq -r '.score // "null"')
echo "scored agent=$AGENT_ID pr=$PR_NUMBER score=$SCORE plan=$PLAN_SLUG → $SCORES_PATH"
