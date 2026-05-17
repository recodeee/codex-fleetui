#!/usr/bin/env bash
# protocol-state.sh — summarise lifecycle states across docs/future/PROTOCOL.md.
#
# Scans every "- state: <STATE>" line under improvement headings and prints
# a count per state. Exits 0 even when counts are skewed; callers that want
# enforcement should pipe through a grep.
#
# Usage:
#   scripts/protocol/protocol-state.sh [--summary|--table|--list <STATE>]
#
# Modes:
#   --summary  (default) one-line counts per state
#   --table    a markdown table suitable for paste-back
#   --list S   list every improvement title whose state == S
#
# Implements the meta-protocol "formal lifecycle states" improvement.

set -euo pipefail

PROTOCOL="${PROTOCOL_PATH:-docs/future/PROTOCOL.md}"
MODE="${1:---summary}"

if [[ ! -f "$PROTOCOL" ]]; then
    echo "protocol-state: missing $PROTOCOL" >&2
    exit 78
fi

STATES=(PROPOSED ACCEPTED SCHEDULED IN-PROGRESS SHIPPED DEFERRED REJECTED)

count_state() {
    local state="$1"
    grep -cE "^- state: ${state}\$" "$PROTOCOL" || true
}

case "$MODE" in
    --summary)
        printf "protocol-state %s\n" "$PROTOCOL"
        for s in "${STATES[@]}"; do
            printf "  %-12s %s\n" "$s" "$(count_state "$s")"
        done
        ;;
    --table)
        printf "| State | Count |\n|-------|-------|\n"
        for s in "${STATES[@]}"; do
            printf "| %s | %s |\n" "$s" "$(count_state "$s")"
        done
        ;;
    --list)
        target="${2:-}"
        if [[ -z "$target" ]]; then
            echo "protocol-state: --list requires a STATE argument" >&2
            exit 64
        fi
        awk -v want="$target" '
            /^#### / { title = $0 }
            $0 == "- state: " want { print title }
        ' "$PROTOCOL"
        ;;
    -h|--help)
        sed -n '2,16p' "$0"
        ;;
    *)
        echo "protocol-state: unknown mode '$MODE'" >&2
        exit 64
        ;;
esac
