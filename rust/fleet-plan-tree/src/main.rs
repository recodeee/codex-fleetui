// fleet-plan-tree — drop-in replacement for `plan-tree-anim.sh`. Renders
// the Kahn topological-levels sketch using fleet-data::plan loaders. The
// full PROPOSALS card grid lands later. The pane runs inside the
// `codex-fleet` tmux session, whose status bar (`style-tabs.sh`) supplies
// the canonical tab strip; this binary therefore does not draw one of its
// own.

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

struct App {
    plan: Option<Plan>,
}

impl App {
    fn new() -> Self {
        let plan = std::env::var("FLEET_PLAN_REPO_ROOT")
            .ok()
            .or_else(|| std::env::var("CODEX_FLEET_PLAN_REPO_ROOT").ok())
            .or_else(|| Some("/home/deadpool/Documents/recodee".to_string()))
            .and_then(|root| plan::newest_plan(&PathBuf::from(root)).ok().flatten())
            .and_then(|p| plan::load(&p).ok());
        Self { plan }
    }
}

/// Kahn topological levels — assign each subtask to a wave such that all
/// `depends_on` predecessors are in lower waves. Same algorithm as the
/// Python sketch in plan-tree-anim.sh; lifted into Rust here.
fn waves(subtasks: &[Subtask]) -> Vec<Vec<u32>> {
    let mut level: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let by_idx: std::collections::HashMap<u32, &Subtask> = subtasks.iter().map(|s| (s.subtask_index, s)).collect();
    fn resolve(idx: u32, by: &std::collections::HashMap<u32, &Subtask>, memo: &mut std::collections::HashMap<u32, u32>) -> u32 {
        if let Some(&v) = memo.get(&idx) { return v; }
        let s = match by.get(&idx) { Some(s) => s, None => { memo.insert(idx, 0); return 0; } };
        let lvl = if s.depends_on.is_empty() { 0 } else {
            s.depends_on.iter().map(|d| resolve(*d, by, memo)).max().unwrap_or(0) + 1
        };
        memo.insert(idx, lvl);
        lvl
    }
    for s in subtasks { resolve(s.subtask_index, &by_idx, &mut level); }
    let max = level.values().copied().max().unwrap_or(0);
    let mut out: Vec<Vec<u32>> = (0..=max).map(|_| Vec::new()).collect();
    let mut idxs: Vec<u32> = level.keys().copied().collect();
    idxs.sort();
    for i in idxs { out[level[&i] as usize].push(i); }
    out
}

fn render(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

    let title = app.plan.as_ref().map(|p| format!("PLAN TREE · {}", p.plan_slug)).unwrap_or_else(|| "PLAN TREE · no plan found".to_string());
    let header = card(Some(&title), false);
    frame.render_widget(header, rows[0]);

    let block = card(Some("WAVES W1 → Wn (Kahn topological levels via fleet-data::plan)"), false);
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);

    let Some(plan) = app.plan.as_ref() else {
        frame.render_widget(Paragraph::new(Line::from(Span::styled("  no plan available — set FLEET_PLAN_REPO_ROOT", Style::default().fg(IOS_FG_MUTED)))), Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 });
        return;
    };

    for (w, indices) in waves(&plan.tasks).into_iter().enumerate() {
        let y = inner.y + w as u16;
        if y + 1 > inner.y + inner.height { break; }
        let mut spans = vec![
            Span::styled(format!("  W{} · ", w + 1), Style::default().fg(IOS_FG_MUTED).add_modifier(Modifier::BOLD)),
        ];
        for idx in indices {
            let s = plan.tasks.iter().find(|t| t.subtask_index == idx).unwrap();
            let kind = match s.status.as_str() {
                "completed" => ChipKind::Done,
                "claimed" => ChipKind::Working,
                "blocked" => ChipKind::Blocked,
                _ => ChipKind::Idle,
            };
            spans.extend(status_chip(kind));
            spans.push(Span::styled(format!(" sub-{} ", s.subtask_index), Style::default().fg(IOS_FG)));
        }
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
