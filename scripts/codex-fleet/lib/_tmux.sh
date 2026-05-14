# _tmux.sh — env-driven wrapper around the `tmux` binary.
#
# Source this from any fleet script to make subsequent `tmux ...` calls
# honor the optional CODEX_FLEET_TMUX_SOCKET env var. When the env is set,
# every `tmux` call is routed to that socket (`tmux -L "$SOCKET" ...`),
# which lets the fleet run on a dedicated server (see
# scripts/codex-fleet/tmux/up.sh and the option-(b) integration in PR
# #__).
#
# When the env is unset (default), the function is a transparent pass-through
# — `tmux ...` behaves exactly as before. This means migrating a fleet script
# to support the dedicated-socket setup is a one-line addition (source this
# file at the top), and the default behavior is unchanged for operators not
# using the new integration yet.
#
# Bash function shadowing rules: this defines a `tmux` shell function that
# takes precedence over the binary inside the sourcing script's process.
# Subprocess invocations of `tmux` (e.g. `$(tmux ...)`, or scripts spawned
# from this one) will only see the function if it's exported. We export it
# via `export -f tmux` so child bash scripts pick it up automatically. Non-
# bash subprocesses still hit the binary; if `CODEX_FLEET_TMUX_SOCKET` is
# set in the env, those subprocesses can also opt in with their own source
# of this file.
#
# Escape hatch: call `command tmux ...` to invoke the real binary regardless
# of the wrapper. Useful when you specifically need to touch the operator's
# default tmux server (e.g. detecting whether the operator already has any
# tmux sessions running before deciding which socket to use).
#
# Usage in a fleet script:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"
#   tmux ls   # → `tmux -L codex-fleet ls` when CODEX_FLEET_TMUX_SOCKET=codex-fleet
#   tmux ls   # → `tmux ls`                 when CODEX_FLEET_TMUX_SOCKET is unset

tmux() {
  if [[ -n "${CODEX_FLEET_TMUX_SOCKET:-}" ]]; then
    command tmux -L "$CODEX_FLEET_TMUX_SOCKET" "$@"
  else
    command tmux "$@"
  fi
}
export -f tmux
