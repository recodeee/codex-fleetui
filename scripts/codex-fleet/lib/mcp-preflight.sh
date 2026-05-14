#!/usr/bin/env bash
# codex-fleet MCP preflight — probe the MCP servers the staged fleet
# config will reference, and export per-MCP enable flags + timeouts
# consumed by the fleet-config.toml.tmpl renderer.
#
# The fleet worker prompt only calls `mcp__colony__*`, so this script
# focuses on Colony. We deliberately do NOT probe recodee / drawio /
# Higgsfield / coolify / hostinger-api / soul-skills — they are absent
# from the rendered fleet config by design, so probing them is wasted
# work AND would falsely block bringup when those daemons are down.
#
# Outputs (exported on success, so the caller can substitute them into
# fleet-config.toml.tmpl with sed):
#
#   FLEET_COLONY_BIN          absolute path to the colony CLI
#   FLEET_COLONY_HOME         absolute path to COLONY_HOME
#   FLEET_COLONY_ENABLED      "true" | "false"   (lowercase TOML literal)
#   FLEET_COLONY_TIMEOUT_SEC  integer            (default 60)
#   FLEET_PATH                PATH the staged config inherits
#
# Failures degrade gracefully: if Colony is unreachable, the fleet still
# stages a config that has `enabled = false` for Colony rather than
# refusing to spawn. Workers will fall back to invoking `colony` as a
# shell CLI (the worker-prompt loop already handles this), and the
# preflight log makes the degradation visible.
#
# Source it; do not exec it. Caller must have already sourced lib/_env.sh.
# This file deliberately does NOT enable `set -u` / `set -e`: those flags
# would bleed into the sourcing shell (`up.sh`, `full-bringup.sh`) and
# trip on shell-snapshot lookups (e.g. unbound ZSH_VERSION) outside our
# control. The caller picks its own strictness; we keep the lib quiet.

# --- log helpers (no-op friendly if caller defines their own) -----------
if ! declare -F preflight_log >/dev/null 2>&1; then
  preflight_log() { printf "[fleet-preflight] %s\n" "$*" >&2; }
fi
if ! declare -F preflight_warn >/dev/null 2>&1; then
  preflight_warn() { printf "[fleet-preflight] WARN %s\n" "$*" >&2; }
fi

# --- defaults -----------------------------------------------------------
: "${FLEET_COLONY_TIMEOUT_SEC:=60}"
: "${FLEET_COLONY_HOME_DEFAULT:=$HOME/Documents/recodee/colony/.omx/colony-home}"
: "${FLEET_PATH:=$HOME/.bun/bin:$HOME/.nvm/versions/node/v22.22.0/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin}"

# --- locate colony binary ------------------------------------------------
# Prefer the version on PATH (honors $HOME/.nvm symlinks). Fall back to a
# few well-known absolute paths so the preflight works from a non-login
# shell (cron, systemd) that hasn't loaded nvm.
_fleet_locate_colony() {
  local candidate
  candidate="$(command -v colony 2>/dev/null || true)"
  if [[ -n "$candidate" && -x "$candidate" ]]; then
    printf '%s\n' "$candidate"; return 0
  fi
  for candidate in \
    "$HOME/.nvm/versions/node/v22.22.0/bin/colony" \
    "$HOME/.local/bin/colony" \
    "/usr/local/bin/colony"
  do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"; return 0
    fi
  done
  return 1
}

FLEET_COLONY_BIN="$(_fleet_locate_colony || true)"

# --- probe colony --------------------------------------------------------
# Two-step health check:
#   1. Binary present + invokable (`colony --help` exits 0 fast).
#   2. COLONY_HOME directory readable.
# We do NOT spawn the MCP server itself here — that would race with the
# pane spawn and double the bringup time. The codex startup will spin it
# up; the preflight just guarantees the spawn won't blow up immediately.
FLEET_COLONY_HOME="${COLONY_HOME:-$FLEET_COLONY_HOME_DEFAULT}"
FLEET_COLONY_ENABLED="false"

if [[ -z "$FLEET_COLONY_BIN" ]]; then
  preflight_warn "colony CLI not found on PATH or in known locations — fleet panes will fall back to shell calls"
elif [[ ! -d "$FLEET_COLONY_HOME" ]]; then
  preflight_warn "COLONY_HOME missing: $FLEET_COLONY_HOME — disabling colony MCP in staged config"
elif ! "$FLEET_COLONY_BIN" --help >/dev/null 2>&1; then
  preflight_warn "colony CLI at $FLEET_COLONY_BIN failed --help probe — disabling colony MCP in staged config"
else
  FLEET_COLONY_ENABLED="true"
  preflight_log "colony MCP healthy: bin=$FLEET_COLONY_BIN home=$FLEET_COLONY_HOME timeout=${FLEET_COLONY_TIMEOUT_SEC}s"
fi

export FLEET_COLONY_BIN FLEET_COLONY_HOME FLEET_COLONY_ENABLED FLEET_COLONY_TIMEOUT_SEC FLEET_PATH

# --- render helper -------------------------------------------------------
# fleet_render_config <tmpl_path> <dst_path>
#   Materializes the fleet-config.toml.tmpl into <dst_path>, substituting
#   the FLEET_* env vars above. Caller is responsible for making sure
#   FLEET_COLONY_BIN is non-empty when FLEET_COLONY_ENABLED=true.
#
#   Tier wiring: the spawn loop sets `FLEET_REASONING_EFFORT` per pane
#   based on the account's `tier` field in accounts.yml:
#     tier=high   → FLEET_REASONING_EFFORT=xhigh   (default if unset)
#     tier=medium → FLEET_REASONING_EFFORT=medium
#     tier=low    → FLEET_REASONING_EFFORT=low
#   This substitutes __REASONING_EFFORT__ in the template. Codex locks
#   model_reasoning_effort at startup, so the tier MUST be decided
#   before render, not at task-claim time.
fleet_render_config() {
  local tmpl="$1" dst="$2"
  if [[ ! -f "$tmpl" ]]; then
    preflight_warn "fleet config template missing: $tmpl"
    return 1
  fi
  # Use a bash-only substitution loop instead of sed -e to avoid quoting
  # surprises with the PATH value (contains `/`).
  local content
  content="$(<"$tmpl")"
  content="${content//__COLONY_ENABLED__/$FLEET_COLONY_ENABLED}"
  content="${content//__COLONY_HOME__/$FLEET_COLONY_HOME}"
  content="${content//__COLONY_BIN__/${FLEET_COLONY_BIN:-colony}}"
  content="${content//__COLONY_TIMEOUT_SEC__/$FLEET_COLONY_TIMEOUT_SEC}"
  content="${content//__PATH__/$FLEET_PATH}"
  content="${content//__REASONING_EFFORT__/${FLEET_REASONING_EFFORT:-xhigh}}"
  printf '%s' "$content" > "$dst"
}
