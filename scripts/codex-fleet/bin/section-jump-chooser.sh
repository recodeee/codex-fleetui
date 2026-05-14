#!/usr/bin/env bash
# section-jump-chooser.sh — launches the ratatui section-jump overlay
# bound to `prefix Tab` (or whatever key the operator picked).
#
# The Rust binary owns the rendering + dispatch; this thin wrapper just
# resolves the binary path, infers the current tmux session/window, and
# fails loudly if the binary hasn't been built yet (unlike the context
# menu there is no bash fallback for this overlay).
#
# Usage: section-jump-chooser.sh [session-name]

set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/../lib/_tmux.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"

# Workspace target, same convention as pane-context-menu-chooser.sh.
RUST_BIN="$REPO_ROOT/rust/target/release/fleet-tui-poc"
if [ ! -x "$RUST_BIN" ]; then
  # Fall back to the debug build if release isn't compiled yet; useful
  # for `cargo build` (no --release) during local development.
  RUST_BIN="$REPO_ROOT/rust/target/debug/fleet-tui-poc"
fi

if [ ! -x "$RUST_BIN" ]; then
  tmux display-message -d 3000 \
    " section-jump: fleet-tui-poc not built — run cargo build -p fleet-tui-poc "
  exit 0
fi

# Session: explicit arg wins, else $TMUX_SESSION env, else `tmux display-message`
# (which prints the focused session name), else hardcoded "codex-fleet".
SESSION="${1:-${TMUX_SESSION:-$(tmux display-message -p -F '#{session_name}' 2>/dev/null || echo codex-fleet)}}"
ACTIVE_WIN="$(tmux display-message -p -F '#{window_name}' 2>/dev/null || true)"

ARGS=(--overlay section-jump --session "$SESSION")
if [ -n "$ACTIVE_WIN" ]; then
  ARGS+=(--active "$ACTIVE_WIN")
fi

exec "$RUST_BIN" "${ARGS[@]}"
