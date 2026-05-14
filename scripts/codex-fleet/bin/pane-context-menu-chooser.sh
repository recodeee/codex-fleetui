#!/usr/bin/env bash
# pane-context-menu-chooser.sh — picks between the ratatui-rendered iOS
# context menu (rust/fleet-tui-poc binary) and the bash renderer.
#
# The Rust binary draws the same design with a 3D drop shadow + smoother
# chrome; if it's been built, prefer it. Otherwise fall back to the bash
# renderer so right-click still works on hosts that haven't compiled the
# crate.
#
# Usage: pane-context-menu-chooser.sh <pane_id>

set -eo pipefail

PANE_ID="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${CODEX_FLEET_REPO_ROOT:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"

# Cargo workspace target lives at rust/target/, not rust/fleet-tui-poc/target/.
RUST_BIN="$REPO_ROOT/rust/target/release/fleet-tui-poc"
BASH_MENU="$SCRIPT_DIR/pane-context-menu.sh"

if [ -x "$RUST_BIN" ]; then
  exec "$RUST_BIN" --overlay context-menu --pane "$PANE_ID"
else
  exec bash "$BASH_MENU" "$PANE_ID"
fi
