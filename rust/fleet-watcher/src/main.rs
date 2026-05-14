// fleet-watcher — tuirealm port. Renders the watcher-board chrome (header
// banner + 4 stat cards + per-pane table placeholder) as a tuirealm
// `AppComponent`. Fifth binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (#50), fleet-state (#52),
// fleet-plan-tree (#53), fleet-waves (#54).

use std::io;
use std::time::Duration;

use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::{Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::Paragraph;
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Watcher,
}

#[derive(Default)]
struct WatcherView {
    props: Props,
}

impl Component for WatcherView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 8 {
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header banner
                Constraint::Length(5), // 4 stat cards
                Constraint::Min(0),    // per-pane table area
            ])
            .split(area);

        let header = card(Some("WATCHER · all clear · live"), false);
        frame.render_widget(header, rows[0]);

        let stats = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(25); 4])
            .split(rows[1]);
        for (i, (label, value, kind)) in [
            ("PANES", "8", ChipKind::Working),
            ("CAPPED", "0", ChipKind::Idle),
            ("SWAPPED", "0", ChipKind::Done),
            ("RANKED", "20", ChipKind::Working),
        ]
        .iter()
        .enumerate()
        {
            let block = card(Some(label), false);
            let inner = block.inner(stats[i]);
            frame.render_widget(block, stats[i]);
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    format!("  {value}  "),
                    Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
                )])),
                Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
            );
            let chip = status_chip(*kind);
            if inner.height >= 2 {
                frame.render_widget(
                    Paragraph::new(Line::from(chip)),
                    Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
                );
            }
        }

        let panes_block = card(Some("FLEET PANES — port of watcher-board.sh deferred to follow-up"), false);
        let inner = panes_block.inner(rows[2]);
        frame.render_widget(panes_block, rows[2]);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "  fleet-data::panes::list_panes(\"codex-fleet\", Some(\"overview\")) → PaneState classifier",
                    Style::default().fg(IOS_FG_MUTED),
                )),
                Line::from(Span::styled(
                    "  use tmux's status-bar tabs (style-tabs.sh) to switch windows.",
                    Style::default().fg(IOS_FG_MUTED),
                )),
            ]),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: inner.height.min(3) },
        );
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

impl AppComponent<Msg, NoUserEvent> for WatcherView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Char('q'), .. })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => Some(Msg::Tick),
            _ => None,
        }
    }
}

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
            Id::Watcher,
            Box::new(WatcherView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Watcher)?;
        Ok(app)
    }

    fn init_adapter() -> Result<CrosstermTerminalAdapter, Box<dyn std::error::Error>> {
        let mut adapter = CrosstermTerminalAdapter::new()?;
        adapter.enable_raw_mode()?;
        adapter.enter_alternate_screen()?;
        Ok(adapter)
    }
}

impl<T: TerminalAdapter> Model<T> {
    fn view(&mut self) {
        let _ = self.terminal.draw(|frame| {
            let area = frame.area();
            let _ = self.app.view(&Id::Watcher, frame, area);
        });
    }

    fn update(&mut self, msg: Msg) {
        self.redraw = true;
        match msg {
            Msg::Quit => self.quit = true,
            Msg::Tick => {}
        }
    }
}

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
    let _ = model.terminal.disable_raw_mode();
    let _ = model.terminal.leave_alternate_screen();
    result
}
