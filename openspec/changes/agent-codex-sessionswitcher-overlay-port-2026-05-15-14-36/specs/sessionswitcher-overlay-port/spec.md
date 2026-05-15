## ADDED Requirements

### Requirement: SessionSwitcher Action Row Labels
The SessionSwitcher overlay SHALL show readable action labels on the selected
session card when there is enough card width.

#### Scenario: Spacious selected card shows full actions
- **WHEN** the SessionSwitcher renders on a wide terminal
- **THEN** the selected card shows `Focus`, `Queue`, `Pause`, and `Kill` action
  controls.

#### Scenario: Narrow cards remain compact
- **WHEN** the SessionSwitcher renders on a narrow terminal
- **THEN** secondary action controls may use compact symbols instead of full
  labels
- **AND** the controls do not overflow the card.
