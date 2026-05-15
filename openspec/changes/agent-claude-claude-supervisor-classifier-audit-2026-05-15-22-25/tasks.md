## Definition of Done

This change is complete only when **all** of the following are true:

- Every checkbox below is checked.
- The agent branch reaches `MERGED` state on `origin` and the PR URL + state are recorded in the completion handoff.
- If any step blocks, append a `BLOCKED:` line under section 4 and stop.

## Handoff

- Handoff: change=`agent-claude-claude-supervisor-classifier-audit-2026-05-15-22-25`; branch=`agent/claude/claude-supervisor-classifier-audit-2026-05-15-22-25`; scope=`tighten the asking/blocked classifier inside claude-supervisor.sh, extract it to a pure lib, add fixture-driven replay harness`; evidence=`scripts/codex-fleet/test/test-claude-supervisor-classifier.sh`.

## 1. Specification

- [x] 1.1 Finalize proposal scope and acceptance criteria.
- [x] 1.2 Define normative requirements in `specs/claude-supervisor-classifier-audit/spec.md`.

## 2. Implementation

- [x] 2.1 Extract classifier into `scripts/codex-fleet/lib/claude-supervisor-classifier.sh`.
- [x] 2.2 Tighten `last_line_is_prompt` (drop bare `:$`; gate `?$` on a question lead-word).
- [x] 2.3 Anchor `is_busy` to the last non-empty line.
- [x] 2.4 Scope `is_asking` to recent N lines AND require `last_line_is_prompt`.
- [x] 2.5 Extend `BLOCKED_PATTERNS` with codex-fleet stuck states.
- [x] 2.6 Patch `claude-supervisor.sh` to `source` the new lib.
- [x] 2.7 Add replay harness + 24 fixtures under `scripts/codex-fleet/test/`.

## 3. Verification

- [x] 3.1 `bash -n` clean on lib, harness, daemon.
- [x] 3.2 `bash scripts/codex-fleet/test/test-claude-supervisor-classifier.sh` — 24 pass, 0 fail.
- [x] 3.3 `claude-supervisor.sh --once --dry-run` runs without tmux (no-op tick, rc=0).

## 4. Cleanup (mandatory; run before claiming completion)

- [ ] 4.1 Run the cleanup pipeline: `gx branch finish --branch agent/claude/claude-supervisor-classifier-audit-2026-05-15-22-25 --base main --via-pr --wait-for-merge --cleanup`.
- [ ] 4.2 Record the PR URL and final merge state (`MERGED`) in the completion handoff.
- [ ] 4.3 Confirm the sandbox worktree is gone.
