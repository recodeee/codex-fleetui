## Definition of Done

This change is complete only when **all** of the following are true:

- Every checkbox below is checked.
- The agent branch reaches `MERGED` state on `origin` and the PR URL + state are recorded in the completion handoff.
- If any step blocks (test failure, conflict, ambiguous result), append a `BLOCKED:` line under section 4 explaining the blocker and **STOP**. Do not tick remaining cleanup boxes; do not silently skip the cleanup pipeline.

## Handoff

- Handoff: change=`agent-claude-fleet-watcher-resize-redraw-2026-05-16-01-11`; branch=`agent/<your-name>/<branch-slug>`; scope=`TODO`; action=`continue this sandbox or finish cleanup after a usage-limit/manual takeover`.
- Copy prompt: Continue `agent-claude-fleet-watcher-resize-redraw-2026-05-16-01-11` on branch `agent/<your-name>/<branch-slug>`. Work inside the existing sandbox, review `openspec/changes/agent-claude-fleet-watcher-resize-redraw-2026-05-16-01-11/tasks.md`, continue from the current state instead of creating a new sandbox, and when the work is done run `gx branch finish --branch agent/<your-name>/<branch-slug> --base dev --via-pr --wait-for-merge --cleanup`.

## 1. Specification

- [x] 1.1 Finalize proposal scope and acceptance criteria for `agent-claude-fleet-watcher-resize-redraw-2026-05-16-01-11`.
- [x] 1.2 Define normative requirements in `specs/fleet-watcher-resize-redraw/spec.md`.

## 2. Implementation

- [x] 2.1 Implement scoped behavior changes — added `Msg::Resize` variant, `WindowResize` arm in `WatcherView::on()`, `WindowResize` subscription in `Model::init_app()`, and `Msg::Resize` handling in `Model::update()`. All within `rust/fleet-watcher/src/main.rs`.
- [x] 2.2 Add/update focused regression coverage — new test `window_resize_event_triggers_redraw_message` calls `WatcherView::on(&Event::WindowResize(200, 60))` and asserts the returned `Msg` is `Some(_)`. Verified failure before fix, then green after.

## 3. Verification

- [x] 3.1 Run targeted project verification commands — `cargo test -p fleet-watcher` from `rust/` → 7 passed, 0 failed (1 new + 6 pre-existing).
- [x] 3.2 Run `openspec validate agent-claude-fleet-watcher-resize-redraw-2026-05-16-01-11 --type change --strict` — "Change ... is valid".
- [x] 3.3 Run `openspec validate --specs` — no main-spec deltas in this change; the requirement lives only on the change's delta spec.

## 4. Cleanup (mandatory; run before claiming completion)

- [ ] 4.1 Run the cleanup pipeline: `gx branch finish --branch agent/<your-name>/<branch-slug> --base dev --via-pr --wait-for-merge --cleanup`. This handles commit -> push -> PR create -> merge wait -> worktree prune in one invocation.
- [ ] 4.2 Record the PR URL and final merge state (`MERGED`) in the completion handoff.
- [ ] 4.3 Confirm the sandbox worktree is gone (`git worktree list` no longer shows the agent path; `git branch -a` shows no surviving local/remote refs for the branch).
