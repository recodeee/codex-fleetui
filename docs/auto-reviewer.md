# Auto-Reviewer

`scripts/codex-fleet/auto-reviewer.sh` reviews merged PRs attached to Colony
plan sub-tasks and writes compact review artifacts back into the plan change.

## Review Storage

Reviews live under the OpenSpec change for the plan:

```text
openspec/changes/<plan-slug>/auto-reviews/PR-<N>.md
```

Each file starts with the PR number, the plan slug, the generation timestamp,
and the reviewer output. The script tracks reviewed `(plan, PR)` pairs in
`AUTO_REVIEW_STATE_FILE`, defaulting to:

```text
/tmp/claude-viz/auto-reviewer-state.tsv
```

Delete the matching state row only when a review must be regenerated.

## Rank Line

The reviewer prompt requires a final rank line:

```text
RANK: N/10
```

`N` is an integer from 1 through 10. The script parses the first matching line
with this shape and records `unranked` if no line is present. Treat `unranked`
as a prompt or reviewer-output defect, not as a neutral score.

## Colony Integration

After writing a review artifact, the script posts a task-scoped Colony note on
the parent plan task:

```text
Auto-review PR #<N>: <rank>; plan=<plan-slug>; file=<review-path>
```

That note is the durable pointer for operators and dashboards. The markdown file
is the full evidence; the Colony note stays short enough for task timelines.

## Re-Run One Review

Run a single PR review with:

```bash
bash scripts/codex-fleet/auto-reviewer.sh --once --pr <N> --slug <plan-slug>
```

Useful overrides:

```bash
AUTO_REVIEW_STATE_FILE=/tmp/auto-reviewer-test.tsv \
AUTO_REVIEW_DESIGN_BYTES=12000 \
AUTO_REVIEW_DIFF_BYTES=180000 \
bash scripts/codex-fleet/auto-reviewer.sh --once --pr <N> --slug <plan-slug>
```

Use `--dry-run` to confirm discovery without calling `claude`:

```bash
bash scripts/codex-fleet/auto-reviewer.sh --once --pr <N> --slug <plan-slug> --dry-run
```

## Worked Example

PR #69, "Match fleet tab strip glass dock design", was merged on
`2026-05-14T23:06:04Z`:

```text
https://github.com/recodeee/codex-fleetui/pull/69
```

To regenerate its auto-review for the design-match plan:

```bash
bash scripts/codex-fleet/auto-reviewer.sh \
  --once \
  --pr 69 \
  --slug codex-fleetui-design-match-2026-05-15
```

Expected artifact:

```text
openspec/changes/codex-fleetui-design-match-2026-05-15/auto-reviews/PR-69.md
```

Expected Colony note shape:

```text
Auto-review PR #69: <rank>; plan=codex-fleetui-design-match-2026-05-15; file=openspec/changes/codex-fleetui-design-match-2026-05-15/auto-reviews/PR-69.md
```
