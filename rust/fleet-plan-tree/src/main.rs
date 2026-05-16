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

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use fleet_data::plan::{self, Plan, Subtask};
use fleet_data::toposort::waves;
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
    spotlight_overlay::{
        shared_spotlight_filter, Spotlight, SpotlightState, SHARED_SPOTLIGHT_ITEMS,
    },
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
use tuirealm::ratatui::widgets::{Block, Paragraph};
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

const DEFAULT_PIN_FILE: &str = "/tmp/claude-viz/plan-tree-pin.txt";
const DEFAULT_REPO_ROOT: &str = "/home/deadpool/Documents/codex-fleetui";
const RELOAD_EVERY: Duration = Duration::from_millis(1000);
/// Recent-merges section refreshes less often than plan state — a git
/// invocation per second is fine but wasteful. 5s feels live without
/// burning CPU.
const RELOAD_GIT_EVERY: Duration = Duration::from_secs(5);
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Overlay {
    #[default]
    None,
    Spotlight,
}

// ---------- The PlanView component ----------

struct PlanView {
    plan: Option<Plan>,
    plan_path: Option<PathBuf>,
    last_reload: Instant,
    /// Last 5 lines of `git log --oneline origin/main`. Rendered in the
    /// RECENT MERGES footer card so the operator sees PR landings without
    /// leaving the dashboard.
    recent_prs: Vec<String>,
    last_git_reload: Instant,
    overlay: Overlay,
    spotlight: Spotlight<'static>,
    spotlight_state: SpotlightState,
    props: Props,
}

impl Default for PlanView {
    fn default() -> Self {
        let plan_path = resolve_plan_path();
        let plan = plan_path.as_ref().and_then(|p| plan::load(p).ok());
        let mut view = Self {
            plan,
            plan_path,
            last_reload: Instant::now(),
            recent_prs: Vec::new(),
            // Force the first git refresh on the next tick by backdating.
            last_git_reload: Instant::now()
                .checked_sub(RELOAD_GIT_EVERY)
                .unwrap_or_else(Instant::now),
            overlay: Overlay::None,
            spotlight: Spotlight::new(SHARED_SPOTLIGHT_ITEMS.to_vec()),
            spotlight_state: SpotlightState::default(),
            props: Props::default(),
        };
        view.refresh_git();
        view
    }
}

impl PlanView {
    fn open_spotlight(&mut self) {
        self.overlay = Overlay::Spotlight;
        self.spotlight_state.query.clear();
        self.spotlight_state.selected = 0;
    }

    fn close_spotlight(&mut self) {
        self.overlay = Overlay::None;
        self.spotlight_state.query.clear();
        self.spotlight_state.selected = 0;
    }

    fn maybe_reload(&mut self) {
        if self.last_reload.elapsed() >= RELOAD_EVERY {
            self.last_reload = Instant::now();
            if let Some(p) = &self.plan_path {
                if let Ok(plan) = plan::load(p) {
                    self.plan = Some(plan);
                }
            }
        }
        if self.last_git_reload.elapsed() >= RELOAD_GIT_EVERY {
            self.last_git_reload = Instant::now();
            self.refresh_git();
        }
        self.spotlight_state.tick = self.spotlight_state.tick.wrapping_add(1);
    }

    /// Shell `git log --oneline -5 origin/main` against the active repo
    /// root. Cheap (<50ms on this repo) and fenced to once per ~5s by
    /// [`Self::maybe_reload`], so the per-render cost stays near zero.
    fn refresh_git(&mut self) {
        let repo = std::env::var("CODEX_FLEET_PLAN_REPO_ROOT")
            .or_else(|_| std::env::var("FLEET_PLAN_REPO_ROOT"))
            .unwrap_or_else(|_| DEFAULT_REPO_ROOT.to_string());
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["log", "--oneline", "-5", "origin/main"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                self.recent_prs = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }
}

/// Strip the `codex-` prefix common to every fleet agent name so the
/// ACTIVE NOW row stays narrow. `codex-zeus-magnolia` → `zeus-magnolia`.
fn short_agent(agent: &str) -> &str {
    agent.strip_prefix("codex-").unwrap_or(agent)
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut chars = 0;
    for c in s.chars() {
        if chars + 1 > n {
            out.push('…');
            break;
        }
        out.push(c);
        chars += 1;
    }
    out
}

impl Component for PlanView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 8 {
            return;
        }

        // Vertical layout: header(3) + body(rest) + recent-merges(8).
        // body is then split into ACTIVE NOW + WAVES, sized by how many
        // sub-tasks are currently `claimed` so the active list always
        // shows everyone but never starves the wave grid.
        let recent_h: u16 = if self.recent_prs.is_empty() { 0 } else { 8 };
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(recent_h),
            ])
            .split(area);

        // ── Header card with slug + live rollup chips ──────────────────────
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
                    if b > 0 {
                        format!(" · {} blocked", b)
                    } else {
                        String::new()
                    },
                )
            }
            None => "PLAN TREE · no plan found".to_string(),
        };
        frame.render_widget(card(Some(&title), false), outer[0]);

        if let Some(plan) = self.plan.as_ref() {
            // ── Body: ACTIVE NOW (claimed list) + WAVES (Kahn grid) ────────────
            let claimed: Vec<&Subtask> = plan
                .tasks
                .iter()
                .filter(|t| t.status == "claimed")
                .collect();
            // 2 rows of chrome plus content strips and hairline separators.
            let active_rows = claimed.len().max(1) as u16;
            let active_h: u16 = (active_rows.saturating_mul(2) + 2).clamp(4, 14);
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(active_h), Constraint::Min(0)])
                .split(outer[1]);

            render_active_now(frame, body[0], &claimed);
            render_waves(frame, body[1], plan);
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no plan available — set CODEX_FLEET_PLAN_REPO_ROOT or pin via plan-tree-pin.sh",
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect { x: outer[1].x, y: outer[1].y, width: outer[1].width, height: 1 },
            );
        }

        // ── Footer: recent merges (latest 5 PRs on origin/main) ────────────
        render_recent_merges(frame, outer[2], &self.recent_prs);

        if self.overlay == Overlay::Spotlight {
            self.spotlight.render(frame, area, &self.spotlight_state);
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
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. })
                if self.overlay == Overlay::Spotlight =>
            {
                self.close_spotlight();
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) if self.overlay == Overlay::Spotlight => {
                self.close_spotlight();
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) if self.overlay == Overlay::Spotlight => {
                self.spotlight_state.query.pop();
                self.spotlight_state.selected = 0;
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
                if self.overlay == Overlay::Spotlight =>
            {
                self.spotlight_state.selected = self.spotlight_state.selected.saturating_sub(1);
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) if self.overlay == Overlay::Spotlight => {
                let max = shared_spotlight_filter(&self.spotlight_state.query)
                    .len()
                    .saturating_sub(1);
                self.spotlight_state.selected = (self.spotlight_state.selected + 1).min(max);
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(c), ..
            }) if self.overlay == Overlay::Spotlight && !c.is_control() => {
                self.spotlight_state.query.push(*c);
                self.spotlight_state.selected = 0;
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('/'),
                ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('?'),
                ..
            }) => {
                self.open_spotlight();
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => {
                self.maybe_reload();
                Some(Msg::Tick)
            }
            _ => None,
        }
    }
}

// ---------- Section renderers ----------

fn render_row_surface(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(IOS_BG_GLASS)),
        area,
    );
}

fn render_row_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let divider = if area.width > 2 {
        format!(" {}", "─".repeat(area.width.saturating_sub(2) as usize))
    } else {
        "─".repeat(area.width as usize)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(IOS_HAIRLINE).bg(IOS_BG_GLASS),
        )))
        .style(Style::default().bg(IOS_BG_GLASS)),
        area,
    );
}

fn render_glass_row(frame: &mut Frame, area: Rect, line: Line<'static>) {
    render_row_surface(frame, area);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(IOS_BG_GLASS)),
        area,
    );
}

/// ACTIVE NOW card — one row per `status=claimed` sub-task.
/// Format: ` ● <agent-name>  sub-<N>  <truncated title>`
/// When nothing is claimed, show a placeholder so the layout stays stable.
fn render_active_now(frame: &mut Frame, area: Rect, claimed: &[&Subtask]) {
    let block = card(Some("ACTIVE NOW · agents on Colony tasks"), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if claimed.is_empty() {
        let row = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        render_glass_row(
            frame,
            row,
            Line::from(Span::styled(
                "  no active claims — workers polling for work",
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
            )),
        );
        if inner.height > 1 {
            render_row_divider(
                frame,
                Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: 1,
                },
            );
        }
        return;
    }
    let row_step = if inner.height >= claimed.len() as u16 * 2 {
        2
    } else {
        1
    };
    for (i, s) in claimed.iter().enumerate() {
        let y = inner.y + i as u16 * row_step;
        if y >= inner.y + inner.height {
            break;
        }
        let row = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        };
        render_row_surface(frame, row);
        let agent = s
            .claimed_by_agent
            .as_deref()
            .map(short_agent)
            .unwrap_or("(unknown)");
        // Agent column is fixed-width 20 so titles align vertically. The
        // title gets whatever remains after agent + sub-N + chrome.
        let agent_col_w: usize = 22;
        let sub_col_w: usize = 8;
        let chrome_w: usize = 4; // ` ● ` + trailing spacer
        let title_budget =
            (inner.width as usize).saturating_sub(agent_col_w + sub_col_w + chrome_w);
        let title = truncate_chars(&s.title, title_budget);
        let agent_str = format!("{:<width$}", agent, width = agent_col_w);
        let sub_str = format!("sub-{:<3}", s.subtask_index);
        let spans = vec![
            Span::styled(
                " ● ",
                Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                agent_str,
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(sub_str, Style::default().fg(IOS_FG_MUTED)),
            Span::raw("  "),
            Span::styled(title, Style::default().fg(IOS_FG)),
        ];
        render_glass_row(frame, row, Line::from(spans));
        if row_step == 2 && y + 1 < inner.y + inner.height {
            render_row_divider(
                frame,
                Rect {
                    x: inner.x,
                    y: y + 1,
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }
}

fn render_waves(frame: &mut Frame, area: Rect, plan: &Plan) {
    let block = card(
        Some("WAVES W1 → Wn (Kahn topological levels via fleet-data::plan)"),
        false,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let label_width: u16 = 8;
    let pills_per_row: u16 = ((inner.width.saturating_sub(label_width)) / PILL_WIDTH).max(1);
    let wave_levels = waves(&plan.tasks);
    let content_rows: u16 = wave_levels
        .iter()
        .map(|indices| {
            if indices.is_empty() {
                0
            } else {
                indices.len().div_ceil(pills_per_row as usize) as u16
            }
        })
        .sum();
    let use_dividers = content_rows.saturating_mul(2) <= inner.height;
    let mut y = inner.y;
    for (w, indices) in wave_levels.into_iter().enumerate() {
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
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .add_modifier(Modifier::BOLD),
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
            let row = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            render_glass_row(frame, row, Line::from(spans));
            if use_dividers && y + 1 < inner.y + inner.height {
                render_row_divider(
                    frame,
                    Rect {
                        x: inner.x,
                        y: y + 1,
                        width: inner.width,
                        height: 1,
                    },
                );
                y += 2;
            } else {
                y += 1;
            }
        }
        if y < inner.y + inner.height {
            render_row_divider(
                frame,
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
            );
            y += 1;
        }
    }

    let remaining = inner.y + inner.height - y;
    if remaining >= 4 {
        render_agent_assignment_map(
            frame,
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: remaining,
            },
            plan,
        );
    }
}

fn render_agent_assignment_map(frame: &mut Frame, area: Rect, plan: &Plan) {
    if area.width == 0 || area.height < 2 {
        return;
    }

    let divider_width = area.width.saturating_sub(4) as usize;
    render_row_surface(
        frame,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 2.min(area.height),
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", "─".repeat(divider_width)),
            Style::default().fg(IOS_HAIRLINE).bg(IOS_BG_GLASS),
        ))),
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " CLAIM MAP ",
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_BG_GLASS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " grouped by claimed agent · live plan.json",
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
            ),
        ])),
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        },
    );

    let rows = assignment_lines(plan, area.width, area.height.saturating_sub(2));
    for (i, line) in rows.into_iter().enumerate() {
        let y = area.y + 2 + i as u16;
        if y >= area.y + area.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(line),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
        );
    }
}

fn assignment_lines(plan: &Plan, width: u16, max_rows: u16) -> Vec<Line<'static>> {
    if max_rows == 0 {
        return Vec::new();
    }

    let groups = claimed_agent_groups(plan);
    if groups.is_empty() {
        let (available, _claimed, completed, blocked) = rollup(plan);
        return vec![Line::from(vec![
            Span::raw("  "),
            Span::styled("○ ", Style::default().fg(IOS_FG_MUTED)),
            Span::styled("no claimed agents", Style::default().fg(IOS_FG)),
            Span::styled(
                format!(
                    " · {} available · {} done · {} blocked",
                    available, completed, blocked
                ),
                Style::default().fg(IOS_FG_MUTED),
            ),
        ])];
    }

    let mut assignments: Vec<(String, bool, &Subtask)> = Vec::new();
    for (agent, tasks) in &groups {
        for (i, task) in tasks.iter().enumerate() {
            assignments.push((agent.clone(), i == 0, *task));
        }
    }

    let max_rows = max_rows as usize;
    let visible_rows = if assignments.len() > max_rows {
        max_rows.saturating_sub(1)
    } else {
        max_rows
    };
    let hidden = assignments.len().saturating_sub(visible_rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (agent, first_for_agent, task) in assignments.into_iter().take(visible_rows) {
        let title_budget = (width as usize).saturating_sub(38);
        let title = truncate_chars(&task.title, title_budget);
        let agent_label = if first_for_agent {
            truncate_chars(&agent, 16)
        } else {
            String::new()
        };
        lines.push(Line::from(vec![
            Span::styled(
                if first_for_agent { "  ● " } else { "    " },
                Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {:<16} ", agent_label),
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_BG_GLASS)
                    .add_modifier(if first_for_agent {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::raw("  "),
            Span::styled(
                format!(" sub-{} ", task.subtask_index),
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(title, Style::default().fg(IOS_FG)),
        ]));
    }

    if hidden > 0 {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("+{} more claimed subtasks", hidden),
                Style::default().fg(IOS_FG_MUTED),
            ),
        ]));
    }

    lines
}

fn claimed_agent_groups(plan: &Plan) -> BTreeMap<String, Vec<&Subtask>> {
    let mut groups: BTreeMap<String, Vec<&Subtask>> = BTreeMap::new();
    for task in plan.tasks.iter().filter(|t| t.status == "claimed") {
        let agent = task
            .claimed_by_agent
            .as_deref()
            .map(short_agent)
            .unwrap_or("(unknown)")
            .to_string();
        groups.entry(agent).or_default().push(task);
    }
    groups
}

/// RECENT MERGES card — last 5 `git log --oneline origin/main` rows.
/// Dim sha + bright subject so the operator scans the merge column.
fn render_recent_merges(frame: &mut Frame, area: Rect, prs: &[String]) {
    if area.height == 0 {
        return;
    }
    let block = card(
        Some("RECENT MERGES · git log --oneline -5 origin/main"),
        false,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if prs.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (git unreachable from this pane)",
                Style::default().fg(IOS_FG_MUTED),
            ))),
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
        );
        return;
    }
    for (i, line) in prs.iter().take(inner.height as usize).enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let (sha, subject) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let max_subj = (inner.width as usize).saturating_sub(sha.chars().count() + 4);
        let subject = truncate_chars(subject, max_subj);
        let spans = vec![
            Span::raw("  "),
            Span::styled(sha.to_string(), Style::default().fg(IOS_FG_MUTED)),
            Span::raw("  "),
            Span::styled(subject, Style::default().fg(IOS_FG)),
        ];
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

// ---------- Helpers (path resolution + plan math) ----------

fn resolve_plan_path() -> Option<PathBuf> {
    let pin_file =
        std::env::var("PLAN_TREE_ANIM_PIN_FILE").unwrap_or_else(|_| DEFAULT_PIN_FILE.to_string());
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

#[cfg(test)]
mod claim_map_tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn test_plan(tasks: Vec<Subtask>) -> Plan {
        Plan {
            schema_version: 1,
            plan_slug: "creative-fill-test".to_string(),
            title: "creative fill test".to_string(),
            problem: "test".to_string(),
            acceptance_criteria: Vec::new(),
            roles: Vec::new(),
            tasks,
            created_at: None,
            updated_at: None,
            published: None,
        }
    }

    fn subtask(idx: u32, title: &str, status: &str, agent: Option<&str>) -> Subtask {
        Subtask {
            subtask_index: idx,
            title: title.to_string(),
            description: String::new(),
            file_scope: Vec::new(),
            depends_on: Vec::new(),
            capability_hint: None,
            spec_row_id: None,
            status: status.to_string(),
            claimed_by_session_id: agent.map(|_| "session".to_string()),
            claimed_by_agent: agent.map(str::to_string),
            completed_summary: None,
        }
    }

    fn plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn assignment_lines_group_claimed_tasks_by_agent() {
        let plan = test_plan(vec![
            subtask(0, "completed bootstrap", "completed", None),
            subtask(3, "zeta owned task", "claimed", Some("codex-zeta")),
            subtask(1, "alpha owned task", "claimed", Some("codex-alpha")),
            subtask(2, "available task", "available", None),
            subtask(4, "alpha second task", "claimed", Some("codex-alpha")),
        ]);

        let rows = assignment_lines(&plan, 96, 8);
        let rendered = rows.iter().map(plain).collect::<Vec<_>>().join("\n");

        let alpha = rendered.find("alpha").expect("alpha group rendered");
        let zeta = rendered.find("zeta").expect("zeta group rendered");
        assert!(
            alpha < zeta,
            "BTreeMap keeps agent groups stable alphabetically"
        );
        assert!(rendered.contains("sub-1"));
        assert!(rendered.contains("sub-4"));
        assert!(rendered.contains("sub-3"));
        assert!(!rendered.contains("completed bootstrap"));
        assert!(!rendered.contains("available task"));
    }

    #[test]
    fn assignment_lines_use_real_rollup_when_no_claims() {
        let plan = test_plan(vec![
            subtask(0, "done", "completed", None),
            subtask(1, "ready", "available", None),
            subtask(2, "blocked", "blocked", None),
        ]);

        let rows = assignment_lines(&plan, 80, 4);
        let rendered = plain(&rows[0]);

        assert!(rendered.contains("no claimed agents"));
        assert!(rendered.contains("1 available"));
        assert!(rendered.contains("1 done"));
        assert!(rendered.contains("1 blocked"));
    }

    #[test]
    fn active_now_rows_use_glass_surface_and_hairline() {
        let tasks = [subtask(
            7,
            "owned implementation",
            "claimed",
            Some("codex-alpha"),
        )];
        let claimed = tasks.iter().collect::<Vec<_>>();
        let mut terminal = Terminal::new(TestBackend::new(80, 6)).unwrap();

        terminal
            .draw(|frame| render_active_now(frame, frame.area(), &claimed))
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].bg, IOS_BG_GLASS);
        assert_eq!(buffer[(2, 2)].fg, IOS_HAIRLINE);
        assert_eq!(buffer[(2, 2)].bg, IOS_BG_GLASS);
    }

    #[test]
    fn waves_render_includes_claim_map_section() {
        let plan = test_plan(vec![
            subtask(0, "bootstrap", "completed", None),
            subtask(1, "owned implementation", "claimed", Some("codex-alpha")),
        ]);
        let mut terminal = Terminal::new(TestBackend::new(100, 22)).unwrap();

        terminal
            .draw(|frame| render_waves(frame, frame.area(), &plan))
            .unwrap();

        let rendered = format!("{}", terminal.backend());
        assert!(rendered.contains("CLAIM MAP"));
        assert!(rendered.contains("alpha"));
        assert!(rendered.contains("owned implementation"));
    }

    #[test]
    fn wave_rows_use_glass_surface_and_hairline() {
        let plan = test_plan(vec![
            subtask(0, "bootstrap", "completed", None),
            subtask(1, "owned implementation", "claimed", Some("codex-alpha")),
        ]);
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();

        terminal
            .draw(|frame| render_waves(frame, frame.area(), &plan))
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].bg, IOS_BG_GLASS);
        assert_eq!(buffer[(2, 2)].fg, IOS_HAIRLINE);
        assert_eq!(buffer[(2, 2)].bg, IOS_BG_GLASS);
    }

    #[test]
    fn wave_level_spacing_uses_glass_hairline_not_dead_air() {
        let mut dependent = subtask(
            1,
            "dependent implementation",
            "claimed",
            Some("codex-alpha"),
        );
        dependent.depends_on = vec![0];
        let plan = test_plan(vec![subtask(0, "bootstrap", "completed", None), dependent]);
        let mut terminal = Terminal::new(TestBackend::new(100, 14)).unwrap();

        terminal
            .draw(|frame| render_waves(frame, frame.area(), &plan))
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 3)].fg, IOS_HAIRLINE);
        assert_eq!(buffer[(2, 3)].bg, IOS_BG_GLASS);
    }
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
        Ok(Self {
            app,
            terminal,
            quit: false,
            redraw: true,
        })
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
            if let Ok(messages) = model
                .app
                .tick(PollStrategy::Once(Duration::from_millis(100)))
            {
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

#[cfg(test)]
mod spotlight_tests {
    use super::*;

    #[test]
    fn spotlight_catalogue_matches_watcher_order() {
        let hits = shared_spotlight_filter("");
        assert_eq!(hits.len(), SHARED_SPOTLIGHT_ITEMS.len());
        assert_eq!(hits.first().unwrap().title, "Horizontal split");
        assert_eq!(hits.last().unwrap().title, "Switch worktree…");
    }

    #[test]
    fn spotlight_open_and_close_reset_state() {
        let mut view = PlanView::default();

        view.spotlight_state.query = "zoom".into();
        view.spotlight_state.selected = 2;
        view.open_spotlight();
        assert_eq!(view.overlay, Overlay::Spotlight);
        assert!(view.spotlight_state.query.is_empty());
        assert_eq!(view.spotlight_state.selected, 0);

        view.spotlight_state.query = "split".into();
        view.spotlight_state.selected = 1;
        view.close_spotlight();
        assert_eq!(view.overlay, Overlay::None);
        assert!(view.spotlight_state.query.is_empty());
        assert_eq!(view.spotlight_state.selected, 0);
    }
}
