#!/usr/bin/env bash
# review-prepass.sh — assemble the auto-reviewer's input blob for a plan slug.
#
# Usage:
#   review-prepass.sh <plan-slug>
#
# Emits a single markdown document on stdout with three sections:
#   ## Plan acceptance criteria   (verbatim from openspec/plans/<slug>/plan.json)
#   ## Sub-task scope claims      (per-lane file_scope from plan.json)
#   ## PR diffs                   (one fenced block per agent/* PR; capped at 200 lines each)
#
# Designed to be piped to a reviewer model alongside scripts/codex-fleet/lib/review-rubric.md.
# Read-only: never writes anywhere outside stdout/stderr.

set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: review-prepass.sh <plan-slug>

Emits a markdown prepass blob for the auto-reviewer on stdout.
USAGE
  exit 2
}

if [[ ${#} -ne 1 ]] || [[ -z "${1:-}" ]] || [[ "${1:-}" == "-h" ]] || [[ "${1:-}" == "--help" ]]; then
  usage
fi

slug="$1"

# Resolve repo root from this script's location: lib -> codex-fleet -> scripts -> repo.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
plan_json="${repo_root}/openspec/plans/${slug}/plan.json"

if [[ ! -f "${plan_json}" ]]; then
  echo "review-prepass: plan.json not found at ${plan_json}" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "review-prepass: jq is required but not installed" >&2
  exit 1
fi

# shellcheck disable=SC2016  # backticks here are markdown code-spans, not shell
printf '# Auto-review prepass for plan `%s`\n\n' "${slug}"

printf '## Plan acceptance criteria\n\n'
if jq -e '.acceptance_criteria | type == "array" and length > 0' "${plan_json}" >/dev/null 2>&1; then
  jq -r '.acceptance_criteria[] | "- " + .' "${plan_json}"
else
  echo "_(no acceptance_criteria found in plan.json)_"
fi
printf '\n'

printf '## Sub-task scope claims\n\n'
if jq -e '.tasks | type == "array" and length > 0' "${plan_json}" >/dev/null 2>&1; then
  jq -r '
    .tasks[] |
    "### Lane \(.subtask_index): \(.title)\n\nfile_scope:\n" +
    ((.file_scope // []) | map("- `" + . + "`") | join("\n")) + "\n"
  ' "${plan_json}"
else
  echo "_(no tasks found in plan.json)_"
fi
printf '\n'

printf '## PR diffs\n\n'

pr_json=""
if command -v gh >/dev/null 2>&1; then
  # `gh pr list --search` matches the head branch glob + plan slug as a free-text term.
  # Worst case the slug doesn't appear in the PR title/body and we get fewer matches;
  # that's why the auto-reviewer is allowed to also harvest URLs from plan.json itself.
  if pr_json="$(gh pr list \
      --state all \
      --limit 100 \
      --search "head:agent/* ${slug}" \
      --json number,title,headRefName,url 2>/dev/null)"; then
    :
  else
    pr_json=""
  fi
fi

if [[ -z "${pr_json}" ]] || ! printf '%s' "${pr_json}" | jq -e 'length > 0' >/dev/null 2>&1; then
  echo "_(no PRs found via \`gh pr list --search 'head:agent/* ${slug}'\`; reviewer should fall back to plan.json URLs)_"
  exit 0
fi

# Iterate PRs; each diff fenced and capped at 200 lines.
printf '%s' "${pr_json}" | jq -c '.[]' | while IFS= read -r pr; do
  number="$(printf '%s' "${pr}" | jq -r '.number')"
  title="$(printf '%s' "${pr}" | jq -r '.title')"
  head_ref="$(printf '%s' "${pr}" | jq -r '.headRefName')"
  url="$(printf '%s' "${pr}" | jq -r '.url')"

  printf '### PR #%s — %s\n\n' "${number}" "${title}"
  # shellcheck disable=SC2016  # backticks here are markdown code-spans, not shell
  printf '- branch: `%s`\n- url: %s\n\n' "${head_ref}" "${url}"
  printf '```diff\n'
  if ! gh pr diff "${number}" 2>/dev/null | head -200; then
    printf '(diff unavailable for PR #%s)\n' "${number}"
  fi
  printf '```\n\n'
done
