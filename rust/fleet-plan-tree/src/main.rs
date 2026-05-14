// fleet-plan-tree — drop-in replacement for `plan-tree-anim.sh`. Renders
// the Kahn topological-levels sketch using fleet-data::plan loaders. The
// pane runs inside the `codex-fleet` tmux session; the in-binary tab strip
// at the top of every dashboard supplies the canonical navigation.
//
// Plan selection (in priority order):
//   1. `$PLAN_TREE_ANIM_PIN_FILE` (default `/tmp/claude-viz/plan-tree-pin.txt`)
//      — absolute path to a plan.json. Written by `plan-tree-pin.sh`. Survives
//      respawns + bringups.
//   2. `$FLEET_PLAN_REPO_ROOT` or `$CODEX_FLEET_PLAN_REPO_ROOT` — repo root
//      whose newest plan we pick.
//   3. Hardcoded fallback to the codex-fleet repo (previously pointed at
//      recodee, which is why the plan tab kept showing recodee's plans even
//      after the user published codex-fleet plans).
//
// Live state: the plan.json on disk is updated by Colony as sub-tasks are
// claimed / completed (we verified by inspecting completed plans like
// warp-borrow vs in-flight ones like parallel-polish). The binary re-reads
// the plan file every 1s so the wave grid shows current `claimed` /
// `completed` status without the operator having to respawn the pane.

use std::{fs, io, path::PathBuf, time::{Duration, Instant}};

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

/// Default pin file. Written by `scripts/codex-fleet/plan-tree-pin.sh`.
const DEFAULT_PIN_FILE: &str = "/tmp/claude-viz/plan-tree-pin.txt";

/// Default fallback repo root when no env var is set. Points at codex-fleet
/// (the binary's home repo) — recodee is no longer the implicit target.
const DEFAULT_REPO_ROOT: &str = "/home/deadpool/Documents/codex-fleet";

/// How often to re-read the plan.json from disk to pick up Colony's
/// claimed/completed status updates.
const RELOAD_EVERY: Duration = Duration::from_millis(1000);

struct App {
    plan: Option<Plan>,
    plan_path: Option<PathBuf>,
    last_reload: Instant,
}

impl App {
    fn new() -> Self {
        let plan_path = resolve_plan_path();
        let plan = plan_path.as_ref().and_then(|p| plan::load(p).ok());
        Self {
            plan,
            plan_path,
            last_reload: Instant::now(),
        }
    }

    fn maybe_reload(&mut self) {
        if self.last_reload.elapsed() < RELOAD_EVERY {
            return;
        }
        self.last_reload = Instant::now();
        if let Some(p) = &self.plan_path {
            // Best-effort: a transient read error (Colony rewriting the file
            // atomically, fs hiccup) shouldn't lose the last good frame.
            if let Ok(plan) = plan::load(p) {
                self.plan = Some(plan);
            }
        }
    }
}

/// Resolve the plan.json this binary should render, in priority order.
fn resolve_plan_path() -> Option<PathBuf> {
    // 1. Pin file (operator-set, sticky across respawns).
    let pin_file = std::env::var("PLAN_TREE_ANIM_PIN_FILE")
        .unwrap_or_else(|_| DEFAULT_PIN_FILE.to_string());
    if let Ok(raw) = fs::read_to_string(&pin_file) {
        let pinned = raw.trim();
        if !pinned.is_empty() {
            let path = PathBuf::from(pinned);
            if path.exists() {
                return Some(path);
            }
        }
    }

    // 2. Repo root via env, falling back to the codex-fleet repo.
    let root = std::env::var("FLEET_PLAN_REPO_ROOT")
        .or_else(|_| std::env::var("CODEX_FLEET_PLAN_REPO_ROOT"))
        .unwrap_or_else(|_| DEFAULT_REPO_ROOT.to_string());
    plan::newest_plan(&PathBuf::from(root)).ok().flatten()
}

/// Kahn topological levels — assign each subtask to a wave such that all
/// `depends_on` predecessors are in lower waves.
fn waves(subtasks: &[Subtask]) -> Vec<Vec<u32>> {
    let mut level: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let by_idx: std::collections::HashMap<u32, &Subtask> =
        subtasks.iter().map(|s| (s.subtask_index, s)).collect();
    fn resolve(
        idx: u32,
        by: &std::collections::HashMap<u32, &Subtask>,
        memo: &mut std::collections::HashMap<u32, u32>,
    ) -> u32 {
        if let Some(&v) = memo.get(&idx) {
            return v;
        }
        let s = match by.get(&idx) {
            Some(s) => s,
            None => {
                memo.insert(idx, 0);
                return 0;
            }
        };
        let lvl = if s.depends_on.is_empty() {
            0
        } else {
            s.depends_on.iter().map(|d| resolve(*d, by, memo)).max().unwrap_or(0) + 1
        };
        memo.insert(idx, lvl);
        lvl
    }
    for s in subtasks {
        resolve(s.subtask_index, &by_idx, &mut level);
    }
    let max = level.values().copied().max().unwrap_or(0);
    let mut out: Vec<Vec<u32>> = (0..=max).map(|_| Vec::new()).collect();
    let mut idxs: Vec<u32> = level.keys().copied().collect();
    idxs.sort();
    for i in idxs {
        out[level[&i] as usize].push(i);
    }
    out
}

/// Tally the per-status counts across all sub-tasks for the header chip row.
fn rollup(plan: &Plan) -> (usize, usize, usize, usize) {
    let mut available = 0;
    let mut claimed = 0;
    let mut completed = 0;
    let mut blocked = 0;
    for t in &plan.tasks {
        match t.status.as_str() {
            "available" => available += 1,
            "claimed" => claimed += 1,
            "completed" => completed += 1,
            "blocked" => blocked += 1,
            _ => {}
        }
    }
    (available, claimed, completed, blocked)
}

/// Each pill is roughly ` ◖ <chip>    ◗ sub-N ` — measured wide to leave a
/// comfortable trailing gap so pills don't visually fuse on narrow panes.
const PILL_WIDTH: u16 = 22;

fn render(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 {
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // ── Header card with slug + live rollup chips ──────────────────────────
    let title = match app.plan.as_ref() {
        Some(p) => {
            let (a, c, d, b) = rollup(p);
            format!(
                "PLAN TREE · {} · {}/{} done · {} claimed · {} available{}",
                p.plan_slug,
                d,
                p.tasks.len(),
                c,
                a,
                if b > 0 { format!(" · {} blocked", b) } else { String::new() },
            )
        }
        None => "PLAN TREE · no plan found".to_string(),
    };
    let header = card(Some(&title), false);
    frame.render_widget(header, rows[0]);

    let block = card(
        Some("WAVES W1 → Wn (Kahn topological levels via fleet-data::plan)"),
        false,
    );
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);

    let Some(plan) = app.plan.as_ref() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no plan available — set CODEX_FLEET_PLAN_REPO_ROOT or pin via plan-tree-pin.sh",
                Style::default().fg(IOS_FG_MUTED),
            ))),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
        );
        return;
    };

    // Compute pills-per-row from the available width. Reserve ~6 cols for
    // the leading `  Wn · ` label.
    let label_width: u16 = 8;
    let pills_per_row: u16 = ((inner.width.saturating_sub(label_width)) / PILL_WIDTH).max(1);

    let mut y = inner.y;
    for (w, indices) in waves(&plan.tasks).into_iter().enumerate() {
        if indices.is_empty() {
            continue;
        }
        let wave_label = format!("  W{} · ", w + 1);
        let mut emitted_label = false;

        // Walk the wave in chunks of pills_per_row pills per visible row.
        for chunk in indices.chunks(pills_per_row as usize) {
            if y >= inner.y + inner.height {
                break;
            }
            let mut spans: Vec<Span> = Vec::new();
            // Only the first row of a wave shows the W{n} label; subsequent
            // wrap rows indent the same width so pills line up vertically.
            let prefix = if !emitted_label {
                emitted_label = true;
                wave_label.clone()
            } else {
                " ".repeat(label_width as usize)
            };
            spans.push(Span::styled(
                prefix,
                Style::default().fg(IOS_FG_MUTED).add_modifier(Modifier::BOLD),
            ));
            for idx in chunk {
                let s = match plan.tasks.iter().find(|t| t.subtask_index == *idx) {
                    Some(s) => s,
                    None => continue,
                };
                let kind = match s.status.as_str() {
                    "completed" => ChipKind::Done,
                    "claimed" => ChipKind::Working,
                    "blocked" => ChipKind::Blocked,
                    _ => ChipKind::Idle,
                };
                spans.extend(status_chip(kind));
                spans.push(Span::styled(
                    format!(" sub-{} ", s.subtask_index),
                    Style::default().fg(IOS_FG),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
            y += 1;
        }
        // One blank row between waves for visual breathing room.
        if y < inner.y + inner.height {
            y += 1;
        }
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new();
    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &app))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(k) = event::read()? {
                    if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                        break;
                    }
                }
            }
            // Re-read plan.json so Colony's claim / complete updates surface
            // without a respawn. Cheap (<1ms) — the file is a few KB.
            app.maybe_reload();
        }
        Ok(())
    })();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}
