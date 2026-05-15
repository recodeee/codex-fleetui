## Definition of Done

This change is complete only when **all** of the following are true:

- Every checkbox below is checked.
- The agent branch reaches `MERGED` state on `origin` and the PR URL + state are recorded in the completion handoff.
- If any step blocks (test failure, conflict, ambiguous result), append a `BLOCKED:` line under section 4 explaining the blocker and **STOP**. Do not tick remaining cleanup boxes; do not silently skip the cleanup pipeline.

## Handoff

- Handoff: change=`agent-codex-ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02`; branch=`agent/codex/ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02`; scope=`scripts/codex-fleet/auto-reviewer.sh`; action=`finish PR cleanup if interrupted`.
- Copy prompt: Continue `agent-codex-ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02` on branch `agent/codex/ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02`. Work inside the existing sandbox, review `openspec/changes/agent-codex-ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02/tasks.md`, continue from the current state instead of creating a new sandbox, and when the work is done run `gx branch finish --branch agent/codex/ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02 --base main --via-pr --wait-for-merge --cleanup`.

## 1. Specification

- [x] 1.1 Finalize proposal scope and acceptance criteria for `agent-codex-ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02`.
- [x] 1.2 Define normative requirements in `specs/ios-bordered-auto-reviewer-script-sub6/spec.md`.

## 2. Implementation

- [x] 2.1 Implement scoped behavior changes.
- [x] 2.2 Add/update focused regression coverage through dry-run discovery and shell syntax verification.

## 3. Verification

- [x] 3.1 Run targeted project verification commands: `bash -n scripts/codex-fleet/auto-reviewer.sh` passed; dry-run discovery found PR #82 and PR #87 for the plan.
- [x] 3.2 Run `openspec validate agent-codex-ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02 --type change --strict`: valid.
- [x] 3.3 Run `openspec validate --specs`: no items found to validate.

## 4. Cleanup (mandatory; run before claiming completion)

- [ ] 4.1 Run the cleanup pipeline: `gx branch finish --branch agent/codex/ios-bordered-auto-reviewer-script-sub6-2026-05-15-13-02 --base main --via-pr --wait-for-merge --cleanup`. This handles commit -> push -> PR create -> merge wait -> worktree prune in one invocation.
- [ ] 4.2 Record the PR URL and final merge state (`MERGED`) in the completion handoff.
- [ ] 4.3 Confirm the sandbox worktree is gone (`git worktree list` no longer shows the agent path; `git branch -a` shows no surviving local/remote refs for the branch).
