#!/usr/bin/env bash
# discover-accounts.sh — list every authenticated codex account on disk.
#
# The fleet's account pool was historically declared by hand in
# scripts/codex-fleet/accounts.yml — 4 base entries. Tonight we discovered
# the host had 18+ authenticated codex CLI homes under /tmp/codex-fleet/
# but the spawner couldn't see them because they weren't in accounts.yml.
# This helper closes that gap: it walks /tmp/codex-fleet/*/auth.json and
# emits one `id\temail` line per discoverable account by decoding the
# OAuth id_token's email claim.
#
# Output (TSV, one row per authenticated account, alphabetical by id):
#   admin-magnolia\tadmin@magnoliavilag.hu
#   bia-zazrifka\tbia@zazrifka.sk
#   ...
#
# Usage:
#   bash scripts/codex-fleet/lib/discover-accounts.sh                 # list every account
#   bash scripts/codex-fleet/lib/discover-accounts.sh --exclude-active # skip accounts already in $ACTIVE_FILE
#   ACTIVE_FILE=/path bash …/discover-accounts.sh --exclude-active
#
# Env:
#   CODEX_HOMES_DIR  (default `/tmp/codex-fleet`)
#   ACTIVE_FILE      (default `/tmp/claude-viz/fleet-active-accounts.txt`)
set -eo pipefail

CODEX_HOMES_DIR="${CODEX_HOMES_DIR:-/tmp/codex-fleet}"
ACTIVE_FILE="${ACTIVE_FILE:-/tmp/claude-viz/fleet-active-accounts.txt}"
EXCLUDE_ACTIVE=0
EXCLUDE_TMUX=""    # session name to introspect for live `codex-<aid>` panel labels

while [ $# -gt 0 ]; do
  case "$1" in
    --exclude-active) EXCLUDE_ACTIVE=1; shift ;;
    --exclude-tmux) EXCLUDE_TMUX="$2"; shift 2 ;;
    --homes-dir) CODEX_HOMES_DIR="$2"; shift 2 ;;
    --active-file) ACTIVE_FILE="$2"; shift 2 ;;
    -h|--help) sed -n '1,28p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Build a synthetic "active set" from live tmux @panel labels too. ACTIVE_FILE
# can lag (it's only refreshed when add-workers actually spawns); the tmux
# read is authoritative for "is this account already running in a pane right
# now".
ACTIVE_TMUX_AIDS=""
if [ -n "$EXCLUDE_TMUX" ] && command -v tmux >/dev/null 2>&1; then
  ACTIVE_TMUX_AIDS="$(tmux list-panes -s -t "$EXCLUDE_TMUX" -F '#{@panel}' 2>/dev/null \
    | sed -nE 's/^\[codex-(.+)\]$/\1/p' | sort -u | tr '\n' ',' | sed 's/,$//')"
fi
export ACTIVE_TMUX_AIDS

if [ ! -d "$CODEX_HOMES_DIR" ]; then
  exit 0   # nothing to discover; emit nothing
fi

EXCLUDE_ACTIVE="$EXCLUDE_ACTIVE" \
ACTIVE_FILE="$ACTIVE_FILE" \
CODEX_HOMES_DIR="$CODEX_HOMES_DIR" \
python3 - <<'PY'
import base64, glob, json, os, sys

homes_dir = os.environ["CODEX_HOMES_DIR"]
active_file = os.environ["ACTIVE_FILE"]
exclude_active = os.environ["EXCLUDE_ACTIVE"] == "1"

active = set()
if exclude_active and os.path.exists(active_file):
    with open(active_file) as fh:
        active = {line.strip() for line in fh if line.strip()}

# Live tmux check: add any account currently labeled `[codex-<aid>]` on a
# pane in the target session. Comma-separated, populated by the bash side
# when --exclude-tmux <session> is passed. This filter is independent of
# --exclude-active so the caller can rely on a live tmux view alone.
tmux_active = os.environ.get("ACTIVE_TMUX_AIDS", "")
if tmux_active:
    active |= {aid for aid in tmux_active.split(",") if aid}
# Either filter source being populated should activate the exclusion path,
# regardless of how the caller flagged it.
filter_active = bool(active)

def jwt_email(token: str) -> str:
    """Decode the OAuth id_token payload and pull out the email claim.

    Codex CLI auth.json stores the chatgpt OAuth tokens; the id_token is a
    standard JWT whose payload includes `email`. We don't verify the
    signature here — we're not authenticating, just naming.
    """
    if not token:
        return ""
    try:
        parts = token.split(".")
        if len(parts) < 2:
            return ""
        # base64url decode the payload, padding as needed.
        payload = parts[1] + "=" * (-len(parts[1]) % 4)
        decoded = base64.urlsafe_b64decode(payload)
        return json.loads(decoded).get("email", "") or ""
    except Exception:
        return ""

rows = []
for auth_path in sorted(glob.glob(os.path.join(homes_dir, "*", "auth.json"))):
    aid = os.path.basename(os.path.dirname(auth_path))
    # Skip known non-account dirs (`wake-prompts`, etc. don't have auth.json
    # anyway — this is defensive).
    if aid in {"wake-prompts"}:
        continue
    if filter_active and aid in active:
        continue
    try:
        with open(auth_path) as fh:
            data = json.load(fh)
    except Exception:
        continue
    token = (data.get("tokens") or {}).get("id_token", "")
    email = jwt_email(token)
    if not email:
        # Auth dir exists but the id_token doesn't decode — likely a stale
        # or corrupted login. Skip rather than emit half-data.
        continue
    rows.append((aid, email))

for aid, email in rows:
    print(f"{aid}\t{email}")
PY
