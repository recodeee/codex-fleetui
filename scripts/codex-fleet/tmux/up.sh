#!/usr/bin/env bash
# up.sh — start the fleet's tmux server on a dedicated socket with oh-my-tmux
# vendored from scripts/codex-fleet/tmux/vendor/.
#
# Architecture (option (b), fleet-bundled):
#   - Dedicated socket: tmux -L codex-fleet
#   - Dedicated config: -f scripts/codex-fleet/tmux/vendor/oh-my-tmux/.tmux.conf
#   - Operator's normal tmux server (default socket) is entirely untouched.
#
# This launcher only stands up the empty tmux server + the codex-fleet
# session. It does NOT spawn codex panes — that's full-bringup.sh's job.
# Spawning the server first means full-bringup.sh's existing `tmux ...`
# commands route here if the operator sets CODEX_FLEET_TMUX_SOCKET=codex-fleet
# in the env (the scripts/codex-fleet/lib/_tmux.sh wrapper picks that up).
#
# After starting the server, up.sh imperatively stamps the codex-fleet option
# overrides (mouse on, history-limit, pane border colors) via `tmux
# set-option`. We use this instead of `#!important` lines in
# .tmux.conf.local because oh-my-tmux's `_apply_important` is documented to
# re-apply those, but in practice the deferred run-shell timing means it
# doesn't reliably fire in the no-client-yet startup path. Imperative
# set-options after the server is up are unconditional — they win.
#
# Usage:
#   ./scripts/codex-fleet/tmux/up.sh
#   ./scripts/codex-fleet/tmux/up.sh --attach   # also attaches on success
#
# Env:
#   CODEX_FLEET_SESSION  session name to create (default: codex-fleet)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SOCKET="codex-fleet"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
CONF="$SCRIPT_DIR/vendor/oh-my-tmux/.tmux.conf"

log() { printf '[tmux/up] %s\n' "$*" >&2; }

# 0. Setup must have run at least once.
if [[ ! -f "$CONF" ]]; then
  log "vendored oh-my-tmux not found at $CONF"
  log "run ./scripts/codex-fleet/tmux/setup.sh first."
  exit 2
fi

# 1. Export the repo root so any future binding hookups know where to find
#    their helper scripts.
export CODEX_FLEET_REPO_ROOT="$REPO_ROOT"

# 2. Start the dedicated server + session if not already up.
if tmux -L "$SOCKET" has-session -t "$SESSION" 2>/dev/null; then
  log "fleet tmux session '$SESSION' already running on socket '$SOCKET'"
else
  log "starting fleet tmux server (socket=$SOCKET, session=$SESSION)"
  log "config: $CONF"
  tmux -L "$SOCKET" -f "$CONF" new-session -d -s "$SESSION" -n overview -x 274 -y 76
fi

# 3. Apply codex-fleet option overrides. See architecture note above for why
#    this is imperative rather than declarative in .tmux.conf.local.
log "applying codex-fleet option overrides (mouse, history-limit, borders)"
tmux -L "$SOCKET" set-option -g mouse on
tmux -L "$SOCKET" set-option -g history-limit 50000
tmux -L "$SOCKET" set-option -g pane-border-style 'fg=#3c3c41'
tmux -L "$SOCKET" set-option -g pane-active-border-style 'fg=#0a84ff'

# 4. Inject CODEX_FLEET_REPO_ROOT into the tmux server's global env so
#    bindings sourced below (the iOS action sheet, section jumper, help
#    popup) can find their helper scripts. The bindings use single-quoted
#    display-popup commands that expand `${CODEX_FLEET_REPO_ROOT}` via
#    the shell at fire time, and the shell inside display-popup reads
#    from tmux's server env.
tmux -L "$SOCKET" set-environment -g CODEX_FLEET_REPO_ROOT "$REPO_ROOT"

# 5. Source the iOS-style bindings AFTER the server is up. Done here
#    (rather than from codex-fleet-overlay.conf) because oh-my-tmux's
#    `_apply_bindings` runs late and would otherwise re-stamp prefix-m /
#    prefix-Tab / prefix-C-h back to its defaults. Imperative source-file
#    after the initial init is unconditional — these bindings win.
tmux -L "$SOCKET" source-file "$REPO_ROOT/scripts/codex-fleet/tmux-bindings.conf"

log ""
log "session ready. Useful next commands:"
log "  ./scripts/codex-fleet/tmux/attach.sh        # attach to the fleet session"
log "  ./scripts/codex-fleet/full-bringup.sh       # default fleet bring-up — now"
log "                                              # routes through the dedicated"
log "                                              # socket (set CODEX_FLEET_TMUX_SOCKET=''"
log "                                              # to opt out)"
log "  ./scripts/codex-fleet/tmux/down.sh          # kill the fleet tmux server"

if [[ "${1:-}" == "--attach" ]]; then
  exec "$SCRIPT_DIR/attach.sh"
fi
