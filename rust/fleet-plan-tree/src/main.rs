// fleet-plan-tree — tuirealm port. Renders the Kahn topological-levels
// wave grid for the active openspec plan inside a tuirealm
// `AppComponent`. Third binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (#50) and fleet-state (#52).
//
// Plan selection (unchanged, in priority order):
//   1. `$PLAN_TREE_ANIM_PIN_FILE` (default `/tmp/claude-viz/plan-tree-pin.txt`)
//      — absolute path to a plan.json. Written by `plan-tree-pin.sh`.
//   2. `$FLEET_PLAN_REPO_ROOT` or `$CODEX_FLEET_PLAN_REPO_ROOT` — repo root
//      whose newest plan we pick.
//   3. Hardcoded fallback to the codex-fleet repo.
//
// Live state: the plan.json on disk is updated by Colony as sub-tasks are
// claimed / completed. The component re-reads it on every tuirealm Tick
// (250ms) — the live `claimed` / `completed` status surfaces without a
// pane respawn.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use fleet_data::plan::{self, Plan, Subtask};
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

const DEFAULT_PIN_FILE: &str = "/tmp/claude-viz/plan-tree-pin.txt";
const DEFAULT_REPO_ROOT: &str = "/home/deadpool/Documents/codex-fleet";
const RELOAD_EVERY: Duration = Duration::from_millis(1000);
const PILL_WIDTH: u16 = 22;

// ---------- Messages and component IDs ----------

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Plan,
}

// ---------- The PlanView component ----------

struct PlanView {
    plan: Option<Plan>,
    plan_path: Option<PathBuf>,
    last_reload: Instant,
    props: Props,
}

impl Default for PlanView {
    fn default() -> Self {
        let plan_path = resolve_plan_path();
        let plan = plan_path.as_ref().and_then(|p| plan::load(p).ok());
        Self { plan, plan_path, last_reload: Instant::now(), props: Props::default() }
    }
}

impl PlanView {
    fn maybe_reload(&mut self) {
        if self.last_reload.elapsed() < RELOAD_EVERY {
            return;
        }
        self.last_reload = Instant::now();
        if let Some(p) = &self.plan_path {
            if let Ok(plan) = plan::load(p) {
                self.plan = Some(plan);
            }
        }
    }
}

impl Component for PlanView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 8 {
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Header card with slug + live rollup chips.
        let title = match self.plan.as_ref() {
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

        let Some(plan) = self.plan.as_ref() else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no plan available — set CODEX_FLEET_PLAN_REPO_ROOT or pin via plan-tree-pin.sh",
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
            );
            return;
        };

        let label_width: u16 = 8;
        let pills_per_row: u16 = ((inner.width.saturating_sub(label_width)) / PILL_WIDTH).max(1);

        let mut y = inner.y;
        for (w, indices) in waves(&plan.tasks).into_iter().enumerate() {
            if indices.is_empty() {
                continue;
            }
            let wave_label = format!("  W{} · ", w + 1);
            let mut emitted_label = false;

            for chunk in indices.chunks(pills_per_row as usize) {
                if y >= inner.y + inner.height {
                    break;
                }
                let mut spans: Vec<Span> = Vec::new();
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
            if y < inner.y + inner.height {
                y += 1;
            }
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

impl AppComponent<Msg, NoUserEvent> for PlanView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Char('q'), .. })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => {
                self.maybe_reload();
                Some(Msg::Tick)
            }
            _ => None,
        }
    }
}

// ---------- Helpers (unchanged) ----------

fn resolve_plan_path() -> Option<PathBuf> {
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
    let root = std::env::var("FLEET_PLAN_REPO_ROOT")
        .or_else(|_| std::env::var("CODEX_FLEET_PLAN_REPO_ROOT"))
        .unwrap_or_else(|_| DEFAULT_REPO_ROOT.to_string());
    plan::newest_plan(&PathBuf::from(root)).ok().flatten()
}

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
                .tick_interval(Duration::from_millis(250)),
        );
        app.mount(
            Id::Plan,
            Box::new(PlanView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Plan)?;
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
            let _ = self.app.view(&Id::Plan, frame, area);
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
