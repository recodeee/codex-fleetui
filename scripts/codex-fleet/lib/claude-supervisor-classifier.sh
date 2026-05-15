#!/usr/bin/env bash
# claude-supervisor-classifier.sh — pure pane-tail classifier extracted from
# claude-supervisor.sh so the daemon and the replay harness share one
# implementation. This file is sourced; it MUST NOT call tmux, claude,
# date(1) for state, or write any files at source time.
#
# Public functions:
#   classify_tail "<tail>"   → echoes one of: busy | asking | blocked | quiet
#   is_busy / is_asking / is_blocked / last_line_is_prompt
#   tail_hash "<tail>"       → 12-char stable hash with timestamps stripped
#
# Public arrays (read-only after source):
#   BUSY_PATTERNS, ASK_PATTERNS, BLOCKED_PATTERNS

# How many trailing non-empty lines count as "recent" for the busy/ask gates.
# 80 is the full capture window; 8 is roughly the on-screen visible region of
# a codex pane after ANSI is stripped. Stale busy/ask signals further back in
# scrollback do not represent the worker's current state, so we ignore them.
: "${CLAUDE_SUPERVISOR_RECENT_LINES:=8}"

# Patterns that mean the worker is genuinely blocked and needs help — not
# just "polling for the next ready task". Matched with grep -iE on the full
# tail (blockers are sticky; a stale blocker the worker hasn't unstuck is
# still a real blocker).
BLOCKED_PATTERNS=(
  "PLAN_SUBTASK_NOT_FOUND"
  "PLAN_SUBTASK_NOT_AVAILABLE"
  "stale.*blocker"
  "stale-claim"
  "did not rescue"
  "told me not to rescue"
  "No branch, worktree, edits, or PR were created"
  "BLOCKED preflight"
  "BLOCKED writable-root"
  "I waited and rechecked"
  "less than 5% of your 5h limit"
  "^.*Blocked\."
  # Added 2026-05-15 — real failure modes seen in the wild that the
  # supervisor was deaf to:
  "CONFLICT \(content"             # git merge conflict
  "merge conflict"                  # plain-text variant
  "Your local changes .* would be overwritten"
  "error: uncommitted changes"
  "^fatal: "                        # git fatal
  "Permission denied \(publickey\)" # ssh auth bust
  "gh: command not found"
  "gh auth: "                       # gh auth error variants
  "Bad credentials"                 # GitHub token rejected
  "MCP server .*(not found|missing|unavailable)"
  "429 Too Many Requests"           # cap-swap should catch this, but in
                                    # case it doesn't, surface to supervisor
  "remote: Permission to .* denied"
  "BLOCKED:"                        # canonical Guardex blocker prefix
)

# Patterns that mean codex is talking BACK — a question, a menu, an approval
# prompt. These are matched on a *narrow* trailing window combined with the
# last_line_is_prompt cursor gate so that mid-task narration ("Should I
# also lint?") does not trip the classifier.
ASK_PATTERNS=(
  "\(recommended\)"
  "\(default\)"
  "\[Y/n\]"
  "\[y/N\]"
  "\(y/n\)"
  "Continue\?"
  "Approve\?"
  "Proceed\?"
  "Confirm\?"
  "Apply\?"
  "Would you like"
  "Do you want"
  "Should I"
  "Which option"
  "Which approach"
  "Which one"
  "Choose one"
  "Choose:"
  "Select one"
  "Select:"
  "Pick one"
  "press [0-9] to"
  "type [0-9] to"
  "Need clarification"
  "Need more (info|context|detail)"
  "Please (clarify|confirm|choose|specify)"
  "^[[:space:]]*[1-9][.)] .*\?$"
  "^[[:space:]]*[1-9][.)] .*\(recommended\)"
)

# Patterns that mean the worker is actively executing — DO NOT interrupt.
# Matched only on the recent window so a 40-line-old `Working (12s)` doesn't
# mask a brand-new `[Y/n]` cursor on the bottom line.
BUSY_PATTERNS=(
  "esc to interrupt"
  "Working ("
  # codex's post-completion footer reads "Worked for 5m 12s …". That line
  # by itself means the worker is now IDLE (quiet), not busy — leave it
  # out of BUSY_PATTERNS. The plan-watcher fast path picks it up.
)

# Strip ANSI then return the last N non-empty lines, newline-separated.
# Used by is_busy and is_asking to scope detection to the recent on-screen
# region. N defaults to CLAUDE_SUPERVISOR_RECENT_LINES.
tail_recent_lines() {
  local tail="$1" n="${2:-$CLAUDE_SUPERVISOR_RECENT_LINES}"
  printf '%s\n' "$tail" \
    | sed -E 's/\x1b\[[0-9;?]*[a-zA-Z]//g' \
    | awk 'NF' \
    | tail -n "$n"
}

# Busy detection is anchored to the LAST non-empty line. codex's CLI rewrites
# the "Working (12s • ↑ 4.2k tokens • esc to interrupt)" footer in place; if
# the worker is genuinely busy that line is the bottom of the capture. Once
# the turn completes, codex replaces it with a "Worked for …" footer and the
# pane is no longer busy. Looking deeper than the last line lets a stale
# `Working (` in scrollback mask a brand-new interactive menu below it
# (FM-B in the audit).
is_busy() {
  local tail="$1"
  local last; last="$(printf '%s\n' "$tail" \
    | sed -E 's/\x1b\[[0-9;?]*[a-zA-Z]//g' \
    | awk 'NF{ln=$0} END{print ln}')"
  [ -z "$last" ] && return 1
  local pat
  for pat in "${BUSY_PATTERNS[@]}"; do
    printf '%s\n' "$last" | grep -qF -- "$pat" && return 0
  done
  return 1
}

is_blocked() {
  local tail="$1"
  local pat
  for pat in "${BLOCKED_PATTERNS[@]}"; do
    if printf '%s\n' "$tail" | grep -qiE -- "$pat"; then return 0; fi
  done
  return 1
}

is_asking() {
  local tail="$1"
  # Two-gate precision filter:
  #  1) the bottom of the pane must LOOK like a waiting cursor
  #     (last_line_is_prompt)
  #  2) one of the ASK_PATTERNS must appear in the same recent window
  # Either gate alone produces false positives — scrollback `Continue?`
  # 60 lines back + a normal status line ending in `:` previously slipped
  # through. Both gates together cut FPs to ~0 on our fixture corpus.
  last_line_is_prompt "$tail" || return 1
  local recent; recent="$(tail_recent_lines "$tail")"
  local pat
  for pat in "${ASK_PATTERNS[@]}"; do
    if printf '%s\n' "$recent" | grep -qiE -- "$pat"; then return 0; fi
  done
  return 1
}

# True iff the bottom of the captured tail looks like codex is sitting at a
# waiting prompt. We accept ONLY high-precision cursor shapes — a bare
# trailing `:` no longer qualifies (workers narrate "Reading file:" all the
# time), and a bare trailing `?` only qualifies when the same line carries a
# known question lead-word.
last_line_is_prompt() {
  local tail="$1"
  local last; last="$(printf '%s\n' "$tail" \
    | sed -E 's/\x1b\[[0-9;?]*[a-zA-Z]//g' \
    | awk 'NF{ln=$0} END{print ln}')"
  [ -z "$last" ] && return 1

  # High-precision cursor shapes — these only appear at the bottom when the
  # CLI is genuinely waiting. Bare `:$` was previously here and was the
  # #1 false-positive source; it is removed deliberately.
  local cursor_patterns=(
    '\[Y/n\][[:space:]]*$'
    '\[y/N\][[:space:]]*$'
    '\(y/n\)[[:space:]]*$'
    '^[[:space:]]*[>❯▶➜▷»][[:space:]]*$'
    '[>❯▶➜▷»][[:space:]]*$'
    '\$[[:space:]]*$'
    '#[[:space:]]*$'
    'press [0-9]'
    'type [0-9]'
    'press [a-z] to'
    '^[[:space:]]*[0-9]+[).][[:space:]]+.*\(recommended\)'
    '^[[:space:]]*[0-9]+[).][[:space:]]+.*\(default\)'
  )
  local pat
  for pat in "${cursor_patterns[@]}"; do
    if printf '%s\n' "$last" | grep -qiE -- "$pat"; then return 0; fi
  done

  # Bare trailing `?` is admitted only when the same line carries a known
  # question lead-word. Catches "Continue?", "Approve?", "Should I … ?"
  # but rejects "do you want to keep going? yes." in mid-paragraph
  # narration that happens to wrap onto the last line of the capture.
  case "$last" in
    *\?|*\?\ )
      printf '%s\n' "$last" | grep -qiE -- '(Continue|Approve|Proceed|Confirm|Apply|Should I|Do you want|Would you like|Which (option|approach|one)|Choose|Select|Pick|Need (clarification|more)|Please (clarify|confirm|choose|specify))'
      return $?
      ;;
  esac
  return 1
}

# Classify the recent tail into busy | asking | blocked | quiet. Ordering
# matters: `asking` outranks `blocked` because a pane that mentioned a stale
# blocker but is now showing an interactive menu wants an answer to the
# menu, not a rescue lecture. `busy` is only checked against the recent
# window now, so a fresh ask under a stale "Working (" no longer slips.
classify_tail() {
  local tail="$1"
  if is_busy    "$tail"; then echo busy;    return; fi
  if is_asking  "$tail"; then echo asking;  return; fi
  if is_blocked "$tail"; then echo blocked; return; fi
  echo quiet
}

# Hash of the relevant tail so we don't re-ask claude the same question.
# Strip ANSI + timestamps so "Worked for 9m 36s → 10m 12s" or wall-clock
# ticking doesn't bust the cache when nothing real changed.
tail_hash() {
  local tail="$1"
  printf '%s\n' "$tail" \
    | sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/Worked for [0-9smh ]+//g; s/[0-9]{2}:[0-9]{2}:[0-9]{2}//g' \
    | sha1sum | cut -c1-12
}
