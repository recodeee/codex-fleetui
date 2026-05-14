#!/usr/bin/env bash
# codex-fleet installer.
#
# What this does (idempotent — safe to re-run):
#   1. Symlink ~/.claude/skills/codex-fleet -> <this-clone>/skills/codex-fleet
#      so Claude Code recognizes the orchestrator skill from any project.
#   2. Seed scripts/codex-fleet/accounts.yml from the .example.yml if missing.
#   3. Print the env additions you should drop in ~/.bashrc (or your shell rc).
#
# Usage:
#   cd <clone>
#   bash install.sh
#
# Override the skill symlink target with CLAUDE_SKILLS_DIR=/path/to/skills.

set -eo pipefail

CLONE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_SRC="$CLONE_ROOT/skills/codex-fleet"
SKILL_DST_DIR="${CLAUDE_SKILLS_DIR:-$HOME/.claude/skills}"
SKILL_DST="$SKILL_DST_DIR/codex-fleet"
ACC_EXAMPLE="$CLONE_ROOT/scripts/codex-fleet/accounts.example.yml"
ACC_LIVE="$CLONE_ROOT/scripts/codex-fleet/accounts.yml"

echo "==> codex-fleet install ($CLONE_ROOT)"

# 1. skill symlink
if [ ! -d "$SKILL_SRC" ]; then
  echo "warn: $SKILL_SRC missing (clone incomplete?); skipping skill link" >&2
else
  mkdir -p "$SKILL_DST_DIR"
  if [ -e "$SKILL_DST" ] && [ ! -L "$SKILL_DST" ]; then
    backup="$SKILL_DST.bak.$(date +%Y%m%d-%H%M%S)"
    echo "==> backing up existing $SKILL_DST to $backup"
    mv "$SKILL_DST" "$backup"
  fi
  ln -sfn "$SKILL_SRC" "$SKILL_DST"
  echo "==> symlinked $SKILL_DST -> $SKILL_SRC"
fi

# 2. accounts.yml seed
if [ ! -f "$ACC_LIVE" ]; then
  if [ -f "$ACC_EXAMPLE" ]; then
    cp "$ACC_EXAMPLE" "$ACC_LIVE"
    echo "==> seeded $ACC_LIVE from example"
    echo "    next: \$EDITOR $ACC_LIVE  # replace example.com placeholders"
  else
    echo "warn: $ACC_EXAMPLE missing; cannot seed accounts.yml" >&2
  fi
else
  echo "==> $ACC_LIVE already exists; not overwriting"
fi

# 3. shell env hint
cat <<HINT

==> add these lines to ~/.bashrc (or your shell rc) so any project can reach
    the fleet scripts and so plan paths resolve against your real project:

    export CODEX_FLEET_REPO_ROOT="$CLONE_ROOT"
    export PATH="\$CODEX_FLEET_REPO_ROOT/scripts/codex-fleet:\$PATH"

    # Optional: point the fleet at a different repo's openspec/plans/ dir:
    # export CODEX_FLEET_PLAN_DIR=\$HOME/Documents/my-project/openspec/plans

==> deps check (warn-only):

HINT

for bin in tmux kitty git python3 colony codex codex-auth inotifywait jq; do
  if command -v "$bin" >/dev/null 2>&1; then
    printf '    [ok]   %s -> %s\n' "$bin" "$(command -v "$bin")"
  else
    printf '    [warn] %s NOT FOUND on PATH\n' "$bin"
  fi
done

echo
echo "==> done. bring up the fleet with:"
echo "    bash $CLONE_ROOT/scripts/codex-fleet/full-bringup.sh --n 8"
