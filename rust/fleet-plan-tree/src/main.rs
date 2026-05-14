// fleet-plan-tree — drop-in replacement for `plan-tree-anim.sh`. Phase-4
// minimal: in-binary tab strip + Kahn topological-levels sketch using
// fleet-data::plan loaders. The full PROPOSALS card grid lands later.

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
const ACTIVE_TAB: usize = 3;

struct App {
    tab_rects: Vec<(Rect, usize)>,
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

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }
    let rows = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0)]).split(area);
    render_tab_strip(frame, rows[0], ACTIVE_TAB, app);

    let title = app.plan.as_ref().map(|p| format!("PLAN TREE · {}", p.plan_slug)).unwrap_or_else(|| "PLAN TREE · no plan found".to_string());
    let header = card(Some(&title), false);
    frame.render_widget(header, rows[1]);

    let block = card(Some("WAVES W1 → Wn (Kahn topological levels via fleet-data::plan)"), false);
    let inner = block.inner(rows[2]);
    frame.render_widget(block, rows[2]);

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
