// fleet-waves — Gantt-style spawn-timeline view of the active openspec plan.
//
// Layout (no in-binary tab strip per PR #30 — tmux's status bar owns nav):
//   rows 0..=2:  header card — caption "WAVES", big "Wn · status · NN% of
//                plan", right-aligned action pills (Re-spawn / Spawn next
//                wave). Pills are visual only — wiring needs a dispatcher
//                this binary doesn't have yet.
//   rows 3..=n:  gantt grid wrapped in a rounded IOS_HAIRLINE block, one row
//                per Kahn topological wave. Each row: wave label, status
//                chip, cascade-positioned bar with the wave's first task
//                title, agent-initial badges on the right.
//
// Data: fleet-data::plan (newest plan.json under openspec/plans/*). Waves
// come from a Kahn topological sort of `Subtask.depends_on`; copied inline
// from fleet-plan-tree::waves() so this binary doesn't bump fleet-ui (locked
// by another agent during warp-borrow validation).
//
// Visual notes:
//   - Idle bars use IOS_CARD_BG (#2c2c30) — slightly above the background
//     so the row reads as a card, with IOS_FG text for readable contrast.
//     v1 used IOS_CHIP_BG (#36363a) + muted text and was hard to read on a
//     monitor.
//   - Done bars use IOS_GREEN with dark-on-bright bold text.
//   - Working bars draw a darker-tint base and overlay a brighter IOS_TINT
//     fill on the left for the wave's completed-task fraction; the bar is
//     bold-white throughout so the inner split is hue-only.
//   - Bar x-positions cascade by wave index. There is no per-wave timestamp
//     in plan.json today, so the mockup's "+Nm" duration axis is omitted
//     rather than rendered with a fake number.

use std::{io, path::PathBuf, time::Duration};

use fleet_data::plan::{self, Plan, Subtask};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind, CHIP_WIDTH},
    palette::*,
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Waves,
}

struct App {
    plan: Option<Plan>,
    props: Props,
}

impl Default for App {
    fn default() -> Self {
        let plan = std::env::var("FLEET_PLAN_REPO_ROOT")
            .ok()
            .or_else(|| Some("/home/deadpool/Documents/recodee".to_string()))
            .and_then(|root| plan::newest_plan(&PathBuf::from(root)).ok().flatten())
            .and_then(|p| plan::load(&p).ok());
        Self { plan, props: Props::default() }
    }
}

impl Component for App {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        render(frame, area, self);
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

impl AppComponent<Msg, NoUserEvent> for App {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Char('q'), .. })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => Some(Msg::Tick),
            _ => None,
        }
    }
}

// Kahn topological levels — assign each subtask to a wave such that all
// `depends_on` predecessors are in lower waves. Copied verbatim from
// fleet-plan-tree::waves(); kept inline to avoid promoting it into a shared
// crate while fleet-ui is locked by another agent.
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaveStatus {
    Done,
    Working,
    Idle,
}

fn wave_status(indices: &[u32], plan: &Plan) -> WaveStatus {
    let statuses: Vec<&str> = indices
        .iter()
        .filter_map(|i| plan.tasks.iter().find(|t| t.subtask_index == *i).map(|t| t.status.as_str()))
        .collect();
    if !statuses.is_empty() && statuses.iter().all(|s| *s == "completed") {
        WaveStatus::Done
    } else if statuses.iter().any(|s| *s == "claimed") {
        WaveStatus::Working
    } else {
        WaveStatus::Idle
    }
}

fn first_task_title<'a>(indices: &[u32], plan: &'a Plan) -> &'a str {
    indices
        .iter()
        .filter_map(|i| plan.tasks.iter().find(|t| t.subtask_index == *i))
        .next()
        .map(|t| t.title.as_str())
        .unwrap_or("—")
}

fn wave_progress(indices: &[u32], plan: &Plan) -> (u32, u32) {
    let mut done = 0u32;
    let mut total = 0u32;
    for i in indices {
        if let Some(t) = plan.tasks.iter().find(|t| t.subtask_index == *i) {
            total += 1;
            if t.status == "completed" {
                done += 1;
            }
        }
    }
    (done, total)
}

fn agents_in_wave(indices: &[u32], plan: &Plan) -> (Vec<char>, u32) {
    let mut seen: Vec<char> = Vec::new();
    let mut count = 0u32;
    for i in indices {
        if let Some(t) = plan.tasks.iter().find(|t| t.subtask_index == *i) {
            if let Some(agent) = t.claimed_by_agent.as_deref() {
                count += 1;
                if let Some(c) = agent.chars().next().map(|c| c.to_ascii_uppercase()) {
                    if !seen.contains(&c) && seen.len() < 2 {
                        seen.push(c);
                    }
                }
            }
        }
    }
    (seen, count)
}

fn active_wave_info(waves: &[Vec<u32>], plan: &Plan) -> (usize, &'static str, u32) {
    let total = plan.tasks.len() as u32;
    let done = plan.tasks.iter().filter(|t| t.status == "completed").count() as u32;
    let pct = if total == 0 { 0 } else { done * 100 / total };
    for (i, indices) in waves.iter().enumerate() {
        match wave_status(indices, plan) {
            WaveStatus::Done => continue,
            WaveStatus::Working => return (i + 1, "in flight", pct),
            WaveStatus::Idle => return (i + 1, "queued", pct),
        }
    }
    (waves.len().max(1), "done", pct)
}

struct WavePreview<'a> {
    wave_index: usize,
    status: WaveStatus,
    title: &'a str,
}

fn unfinished_waves<'a>(waves_v: &'a [Vec<u32>], plan: &'a Plan, limit: usize) -> Vec<WavePreview<'a>> {
    let mut out = Vec::new();
    for (wave_index, indices) in waves_v.iter().enumerate() {
        let status = wave_status(indices, plan);
        if matches!(status, WaveStatus::Done) {
            continue;
        }
        out.push(WavePreview {
            wave_index,
            status,
            title: first_task_title(indices, plan),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn clip(s: &str, max: u16) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if (chars.len() as u16) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return chars.into_iter().take(max as usize).collect();
    }
    let mut out: String = chars.into_iter().take((max - 1) as usize).collect();
    out.push('…');
    out
}

fn pad_visible(s: &str, width: u16) -> String {
    let cur = s.chars().count() as u16;
    if cur >= width {
        s.chars().take(width as usize).collect()
    } else {
        let mut out = s.to_string();
        for _ in cur..width {
            out.push(' ');
        }
        out
    }
}

// iOS-style pill: ◖ <label> ◗ with caps in the fill colour against IOS_BG.
fn pill_spans(label: &str, fill: Color, fg: Color) -> Vec<Span<'static>> {
    vec![
        Span::styled("◖", Style::default().fg(fill).bg(Color::Rgb(0, 0, 0))),
        Span::styled(
            label.to_string(),
            Style::default().fg(fg).bg(fill).add_modifier(Modifier::BOLD),
        ),
        Span::styled("◗", Style::default().fg(fill).bg(Color::Rgb(0, 0, 0))),
    ]
}

fn pill_visible_width(label: &str) -> u16 {
    (label.chars().count() as u16) + 2
}

fn render_header(frame: &mut Frame, area: Rect, plan: Option<&Plan>, waves_v: &[Vec<u32>]) {
    let (wave_n, status_word, pct) = match plan {
        Some(p) => active_wave_info(waves_v, p),
        None => (0, "no plan", 0),
    };

    // Row 0: "WAVES" caption — uppercase, muted, with a leading systemBlue tick.
    let cap_row = Rect { x: area.x + 1, y: area.y, width: area.width.saturating_sub(2), height: 1 };
    let cap_spans = vec![
        Span::styled("· ", Style::default().fg(IOS_TINT)),
        Span::styled("WAVES", Style::default().fg(IOS_FG_MUTED).add_modifier(Modifier::BOLD)),
    ];
    frame.render_widget(Paragraph::new(Line::from(cap_spans)), cap_row);

    // Row 1: big title + right-aligned action pills.
    let title_row = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: 1 };
    let title = if plan.is_some() && wave_n > 0 {
        format!("W{} · {} · {}% of plan", wave_n, status_word, pct)
    } else {
        "no plan loaded".to_string()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(title, Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD))),
        title_row,
    );

    let respawn = pill_spans(" ↻ Re-spawn ", IOS_CHIP_BG, IOS_FG);
    let spawn_next = pill_spans(" ▶ Spawn next wave ", IOS_TINT, IOS_FG);
    let pills_width = pill_visible_width(" ↻ Re-spawn ") + 1 + pill_visible_width(" ▶ Spawn next wave ");
    if title_row.width > pills_width {
        let pills_x = title_row.x + title_row.width - pills_width;
        let pills_area = Rect { x: pills_x, y: title_row.y, width: pills_width, height: 1 };
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(respawn);
        spans.push(Span::raw(" "));
        spans.extend(spawn_next);
        frame.render_widget(Paragraph::new(Line::from(spans)), pills_area);
    }
}

fn render_gantt(frame: &mut Frame, area: Rect, plan: &Plan, waves_v: &[Vec<u32>]) {
    if area.height == 0 || waves_v.is_empty() {
        return;
    }

    // Outer card — rounded hairline border, matches the mockup's enclosing frame.
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Per-row column budget inside the card:
    //   [W-label 4] [chip CHIP_WIDTH] [gap 2] [bar track variable] [gap 2] [avatars 6]
    const LABEL_W: u16 = 4;
    const GAP: u16 = 2;
    const AVATAR_W: u16 = 6;
    let fixed = LABEL_W + CHIP_WIDTH + GAP + GAP + AVATAR_W;
    if inner.width <= fixed + 4 {
        return;
    }
    let track_w = inner.width - fixed;

    // Cascade: bar = 45% of the track, step distributes the remaining 55%
    // across (total-1) intervals so W1 starts at the left edge and Wn lands
    // at the right edge of the track.
    let total = waves_v.len();
    let bar_w = ((track_w as f32) * 0.45).round().max(8.0) as u16;
    let max_offset = track_w.saturating_sub(bar_w);
    let step = if total > 1 {
        (max_offset as f32) / ((total - 1) as f32)
    } else {
        0.0
    };

    for (i, indices) in waves_v.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let label_area = Rect { x: inner.x, y, width: LABEL_W, height: 1 };
        let chip_area = Rect { x: inner.x + LABEL_W, y, width: CHIP_WIDTH, height: 1 };
        let track_x = inner.x + LABEL_W + CHIP_WIDTH + GAP;
        let avatar_area = Rect {
            x: inner.x + inner.width - AVATAR_W,
            y,
            width: AVATAR_W,
            height: 1,
        };

        // W-label — bright white bold so the row anchor reads at a glance.
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("W{}", i + 1),
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            )),
            label_area,
        );

        let ws = wave_status(indices, plan);
        let kind = match ws {
            WaveStatus::Done => ChipKind::Done,
            WaveStatus::Working => ChipKind::Working,
            WaveStatus::Idle => ChipKind::Idle,
        };
        frame.render_widget(Paragraph::new(Line::from(status_chip(kind))), chip_area);

        let offset = (step * i as f32).round() as u16;
        let bar_x = track_x + offset.min(max_offset);
        let bar_area = Rect { x: bar_x, y, width: bar_w, height: 1 };
        render_wave_bar(frame, bar_area, indices, plan, ws);

        let (initials, count) = agents_in_wave(indices, plan);
        let badge_spans: Vec<Span<'static>> = if count == 0 {
            vec![Span::styled("  —   ", Style::default().fg(IOS_FG_FAINT))]
        } else {
            let letters: String = initials.iter().collect();
            vec![
                Span::styled(
                    format!("◉{} ", if letters.is_empty() { "?".to_string() } else { letters }),
                    Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{}", count), Style::default().fg(IOS_FG)),
            ]
        };
        frame.render_widget(Paragraph::new(Line::from(badge_spans)), avatar_area);
    }
}

fn render_wave_bar(
    frame: &mut Frame,
    bar_area: Rect,
    indices: &[u32],
    plan: &Plan,
    ws: WaveStatus,
) {
    let title = first_task_title(indices, plan);
    let inner_w = bar_area.width.saturating_sub(2);
    let clipped = clip(title, inner_w);
    let padded = format!(" {} ", clipped);

    // Base bar colours — readable text on each surface, BOLD for the
    // active states (Done / Working) so they read as foreground items
    // against the muted Idle rows.
    let (bg, fg, bold) = match ws {
        WaveStatus::Done => (IOS_GREEN, Color::Rgb(12, 12, 14), true),
        WaveStatus::Working => (IOS_TINT_DARK, IOS_FG, true),
        WaveStatus::Idle => (IOS_CARD_BG, IOS_FG, false),
    };
    let mut style = Style::default().fg(fg).bg(bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    frame.render_widget(
        Paragraph::new(Span::styled(pad_visible(&padded, bar_area.width), style)),
        bar_area,
    );

    // Working wave: overlay a brighter IOS_TINT fill on the left portion
    // showing the wave's completed-task fraction. Two-pass render — second
    // pass overwrites the first within fill_area.
    if matches!(ws, WaveStatus::Working) {
        let (done, total) = wave_progress(indices, plan);
        let pct = if total == 0 { 0.0 } else { done as f32 / total as f32 };
        let fill_w = ((bar_area.width as f32) * pct).round() as u16;
        if fill_w > 0 {
            let fill_area = Rect {
                x: bar_area.x,
                y: bar_area.y,
                width: fill_w,
                height: 1,
            };
            let fill_text = pad_visible(&clip(&padded, fill_w), fill_w);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    fill_text,
                    Style::default()
                        .fg(IOS_FG)
                        .bg(IOS_TINT)
                        .add_modifier(Modifier::BOLD),
                )),
                fill_area,
            );
        }
    }
}

fn render_wave_preview_card(frame: &mut Frame, area: Rect, plan: &Plan, waves_v: &[Vec<u32>]) {
    if area.width < 24 || area.height < 4 {
        return;
    }

    let block = card(Some("UP NEXT"), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let previews = unfinished_waves(waves_v, plan, inner.height.saturating_sub(2) as usize);
    if previews.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  plan complete — every wave is done",
                Style::default().fg(IOS_FG_MUTED).add_modifier(Modifier::BOLD),
            ))),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
        );
        return;
    }

    let total = plan.tasks.len() as u32;
    let done = plan.tasks.iter().filter(|t| t.status == "completed").count() as u32;
    let remaining = total.saturating_sub(done);
    let lead = &previews[0];
    let lead_word = match lead.status {
        WaveStatus::Working => "in flight",
        WaveStatus::Idle => "queued",
        WaveStatus::Done => "done",
    };
    let summary = clip(
        &format!(
            "  W{} {} · {} unfinished waves · {} tasks remaining",
            lead.wave_index + 1,
            lead_word,
            previews.len(),
            remaining
        ),
        inner.width.saturating_sub(2),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            summary,
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ))),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
    );

    if inner.height > 2 {
        let divider = "─".repeat(inner.width as usize);
        frame.render_widget(
            Paragraph::new(Span::styled(divider, Style::default().fg(IOS_HAIRLINE))),
            Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
        );
    }

    let mut y = inner.y + 2;
    for preview in previews.iter().take(inner.height.saturating_sub(2) as usize) {
        if y >= inner.y + inner.height {
            break;
        }
        let kind = match preview.status {
            WaveStatus::Done => ChipKind::Done,
            WaveStatus::Working => ChipKind::Working,
            WaveStatus::Idle => ChipKind::Idle,
        };
        let prefix = format!("  W{} ", preview.wave_index + 1);
        let prefix_w = prefix.chars().count() as u16;
        let title_budget = inner.width.saturating_sub(prefix_w + CHIP_WIDTH + 1);
        let title = clip(preview.title, title_budget);
        let mut spans = vec![Span::styled(
            prefix,
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        )];
        spans.extend(status_chip(kind));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(title, Style::default().fg(IOS_FG)));
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect { x: inner.x, y, width: inner.width, height: 1 },
        );
        y += 1;
    }
}

fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 40 || area.height < 6 {
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header: caption + title + spacer
            Constraint::Min(0),    // gantt card
        ])
        .split(area);

    let waves_v: Vec<Vec<u32>> = match app.plan.as_ref() {
        Some(p) => waves(&p.tasks),
        None => Vec::new(),
    };

    render_header(frame, rows[0], app.plan.as_ref(), &waves_v);

    if let Some(plan) = app.plan.as_ref() {
        render_gantt(frame, rows[1], plan, &waves_v);
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "  no plan.json under openspec/plans/*/plan.json",
                Style::default().fg(IOS_FG_FAINT),
            )),
            rows[1],
        );
    }

    if let Some(plan) = app.plan.as_ref() {
        let gantt_h = rows[1].height;
        if gantt_h > 0 {
            let waves_rendered = waves_v.len() as u16;
            if gantt_h > waves_rendered {
                let footer_y = rows[1].y + waves_rendered;
                let footer_h = gantt_h - waves_rendered;
                render_wave_preview_card(
                    frame,
                    Rect {
                        x: rows[1].x,
                        y: footer_y,
                        width: rows[1].width,
                        height: footer_h,
                    },
                    plan,
                    &waves_v,
                );
            }
        }
    }
}

// ---------- Model (tuirealm M-V-U) ----------

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
                .tick_interval(Duration::from_millis(500)),
        );
        app.mount(
            Id::Waves,
            Box::new(App::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Waves)?;
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
            let _ = self.app.view(&Id::Waves, frame, area);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_plan() -> Plan {
        Plan {
            schema_version: 1,
            plan_slug: "demo".into(),
            title: "Demo".into(),
            problem: "demo".into(),
            acceptance_criteria: vec![],
            roles: vec![],
            created_at: None,
            updated_at: None,
            published: None,
            tasks: vec![
                Subtask {
                    subtask_index: 0,
                    title: "Wave zero".into(),
                    description: "done".into(),
                    file_scope: vec![],
                    depends_on: vec![],
                    capability_hint: None,
                    spec_row_id: None,
                    status: "completed".into(),
                    claimed_by_session_id: None,
                    claimed_by_agent: None,
                    completed_summary: None,
                },
                Subtask {
                    subtask_index: 1,
                    title: "Wave one".into(),
                    description: "claimed".into(),
                    file_scope: vec![],
                    depends_on: vec![0],
                    capability_hint: None,
                    spec_row_id: None,
                    status: "claimed".into(),
                    claimed_by_session_id: None,
                    claimed_by_agent: Some("codex-alpha".into()),
                    completed_summary: None,
                },
                Subtask {
                    subtask_index: 2,
                    title: "Wave two".into(),
                    description: "available".into(),
                    file_scope: vec![],
                    depends_on: vec![1],
                    capability_hint: None,
                    spec_row_id: None,
                    status: "available".into(),
                    claimed_by_session_id: None,
                    claimed_by_agent: None,
                    completed_summary: None,
                },
            ],
        }
    }

    #[test]
    fn unfinished_waves_skip_completed_rows() {
        let plan = demo_plan();
        let waves_v = waves(&plan.tasks);
        let previews = unfinished_waves(&waves_v, &plan, 3);
        assert_eq!(previews.len(), 2);
        assert_eq!(previews[0].wave_index, 1);
        assert!(matches!(previews[0].status, WaveStatus::Working));
        assert_eq!(previews[0].title, "Wave one");
        assert_eq!(previews[1].wave_index, 2);
        assert!(matches!(previews[1].status, WaveStatus::Idle));
        assert_eq!(previews[1].title, "Wave two");
    }
}
