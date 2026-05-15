## Why

`claude-supervisor.sh`'s asking/blocked classifier had two structural
weaknesses that produced classifier cost and precision problems:

1. The "is this a real interactive cursor?" gate (`last_line_is_prompt`)
   accepted any line ending in `:` or `?` as a waiting cursor. Worker
   tails routinely end with `Reading file:` mid-work, so an old
   `Continue?` or `Should I` 40 lines back in the 80-line capture
   was enough to call sonnet/medium and paste an answer codex never
   asked for. Pure false-positive cost.
2. `is_busy` grep'd the whole 80-line window for `Working (` or
   `esc to interrupt`. A pane that finished `Working (12s)` 40 lines
   ago and is now sitting at a fresh `[Y/n] ` cursor read as "busy"
   and never reached the ask path — real asks slipped past the
   supervisor entirely.

Additionally, several real codex-fleet blockers (merge conflict,
uncommitted-changes, fatal git, ssh permission-denied, MCP server
missing, bad GH credentials, BLOCKED: prefix) were missing from
`BLOCKED_PATTERNS`, so panes parked on these states classified as
`quiet` and the supervisor stayed silent.

## What Changes

- Extract the classifier (BUSY/ASK/BLOCKED patterns + `is_busy`,
  `is_asking`, `is_blocked`, `last_line_is_prompt`, `classify_tail`,
  `tail_hash`) into a pure-bash library at
  `scripts/codex-fleet/lib/claude-supervisor-classifier.sh` so the
  daemon and a replay harness share one implementation.
- Tighten `last_line_is_prompt`: drop the bare `[?:][[:space:]]*$`
  rule. Bare-`?` is only admitted when the line carries a known
  question lead-word. `:$` no longer counts.
- Tighten `is_busy`: anchor BUSY_PATTERNS to the LAST non-empty line
  only. codex rewrites the `Working (…)` footer in place; if the
  worker is busy it's at the bottom. Stale `Working (` in scrollback
  no longer masks a fresh interactive prompt.
- Tighten `is_asking`: scope ASK_PATTERN matching to the recent N
  non-empty lines (default 8 via `CLAUDE_SUPERVISOR_RECENT_LINES`)
  AND require `last_line_is_prompt` to pass.
- Extend `BLOCKED_PATTERNS` with the codex-fleet-specific stuck
  states listed under "Why".
- Add a fixture-driven replay harness at
  `scripts/codex-fleet/test/test-claude-supervisor-classifier.sh`
  with 24 pane-capture fixtures covering the false-positive,
  missed-block, and previously-correct cases. Filename prefix
  encodes the expected classification.

## Impact

- Cost: fewer ASK false positives → fewer sonnet/medium calls per
  tick. Sonnet stays the workhorse for the remaining real asks.
  Opus calls are gated on the (now more accurate) BLOCKED set;
  strike guard caps per-pane spend.
- Behavior: panes the supervisor used to ignore (real ask under
  stale `Working (`, merge-conflict, MCP-missing, bad GH creds)
  now classify correctly.
- Risk: the harness pins the precision/recall trade-off — any
  future loosening of the gates regresses the harness.
- Surfaces touched: `claude-supervisor.sh` replaces its inline
  classifier with `source`; new lib; new test + fixtures. No
  daemon-state files, no plan-watcher changes, no cap-swap-daemon
  changes.
