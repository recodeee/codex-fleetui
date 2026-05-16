# shellcheck shell=bash
# MUST stay in sync with rust/fleet-data/src/fleet.rs::derive_agent_id
# (around lines 105-118). Any change to the domain-stem alias map or the
# email parsing semantics here must be mirrored there.

if [[ -n "${__CODEX_FLEET_LIB_AGENTS_SH:-}" ]]; then
  return 0 2>/dev/null || exit 0
fi
__CODEX_FLEET_LIB_AGENTS_SH=1

derive_aid() {
  local email="${1:-}" part dom
  part="${email%%@*}"
  dom="${email#*@}"
  dom="${dom%%.*}"
  case "$dom" in
    magnoliavilag) dom=magnolia ;;
    gitguardex)    dom=gg ;;
    pipacsclub)    dom=pipacs ;;
  esac
  printf '%s-%s' "$part" "$dom"
}

email_to_id() {
  derive_aid "$@"
}
