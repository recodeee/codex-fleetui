#!/usr/bin/env bash
# inbox-dedup.sh — dedupe Colony attention_inbox JSONL output.
#
# Reads JSONL on stdin (one JSON object per line). Each object has at minimum:
#   task_id, kind, content, timestamp (ISO string or epoch), agent
# Groups near-duplicates by composite key:
#   (task_id, kind, sha1(normalized content))
# Normalization: lowercase, collapse runs of whitespace to single space,
# strip leading/trailing whitespace, then sha1.
# For each group, emits only the LATEST entry (largest timestamp) on stdout,
# preserving original JSON shape. Idempotent.
#
# Pure bash + jq + (sha1sum|openssl). No python.
#
# Self-test (run manually):
#   printf '%s\n' \
#     '{"task_id":"T1","kind":"note","content":"hello","timestamp":"2026-05-15T22:00:00Z","agent":"a"}' \
#     '{"task_id":"T1","kind":"note","content":"hello ","timestamp":"2026-05-15T22:01:00Z","agent":"a"}' \
#     '{"task_id":"T2","kind":"note","content":"other","timestamp":"2026-05-15T22:00:00Z","agent":"b"}' \
#     | bash scripts/codex-fleet/lib/inbox-dedup.sh | wc -l
#   # expected: 2
#
# Idempotency check:
#   <fixture> | bash inbox-dedup.sh | bash inbox-dedup.sh
#   # output identical to first run.

set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "inbox-dedup: jq is required" >&2
  exit 2
fi

# Pick a sha1 implementation.
_sha1() {
  if command -v sha1sum >/dev/null 2>&1; then
    sha1sum | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha1 | awk '{print $NF}'
  else
    echo "inbox-dedux: need sha1sum or openssl" >&2
    return 2
  fi
}

# Normalize content: lowercase, collapse whitespace, trim.
_normalize() {
  # Read all of stdin, lowercase, collapse whitespace runs to single space,
  # then trim leading/trailing whitespace.
  tr '[:upper:]' '[:lower:]' \
    | tr '\t\r\n\v\f' '     ' \
    | tr -s ' ' \
    | sed -e 's/^ //' -e 's/ $//'
}

# Convert a timestamp (ISO 8601 or numeric epoch) to a sortable numeric epoch.
# Bash arithmetic on epoch seconds is enough for "largest timestamp wins".
# Falls back to 0 if parsing fails.
_ts_to_epoch() {
  local raw="$1"
  if [[ "$raw" =~ ^-?[0-9]+(\.[0-9]+)?$ ]]; then
    # Already numeric.
    printf '%s' "$raw"
    return 0
  fi
  # Try GNU date.
  if date -u -d "$raw" +%s 2>/dev/null; then
    return 0
  fi
  # BSD date fallback.
  if date -u -j -f "%Y-%m-%dT%H:%M:%SZ" "$raw" +%s 2>/dev/null; then
    return 0
  fi
  printf '0'
}

# Stream stdin through a single jq pass that emits a TSV stream of:
#   <key>\t<epoch>\t<raw-line>
# where <key> = task_id|kind|sha1(normalized content) and content is
# normalized + sha1'd in shell because portable jq lacks sha1.
#
# We use a per-line loop because we need shell-side sha1sum. The loop is
# only as expensive as the inbox size, which is small.
_tag_stream() {
  # Read JSONL line by line. Use IFS= and -r to preserve content.
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    # Pull fields once via jq. Use -r and tab-separated so we can split.
    # Missing fields become empty strings.
    local fields
    if ! fields=$(jq -r '
      [
        (.task_id // ""),
        (.kind // ""),
        (.content // ""),
        (.timestamp // "")
      ] | @tsv
    ' <<<"$line" 2>/dev/null); then
      # Skip malformed lines silently; do not crash the pipeline.
      continue
    fi
    local task_id kind content ts_raw
    IFS=$'\t' read -r task_id kind content ts_raw <<<"$fields"

    local norm_hash
    norm_hash=$(printf '%s' "$content" | _normalize | _sha1)

    local epoch
    epoch=$(_ts_to_epoch "$ts_raw")

    # Compose the dedup key. Tabs are safe because task_id/kind don't contain
    # tabs in any realistic Colony payload; sanitize defensively.
    local safe_task_id safe_kind
    safe_task_id=${task_id//$'\t'/ }
    safe_kind=${kind//$'\t'/ }

    # Emit: key \t epoch \t original_line
    printf '%s|%s|%s\t%s\t%s\n' \
      "$safe_task_id" "$safe_kind" "$norm_hash" \
      "$epoch" \
      "$line"
  done
}

# Reduce: for each key, keep the row with the largest epoch. Stable ordering:
# when epochs tie, keep the last seen (which matches "latest wins" semantics).
_reduce_latest() {
  awk -F '\t' '
    {
      key=$1
      epoch=$2 + 0
      # Re-join the rest of the fields in case the JSON line contained tabs.
      line=$3
      for (i=4; i<=NF; i++) line=line "\t" $i

      if (!(key in best_epoch) || epoch >= best_epoch[key]) {
        best_epoch[key]=epoch
        best_line[key]=line
        if (!(key in order)) {
          order[key]=++n
        }
      }
    }
    END {
      # Emit in first-seen order for deterministic, idempotent output.
      for (k in order) {
        idx=order[k]
        keys[idx]=k
      }
      for (i=1; i<=n; i++) {
        print best_line[keys[i]]
      }
    }
  '
}

_tag_stream | _reduce_latest
