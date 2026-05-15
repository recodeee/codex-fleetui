#!/usr/bin/env bash
# ios-nav-strip — apply an iOS-style window navigation strip to the TOP of a
# given codex-fleet tmux session. Each tmux window renders as a chip; the
# active window is highlighted in iOS blue, inactive windows in iOS gray.
#
# Palette (iOS systemColors):
#   #007AFF  iOS blue       — active window accent dot
#   #FFFFFF  white          — active window label
#   #8E8E93  iOS gray       — inactive window label / right-side tag
#   #3A3A3C  separator gray — pipe glyph between chips
#
# Sister to scripts/codex-fleet/style-tabs.sh. That script handles the broader
# tab chrome (pills, half-circle caps, multi-row padding); this one is scoped
# narrowly to the TOP nav strip and uses pure tmux options — no extra
# processes spawned, no formatters, no daemons. Re-run is idempotent.
#
# Undo (per-option):
#   tmux -L <session> set-option  -gu status-position
#   tmux -L <session> set-option  -gu status
#   tmux -L <session> set-option  -gu status-justify
#   tmux -L <session> set-option  -gu status-style
#   tmux -L <session> set-option  -gu status-left
#   tmux -L <session> set-option  -gu status-right
#   tmux -L <session> set-option  -gu status-interval
#   tmux -L <session> set-window-option -gu window-status-format
#   tmux -L <session> set-window-option -gu window-status-current-format
#   tmux -L <session> set-window-option -gu window-status-separator
#
# Usage:
#   bash scripts/codex-fleet/lib/ios-nav-strip.sh <session-name>

set -euo pipefail

SESSION="${1:-}"

if [[ -z "$SESSION" ]]; then
  echo "usage: bash scripts/codex-fleet/lib/ios-nav-strip.sh <session-name>" >&2
  exit 1
fi

# Project convention: socket name == session name for codex-fleet sessions.
if ! tmux -L "$SESSION" has-session -t "$SESSION" 2>/dev/null; then
  echo "[ios-nav-strip] session '$SESSION' not running on socket -L $SESSION; skipping (idempotent)."
  exit 0
fi

# iOS palette tokens (kept inline so the chrome reads as one continuous surface
# with fleet-tick.sh / style-tabs.sh).
IOS_BLUE="#007AFF"
IOS_WHITE="#FFFFFF"
IOS_GRAY="#8E8E93"
IOS_SEP="#3A3A3C"

# Inactive chip: gray label + a trailing separator pipe. Separator color is the
# darker iOS gray so it reads as a divider, not as text.
INACTIVE_FMT="#[fg=${IOS_GRAY},nobold] #I #W #[fg=${IOS_SEP}]|#[default]"
# Active chip: leading iOS-blue dot + bold white label.
ACTIVE_FMT="#[fg=${IOS_BLUE},bold]●#[fg=${IOS_WHITE},bold] #I #W #[default]"

# Apply tmux options. Each set-option / set-window-option is idempotent — tmux
# silently overwrites the previous value.
tmux -L "$SESSION" set-option        -g  status-position    top
tmux -L "$SESSION" set-option        -g  status             on
tmux -L "$SESSION" set-option        -g  status-justify     left
# bg=default keeps the strip transparent so it matches the glass chrome the
# rest of the project paints with fleet-tick.sh + style-tabs.sh.
tmux -L "$SESSION" set-option        -g  status-style       "fg=${IOS_WHITE},bg=default"
tmux -L "$SESSION" set-option        -g  status-left        ""
tmux -L "$SESSION" set-option        -g  status-right       "#[fg=${IOS_GRAY}] codex-fleet "
tmux -L "$SESSION" set-option        -g  status-interval    2

tmux -L "$SESSION" set-window-option -g  window-status-format         "$INACTIVE_FMT"
tmux -L "$SESSION" set-window-option -g  window-status-current-format "$ACTIVE_FMT"
# Separator is inlined inside window-status-format so adjacent chips don't get
# a double divider.
tmux -L "$SESSION" set-window-option -g  window-status-separator      ""

# Verify by reading back status-position. Operators can grep this line in logs.
position="$(tmux -L "$SESSION" show-option -gv status-position 2>/dev/null || true)"
echo "[ios-nav-strip] session=${SESSION} status-position=${position:-<unset>}"
