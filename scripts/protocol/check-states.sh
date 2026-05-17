#!/usr/bin/env bash
# check-states.sh — assert every improvement block in PROTOCOL.md has
# exactly one valid `- state: <STATE>` line. Implements the meta-protocol
# "formal lifecycle states" CI check.
#
# Exit codes:
#   0  every improvement has exactly one valid state
#   64 usage
#   70 violations

set -euo pipefail

PROTOCOL="${PROTOCOL_PATH:-docs/future/PROTOCOL.md}"

if [[ ! -f "$PROTOCOL" ]]; then
    echo "check-states: missing $PROTOCOL" >&2
    exit 78
fi

awk '
    BEGIN {
        valid["PROPOSED"] = 1;
        valid["ACCEPTED"] = 1;
        valid["SCHEDULED"] = 1;
        valid["IN-PROGRESS"] = 1;
        valid["SHIPPED"] = 1;
        valid["DEFERRED"] = 1;
        valid["REJECTED"] = 1;
        violations = 0;
    }
    /^#### [0-9]+\.4\.[0-9]+ / {
        if (title != "") {
            if (state_count != 1) {
                printf "bad state count (%d) for: %s\n", state_count, title;
                violations++;
            } else if (!(state in valid)) {
                printf "invalid state (%s) for: %s\n", state, title;
                violations++;
            }
        }
        title = $0;
        state = "";
        state_count = 0;
    }
    /^- state: / {
        if (title != "") {
            state_count++;
            state = $0;
            sub(/^- state: /, "", state);
        }
    }
    END {
        if (title != "") {
            if (state_count != 1) {
                printf "bad state count (%d) for: %s\n", state_count, title;
                violations++;
            } else if (!(state in valid)) {
                printf "invalid state (%s) for: %s\n", state, title;
                violations++;
            }
        }
        if (violations > 0) {
            printf "check-states: %d violation(s)\n", violations;
            exit 70;
        }
        printf "check-states: ok\n";
    }
' "$PROTOCOL"
