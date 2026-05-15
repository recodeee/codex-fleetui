## Why

- The SessionSwitcher overlay should expose the expected action row as readable
  iOS-style controls, not icon-only controls on wide cards.
- The active card needs enough width for labelled actions without clipping while
  preserving the compact fallback for narrow terminals.

## What Changes

- Widen the selected session card so the active stack card can carry full action
  labels.
- Render `Queue`, `Pause`, and `Kill` labels on spacious cards, keeping compact
  symbols only when the card is too narrow.
- Extend the existing SessionSwitcher render test to assert the full labelled
  action row.

## Impact

- Scope is limited to `rust/fleet-ui/src/session_switcher_overlay.rs`.
- Existing public structs and module exports remain unchanged.
- Verification uses the focused `fleet-ui` SessionSwitcher test filter.
