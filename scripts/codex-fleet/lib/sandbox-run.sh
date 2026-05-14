#!/usr/bin/env bash
# shellcheck shell=bash
#
# sandbox-run.sh — run a command inside microsandbox when available, or on
# the host as a fallback when microsandbox is disabled / missing.

set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage: sandbox-run.sh [--image <oci-image>] [--cwd <host-path>] -- <cmd> [args...]
EOF
}

image="rust"
cwd="$PWD"
cmd=()

while (($#)); do
  case "$1" in
    --image)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      image="$2"
      shift 2
      ;;
    --cwd)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      cwd="$2"
      shift 2
      ;;
    --)
      shift
      cmd=("$@")
      break
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ ${#cmd[@]} -eq 0 ]]; then
  usage
  exit 2
fi

cwd="$(cd "$cwd" && pwd -P)"

if [[ "${MICROSANDBOX_DISABLE:-}" == "1" ]] || ! command -v msb >/dev/null 2>&1; then
  printf '%s\n' '[sandbox-run] fallback: running on host (msb not available)' >&2
  (
    cd "$cwd"
    "${cmd[@]}"
  )
  exit $?
fi

exec msb run --image "$image" --cwd "$cwd" -- "${cmd[@]}"
