## Why

- The pane right-click menu should read as glass over the live terminal, not as
  a solid card that hides the underlying pane.
- Operators need a quick visual confirmation of the hotkey they just pressed
  before the popup dispatches the action.

## What Changes

- Rework `scripts/codex-fleet/bin/pane-context-menu.sh` to render foreground-only
  ANSI chrome with no solid background SGR output.
- Use iOS separator, label, secondary-label, success, danger, disabled, and blue
  focus colors directly in the script without changing shared menu helpers.
- Replace bracket shortcut chips with dim foreground-only shortcut hints.
- Add an 80ms blue underline feedback flash for the selected hotkey row before
  dispatch.
- Document a smoke-test command for capturing PR evidence.

## Impact

- Scope is limited to the bash fallback pane context menu.
- Shared `ios-menu.sh`, `help-popup.sh`, and tmux binding scripts are unchanged.
- Verification checks syntax, fg-only rendered output, focus underline output,
  and OpenSpec validity.
