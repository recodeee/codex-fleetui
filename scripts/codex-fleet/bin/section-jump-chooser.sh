#!/usr/bin/env bash
# section-jump-chooser.sh — placeholder for the section-jump overlay
# bound to `prefix Tab` (or whatever key the operator picked).
#
# The Rust binary that owned this overlay (fleet-tui-poc) was deleted
# once fleet-ui shipped the canonical overlay widgets. A fleet-ui-backed
# replacement has not been wired up yet, so this stub simply displays a
# message in tmux instead of erroring out the keybind.
#
# Usage: section-jump-chooser.sh [session-name]

set -eo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/../lib/_tmux.sh"

tmux display-message -d 3000 \
  " section-jump: overlay pending — fleet-tui-poc deleted, fleet-ui port TBD "
exit 0
