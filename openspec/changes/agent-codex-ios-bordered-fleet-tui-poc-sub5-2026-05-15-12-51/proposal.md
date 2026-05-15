## Why

The fleet-tui-poc section jump overlay should carry the same iOS bordered card language as the design reference: compact cards, visible separators, an active card treatment, and dense keyboard chrome that remains readable over the dimmed terminal backdrop.

## What Changes

- Tighten the Section Jump card grid and add an explicit dark grid surface.
- Add vertical and horizontal separator hairlines between card rows and columns.
- Add a subtle active-card glow and reuse shared live-chip green background.
- Update the focused Section Jump regression test to assert the command-key chrome.

## Impact

Only `rust/fleet-tui-poc/src/main.rs` changes. The Section Jump tmux dispatch mapping is unchanged. Verification is `cargo test -p fleet-tui-poc`.
