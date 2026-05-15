## Why

`fleet-watcher` rendered narrower than the actual terminal width and didn't reflow when the tmux pane / kitty window was resized. Root cause: `WatcherView` only subscribed to `EventClause::Tick`. When the terminal emitted a `WindowResize` event, the catch-all `_ => None` arm in `on()` swallowed it, so no `Msg` was returned, `Model::update` never set `redraw = true`, and the dashboard waited up to 2 seconds for the next Tick (or stayed at the cached layout indefinitely if the Tick happened to coincide with a no-op draw).

## What Changes

- `Msg::Resize` variant added so resize events can be dispatched into the existing update loop without overloading the `Tick` semantics.
- `WatcherView::on()` matches `Event::WindowResize(_, _)` and returns `Some(Msg::Resize)`.
- `WatcherView` subscription list now includes `Sub::new(EventClause::WindowResize, SubClause::Always)` alongside the existing `Tick` subscription so tuirealm actually routes resize events to the component.
- `Model::update()` handles `Msg::Resize` symmetrically with `Msg::Tick` — sets `redraw = true` (already unconditional at the top of the match) without further state changes. ratatui's `Terminal::draw` autoresizes the back buffer on the next call, so the dashboard immediately reflows to fill the new surface.
- Regression test `window_resize_event_triggers_redraw_message` exercises the resize path directly through `WatcherView::on()` and asserts a non-`None` `Msg` is returned.

## Impact

- Affected surface: `rust/fleet-watcher/src/main.rs` only. Other fleet TUIs (`fleet-state`, `fleet-waves`, `fleet-plan-tree`, `fleet-tui-poc`) share the same Tick-only subscription pattern and likely have the same bug, but they're out of scope for this PR. The user opted to fix watcher first; a follow-up can mirror the pattern across the family.
- Risk: very low. Adding a Msg variant and a subscription is additive. Existing `Msg::Tick`/`Msg::Quit` behavior is unchanged, all 6 pre-existing tests stay green, and the snapshot rendering is unaffected (it doesn't go through the event loop).
- Rollout: ship with the next `cargo build --release` of `fleet-watcher`. No config flag. Operators don't need to do anything; the watcher will start reflowing on resize automatically.
- No version bump or release-note edit required (internal binary, no published artifact).
