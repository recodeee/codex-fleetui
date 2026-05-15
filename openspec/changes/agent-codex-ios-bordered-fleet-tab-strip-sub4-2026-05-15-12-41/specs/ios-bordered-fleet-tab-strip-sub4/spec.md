## ADDED Requirements

### Requirement: Fleet tab strip iOS glass dock polish
The fleet tab strip SHALL render its Design E dock with compact pill spacing, a visible shadow band below the dock, an active-tab underlight, and a live chip whose pulse affects the glass fill as well as the status dot/edge.

#### Scenario: Full-height dock render
- **WHEN** the fleet tab strip is rendered in a full-height terminal area
- **THEN** all five tab pills and the live chip are present
- **AND** the active tab retains its wider hit-test rectangle
- **AND** the focus card still renders below the dock when space allows.

#### Scenario: One-row fallback render
- **WHEN** the fleet tab strip is rendered in a one-row terminal area
- **THEN** the dock still renders the active tab and usable hit-test rectangles.
