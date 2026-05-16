#!/usr/bin/env bash
# Bring up the codex-fleet demo: synthetic plan + 8 fake worker panes + the
# real fleet-state / fleet-tab-strip / fleet-plan-tree / fleet-waves
# binaries rendering against the fake state. No real codex sessions, no API
# spend.
#
# Usage:
#   bash scripts/codex-fleet/demo/up.sh           # bring up + attach
#   bash scripts/codex-fleet/demo/up.sh --no-attach
#   bash scripts/codex-fleet/demo/up.sh --no-tick   # no auto-animation
#
# Teardown:
#   bash scripts/codex-fleet/demo/down.sh
set -euo pipefail

DEMO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$DEMO_DIR/../../.." && pwd)"
RUST_DIR="$REPO_ROOT/rust"
RELEASE_DIR="$RUST_DIR/target/release"
DEBUG_DIR="$RUST_DIR/target/debug"

SOCKET="${CODEX_FLEET_DEMO_SOCKET:-codex-fleet-demo}"
SESSION="codex-fleet-demo"
STATE_DIR="/tmp/claude-viz"
DEMO_TAG_FILE="$STATE_DIR/demo-active"
PLAN_SLUG="demo-refactor-wave-2026-05-16"
SCENARIO="${CODEX_FLEET_DEMO_SCENARIO:-refactor-wave}"
SCENARIO_DIR="$DEMO_DIR/scenarios/$SCENARIO"
PLAN_TEMPLATE="$SCENARIO_DIR/plan.json"
PLAN_RUNTIME="$REPO_ROOT/openspec/plans/$PLAN_SLUG/plan.json"
WORKERS=8

attach=1
tick=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-attach) attach=0 ;;
        --no-tick) tick=0 ;;
        -h|--help)
            sed -n '2,15p' "$0"
            exit 0
            ;;
        *) echo "unknown flag: $1" >&2; exit 64 ;;
    esac
    shift
done

# --- prerequisites -----------------------------------------------------

need() { command -v "$1" >/dev/null 2>&1 || { echo "demo: missing \`$1\`" >&2; exit 1; }; }
need tmux
need jq

resolve_bin() {
    local name="$1"
    if [[ -x "$RELEASE_DIR/$name" ]]; then echo "$RELEASE_DIR/$name"
    elif [[ -x "$DEBUG_DIR/$name" ]]; then echo "$DEBUG_DIR/$name"
    else return 1
    fi
}

for bin in fleet-state fleet-plan-tree fleet-waves; do
    if ! resolve_bin "$bin" >/dev/null; then
        echo "demo: missing $bin — run \`cargo build --release -p $bin\` from $RUST_DIR" >&2
        exit 1
    fi
done

FLEET_STATE_BIN="$(resolve_bin fleet-state)"
FLEET_PLAN_TREE_BIN="$(resolve_bin fleet-plan-tree)"
FLEET_WAVES_BIN="$(resolve_bin fleet-waves)"
FLEET_WATCHER_BIN="$(resolve_bin fleet-watcher || true)"

# --- state directory ---------------------------------------------------

mkdir -p "$STATE_DIR"
echo "$PLAN_SLUG" >"$STATE_DIR/plan-tree-pin.txt"
echo "$$" >"$DEMO_TAG_FILE"

# Copy the pristine plan template into openspec/plans/ so fleet-plan-tree
# and fleet-waves can discover it. The runtime copy will be mutated by
# tick.sh; down.sh removes it on teardown so the working tree stays clean.
if [[ ! -r "$PLAN_TEMPLATE" ]]; then
    echo "demo: scenario template missing: $PLAN_TEMPLATE" >&2
    exit 1
fi
mkdir -p "$(dirname "$PLAN_RUNTIME")"
cp "$PLAN_TEMPLATE" "$PLAN_RUNTIME"

write_counters() {
    local total ready in_prog blocked
    total=$(jq '.tasks | length' "$PLAN_RUNTIME")
    ready=$(jq '[.tasks[] | select(.status == "available")] | length' "$PLAN_RUNTIME")
    in_prog=$(jq '[.tasks[] | select(.status == "in_progress" or .status == "claimed")] | length' "$PLAN_RUNTIME")
    blocked=$((total - ready - in_prog))
    jq -n \
        --argjson overview "$WORKERS" \
        --argjson fleet "$WORKERS" \
        --argjson plan "$total" \
        --argjson waves "$in_prog" \
        --argjson review "$blocked" \
        --argjson ts "$(date +%s)" \
        '{overview:$overview, fleet:$fleet, plan:$plan, waves:$waves, review:$review, updated_at:$ts}' \
        > "$STATE_DIR/fleet-tab-counters.json"
}

write_quality_scores() {
    jq -n --argjson ts "$(date +%s)" '
    {
        generated_at: ($ts | tostring),
        scores: {
            "magnolia": {score:92, agent_id:"magnolia", pr_number:154, pr_title:"refactor(fleet-data): toposort", branch:"agent/refactor-toposort-extract", plan_slug:"demo-refactor-wave-2026-05-16", criteria_met:["tests pass","public api stable"], criteria_missed:[], reasoning:"Clean extraction, 60/60 tests.", scored_at:($ts|tostring)},
            "clover":   {score:88, agent_id:"clover",   pr_number:155, pr_title:"refactor(fleet-data): scrape",   branch:"agent/refactor-scrape-extract",   plan_slug:"demo-refactor-wave-2026-05-16", criteria_met:["tests pass"], criteria_missed:["docs missing"], reasoning:"Solid split.", scored_at:($ts|tostring)},
            "borage":   {score:74, agent_id:"borage",   pr_number:156, pr_title:"refactor(fleet-ui): tab_strip",  branch:"agent/refactor-tab-strip-split",  plan_slug:"demo-refactor-wave-2026-05-16", criteria_met:["render_pill extracted"], criteria_missed:["snapshot not updated"], reasoning:"Needs snapshot regen.", scored_at:($ts|tostring)}
        }
    }' > "$STATE_DIR/fleet-quality-scores.json"
}

write_counters
write_quality_scores

# Pick the "current" account so the agent-auth shim marks it with `*`.
echo "admin-magnolia@example.dev" > "$STATE_DIR/demo-current-account"

# --- tmux session ------------------------------------------------------

tmux -L "$SOCKET" kill-session -t "$SESSION" 2>/dev/null || true
tmux -L "$SOCKET" new-session -d -s "$SESSION" -n overview -x 220 -y 60

# Worker pane factory: 8 panes, each labeled @panel=[codex-<aid>].
# Aids match the agent-auth shim emails (admin-<aid>@…).
AIDS=(magnolia sumac yarrow clover thistle fennel mallow borage)

# We need a working dir on PATH that has the agent-auth shim FIRST so
# fleet-state's subprocess call resolves to our fake.
DEMO_PATH="$DEMO_DIR:$PATH"

# Worker pane content: a per-aid script that prints fake codex scrollback
# matching the shapes fleet-data::scrape and panes::classify expect, then
# tail -f's a per-aid scrollback file that tick.sh rewrites.
mkdir -p "$STATE_DIR/demo-panes"
for aid in "${AIDS[@]}"; do
    cat > "$STATE_DIR/demo-panes/$aid.txt" <<EOF
codex 0.42.0 — admin-${aid}@example.dev
Connected. Loading codex... Boot complete.

> demo-refactor-wave-2026-05-16 / subtask 0
  Extract render_pill helper from tab_strip

gpt-5.5 high
Working (0m 12s)
EOF
done

# Lay out the overview window as a 4x2 grid of 8 worker panes.
# (The standalone fleet-tab-strip binary was removed by PR #107; the tab
# strip now renders inline inside fleet-state / fleet-plan-tree / etc.
# via fleet_ui::tab_strip reading fleet-tab-counters.json.)
root_pane="$(tmux -L "$SOCKET" display-message -p -t "$SESSION:overview.0" '#{pane_id}')"

# Split: 1 vertical → 2 cols, then 3 horizontal splits per col → 8 panes.
tmux -L "$SOCKET" split-window -h -t "$root_pane" "sleep 86400"
right_col="$(tmux -L "$SOCKET" display-message -p -t "$SESSION:overview" '#{pane_id}')"

for col in "$root_pane" "$right_col"; do
    for _ in 1 2 3; do
        tmux -L "$SOCKET" split-window -v -t "$col" "sleep 86400"
    done
done
tmux -L "$SOCKET" select-layout -t "$SESSION:overview" tiled >/dev/null

mapfile -t worker_panes < <(tmux -L "$SOCKET" list-panes -t "$SESSION:overview" -F '#{pane_id}')

if [[ "${#worker_panes[@]}" -lt $WORKERS ]]; then
    echo "demo: only got ${#worker_panes[@]} worker panes, expected $WORKERS" >&2
    exit 1
fi

for i in "${!AIDS[@]}"; do
    aid="${AIDS[$i]}"
    pane="${worker_panes[$i]}"
    tmux -L "$SOCKET" set-option -p -t "$pane" '@panel' "[codex-$aid]" >/dev/null
    tmux -L "$SOCKET" respawn-pane -k -t "$pane" \
        "bash -c 'cat \"$STATE_DIR/demo-panes/$aid.txt\"; tail -F \"$STATE_DIR/demo-panes/$aid.txt\" 2>/dev/null'"
done

# Dashboard windows running the real binaries.
# Both env vars are set because fleet-plan-tree honors CODEX_FLEET_PLAN_REPO_ROOT
# (and FLEET_PLAN_REPO_ROOT as fallback) but fleet-waves only reads
# FLEET_PLAN_REPO_ROOT — so the unprefixed form is the common contract.
DASH_ENV="PATH='$DEMO_PATH' CODEX_FLEET_PLAN_REPO_ROOT='$REPO_ROOT' FLEET_PLAN_REPO_ROOT='$REPO_ROOT'"

tmux -L "$SOCKET" new-window -t "$SESSION:" -n fleet \
    "env PATH='$DEMO_PATH' '$FLEET_STATE_BIN'; sleep 86400"
tmux -L "$SOCKET" new-window -t "$SESSION:" -n plan \
    "env $DASH_ENV '$FLEET_PLAN_TREE_BIN'; sleep 86400"
tmux -L "$SOCKET" new-window -t "$SESSION:" -n waves \
    "env $DASH_ENV '$FLEET_WAVES_BIN'; sleep 86400"

if [[ -n "${FLEET_WATCHER_BIN:-}" ]]; then
    tmux -L "$SOCKET" new-window -t "$SESSION:" -n watcher \
        "env $DASH_ENV '$FLEET_WATCHER_BIN'; sleep 86400"
fi

tmux -L "$SOCKET" select-window -t "$SESSION:overview"

# --- tick simulator ----------------------------------------------------

if [[ "$tick" -eq 1 ]]; then
    nohup bash "$DEMO_DIR/tick.sh" >/tmp/claude-viz/demo-tick.log 2>&1 &
    echo "$!" > "$STATE_DIR/demo-tick.pid"
fi

echo "demo up. session=$SESSION socket=$SOCKET plan=$PLAN_SLUG"
echo "attach: tmux -L $SOCKET attach -t $SESSION"
echo "tear down: bash scripts/codex-fleet/demo/down.sh"

if [[ "$attach" -eq 1 ]]; then
    exec tmux -L "$SOCKET" attach -t "$SESSION"
fi
