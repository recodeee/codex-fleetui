# agent-claude-ios-style-review-approval-queue-screen-2026-05-14-22-06 (minimal / T1)

Branch: `agent/claude/ios-style-review-approval-queue-screen-2026-05-14-22-06`

Add the iOS-style Review tab (screen 4/4) to the codex-fleet live viz. New
`scripts/codex-fleet/review-anim.sh` renders an auto-reviewer approval queue
with a pending-review card (risk + auth pills, AUTO-REVIEWER RATIONALE block,
file list, Approve/View diff/Deny pill row) on the left and a Recent
Decisions rail on the right. Sibling of plan-anim / waves-anim / plan-tree-
anim — same iOS palette, same rounded card primitives, same 800ms diff-paint
loop, same `--once` mode. Reads `/tmp/claude-viz/live-review-queue.json`
(overridable via `REVIEW_ANIM_QUEUE_JSON`); falls back to a built-in demo
fixture that matches the design comp when no live queue exists.

`scripts/codex-fleet/codex-fleet-2.sh` previously referenced an unwritten
`review-board.sh`; it now prefers the new `review-anim.sh` and keeps the
legacy fallback so existing user-local installs still load.

## Handoff

- Handoff: change=`agent-claude-ios-style-review-approval-queue-screen-2026-05-14-22-06`; branch=`agent/claude/ios-style-review-approval-queue-screen-2026-05-14-22-06`; scope=`scripts/codex-fleet only`; action=`finish via PR after user sign-off on local diff and visual check via 'bash scripts/codex-fleet/review-anim.sh --once'`.

## Cleanup

- [ ] Run: `gx branch finish --branch agent/claude/ios-style-review-approval-queue-screen-2026-05-14-22-06 --base main --via-pr --wait-for-merge --cleanup`
- [ ] Record PR URL + `MERGED` state in the completion handoff.
- [ ] Confirm sandbox worktree is gone (`git worktree list`, `git branch -a`).
