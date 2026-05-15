## ADDED Requirements

### Requirement: AgentKind classification for fleet-pane-health rows
The `fleet-pane-health` dashboard SHALL classify every pane it lists into one of `codex`, `kiro`, `claude`, or `unknown`. The classifier SHALL derive the kind from the `@panel` label first (substring match on `codex` / `kiro` / `claude`, case-insensitive) and SHALL fall back to the `/tmp/claude-viz/{codex,kiro,claude}-worker-*.log` filename prefix only when the panel label is empty or unrecognised. Panel-derived classification SHALL win over log-derived classification when both are available.

#### Scenario: Panel label drives classification
- **WHEN** a pane has `@panel = "codex-admin-mite"`
- **THEN** its kind is `codex`
- **AND** the row renders with the `CODX` badge in the `KIND` column.

#### Scenario: Log filename fallback when panel is blank
- **WHEN** a pane has no `@panel` label and the freshest matching log file is named `kiro-worker-foo.log`
- **THEN** its kind is `kiro`
- **AND** the row renders with the `KIRO` badge.

#### Scenario: Unknown when neither signal matches
- **WHEN** a pane has neither a recognised panel label nor a matching worker log
- **THEN** its kind is `unknown`
- **AND** the row renders the `—` badge in the `KIND` column.

### Requirement: KIND column rendering
The dashboard SHALL render a `KIND` column between `PANE` and `PANEL`. The column SHALL show the kind's four-character badge (`CODX`, `KIRO`, `CLAU`, or `—`) in the kind's tint colour (`IOS_TINT`, `IOS_PURPLE`, `IOS_ORANGE`, `IOS_FG_FAINT` respectively).

#### Scenario: Column heading present
- **WHEN** the dashboard renders any frame
- **THEN** the column-headings row includes the literal `KIND` between `PANE` and `PANEL`.

### Requirement: Group-by-kind toggle
The dashboard SHALL support a grouped view, toggled by pressing `g` or `G`. When grouped, rows SHALL be sorted by kind in the order `codex`, `kiro`, `claude`, `unknown`, and a `── group: <kind> ──` header SHALL be inserted before each group of rows. The header SHALL render in the kind's tint colour. The footer SHALL show `g group: on` when grouped and `g group: off` otherwise.

#### Scenario: Toggling grouping injects headers
- **WHEN** the user presses `g` from the default (ungrouped) view
- **THEN** the next frame shows `── group: codex ──` (and equivalent headers for any other kinds present) and the footer reads `g group: on`.

#### Scenario: Toggling back removes headers
- **WHEN** the user presses `g` again from the grouped view
- **THEN** no `── group: ──` header rows appear in the next frame and the footer reads `g group: off`.

### Requirement: Read-only data flow preserved
This change SHALL NOT introduce any new writes to `/tmp/claude-viz`, any new tmux commands, or any new external processes. All data sources remain best-effort reads as before.

#### Scenario: No writes to /tmp/claude-viz
- **WHEN** the dashboard runs for any length of time
- **THEN** no file under `/tmp/claude-viz` is created, modified, or deleted by `fleet-pane-health`.
