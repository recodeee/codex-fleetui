#!/usr/bin/env bash
# overview-header.sh — split the overview window to host a 1-row pane that
# runs `fleet-tab-strip`, so the worker-grid window carries the same
# in-binary tab strip the ratatui dashboards (windows 1-5) draw.
#
# Idempotent by design: if a header pane already exists (detected via the
# pane-local `@panel` option `[codex-fleet-tab-strip]`), this script is a
# no-op. Safe to run on bringup AND for live retrofit on an already-
# running session that pre-dates the header.
#
# Usage:
#   bash scripts/codex-fleet/overview-header.sh                # default codex-fleet:overview
#   CODEX_FLEET_SESSION=codex-fleet-2 bash …/overview-header.sh
#
# Env:
#   CODEX_FLEET_SESSION              tmux session (default: codex-fleet)
#   CODEX_FLEET_OVERVIEW_WINDOW      window name (default: overview)
#   CODEX_FLEET_OVERVIEW_HEADER_ROWS rows reserved for the header (default: 1).
#                                    Set to 0 to skip the header entirely.
#   CODEX_FLEET_REPO_ROOT            repo root (default: this script's repo root)
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
WINDOW="${CODEX_FLEET_OVERVIEW_WINDOW:-overview}"
ROWS="${CODEX_FLEET_OVERVIEW_HEADER_ROWS:-1}"

log() { printf '\033[36m[overview-header]\033[0m %s\n' "$*"; }
warn() { printf '\033[33m[overview-header]\033[0m %s\n' "$*" >&2; }

if (( ROWS <= 0 )); then
  log "ROWS=$ROWS — header disabled, skipping"
  exit 0
fi

target="$SESSION:$WINDOW"
if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  warn "session $SESSION not running — nothing to do (full-bringup will call us later)"
  exit 0
fi
if ! tmux list-windows -t "$SESSION" -F '#{window_name}' | grep -qx "$WINDOW"; then
  warn "window $target not present — nothing to do"
  exit 0
fi

# Idempotency check: scan existing panes for the @panel marker we stamp on
# the header. Found → already installed, exit clean. `-F` keeps the
# square-bracket marker literal (without it, grep treats `[codex-...]`
# as a character class and emits "Invalid range end").
if tmux list-panes -t "$target" -F '#{@panel}' 2>/dev/null | grep -qFx '[codex-fleet-tab-strip]'; then
  log "header pane already present on $target — no-op"
  exit 0
fi

# Resolve the binary. Prefer release (full-bringup builds release); fall
# back to debug for local cargo build sessions.
BIN="$REPO_ROOT/rust/target/release/fleet-tab-strip"
if [ ! -x "$BIN" ]; then
  BIN="$REPO_ROOT/rust/target/debug/fleet-tab-strip"
fi
if [ ! -x "$BIN" ]; then
  warn "fleet-tab-strip not built at $REPO_ROOT/rust/target/{release,debug}/fleet-tab-strip — run: cargo build --release -p fleet-tab-strip"
  exit 1
fi

# Find the current topmost pane (smallest pane_top). We split it above so
# the header lands at the very top of the window regardless of how the
# existing panes were laid out (tiled, custom, single-pane).
topmost="$(tmux list-panes -t "$target" -F '#{pane_top}|#{pane_id}' \
  | sort -t'|' -k1,1n | head -1 | cut -d'|' -f2)"
if [ -z "$topmost" ]; then
  warn "could not resolve topmost pane on $target"
  exit 1
fi
log "splitting above pane $topmost (rows=$ROWS) on $target"
tmux split-window -vb -t "$topmost" -l "$ROWS" \
  "env CODEX_FLEET_SESSION='$SESSION' '$BIN'"

# Whichever pane is now topmost is the one we just spawned. Stamp the
# marker so the next run treats it as already installed, and disable
# remain-on-exit so a Ctrl+C in the strip closes cleanly.
header_pane="$(tmux list-panes -t "$target" -F '#{pane_top}|#{pane_id}' \
  | sort -t'|' -k1,1n | head -1 | cut -d'|' -f2)"
tmux set-option -p -t "$header_pane" '@panel' '[codex-fleet-tab-strip]'
tmux set-option -p -t "$header_pane" remain-on-exit off

log "header pane installed → $header_pane (binary=$BIN)"
