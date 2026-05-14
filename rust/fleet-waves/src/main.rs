// fleet-waves — drop-in replacement for `waves-anim-generic.sh`. Renders a
// vertical wave flow showing each sub-task chip + status, using
// fleet-data::plan for the source data and fleet-ui chip/card for the
// rendering. The pane runs inside the `codex-fleet` tmux session, whose
// status bar (`style-tabs.sh`) supplies the canonical tab strip; this
// binary therefore does not draw one of its own.

use std::{io, path::PathBuf, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode},
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

struct App { plan: Option<Plan> }

impl App {
    fn new() -> Self {
        let plan = std::env::var("FLEET_PLAN_REPO_ROOT")
            .ok()
            .or_else(|| Some("/home/deadpool/Documents/recodee".to_string()))
            .and_then(|root| plan::newest_plan(&PathBuf::from(root)).ok().flatten())
            .and_then(|p| plan::load(&p).ok());
        Self { plan }
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

fn render(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

    let title = app.plan.as_ref().map(|p| format!("WAVES · {}", p.plan_slug)).unwrap_or_else(|| "WAVES · no plan".to_string());
    frame.render_widget(card(Some(&title), false), rows[0]);

    let block = card(Some("VERTICAL WAVE FLOW"), false);
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);

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

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    let app = App::new();
    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &app))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(k) = event::read()? {
                    if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) { break }
                }
            }
        }
        Ok(())
    })();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}
