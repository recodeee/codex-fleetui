// fleet-tui-poc — validates the three risks called out in
// openspec/changes/fleet-tui-ratatui-port-2026-05-14/design.md before the
// fleet-ui crate scaffolds:
//
//   1. Truecolor (Color::Rgb(0, 122, 255) = systemBlue) renders inside tmux
//      without colour-quantisation against `style-tabs.sh` chrome.
//   2. ratatui's BorderType::Rounded does NOT double-frame against tmux's
//      `pane-border-status top` + `pane-border-format ' #[…] ▭ #{@panel} '`.
//   3. crossterm mouse-click events reach the binary through tmux's
//      `mouse on` pass-through.
//
// Render: a single rounded card containing one ◖ ● working ◗ chip + an
// event log showing the last 5 mouse events.
//
// Run: `cargo run -p fleet-tui-poc`. Press `q` or Esc to quit. Click the
// chip — its coordinates appear in the event log. If they do not, tmux is
// swallowing mouse events and the design must change.

use std::{
    io::{self, stdout, Stdout},
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Terminal,
};

// iOS systemBlue, replicated from `scripts/codex-fleet/fleet-tick.sh`.
// Same hex (#007AFF) the operator's tmux iOS chrome uses for active tabs.
const IOS_BLUE: Color = Color::Rgb(0, 122, 255);
const IOS_BG: Color = Color::Rgb(0, 0, 0);
const IOS_WHITE: Color = Color::Rgb(255, 255, 255);
const IOS_LABEL2: Color = Color::Rgb(174, 174, 178);

// ◖ ● working ◗ — width-1 glyphs verified against `test-status-chips.sh`.
const CHIP_LEFT_CAP: &str = "◖";
const CHIP_RIGHT_CAP: &str = "◗";
const CHIP_DOT: &str = "●";

struct App {
    events: Vec<String>,
    chip_rect: Option<Rect>,
}

impl App {
    fn new() -> Self {
        Self {
            events: vec!["click the systemBlue chip — coords land here".into()],
            chip_rect: None,
        }
    }

    fn record_mouse(&mut self, ev: MouseEvent) {
        let inside = self
            .chip_rect
            .map(|r| {
                ev.column >= r.x
                    && ev.column < r.x + r.width
                    && ev.row >= r.y
                    && ev.row < r.y + r.height
            })
            .unwrap_or(false);
        let tag = if inside { "✓ ON CHIP" } else { "off chip" };
        let line = format!("  ({}, {})  {:?}  {}", ev.column, ev.row, ev.kind, tag);
        self.events.push(line);
        if self.events.len() > 8 {
            self.events.remove(0);
        }
    }
}

fn ios_chip(label: &str, bg: Color) -> Vec<Span<'static>> {
    let label_text = format!("  {}  ", label);
    vec![
        Span::styled(CHIP_LEFT_CAP, Style::default().fg(bg).bg(IOS_BG)),
        Span::styled(
            format!(" {} ", CHIP_DOT),
            Style::default()
                .fg(IOS_WHITE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label_text,
            Style::default()
                .fg(IOS_WHITE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(CHIP_RIGHT_CAP, Style::default().fg(bg).bg(IOS_BG)),
    ]
}

fn run() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let mut app = App::new();
    loop {
        terminal.draw(|frame| render(frame, &mut app))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) => match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                },
                Event::Mouse(m) => {
                    if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                        app.record_mouse(m);
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_LABEL2))
        .title(Span::styled(
            " ◆  fleet-tui-poc  (press q to quit) ",
            Style::default()
                .fg(IOS_WHITE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(IOS_BG));
    let inner = card.inner(area);
    frame.render_widget(card, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let chip_spans = ios_chip("working", IOS_BLUE);
    let chip_width: u16 = chip_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();
    let chip_para = Paragraph::new(Line::from(chip_spans));
    let chip_rect = Rect {
        x: rows[0].x,
        y: rows[0].y,
        width: chip_width.min(rows[0].width),
        height: 1,
    };
    frame.render_widget(chip_para, chip_rect);
    app.chip_rect = Some(chip_rect);

    let hint = Paragraph::new(Line::from(Span::styled(
        "click the chip; coords appear below. expect ✓ ON CHIP when click lands inside.",
        Style::default().fg(IOS_LABEL2),
    )));
    frame.render_widget(hint, rows[1]);

    let log_lines: Vec<Line> = app
        .events
        .iter()
        .map(|e| Line::from(Span::styled(e.clone(), Style::default().fg(IOS_WHITE))))
        .collect();
    let log = Paragraph::new(log_lines);
    frame.render_widget(log, rows[2]);
}

fn main() -> io::Result<()> {
    run()
}
