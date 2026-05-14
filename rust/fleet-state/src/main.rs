// fleet-state — tuirealm port. Renders the live fleet table (accounts +
// panes via `fleet_data::fleet::load_live`) inside a tuirealm
// `AppComponent`. Second binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (PR #50).
//
// Pattern (mirrors fleet-tab-strip):
//   - `FleetView` is the Component. It owns `rows: Option<Vec<WorkerRow>>`,
//     a `load_error: Option<String>`, and refreshes on every Tick.
//   - `Msg::Tick` drives a `refresh()` call; `Msg::Quit` terminates the
//     loop. q / Esc → Msg::Quit handled inline (drops fleet-input).
//   - The existing render functions (`render`, `render_worker_row`,
//     `chip_kind`) are unchanged — the tuirealm wrapper only owns the
//     event loop, not the rendering.
//
// `cargo build -p fleet-state` exercises the full chain:
// tmux::list_panes → panes::list_panes → fleet::join.

use std::io;
use std::time::Duration;

use fleet_data::{
    fleet::{self, FleetSummary, WorkerRow},
    panes::PaneState,
};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
    rail::{progress_rail, RailAxis},
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::Style;
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::Paragraph;
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

// ---------- Messages and component IDs ----------

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Fleet,
}

// ---------- The Fleet component ----------

/// tmux session + window the fleet's worker panes live in. Matches the
/// `codex-fleet:overview` target every dashboard binary uses; overridable
/// via env for parallel fleets (`codex-fleet-2`, …).
fn fleet_target() -> (String, String) {
    let session =
        std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".to_string());
    let window = std::env::var("CODEX_FLEET_WINDOW").unwrap_or_else(|_| "overview".to_string());
    (session, window)
}

struct FleetView {
    rows: Option<Vec<WorkerRow>>,
    load_error: Option<String>,
    props: Props,
}

impl Default for FleetView {
    fn default() -> Self {
        let mut view = Self { rows: None, load_error: None, props: Props::default() };
        view.refresh();
        view
    }
}

impl FleetView {
    fn refresh(&mut self) {
        let (session, window) = fleet_target();
        match fleet::load_live(&session, Some(&window)) {
            Ok(rows) => {
                self.rows = Some(rows);
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
            }
        }
    }
}

impl Component for FleetView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 8 {
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Header banner: "FLEET · N workers · M live · K in review".
        let header_text = match &self.rows {
            Some(worker_rows) => {
                let s = FleetSummary::of(worker_rows);
                format!(
                    "FLEET · {} workers · {} live · {} in review",
                    s.workers, s.live, s.in_review
                )
            }
            None => "FLEET · loading…".to_string(),
        };
        frame.render_widget(card(Some(&header_text), false), rows[0]);

        // Worker table card.
        let block = card(
            Some("ACCOUNT · WEEKLY · 5H · QUALITY · STATUS · WORKING ON"),
            false,
        );
        let inner = block.inner(rows[1]);
        frame.render_widget(block, rows[1]);

        match &self.rows {
            Some(worker_rows) if !worker_rows.is_empty() => {
                for (i, row) in worker_rows.iter().enumerate() {
                    let y = inner.y + i as u16;
                    if y + 1 > inner.y + inner.height {
                        break;
                    }
                    render_worker_row(
                        frame,
                        Rect { x: inner.x, y, width: inner.width, height: 1 },
                        row,
                    );
                }
            }
            Some(_) => {
                let msg = match &self.load_error {
                    Some(_) => "  fleet unreachable — see status below",
                    None => "  no workers — is the codex-fleet session up?",
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(IOS_FG_MUTED)))),
                    Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
                );
            }
            None => {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "  loading fleet state…",
                        Style::default().fg(IOS_FG_MUTED),
                    ))),
                    Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
                );
            }
        }

        if let Some(err) = &self.load_error {
            let y = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  load error: {err}"),
                    Style::default().fg(IOS_FG_FAINT),
                ))),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
        }
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

impl AppComponent<Msg, NoUserEvent> for FleetView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Char('q'), .. })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => {
                // Refresh under the hood so the next frame sees fresh rows.
                self.refresh();
                Some(Msg::Tick)
            }
            _ => None,
        }
    }
}

// ---------- Render helpers (unchanged from pre-tuirealm) ----------

fn chip_kind(state: Option<PaneState>) -> ChipKind {
    match state {
        Some(PaneState::Working) => ChipKind::Working,
        Some(PaneState::Idle) | None => ChipKind::Idle,
        Some(PaneState::Polling) => ChipKind::Polling,
        Some(PaneState::Capped) => ChipKind::Capped,
        Some(PaneState::Approval) => ChipKind::Approval,
        Some(PaneState::Boot) => ChipKind::Boot,
        Some(PaneState::Dead) => ChipKind::Dead,
    }
}

fn render_worker_row(frame: &mut Frame, area: Rect, row: &WorkerRow) {
    let mut spans: Vec<Span> = Vec::new();

    // ACCOUNT.
    let label = if row.is_current {
        format!("★{}", row.email)
    } else {
        row.email.clone()
    };
    spans.push(Span::styled(format!("  {:<26}", label), Style::default().fg(IOS_FG)));
    spans.push(Span::raw(" "));

    // WEEKLY · 5H rails.
    spans.extend(progress_rail(row.weekly_pct, RailAxis::Usage, 8));
    spans.push(Span::raw(" "));
    spans.extend(progress_rail(row.five_h_pct, RailAxis::Usage, 8));
    spans.push(Span::raw(" "));

    // QUALITY rail (advisory).
    match row.quality {
        Some(pct) => spans.extend(progress_rail(pct, RailAxis::Done, 8)),
        None => spans.push(Span::raw(" ".repeat(10))),
    }
    spans.push(Span::raw(" "));

    // STATUS chip.
    spans.extend(status_chip(chip_kind(row.state)));
    spans.push(Span::raw("  "));

    // WORKING ON.
    if row.working_on.is_empty() {
        spans.push(Span::styled("—  reserve", Style::default().fg(IOS_FG_FAINT)));
    } else {
        spans.push(Span::styled(row.working_on.clone(), Style::default().fg(IOS_FG)));
        if !row.pane_subtext.is_empty() {
            spans.push(Span::styled(
                format!("   {}", row.pane_subtext),
                Style::default().fg(IOS_FG_MUTED),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
        // 250ms tick interval matches the pre-tuirealm refresh cadence.
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .crossterm_input_listener(Duration::from_millis(100), 3)
                .tick_interval(Duration::from_millis(250)),
        );
        app.mount(
            Id::Fleet,
            Box::new(FleetView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Fleet)?;
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
            let _ = self.app.view(&Id::Fleet, frame, area);
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

    let _ = model.terminal.disable_raw_mode();
    let _ = model.terminal.leave_alternate_screen();
    result
}
