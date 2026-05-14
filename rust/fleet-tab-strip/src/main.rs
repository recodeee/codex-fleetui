// fleet-tab-strip — tuirealm port. Renders the codex-fleet glass-dock tab
// strip as an `AppComponent`, routing `MouseEvent::Down(Left)` through
// tuirealm's M-V-U cycle to dispatch `tmux select-window`.
//
// First binary in the codex-fleet ratatui → tuirealm migration. The
// existing `fleet_ui::tab_strip::TabStrip` widget stays as the rendering
// backend (now usable directly thanks to the workspace ratatui bump
// 0.28 → 0.30 in this same PR); the binary wraps it in a `Component` +
// `AppComponent<Msg, NoUserEvent>` pair so the click handler, tick
// counter, and active-tab resolution flow through tuirealm's update cycle
// instead of an ad-hoc crossterm event loop.
//
// Why migrate at all: the codex-fleet binaries each grow their own
// hand-rolled crossterm event loop + state. tuirealm gives us a uniform
// (state, update, view) shape so future binaries (fleet-state,
// fleet-plan-tree, fleet-waves, fleet-watcher, fleet-tui-poc) can be
// re-implemented with the same mental model.

use std::io;
// std::process::Command moved into fleet_components::select_tmux_window.
use std::time::Duration;

use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, MouseButton, MouseEvent, MouseEventKind, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::layout::Rect;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

use fleet_ui::tab_strip::{Tab, TabHit, TabStrip};

// ---------- Messages and component IDs ----------

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    TabClicked(usize),
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Strip,
}

// ---------- The Strip component ----------

/// Wraps `fleet_ui::tab_strip::TabStrip` as a tuirealm `Component` +
/// `AppComponent<Msg, NoUserEvent>`. Owns:
///
///   - the wall-clock tick counter shown in the live chip,
///   - the most recent hit-test rects so `on(Event::Mouse(..))` can map a
///     click coordinate to a tmux window index without re-rendering.
///
/// Active tab is resolved on each `view()` from `tmux display-message` —
/// when the operator switches windows, the active pill follows.
struct StripView {
    tick: u64,
    last_hits: Vec<TabHit>,
    props: Props,
}

impl Default for StripView {
    fn default() -> Self {
        Self { tick: 0, last_hits: Vec::new(), props: Props::default() }
    }
}

impl Component for StripView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        let active = current_tab();
        let strip = TabStrip::new(active, area.width).with_tick(self.tick);
        self.last_hits = strip.render(frame, area);
    }

    fn query(&self, attr: Attribute) -> Option<QueryResult<'_>> {
        self.props.get(attr).map(|v| QueryResult::from(v.clone()))
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<Msg, NoUserEvent> for StripView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Char('q'), .. })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => self
                .last_hits
                .iter()
                .find(|h| {
                    *column >= h.rect.x
                        && *column < h.rect.x + h.rect.width
                        && *row >= h.rect.y
                        && *row < h.rect.y + h.rect.height
                })
                .map(|h| Msg::TabClicked(h.window_idx)),
            Event::Tick => Some(Msg::Tick),
            _ => None,
        }
    }
}

// ---------- Model (tuirealm's M in M-V-U) ----------

struct Model<T: TerminalAdapter> {
    app: Application<Id, Msg, NoUserEvent>,
    terminal: T,
    quit: bool,
    redraw: bool,
}

impl Model<CrosstermTerminalAdapter> {
    fn new() -> io::Result<Self> {
        let app = Self::init_app().map_err(|e| io::Error::other(format!("init app: {e:?}")))?;
        let terminal =
            Self::init_adapter().map_err(|e| io::Error::other(format!("init adapter: {e:?}")))?;
        Ok(Self { app, terminal, quit: false, redraw: true })
    }

    fn init_app() -> Result<Application<Id, Msg, NoUserEvent>, Box<dyn std::error::Error>> {
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .crossterm_input_listener(Duration::from_millis(100), 3)
                .tick_interval(Duration::from_millis(500)),
        );
        app.mount(
            Id::Strip,
            Box::new(StripView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Strip)?;
        Ok(app)
    }

    fn init_adapter() -> Result<CrosstermTerminalAdapter, Box<dyn std::error::Error>> {
        // Delegate to the shared helper. `true` opts into
        // EnableMouseCapture because fleet-tab-strip's pills are
        // clickable.
        Ok(fleet_components::init_crossterm_adapter(true)?)
    }
}

impl<T: TerminalAdapter> Model<T> {
    fn view(&mut self) {
        let _ = self.terminal.draw(|frame| {
            let area = frame.area();
            let _ = self.app.view(&Id::Strip, frame, area);
        });
    }

    fn update(&mut self, msg: Msg) {
        self.redraw = true;
        match msg {
            Msg::Quit => self.quit = true,
            Msg::TabClicked(idx) => select_window(idx),
            Msg::Tick => {}
        }
    }
}

// ---------- tmux integration helpers ----------

fn current_tab() -> Tab {
    let idx: usize = std::process::Command::new("tmux")
        .args(["display-message", "-p", "-F", "#{window_index}"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0);
    match idx {
        0 => Tab::Overview,
        1 => Tab::Fleet,
        2 => Tab::Plan,
        3 => Tab::Waves,
        _ => Tab::Review,
    }
}

fn select_window(idx: usize) {
    // Delegated to the shared helper so every dashboard's tmux
    // click-routing semantics match.
    fleet_components::select_tmux_window(idx);
}

// ---------- Entry point ----------

fn main() -> io::Result<()> {
    let mut model = Model::<CrosstermTerminalAdapter>::new()?;

    let result = (|| -> io::Result<()> {
        while !model.quit {
            if let Ok(messages) = model.app.tick(PollStrategy::Once(Duration::from_millis(100))) {
                for msg in messages {
                    model.update(msg);
                }
            }
            if model.redraw {
                model.view();
                model.redraw = false;
            }
        }
        Ok(())
    })();

    fleet_components::shutdown_adapter(&mut model.terminal);
    result
}
