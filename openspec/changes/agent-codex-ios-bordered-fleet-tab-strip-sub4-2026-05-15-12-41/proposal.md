## Why

The fleet tab strip should match the Design E floating iOS glass dock more closely. The current dock already has bordered pills, but the spacing, shadow, active glow, and live pulse need refinement so the navigation reads as one polished floating surface.

## What Changes

- Tighten spacing between tab pills and reduce the dock width budget.
- Add a centered dock shadow band and active-tab underlight.
- Make the live chip pulse its glass fill as well as the dot/edge.
- Keep the change scoped to `rust/fleet-tab-strip/src/main.rs`.

## Impact

Only the `fleet-tab-strip` binary render path changes. Mouse hit testing still uses the same tab rectangles and tmux window indexes. Verification is `cargo test -p fleet-tab-strip`.
