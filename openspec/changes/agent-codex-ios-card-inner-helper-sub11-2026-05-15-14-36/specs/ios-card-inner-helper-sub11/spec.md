## ADDED Requirements

### Requirement: fleet-ui card_inner helper
The `fleet-ui` crate SHALL expose an additive `card_inner(frame, area, tint)` helper that paints an inset iOS glass surface for use inside card content areas.

#### Scenario: Rendering an inner card surface
- **WHEN** `card_inner` renders into a non-empty `Rect`
- **THEN** the rendered surface uses rounded border glyphs
- **AND** its border foreground uses `IOS_HAIRLINE_BORDER`
- **AND** its background uses the caller-provided tint color.

#### Scenario: Empty render area
- **WHEN** `card_inner` receives a zero-width or zero-height `Rect`
- **THEN** it returns without attempting to render.
