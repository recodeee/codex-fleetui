## ADDED Requirements

### Requirement: classifier lives in a pure-bash library

The system SHALL keep the classifier (BUSY/ASK/BLOCKED pattern
arrays, `is_busy`, `is_asking`, `is_blocked`, `last_line_is_prompt`,
`classify_tail`, and `tail_hash`) at
`scripts/codex-fleet/lib/claude-supervisor-classifier.sh`. The lib
SHALL be safe to `source` with no side effects (no tmux calls, no
claude calls, no file writes at source time). `claude-supervisor.sh`
SHALL source this lib rather than inlining the classifier.

#### Scenario: library is sourceable with no side effects
- **WHEN** `scripts/codex-fleet/lib/claude-supervisor-classifier.sh`
  is sourced from a bash script
- **THEN** no tmux, claude, or filesystem-mutating command runs
- **AND** `classify_tail`, `is_busy`, `is_asking`, `is_blocked`,
  `last_line_is_prompt`, and `tail_hash` are defined

### Requirement: classify_tail returns one of four labels

`classify_tail "<tail>"` SHALL echo exactly one of
`busy`, `asking`, `blocked`, `quiet`. `asking` SHALL outrank `blocked`
when both conditions match — a pane that mentioned a stale blocker
but is now showing an interactive menu wants an answer to the menu.

#### Scenario: asking outranks blocked
- **WHEN** the recent tail contains both a BLOCKED_PATTERN (e.g.,
  stale-claim) and an ASK_PATTERN (e.g., a numbered menu with a
  `(recommended)` option) AND `last_line_is_prompt` accepts the
  bottom line
- **THEN** `classify_tail` echoes `asking`

### Requirement: busy is anchored to the last non-empty line

`is_busy` SHALL match BUSY_PATTERNS only against the last non-empty
line of the ANSI-stripped tail. A stale `Working (` or
`esc to interrupt` earlier in scrollback SHALL NOT mask a fresh
interactive cursor at the bottom.

#### Scenario: stale Working in scrollback does not mask a fresh ask
- **WHEN** the tail contains `Working (` nine lines from the bottom
  AND the bottom line is a bare prompt sigil (`❯`) under a numbered
  menu with `(recommended)` option
- **THEN** `classify_tail` echoes `asking`, not `busy`

### Requirement: last_line_is_prompt rejects bare-colon endings

`last_line_is_prompt` SHALL NOT accept a bare trailing `:` as a
waiting cursor. A bare trailing `?` SHALL be accepted only when the
same line carries a known question lead-word (Continue, Approve,
Proceed, Confirm, Apply, Should I, Do you want, Would you like,
Which option/approach/one, Choose, Select, Pick, Need clarification,
Need more …, Please clarify/confirm/choose/specify).

#### Scenario: narrative status line ending in ":" does not trigger asking
- **WHEN** the bottom line of the tail is `Reading file: …:` AND an
  older `Continue?` appears earlier in scrollback
- **THEN** `classify_tail` echoes `quiet`

### Requirement: BLOCKED_PATTERNS cover codex-fleet stuck states

`BLOCKED_PATTERNS` SHALL match each of: git merge conflict
(`CONFLICT (content`), `error: uncommitted changes`, `fatal: ` git
errors, `Permission denied (publickey)`, `gh: command not found`,
`Bad credentials`, `MCP server <name> (not found|missing|unavailable)`,
`429 Too Many Requests`, and the canonical `BLOCKED:` prefix —
in addition to the prior set (PLAN_SUBTASK_NOT_FOUND, stale-claim,
told-not-to-rescue, less-than-5%-limit, etc.).

#### Scenario: missing MCP server is classified as blocked
- **WHEN** the tail contains `Error: MCP server colony not found in
  the registered servers`
- **THEN** `classify_tail` echoes `blocked`

### Requirement: replay harness pins classifier behavior

The system SHALL ship a replay harness at
`scripts/codex-fleet/test/test-claude-supervisor-classifier.sh`
that discovers every `*.txt` fixture under
`scripts/codex-fleet/test/fixtures/claude-supervisor-classifier/`,
parses the expected label from the filename prefix
(`<label>__*.txt`), and asserts `classify_tail` returns that label.
The harness SHALL exit non-zero if any fixture mismatches.

#### Scenario: harness passes against the current classifier
- **WHEN** the harness is run with the lib at its current head
- **THEN** every fixture's `actual` label equals its `expected` label
- **AND** the harness exits 0
