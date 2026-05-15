## ADDED Requirements

### Requirement: Auto-reviewer plan PR daemon
The auto-reviewer script SHALL review merged PRs linked to Colony plan sub-task completion evidence and persist one review artifact per plan PR.

#### Scenario: One-shot plan review
- **WHEN** `scripts/codex-fleet/auto-reviewer.sh --once --plan <slug>` runs
- **THEN** the script discovers PR numbers from plan completion summaries or Colony task notes
- **AND** it ignores PR numbers that only appear inside original sub-task descriptions.

#### Scenario: Review artifact
- **WHEN** a discovered PR has not already been recorded in the state file
- **THEN** the script loads PR metadata and diff with `gh`
- **AND** it invokes `claude -p --add-dir <repo> --append-system-prompt-file ...` with plan acceptance context
- **AND** it writes the output to `openspec/changes/<slug>/auto-reviews/PR-<N>.md`
- **AND** it records the parsed `RANK: N/10` in the idempotency state file.

#### Scenario: Loop mode
- **WHEN** `scripts/codex-fleet/auto-reviewer.sh --loop --interval=<seconds>` runs
- **THEN** the script polls `colony plan status`
- **AND** it reviews plans whose local rollup has completed work and no claimed, available, or blocked sub-tasks.
