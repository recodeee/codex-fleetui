# agent-claude-review-pane-scanner-daemon-bringup-2026-05-14-23-06 (minimal / T1)

Branch: `agent/claude/review-pane-scanner-daemon-bringup-2026-05-14-23-06`

Close the Review-tab loop end-to-end:

1. `scripts/codex-fleet/review-pane-scanner.sh` — walks the codex-fleet
   worker panes via `tmux capture-pane`, detects the Codex auto-reviewer
   block (`⚠ Automatic approval review …` + `(risk: …, authorization: …)` +
   optional `✓ Request approved for …`), and emits matching events through
   `review-queue.sh`. Deduplicated via a stable `REV-<sha1[0:6]>` id keyed
   on `(agent, normalized payload)`; safe to run on a tight tick.
2. `scripts/codex-fleet/full-bringup.sh` — adds `review-queue` and
   `review-scanner` ticker windows next to the existing fleet-tick / cap-
   swap / state-pump / review-detector daemons, and prefers `review-anim.sh`
   over the legacy `review-board.sh` for the review tab.
3. Fixes a producer bug surfaced by the scanner: `review-queue.sh
   emit-pending` previously serialized `--pane` via `--argjson`, which
   blew up on non-numeric tmux pane ids (`%5`, fixture names). Pane is now
   serialized as a JSON string (or null when absent); the renderer's
   `jq -r '.pane // ""'` consumes either shape unchanged.

Test (`scripts/codex-fleet/test/test-review-pane-scanner.sh`) covers approved
/ denied / pending blocks, dedup across repeated scans, quiet panes emitting
nothing, and `--dry-run` not mutating the event log. PASS together with the
unchanged review-anim + review-queue suites.

## Handoff

- Handoff: change=`agent-claude-review-pane-scanner-daemon-bringup-2026-05-14-23-06`; branch=`agent/claude/review-pane-scanner-daemon-bringup-2026-05-14-23-06`; scope=`scripts/codex-fleet only`; action=`finish via PR after user sign-off`.

## Cleanup

- [ ] Run: `gx branch finish --branch agent/claude/review-pane-scanner-daemon-bringup-2026-05-14-23-06 --base main --via-pr --wait-for-merge --cleanup`
- [ ] Record PR URL + `MERGED` state in the completion handoff.
- [ ] Confirm sandbox worktree is gone (`git worktree list`, `git branch -a`).
