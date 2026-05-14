#!/usr/bin/env bash
# plan-tree-pin — pin the plan tab to a specific openspec plan.
#
# plan-tree-anim.sh reads $PLAN_TREE_ANIM_PIN_FILE (default
# /tmp/claude-viz/plan-tree-pin.txt) at startup. This helper writes the
# path of a plan.json into that file, so respawns of the plan pane
# (after tmux kill-server / reboot) survive your selection.
#
# Usage:
#   bash scripts/codex-fleet/plan-tree-pin.sh                   # list plans
#   bash scripts/codex-fleet/plan-tree-pin.sh <plan-slug>       # pin it
#   bash scripts/codex-fleet/plan-tree-pin.sh --clear           # remove pin
#   bash scripts/codex-fleet/plan-tree-pin.sh --show            # print current pin
set -eo pipefail

REPO="${PLAN_TREE_PIN_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}}"
PIN_FILE="${PLAN_TREE_ANIM_PIN_FILE:-/tmp/claude-viz/plan-tree-pin.txt}"
mkdir -p "$(dirname "$PIN_FILE")"

list_plans() {
  local p
  for p in "$REPO"/openspec/plans/*/plan.json; do
    [[ -f "$p" ]] || continue
    local slug n
    slug=$(basename "$(dirname "$p")")
    n=$(jq '.tasks | length' "$p" 2>/dev/null || echo 0)
    printf '  %s  (%d task%s)\n' "$slug" "$n" "$([ "$n" = "1" ] && echo "" || echo "s")"
  done
}

case "${1:-}" in
  ""|--help|-h)
    echo "plan-tree-pin — pin the plan tab to a specific openspec plan."
    echo
    echo "Usage:"
    echo "  $0                # list plans"
    echo "  $0 <plan-slug>    # pin"
    echo "  $0 --clear        # clear pin (back to auto-pick)"
    echo "  $0 --show         # print current pin"
    echo
    echo "Available plans:"
    list_plans
    ;;
  --clear)
    rm -f "$PIN_FILE"
    echo "plan-tree pin cleared. plan-tree-anim will auto-pick the newest non-empty plan."
    ;;
  --show)
    if [[ -f "$PIN_FILE" ]]; then
      cat "$PIN_FILE"
    else
      echo "(no pin set — auto-picks newest non-empty plan)"
    fi
    ;;
  *)
    slug="$1"
    target="$REPO/openspec/plans/$slug/plan.json"
    if [[ ! -f "$target" ]]; then
      echo "fatal: plan not found: $target" >&2
      echo
      echo "Available plans:" >&2
      list_plans >&2
      exit 2
    fi
    printf '%s\n' "$target" > "$PIN_FILE"
    echo "pinned plan-tree to: $slug"
    echo "($PIN_FILE → $target)"
    ;;
esac
