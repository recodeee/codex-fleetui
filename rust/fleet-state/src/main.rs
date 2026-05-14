// fleet-state — drop-in replacement for `fleet-state-anim.sh` + render half
// of `fleet-tick.sh`. Phase-4 minimal scaffold: in-binary tab strip + iOS
// cockpit card frame; full ACTIVE/RESERVE tables land in follow-up PRs.

use std::{io, process::Command, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fleet_ui::{card::card, chip::{status_chip, ChipKind}, palette::*, rail::{progress_rail, RailAxis}};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};

const TABS: &[(&str, &str)] = &[
    ("0", "watcher"),
    ("1", "overview"),
    ("2", "fleet"),
    ("3", "plan"),
    ("4", "waves"),
];

const ACTIVE_TAB: usize = 2;

struct App { tab_rects: Vec<(Rect, usize)> }
impl App {
    fn new() -> Self { Self { tab_rects: Vec::new() } }
    fn handle_click(&self, col: u16, row: u16) -> Option<usize> {
        self.tab_rects.iter().find(|(r, _)| col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height).map(|(_, i)| *i)
    }
}

fn render_tab_strip(frame: &mut ratatui::Frame, area: Rect, active: usize, app: &mut App) {
    app.tab_rects.clear();
    let mut x = area.x;
    for (i, (idx, name)) in TABS.iter().enumerate() {
        let label = format!(" {} {} ", idx, name);
        let w = label.chars().count() as u16;
        if x + w + 1 >= area.x + area.width { break; }
        let r = Rect { x, y: area.y, width: w, height: 1 };
        let style = if i == active { Style::default().fg(IOS_FG).bg(IOS_TINT).add_modifier(Modifier::BOLD) } else { Style::default().fg(IOS_FG_MUTED).bg(IOS_CHIP_BG) };
        frame.render_widget(Paragraph::new(Span::styled(label, style)), r);
        app.tab_rects.push((r, i));
        x += w + 1;
    }
}

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }

    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0)]).split(area);
    render_tab_strip(frame, rows[0], ACTIVE_TAB, app);

    let header = card(Some("FLEET · iOS cockpit · 8 workers"), false);
    frame.render_widget(header, rows[1]);

    let block = card(Some("ACTIVE · account · 5h · WEEKLY · WORKER · WORKING ON"), false);
    let inner = block.inner(rows[2]);
    frame.render_widget(block, rows[2]);

    // Mock rows demonstrating fleet-ui widgets composing together.
    let mocks: &[(&str, u8, u8, ChipKind, &str)] = &[
        ("admin@kollarrobert.sk",   12, 62, ChipKind::Working, "scaffold rust/fleet-ui"),
        ("admin@magnoliavilag.hu",   6, 54, ChipKind::Polling, "task_ready_for_agent"),
        ("admin@mite.hu",            0, 47, ChipKind::Working, "port plan.rs"),
        ("admin@zazrifka.sk",       28, 50, ChipKind::Approval, "Auto-reviewer approved"),
    ];
    for (i, (email, fivem, week, kind, what)) in mocks.iter().enumerate() {
        let y = inner.y + i as u16;
        if y + 1 > inner.y + inner.height { break; }
        let mut spans = vec![
            Span::styled(format!("  {:<24}", email), Style::default().fg(IOS_FG)),
            Span::raw(" "),
        ];
        spans.extend(progress_rail(*fivem, RailAxis::Usage, 10));
        spans.push(Span::raw(" "));
        spans.extend(progress_rail(*week, RailAxis::Usage, 10));
        spans.push(Span::raw(" "));
        spans.extend(status_chip(*kind));
        spans.push(Span::styled(format!("  {what}"), Style::default().fg(IOS_FG_MUTED)));
        frame.render_widget(Paragraph::new(Line::from(spans)), Rect { x: inner.x, y, width: inner.width, height: 1 });
    }
}

fn select_window(idx: usize) {
    let _ = Command::new("tmux").args(["select-window", "-t", &format!("codex-fleet:{}", idx)]).status();
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &mut app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) { break },
                    Event::Mouse(m) => {
                        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                            if let Some(idx) = app.handle_click(m.column, m.row) { select_window(idx); }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    result
}
