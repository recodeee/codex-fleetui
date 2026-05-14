#!/usr/bin/env bash
# patch-codex-prompts — replace codex CLI's hardcoded idle-prompt
# suggestions with fleet-relevant text, in-place on the native binary.
#
# Why: codex 0.130 bakes the suggestion strip ("Write tests for @filename",
# "Implement {feature}", ...) into tui/src/chatwidget.rs. There is no
# config knob. Operators staring at the codex-fleet overview tab see eight
# panes of those generic prompts and can't tell at a glance what the fleet
# is supposed to be doing instead. This patches the byte ranges with
# fleet-relevant equivalents (Colony / sub-task / plan-tree vocabulary)
# preserving exact byte lengths so the binary stays valid.
#
# Behavior:
#   - Resolves the codex native binary under ~/.nvm/.../@openai/codex-linux-x64.
#   - Backs the binary up to <bin>.pre-fleet-patch-<unix-ts>.
#   - Applies the byte-level replacements (Python; no `sed -i` on binaries).
#   - Verifies `codex --version` exits 0 against the patched binary.
#   - Logs to /tmp/claude-viz/codex-fleet-prompt-patch.log.
#
# Idempotent: if all original strings are already absent (binary previously
# patched), the script exits 0 without writing.
#
# Re-application: codex npm upgrades replace the native binary, reverting
# the patch. Re-run this script after every `npm i -g @openai/codex@latest`.
#
# Usage:
#   bash scripts/codex-fleet/patch-codex-prompts.sh
#   bash scripts/codex-fleet/patch-codex-prompts.sh --binary /path/to/codex
#   bash scripts/codex-fleet/patch-codex-prompts.sh --restore  # roll back to latest backup
set -euo pipefail

LOG="${PATCH_LOG:-/tmp/claude-viz/codex-fleet-prompt-patch.log}"
mkdir -p "$(dirname "$LOG")"
log() { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$*" | tee -a "$LOG"; }

BIN=""
MODE="patch"
for arg in "$@"; do
  case "$arg" in
    --binary=*) BIN="${arg#--binary=}" ;;
    --binary)   shift; BIN="${1:-}" ;;
    --restore)  MODE="restore" ;;
    -h|--help)
      grep -E '^# ' "$0" | sed 's/^# \?//'
      exit 0 ;;
  esac
done

if [[ -z "$BIN" ]]; then
  # Prefer the active node version's codex install; fall back to newest.
  for cand in \
    "$HOME/.nvm/versions/node/v22.22.0/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/codex/codex" \
    "$HOME/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/codex/codex"; do
    if [[ -f "$cand" ]]; then BIN="$cand"; break; fi
  done
fi

if [[ ! -f "$BIN" ]]; then
  log "FATAL: codex binary not found. Pass --binary <path>."
  exit 1
fi

if [[ "$MODE" == "restore" ]]; then
  latest_backup=$(ls -1t "$BIN".pre-fleet-patch-* 2>/dev/null | head -1 || true)
  if [[ -z "$latest_backup" ]]; then
    log "FATAL: no backup found alongside $BIN"
    exit 1
  fi
  cp -p "$latest_backup" "$BIN"
  log "Restored $BIN from $latest_backup"
  exit 0
fi

log "Target binary: $BIN ($(stat -c '%s bytes, mtime %y' "$BIN"))"

# Replacement table — each entry must be ORIGINAL|REPLACEMENT with EXACTLY
# the same byte length. Trailing spaces in the replacement are intentional
# padding; codex strips them before rendering anyway.
PAIRS=(
  'Explain this codebase|Show plan tree status'
  'Summarize recent commits|Show Colony attention   '
  'Implement {feature}|Claim next sub-task'
  'Find and fix a bug in @filename|Check attention_inbox handoffs '
  'Write tests for @filename|Post evidence to Colony  '
  'Improve documentation in @filename|Match plan touches_files exactly  '
  'Run /review on my current changes|Run task_plan_complete_subtask   '
  'Use /skills to list available skills|Use task_ready_for_agent now        '
)

# Length validation up front — refuse to corrupt the binary if any pair drifts.
for pair in "${PAIRS[@]}"; do
  old="${pair%%|*}"
  new="${pair##*|}"
  if [[ "${#old}" -ne "${#new}" ]]; then
    log "FATAL: replacement length mismatch: '$old' (${#old}) -> '$new' (${#new})"
    exit 1
  fi
done

# Idempotency probe: are any originals still in the binary?
need_patch=0
for pair in "${PAIRS[@]}"; do
  old="${pair%%|*}"
  if grep -aqF "$old" "$BIN"; then need_patch=1; break; fi
done
if (( need_patch == 0 )); then
  log "All originals absent — binary already patched. No-op."
  exit 0
fi

backup="$BIN.pre-fleet-patch-$(date +%s)"
cp -p "$BIN" "$backup"
log "Backup written to $backup"

python3 - "$BIN" <<'PY' "${PAIRS[@]}"
import sys, pathlib
bin_path = pathlib.Path(sys.argv[1])
data = bytearray(bin_path.read_bytes())
replaced = 0
for pair in sys.argv[2:]:
    old, new = pair.split('|', 1)
    old_b, new_b = old.encode(), new.encode()
    assert len(old_b) == len(new_b), f"length mismatch for {old!r}"
    idx = 0
    pair_hits = 0
    while True:
        found = data.find(old_b, idx)
        if found == -1:
            break
        data[found:found+len(old_b)] = new_b
        idx = found + len(new_b)
        pair_hits += 1
    print(f"  {pair_hits} × {old!r} -> {new!r}")
    replaced += pair_hits
bin_path.write_bytes(bytes(data))
print(f"TOTAL replacements: {replaced}")
PY

log "Patch applied; verifying binary still runs..."
if "$BIN" --version >>"$LOG" 2>&1; then
  log "OK: $(${BIN} --version 2>&1 | head -1) — patched binary launches cleanly"
else
  log "FATAL: patched binary failed --version. Rolling back."
  cp -p "$backup" "$BIN"
  log "Restored from $backup. Investigate before re-running."
  exit 2
fi

log "Done. Run codex on a fresh tmux pane to see the new prompts."
