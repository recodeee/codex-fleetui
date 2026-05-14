#!/usr/bin/env bash
#
# codex-fleet-2 — open a second, read-only fleet dashboard in a kitty window.
#
# Lets you compare a candidate plan/fleet/waves rendering against the live
# `codex-fleet` session without touching it. Spawns a sibling tmux session
# `codex-fleet-2` whose tabs run only the visualization scripts (no codex
# workers, no force-claim, no cap-swap). Safe to start while the live fleet
# is running.
#
# Usage:
#   bash scripts/codex-fleet/codex-fleet-2.sh                    # auto-pick newest plan
#   bash scripts/codex-fleet/codex-fleet-2.sh --plan-slug <slug> # pin a specific plan
#   bash scripts/codex-fleet/codex-fleet-2.sh --no-kitty         # tmux-only (current terminal)
#   bash scripts/codex-fleet/codex-fleet-2.sh --kill             # close the session + kitty window
#
# All windows here use the SAME on-disk scripts as the live fleet, so editing
# plan-tree-anim.sh / waves-anim-generic.sh hot-reloads on the next respawn.

set -euo pipefail

# Route every `tmux ...` call through scripts/codex-fleet/lib/_tmux.sh — when
# CODEX_FLEET_TMUX_SOCKET is set in the env (e.g. by full-bringup.sh), this
# transparently rewrites the call to `tmux -L $SOCKET ...`. Default behavior
# (env unset) is identical to the prior `tmux` binary call.
source "$(dirname "${BASH_SOURCE[0]}")/lib/_tmux.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"
SESSION="${CODEX_FLEET_2_SESSION:-codex-fleet-2}"
PLAN_SLUG=""
USE_KITTY=1
KILL=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --plan-slug) PLAN_SLUG="$2"; shift 2 ;;
    --session)   SESSION="$2";   shift 2 ;;
    --no-kitty)  USE_KITTY=0;    shift   ;;
    --kill)      KILL=1;         shift   ;;
    -h|--help)
      sed -n '2,17p' "$0"; exit 0 ;;
    *) echo "fatal: unknown arg: $1" >&2; exit 2 ;;
  esac
done

# --kill: tear down the session and any kitty window that hosts it.
if (( KILL == 1 )); then
  tmux kill-session -t "$SESSION" 2>/dev/null || true
  pkill -f "kitty --title $SESSION" 2>/dev/null || true
  echo "[codex-fleet-2] killed session=$SESSION + kitty window"
  exit 0
fi

# Newest non-empty plan if not pinned (same heuristic as full-bringup.sh).
if [[ -z "$PLAN_SLUG" ]]; then
  PLAN_SLUG="$(python3 - "$REPO" <<'PY'
import os, sys, re, glob, json
repo = sys.argv[1]
plans = glob.glob(f"{repo}/openspec/plans/*/plan.json")
def key(p):
    s = os.path.basename(os.path.dirname(p))
    m = re.search(r'(\d{4})-(\d{2})-(\d{2})$', s)
    d = (int(m[1]), int(m[2]), int(m[3])) if m else (0, 0, 0)
    return (d, os.path.getmtime(p))
plans.sort(key=key, reverse=True)
for p in plans:
    try:
        with open(p) as fh:
            if json.load(fh).get("tasks"):
                print(os.path.basename(os.path.dirname(p))); sys.exit(0)
    except Exception:
        continue
print("")
PY
)"
fi
if [[ -z "$PLAN_SLUG" ]] || [[ ! -f "$REPO/openspec/plans/$PLAN_SLUG/plan.json" ]]; then
  echo "fatal: no plan found at $REPO/openspec/plans/$PLAN_SLUG/plan.json" >&2
  echo "       pass --plan-slug <slug> or populate openspec/plans/." >&2
  exit 2
fi

# Pin the plan for plan-tree-anim so a second running instance doesn't pick a
# different newest-plan during the window's lifetime.
PIN_DIR="${PLAN_TREE_ANIM_PIN_DIR:-/tmp/claude-viz}"
PIN_FILE="$PIN_DIR/plan-tree-pin-2.txt"
mkdir -p "$PIN_DIR"
printf '%s\n' "$REPO/openspec/plans/$PLAN_SLUG/plan.json" > "$PIN_FILE"

# Tear down any stale session first so we always start from a clean canvas.
tmux kill-session -t "$SESSION" 2>/dev/null || true

echo "[codex-fleet-2] session=$SESSION plan=$PLAN_SLUG"

# Build the session detached so kitty can attach to a stable target.
#
# Script resolution prefers the launcher's own checkout ($REPO) so an
# in-worktree edit shows up immediately, then falls back to the primary
# checkout so dashboards that live only in user-untracked locations
# (waves-anim-generic.sh, review-board.sh, ...) still work.
FALLBACK_REPO="${CODEX_FLEET_2_FALLBACK_REPO:-${CODEX_FLEET_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}}"
resolve_script() {
  local name="$1"
  if [[ -f "$SCRIPT_DIR/$name" ]]; then
    printf '%s\n' "$SCRIPT_DIR/$name"
  elif [[ -f "$FALLBACK_REPO/scripts/codex-fleet/$name" ]]; then
    printf '%s\n' "$FALLBACK_REPO/scripts/codex-fleet/$name"
  fi
}

add_window() {
  local win="$1" script="$2" env_prefix="${3:-}"
  local path
  path="$(resolve_script "$script")"
  if [[ -n "$path" ]]; then
    tmux new-window -d -t "$SESSION:" -n "$win" \
      "${env_prefix}bash $path"
  else
    tmux new-window -d -t "$SESSION:" -n "$win" \
      "printf '[codex-fleet-2] missing: scripts/codex-fleet/%s\n' '$script'"
    tmux set-option -w -t "$SESSION:$win" remain-on-exit on
  fi
}

PLAN_SCRIPT="$(resolve_script plan-tree-anim.sh)"
[[ -z "$PLAN_SCRIPT" ]] && { echo "fatal: plan-tree-anim.sh missing in $REPO and $FALLBACK_REPO" >&2; exit 2; }
tmux new-session -d -s "$SESSION" -n plan \
  "PLAN_TREE_ANIM_PIN_FILE=$PIN_FILE bash $PLAN_SCRIPT"
add_window fleet   fleet-state-anim.sh
# Prefer waves-anim-generic.sh (newer) when present; fall back to the
# committed waves-anim.sh otherwise.
if [[ -n "$(resolve_script waves-anim-generic.sh)" ]]; then
  add_window waves waves-anim-generic.sh "FLEET_WAVES_PLAN_SLUG=$PLAN_SLUG "
else
  add_window waves waves-anim.sh         "FLEET_WAVES_PLAN_SLUG=$PLAN_SLUG "
fi
# Prefer review-anim.sh (committed sibling of plan-anim / waves-anim) when
# present; fall back to the historical review-board.sh placeholder otherwise.
if [[ -n "$(resolve_script review-anim.sh)" ]]; then
  add_window review review-anim.sh
else
  add_window review review-board.sh
fi
add_window watcher watcher-board.sh

tmux set-option -w -t "$SESSION:plan"  remain-on-exit on
tmux set-option -w -t "$SESSION:waves" remain-on-exit on
tmux select-window -t "$SESSION:plan"

# Apply the canonical iOS-palette tab chrome (style-tabs.sh) — the second
# session no longer ships a custom orange-tinted status. Per-session distinction
# now comes from the `#S` token rendered inside style-tabs.sh's session badge
# pill (e.g. `◖ ◆ codex-fleet-2 ◗`), so both sessions read as one fleet visual
# language and pre-iOS overrides don't shadow the global palette.
if [[ -x "$SCRIPT_DIR/style-tabs.sh" ]]; then
  CODEX_FLEET_SESSION="$SESSION" bash "$SCRIPT_DIR/style-tabs.sh" >/dev/null 2>&1 || true
fi

if (( USE_KITTY == 1 )) && command -v kitty >/dev/null 2>&1; then
  # Detached kitty window so this script returns immediately.
  setsid kitty --title "$SESSION" \
    bash -lc "tmux attach -t '$SESSION'" \
    >/dev/null 2>&1 &
  disown 2>/dev/null || true
  echo "[codex-fleet-2] opened kitty window (--title $SESSION)"
  echo "[codex-fleet-2] tear down: bash scripts/codex-fleet/codex-fleet-2.sh --kill"
else
  echo "[codex-fleet-2] tmux-only mode — attach manually:"
  echo "    tmux attach -t $SESSION"
fi
