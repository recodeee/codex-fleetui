# codex-fleet top-level taskfile.
#
# Recipes group commands the project actually exposes; favour adding to
# this file rather than expanding README prose. Each recipe is a single
# entry-point that CI and humans can both call.

default:
    @just --list

# --- Protocol governance ---------------------------------------------------

# Summarise lifecycle states across docs/future/PROTOCOL.md.
protocol-state:
    bash scripts/protocol/protocol-state.sh --summary

# Render the state distribution as a markdown table for paste-back.
protocol-state-table:
    bash scripts/protocol/protocol-state.sh --table

# Assert every improvement carries a valid lifecycle state line.
protocol-check-states:
    bash scripts/protocol/check-states.sh

# Assert every improvement cites at least one real path in its References.
protocol-check-refs:
    bash scripts/protocol/check-refs.sh --quiet

# Warn if any section blows past 1.5x its budget.
protocol-check-budget:
    bash scripts/protocol/check-budget.sh

# Run every protocol governance check.
protocol-check:
    just protocol-check-states
    just protocol-check-refs
    just protocol-check-budget

# --- Rust workspace --------------------------------------------------------

rust-build:
    cd rust && cargo build --workspace

rust-test:
    cd rust && cargo test --workspace

rust-fmt:
    cd rust && cargo fmt --all

rust-clippy:
    cd rust && cargo clippy --workspace --all-targets

# --- Composite -------------------------------------------------------------

# Single entrypoint CI calls; expand as new gates land.
ci:
    just protocol-check
