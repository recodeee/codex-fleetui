#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: sandboxed-cargo-test.sh <crate-name> [extra cargo test args...]

Runs:
  cargo test -p <crate-name> [extra cargo test args...]

When microsandbox is available, delegates through scripts/codex-fleet/lib/sandbox-run.sh
with the codex-fleet repo root mounted as the working directory. When the helper is
not available yet, it runs the same cargo command on the host.
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  if [[ $# -lt 1 ]]; then
    exit 64
  fi
  exit 0
fi

SCRIPT_SOURCE="${BASH_SOURCE[0]}"
while [[ -L "$SCRIPT_SOURCE" ]]; do
  SCRIPT_DIR="$(cd -P -- "$(dirname -- "$SCRIPT_SOURCE")" && pwd)"
  SCRIPT_NEXT="$(readlink "$SCRIPT_SOURCE")"
  case "$SCRIPT_NEXT" in
    /*) SCRIPT_SOURCE="$SCRIPT_NEXT" ;;
    *) SCRIPT_SOURCE="$SCRIPT_DIR/$SCRIPT_NEXT" ;;
  esac
done

SCRIPT_DIR="$(cd -P -- "$(dirname -- "$SCRIPT_SOURCE")" && pwd)"
# shellcheck source=scripts/codex-fleet/lib/_env.sh
source "$SCRIPT_DIR/_env.sh"

CRATE_NAME="$1"
shift

CARGO_CMD=(cargo test -p "$CRATE_NAME" "$@")
SANDBOX_RUNNER="$CODEX_FLEET_LIB_DIR/sandbox-run.sh"

if [[ ! -f "$SANDBOX_RUNNER" ]]; then
  printf '[sandboxed-cargo-test] fallback: sandbox-run.sh not found; running on host\n' >&2
  cd "$CODEX_FLEET_REPO_ROOT"
  exec "${CARGO_CMD[@]}"
fi

if [[ -x "$SANDBOX_RUNNER" ]]; then
  exec "$SANDBOX_RUNNER" --image rust --cwd "$CODEX_FLEET_REPO_ROOT" -- "${CARGO_CMD[@]}"
fi

exec bash "$SANDBOX_RUNNER" --image rust --cwd "$CODEX_FLEET_REPO_ROOT" -- "${CARGO_CMD[@]}"
