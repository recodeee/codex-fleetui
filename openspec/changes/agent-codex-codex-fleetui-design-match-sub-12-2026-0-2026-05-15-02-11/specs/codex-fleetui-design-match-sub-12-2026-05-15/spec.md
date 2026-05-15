## ADDED Requirements

### Requirement: codex-fleetui-design-match-sub-12-2026-05-15 behavior
The system SHALL render `fleet_ui::chip::status_chip` as a fixed-width
three-span pill with the left and right caps painted against the base dark
surface token and the label painted in the semantic chip fill color.

#### Scenario: Semantic chip mapping
- **WHEN** `ChipKind::Working`, `Idle`, `Polling`, `Done`, `Live`,
  `Blocked`, `Capped`, `Approval`, `Boot`, or `Dead` is rendered
- **THEN** the chip SHALL use the documented semantic fill color for that kind
- **AND** the chip label SHALL remain within the fixed seven-cell budget
- **AND** the chip caps SHALL continue to render as a pill outline on the
  base surface.

#### Scenario: Success-state live chip
- **WHEN** `ChipKind::Live` is rendered
- **THEN** the chip SHALL use `IOS_GREEN`
- **AND** the visible label SHALL read `live`.

#### Scenario: Accessibility contrast
- **WHEN** any chip kind is rendered
- **THEN** the label text SHALL use the foreground token on top of the fill
- **AND** the caps SHALL remain legible against the dark surface background.
