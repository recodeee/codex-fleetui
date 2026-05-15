## ADDED Requirements

### Requirement: Pane Context Menu Transparent Chrome
The pane context menu SHALL render its bash fallback chrome using foreground-only
ANSI color escapes so the tmux popup remains visually transparent over the
underlying pane.

#### Scenario: Menu renders without solid backgrounds
- **WHEN** `scripts/codex-fleet/bin/pane-context-menu.sh` renders the popup
- **THEN** the emitted menu chrome uses `38;2;R;G;B` foreground color escapes
- **AND** the emitted menu chrome does not use `48;2;R;G;B` background color escapes.

#### Scenario: Menu colors match the requested iOS tones
- **WHEN** the menu renders normal, disabled, danger, header, and divider rows
- **THEN** dividers use `#3A3A3C`
- **AND** normal labels use `#FFFFFF`
- **AND** normal icons use `#8E8E93`
- **AND** shortcut text uses `#AEAEB2`
- **AND** disabled rows use `#48484A`
- **AND** danger rows use `#FF3B30`
- **AND** the header dot and `LIVE` text use `#34C759`.

#### Scenario: Hotkey feedback flashes before dispatch
- **WHEN** the operator presses a recognized hotkey
- **THEN** the matching row is re-rendered briefly with iOS-blue underline
- **AND** the popup clears before dispatching the selected action.
