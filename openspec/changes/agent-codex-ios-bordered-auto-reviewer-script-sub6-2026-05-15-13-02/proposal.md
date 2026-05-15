## Why

The published design polish plan needs an automated reviewer pass after sub-task PRs merge. Without a supervisor script, merged PRs can sit without the promised Claude review artifact, score line, or Colony evidence note.

## What Changes

- Add `scripts/codex-fleet/auto-reviewer.sh`.
- Support `--once --plan <slug>` and `--loop --interval=<seconds>`.
- Extract PR numbers from plan completion summaries and Colony task notes, skipping source PR references in sub-task descriptions.
- Fetch PR metadata/diff, build a Claude review prompt with acceptance criteria and design-reference snippets, save `auto-reviews/PR-<N>.md`, parse `RANK: N/10`, and post a compact Colony note when a spec task id is available.
- Keep idempotency in `/tmp/claude-viz/auto-reviewer-state.tsv`.

## Impact

Only the new auto-reviewer shell script is added. Runtime dependencies are existing fleet tools: `gh`, `claude`, `colony`, `sqlite3`, and `python3`. Verification is `bash -n` plus a dry-run against the iOS bordered plan.
