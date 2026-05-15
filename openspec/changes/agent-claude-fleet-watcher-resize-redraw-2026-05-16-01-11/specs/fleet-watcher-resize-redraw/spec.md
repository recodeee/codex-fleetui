## ADDED Requirements

### Requirement: fleet-watcher MUST reflow on terminal resize
`fleet-watcher` SHALL trigger a redraw within one event-loop iteration when the terminal or hosting tmux pane reports a `WindowResize` event, so the dashboard immediately uses the full new width/height instead of waiting for the next periodic tick.

#### Scenario: WindowResize event produces a redraw-triggering Msg
- **GIVEN** a `WatcherView` mounted in the tuirealm `Application` with both `EventClause::Tick` and `EventClause::WindowResize` subscriptions active
- **WHEN** the terminal backend emits `Event::WindowResize(width, height)` (e.g. tmux pane grew, kitty window resized, or `SIGWINCH` after a layout change)
- **THEN** `WatcherView::on()` SHALL return `Some(Msg::Resize)` (not `None`)
- **AND** `Model::update(Msg::Resize)` SHALL set `redraw = true`
- **AND** the next `Model::view()` SHALL render at the new `frame.area()` so headers, stat cards, the review queue, and the diff sparkline span the full width

#### Scenario: Existing Tick and Quit paths are unaffected
- **GIVEN** the same mounted `WatcherView`
- **WHEN** `Event::Tick` fires every 2 seconds
- **THEN** `Msg::Tick` SHALL still be returned and the tick counter SHALL advance
- **AND** keyboard `q` / `Esc` SHALL still produce `Msg::Quit` and terminate the app on the next loop iteration
