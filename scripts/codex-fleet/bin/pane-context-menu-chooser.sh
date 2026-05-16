#!/usr/bin/env bash
# pane-context-menu-chooser.sh — dispatches the iOS pane context menu.
#
# Historically this script could prefer a ratatui-rendered overlay in the
# fleet-tui-poc binary; that POC was deleted once fleet-ui shipped the
# canonical overlay widgets. The bash renderer is now the sole renderer
# until/unless a fleet-ui-backed binary replaces it.
#
# Usage: pane-context-menu-chooser.sh <pane_id>

set -eo pipefail

PANE_ID="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BASH_MENU="$SCRIPT_DIR/pane-context-menu.sh"

exec bash "$BASH_MENU" "$PANE_ID"
