## Why

- The fleet tab strip pane has been removed, so `plan-watcher.sh` should no
  longer special-case or skip panes marked `[codex-fleet-tab-strip]`.
- Keeping the old exclusion can hide a labelled pane from idle-worker detection
  and leaves obsolete tab-strip terminology in the watcher comments.

## What Changes

- Remove the `[codex-fleet-tab-strip]` skip from `list_idle_workers`.
- Update the nearby comment block to describe the current worker-pane filtering
  rules after tab-strip removal.

## Impact

- Scope is limited to `scripts/codex-fleet/plan-watcher.sh`.
- Runtime behavior now treats every labelled pane as eligible for idle-worker
  detection when its recent output matches the idle patterns.
- Verification covers shell syntax, optional one-shot dry-run when a fleet tmux
  session exists, and OpenSpec validation.
