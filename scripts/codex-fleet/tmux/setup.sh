#!/usr/bin/env bash
# setup.sh — vendor oh-my-tmux into scripts/codex-fleet/tmux/vendor/ so
# codex-fleet can run on its own tmux server with a curated config, without
# touching the operator's normal ~/.tmux.conf or affecting any tmux session
# outside the fleet.
#
# Architecture, fleet-bundled (option (b)):
#   - vendor/oh-my-tmux/.tmux.conf       — upstream (gpakosz/.tmux), cloned
#                                          on first run. Re-runnable.
#   - vendor/oh-my-tmux/.tmux.conf.local — sample copied in by upstream's
#                                          clone. setup.sh stamps a marker
#                                          block at the end with a
#                                          `source-file` line pointing at the
#                                          overlay. Operator edits above the
#                                          marker survive re-runs.
#   - codex-fleet-overlay.conf           — committed; plugins + future fleet
#                                          binding extensions. Loaded via the
#                                          source-file line stamped in .local.
#
# Why we don't try to override oh-my-tmux defaults from .tmux.conf.local:
#   Oh-my-tmux's `_apply_configuration` runs LAST and resets many options to
#   its theme defaults. `_apply_important` is documented to re-apply lines
#   ending in `#!important`, but in practice the deferred run-shell timing
#   means those re-applications don't reliably fire in our launcher path
#   (server start without a long-lived attached client). Rather than fight
#   the framework, `up.sh` explicitly stamps the codex-fleet option overrides
#   (mouse on, history-limit, pane border colors) via `tmux set-option` AFTER
#   the server is up. Those calls win unconditionally because they're
#   imperative, not config-time.
#
# Idempotent. Safe to re-run; only updates what's missing or stale.
#
# Usage:
#   ./scripts/codex-fleet/tmux/setup.sh
#
# Env (optional):
#   OH_MY_TMUX_REF   git ref to check out (default: master)
#   OH_MY_TMUX_REPO  upstream URL (default: https://github.com/gpakosz/.tmux.git)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="$SCRIPT_DIR/vendor"
OMT_DIR="$VENDOR_DIR/oh-my-tmux"
OVERLAY="$SCRIPT_DIR/codex-fleet-overlay.conf"
OMT_LOCAL="$OMT_DIR/.tmux.conf.local"
MARKER_BEGIN="# --- codex-fleet overlay BEGIN (stamped by tmux/setup.sh) ---"
MARKER_END="# --- codex-fleet overlay END ---"

REF="${OH_MY_TMUX_REF:-master}"
REPO="${OH_MY_TMUX_REPO:-https://github.com/gpakosz/.tmux.git}"

log() { printf '[tmux/setup] %s\n' "$*" >&2; }

mkdir -p "$VENDOR_DIR"

# 1. Clone or refresh oh-my-tmux.
if [[ ! -d "$OMT_DIR/.git" ]]; then
  log "cloning oh-my-tmux ($REPO @ $REF) → $OMT_DIR"
  git clone --single-branch --branch "$REF" --depth 1 "$REPO" "$OMT_DIR"
else
  log "oh-my-tmux already present at $OMT_DIR; skipping clone (delete vendor/ to refresh)"
fi

# 2. Sanity check upstream layout.
for required in "$OMT_DIR/.tmux.conf" "$OMT_DIR/.tmux.conf.local"; do
  if [[ ! -f "$required" ]]; then
    log "ERROR: upstream layout drift — expected $required"
    log "       did the upstream repo restructure? bail before writing junk."
    exit 3
  fi
done

# 3. Strip any prior overlay block from .local so we re-stamp cleanly on
#    re-run. The marker pair makes the region addressable; operator edits
#    above the marker survive untouched.
if grep -qF "$MARKER_BEGIN" "$OMT_LOCAL"; then
  log "removing previous overlay block from $OMT_LOCAL"
  TMP=$(mktemp)
  awk -v begin="$MARKER_BEGIN" -v end="$MARKER_END" '
    $0 == begin { skip=1; next }
    skip && $0 == end { skip=0; next }
    !skip { print }
  ' "$OMT_LOCAL" > "$TMP"
  mv "$TMP" "$OMT_LOCAL"
fi

# 4. Stamp our overlay block. Only contains the source-file line; the actual
#    fleet-vs-default option overrides are applied imperatively by up.sh (see
#    the architecture note above).
log "stamping overlay block into $OMT_LOCAL"
cat >> "$OMT_LOCAL" <<EOF

$MARKER_BEGIN
# Do not edit this block — re-run scripts/codex-fleet/tmux/setup.sh to re-stamp.
# Operator customizations belong ABOVE this marker (they survive re-runs).
# Fleet-vs-default option overrides (mouse, history-limit, pane border colors)
# are NOT here — up.sh applies them imperatively after the server is up.
source-file '$OVERLAY'
$MARKER_END
EOF

log "done."
log ""
log "next steps:"
log "  1. ./scripts/codex-fleet/tmux/up.sh    — start the fleet's tmux server"
log "  2. ./scripts/codex-fleet/tmux/attach.sh — attach to the fleet session"
log "  3. inside tmux, hit <prefix> I — TPM installs the configured plugins"
log "                                  (tmux-autoreload is in the overlay)"
