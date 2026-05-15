## ADDED Requirements

### Requirement: Plan Watcher Handles Post Tab-Strip Panes
`plan-watcher.sh` SHALL not exclude panes only because they carry the removed
`[codex-fleet-tab-strip]` panel label.

#### Scenario: Labelled panes remain eligible
- **WHEN** `list_idle_workers` scans panes in the overview window
- **THEN** panes with a non-empty `@panel` label are eligible for idle-pattern
  matching
- **AND** there is no skip for `[codex-fleet-tab-strip]`.

#### Scenario: Uninitialised panes remain ignored
- **WHEN** `list_idle_workers` scans a pane with an empty `@panel` value
- **THEN** that pane is still skipped before tail capture.
