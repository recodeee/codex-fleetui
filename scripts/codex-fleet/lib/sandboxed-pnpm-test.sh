#!/usr/bin/env bash
set -eo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: sandboxed-pnpm-test.sh <package-filter> [extra pnpm args...]

Runs:
  pnpm --filter <package-filter> test [extra pnpm args...]

The command delegates to sandbox-run.sh with the node image when available.
If sandbox-run.sh or microsandbox is unavailable, it runs pnpm directly on the
host.
USAGE
}

log() { printf '[sandboxed-pnpm-test] %s\n' "$*" >&2; }

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

if [ "$#" -lt 1 ]; then
  usage
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}}"
SANDBOX_RUN="$SCRIPT_DIR/sandbox-run.sh"

package_filter="$1"
shift

cmd=(pnpm --filter "$package_filter" test "$@")

if [ -f "$SANDBOX_RUN" ] && [ "${MICROSANDBOX_DISABLE:-0}" != "1" ] && command -v msb >/dev/null 2>&1; then
  exec bash "$SANDBOX_RUN" --image node --cwd "$REPO" -- "${cmd[@]}"
fi

if [ ! -f "$SANDBOX_RUN" ]; then
  log "fallback: sandbox-run.sh not found; running on host"
elif [ "${MICROSANDBOX_DISABLE:-0}" = "1" ]; then
  log "fallback: MICROSANDBOX_DISABLE=1; running on host"
else
  log "fallback: msb not available; running on host"
fi

cd "$REPO"
exec "${cmd[@]}"
