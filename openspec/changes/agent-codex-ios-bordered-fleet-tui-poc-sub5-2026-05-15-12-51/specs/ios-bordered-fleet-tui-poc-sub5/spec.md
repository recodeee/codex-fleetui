## ADDED Requirements

### Requirement: Section Jump iOS bordered card grid
The fleet-tui-poc Section Jump overlay SHALL render a compact iOS bordered card grid with active-card emphasis, visible row/column separator hairlines, command-key chrome, and unchanged tmux section dispatch behavior.

#### Scenario: Section Jump render
- **WHEN** the Section Jump overlay is rendered with an active section
- **THEN** the overlay includes the codex-fleet header, command-key chip, five section cards, active LIVE treatment, shortcuts panel, and footer hints.

#### Scenario: Section Jump dispatch
- **WHEN** number keys 1 through 5 are handled by the Section Jump overlay
- **THEN** the existing tmux `select-window -t <session>:<window>` mapping is preserved.
