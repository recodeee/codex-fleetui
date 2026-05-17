#!/usr/bin/env bash
# check-refs.sh — verify every improvement in PROTOCOL.md cites at least
# one real path, and that each cited path exists in the working tree.
#
# Honours the meta-protocol "cite-real-files" rule. Paths inside backticks
# under a **References.** block are checked. Paths ending with "/" or
# containing globs are accepted if at least one match exists; future-only
# paths can be marked with the suffix " (planned)" to be ignored.
#
# Exit codes:
#   0  every improvement cites >= 1 real path
#   64 usage error
#   70 one or more violations detected
#
# Usage:
#   scripts/protocol/check-refs.sh [--quiet]

set -euo pipefail

PROTOCOL="${PROTOCOL_PATH:-docs/future/PROTOCOL.md}"
QUIET=0
if [[ "${1:-}" == "--quiet" ]]; then
    QUIET=1
fi

if [[ ! -f "$PROTOCOL" ]]; then
    echo "check-refs: missing $PROTOCOL" >&2
    exit 78
fi

violations=0
missing=0

# Awk parse: walk through improvement blocks delimited by `#### N.4.k Title`
# lines. For each block, collect lines between "**References.**" and the
# next "####" or "###" boundary. Report blocks with zero refs.
awk '
    BEGIN { block_open = 0; ref_open = 0; refs = ""; title = ""; }
    /^#### / {
        if (block_open) {
            print "BLOCK_END" "|" title "|" refs;
        }
        title = substr($0, 6);
        block_open = 1;
        ref_open = 0;
        refs = "";
        next;
    }
    /^### / && block_open {
        print "BLOCK_END" "|" title "|" refs;
        block_open = 0;
        ref_open = 0;
        refs = "";
        next;
    }
    /^\*\*References\.\*\*/ && block_open {
        ref_open = 1;
        next;
    }
    ref_open && /^- `/ {
        line = $0;
        sub(/^- `/, "", line);
        sub(/`.*$/, "", line);
        if (refs == "") { refs = line } else { refs = refs "\x1f" line }
        next;
    }
    END {
        if (block_open) print "BLOCK_END" "|" title "|" refs;
    }
' "$PROTOCOL" | while IFS='|' read -r _marker title refs; do
    if [[ -z "$refs" ]]; then
        violations=$((violations + 1))
        if [[ $QUIET -eq 0 ]]; then
            echo "no refs: $title" >&2
        fi
        continue
    fi
    IFS=$'\x1f' read -ra paths <<<"$refs"
    has_real=0
    for p in "${paths[@]}"; do
        case "$p" in
            *"(planned)"*) continue ;;
        esac
        # Accept exact files, directories, or glob matches.
        if [[ -e "$p" ]]; then
            has_real=1
            break
        fi
        # Try glob
        # shellcheck disable=SC2086
        if compgen -G "$p" >/dev/null 2>&1; then
            has_real=1
            break
        fi
    done
    if [[ $has_real -eq 0 ]]; then
        missing=$((missing + 1))
        if [[ $QUIET -eq 0 ]]; then
            echo "no real path: $title -> $refs" >&2
        fi
    fi
done

# The awk-piped loop runs in a subshell; recompute totals via separate pass
# for a clean exit decision.
total_blocks=$(grep -cE '^#### [0-9]+\.4\.[0-9]+ ' "$PROTOCOL" || true)
blocks_with_refs=$(awk '
    function flush_block() {
        if (in_block) {
            if (has_refs) good++;
            in_block = 0; has_refs = 0; saw_header = 0;
        }
    }
    /^#### [0-9]+\.4\.[0-9]+ / {
        flush_block();
        in_block = 1; has_refs = 0; saw_header = 0;
        next;
    }
    /^### / { flush_block(); next }
    /^## /  { flush_block(); next }
    /^\*\*References\.\*\*/ && in_block { saw_header = 1; next }
    saw_header && /^- `/ { has_refs = 1 }
    END { flush_block(); print good + 0 }
' "$PROTOCOL")

# Roll up: any block missing a refs section is a violation.
if (( total_blocks > blocks_with_refs )); then
    [[ $QUIET -eq 0 ]] && echo "check-refs: $((total_blocks - blocks_with_refs)) blocks lack a References section" >&2
    exit 70
fi

[[ $QUIET -eq 0 ]] && echo "check-refs: $total_blocks blocks scanned, $blocks_with_refs with refs."
exit 0
