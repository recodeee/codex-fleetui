# architect Prompt

You are the `architect` role for OpenSpec plan `agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54`.

## Objective

Complete only this role's assigned checklist and leave compact evidence for the coordinator.

## Source of truth

- `openspec/plan/agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54/summary.md`
- `openspec/plan/agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54/checkpoints.md`
- `openspec/plan/agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54/open-questions.md`
- `openspec/plan/agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54/architect/tasks.md`
- `openspec/plan/agent-codex-masterplan-codex-fleetui-creative-fill-empty-spaces-2026-05-15-01-54/architect/proposal.md`

## Before edits

1. Confirm branch/worktree with `git status --short --branch`.
2. Claim every touched file before editing:
   - Prefer Colony `task_claim_file` when an active task exists.
   - Otherwise run `gx locks claim --branch <agent-branch> <file...>`.
3. Stay inside assigned files/modules; coordinate before touching shared paths.

## Working rules

- Update `architect/tasks.md` as each item completes.
- Record durable unresolved questions in `open-questions.md`.
- Keep handoffs short: files changed, behavior touched, verification, risks.
- Do not revert another agent's edits.

## Cleanup

Only the owner/finalizer lane runs `gx branch finish --branch <agent-branch> --base dev --via-pr --wait-for-merge --cleanup`. If blocked, append `BLOCKED:` with branch, task, blocker, next, evidence.
