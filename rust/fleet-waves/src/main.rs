// fleet-waves — drop-in replacement for `waves-anim-generic.sh`. Phase-4
// minimal: in-binary tab strip + vertical wave flow showing each sub-task
// chip + status. Uses fleet-data::plan for the source data and fleet-ui
// chip/card for the rendering.

use std::{io, path::PathBuf, process::Command, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fleet_ui::{card::card, chip::{status_chip, ChipKind}, palette::*};
use fleet_data::plan::{self, Plan, Subtask};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};

const TABS: &[(&str, &str)] = &[("0","watcher"),("1","overview"),("2","fleet"),("3","plan"),("4","waves")];
const ACTIVE_TAB: usize = 4;

struct App { tab_rects: Vec<(Rect, usize)>, plan: Option<Plan> }

impl App {
    fn new() -> Self {
        let plan = std::env::var("FLEET_PLAN_REPO_ROOT")
            .ok()
            .or_else(|| Some("/home/deadpool/Documents/recodee".to_string()))
            .and_then(|root| plan::newest_plan(&PathBuf::from(root)).ok().flatten())
            .and_then(|p| plan::load(&p).ok());
        Self { tab_rects: Vec::new(), plan }
    }
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

fn classify(s: &Subtask) -> ChipKind {
    match s.status.as_str() {
        "completed" => ChipKind::Done,
        "claimed" => ChipKind::Working,
        "blocked" => ChipKind::Blocked,
        _ => ChipKind::Idle,
    }
}

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0)]).split(area);
    render_tab_strip(frame, rows[0], ACTIVE_TAB, app);

    let title = app.plan.as_ref().map(|p| format!("WAVES · {}", p.plan_slug)).unwrap_or_else(|| "WAVES · no plan".to_string());
    frame.render_widget(card(Some(&title), false), rows[1]);

    let block = card(Some("VERTICAL WAVE FLOW"), false);
    let inner = block.inner(rows[2]);
    frame.render_widget(block, rows[2]);

    let Some(plan) = app.plan.as_ref() else { return; };
    let total = plan.tasks.len();
    let done = plan.tasks.iter().filter(|t| t.status == "completed").count();
    let header = format!("  {done}/{total} sub-tasks complete");
    frame.render_widget(Paragraph::new(Line::from(Span::styled(header, Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)))), Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 });

    for (i, s) in plan.tasks.iter().enumerate() {
        let y = inner.y + 1 + i as u16;
        if y + 1 > inner.y + inner.height { break; }
        let mut spans = vec![
            Span::styled(format!("  sub-{:>2} · ", s.subtask_index), Style::default().fg(IOS_FG_MUTED)),
        ];
        spans.extend(status_chip(classify(s)));
        spans.push(Span::styled(format!("  {}", s.title), Style::default().fg(IOS_FG)));
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
