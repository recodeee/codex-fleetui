#!/usr/bin/env bash
# check-budget.sh — warn when any PROTOCOL.md section exceeds 1.5x its
# declared budget. Implements the meta-protocol "anti-bikeshed: budget
# per section" improvement.
#
# Sections start with `## N. Title` and continue until the next `## ` at
# the top level (after the TOC). The first ~80 lines of preamble/TOC are
# skipped.
#
# Exit codes:
#   0  every section under 1.5x budget
#   65 at least one section over budget (warning, soft fail)
#
# Usage:
#   scripts/protocol/check-budget.sh [--strict]
#
#   --strict   treat over-budget sections as a hard error (exit 70).

set -euo pipefail

PROTOCOL="${PROTOCOL_PATH:-docs/future/PROTOCOL.md}"
BUDGET="${PROTOCOL_BUDGET:-300}"
OVER_FACTOR="${PROTOCOL_BUDGET_FACTOR:-1.5}"
STRICT=0
if [[ "${1:-}" == "--strict" ]]; then
    STRICT=1
fi

if [[ ! -f "$PROTOCOL" ]]; then
    echo "check-budget: missing $PROTOCOL" >&2
    exit 78
fi

awk -v budget="$BUDGET" -v factor="$OVER_FACTOR" '
    function flush(   over_limit) {
        if (current == "") return;
        over_limit = budget * factor;
        if (line_count > over_limit) {
            printf "OVER %d %d %s\n", line_count, over_limit, current;
            over_count++;
        }
    }
    /^## [0-9]+\. / {
        flush();
        current = $0;
        line_count = 0;
        next;
    }
    /^## / && current != "" {
        # Non-numbered top-level heading closes the current section.
        flush();
        current = "";
        line_count = 0;
        next;
    }
    current != "" { line_count++ }
    END {
        flush();
        printf "TOTAL %d\n", over_count + 0;
    }
' "$PROTOCOL" > /tmp/protocol_budget.$$

over_count=$(awk '/^TOTAL / { print $2 }' /tmp/protocol_budget.$$)
while IFS= read -r line; do
    # Format: OVER <lines> <limit> <heading>
    read -r _ lines limit rest <<<"$line"
    printf "over budget: %s (%s lines > %s)\n" "$rest" "$lines" "$limit" >&2
done < <(grep '^OVER ' /tmp/protocol_budget.$$ || true)
rm -f /tmp/protocol_budget.$$

if (( over_count > 0 )); then
    if (( STRICT == 1 )); then
        exit 70
    fi
    echo "check-budget: $over_count section(s) over budget (warning)" >&2
    exit 65
fi

echo "check-budget: all sections under ${OVER_FACTOR}x budget of ${BUDGET} lines."
exit 0
