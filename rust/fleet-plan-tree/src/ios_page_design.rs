use fleet_data::plan::Plan;
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::time::{Duration, Instant};

const LIVE_FRESH_SECS: u64 = 2;
const LIVE_STALE_SECS: u64 = 10;
const MAX_ACTIVE_ROWS: usize = 5;
const MAX_MERGE_ROWS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Working,
    Idle,
    Done,
}

impl EntryState {
    fn chip_kind(self) -> ChipKind {
        match self {
            EntryState::Working => ChipKind::Working,
            EntryState::Idle => ChipKind::Idle,
            EntryState::Done => ChipKind::Done,
        }
    }

    fn accent(self) -> ratatui::style::Color {
        match self {
            EntryState::Working => IOS_TINT,
            EntryState::Idle => IOS_HAIRLINE_STRONG,
            EntryState::Done => IOS_GREEN,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveNowEntry {
    pub agent: String,
    pub task: String,
    pub state: EntryState,
}

impl ActiveNowEntry {
    pub fn new(agent: impl Into<String>, task: impl Into<String>, state: EntryState) -> Self {
        Self {
            agent: agent.into(),
            task: task.into(),
            state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaveSummary {
    pub label: String,
    pub detail: String,
    pub state: EntryState,
}

impl WaveSummary {
    pub fn new(label: impl Into<String>, detail: impl Into<String>, state: EntryState) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveFreshness {
    Fresh { age_secs: u64 },
    Idle { age_secs: u64 },
    Stale { age_secs: u64 },
}

impl LiveFreshness {
    fn age_secs(self) -> u64 {
        match self {
            LiveFreshness::Fresh { age_secs }
            | LiveFreshness::Idle { age_secs }
            | LiveFreshness::Stale { age_secs } => age_secs,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LiveFreshness::Fresh { .. } => "live",
            LiveFreshness::Idle { .. } => "idle",
            LiveFreshness::Stale { .. } => "stale",
        }
    }

    fn color(self) -> ratatui::style::Color {
        match self {
            LiveFreshness::Fresh { .. } => IOS_GREEN,
            LiveFreshness::Idle { .. } => IOS_ORANGE,
            LiveFreshness::Stale { .. } => IOS_HAIRLINE_STRONG,
        }
    }

    fn fg(self) -> ratatui::style::Color {
        match self {
            LiveFreshness::Stale { .. } => IOS_FG_MUTED,
            _ => IOS_FG,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveIndicator {
    tick: u64,
    freshness: LiveFreshness,
}

impl LiveIndicator {
    pub fn from_elapsed(tick: u64, age: Duration) -> Self {
        Self::from_age_secs(tick, age.as_secs())
    }

    pub fn from_age_secs(tick: u64, age_secs: u64) -> Self {
        let freshness = if age_secs < LIVE_FRESH_SECS {
            LiveFreshness::Fresh { age_secs }
        } else if age_secs > LIVE_STALE_SECS {
            LiveFreshness::Stale { age_secs }
        } else {
            LiveFreshness::Idle { age_secs }
        };
        Self { tick, freshness }
    }

    pub fn from_instants(tick: u64, now: Instant, last_refresh: Option<Instant>) -> Self {
        let age_secs = last_refresh
            .map(|seen| now.saturating_duration_since(seen).as_secs())
            .unwrap_or(u64::MAX);
        Self::from_age_secs(tick, age_secs)
    }

    pub fn fresh(tick: u64) -> Self {
        Self::from_age_secs(tick, 0)
    }

    pub fn width(&self) -> u16 {
        self.chip_text().chars().count() as u16 + 2
    }

    fn pulse_glyph(self) -> &'static str {
        match self.tick % 3 {
            0 => "●",
            1 => "◉",
            _ => "◎",
        }
    }

    fn chip_text(self) -> String {
        format!(" {} {} · {}s ", self.pulse_glyph(), self.freshness.label(), self.freshness.age_secs())
    }
}

impl Widget for LiveIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let color = self.freshness.color();
        let chip_bg = if matches!(self.freshness, LiveFreshness::Stale { .. }) {
            IOS_CARD_BG
        } else {
            color
        };
        let fg = self.freshness.fg();
        let spans = vec![
            Span::styled("◖", Style::default().fg(chip_bg).bg(IOS_BG_SOLID)),
            Span::styled(
                self.chip_text(),
                Style::default()
                    .fg(fg)
                    .bg(chip_bg)
                    .add_modifier(if matches!(self.freshness, LiveFreshness::Fresh { .. }) {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled("◗", Style::default().fg(chip_bg).bg(IOS_BG_SOLID)),
        ];

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

#[derive(Clone, Debug)]
pub struct IosPageDesign {
    plan: Plan,
    active_now: Vec<ActiveNowEntry>,
    waves: Vec<WaveSummary>,
    recent_merges: Vec<String>,
    live: LiveIndicator,
}

impl IosPageDesign {
    pub fn new(plan: Plan) -> Self {
        Self {
            plan,
            active_now: Vec::new(),
            waves: Vec::new(),
            recent_merges: Vec::new(),
            live: LiveIndicator::fresh(0),
        }
    }

    pub fn active_now(mut self, entries: Vec<ActiveNowEntry>) -> Self {
        self.active_now = entries;
        self
    }

    pub fn waves(mut self, waves: Vec<WaveSummary>) -> Self {
        self.waves = waves;
        self
    }

    pub fn recent_merges(mut self, merges: Vec<String>) -> Self {
        self.recent_merges = merges;
        self
    }

    pub fn live(mut self, live: LiveIndicator) -> Self {
        self.live = live;
        self
    }

}

impl Widget for IosPageDesign {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 4 {
            return;
        }

        let Self {
            plan,
            active_now,
            waves,
            recent_merges,
            live,
        } = self;

        let block = card(Some("PLAN TREE"), false);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let counts = PlanCounts::from_plan(&plan);
        let active_visible: Vec<_> = active_now.iter().take(MAX_ACTIVE_ROWS).collect();
        let active_overflow = active_now.len().saturating_sub(active_visible.len());
        let merge_visible: Vec<_> = recent_merges.iter().take(MAX_MERGE_ROWS).collect();
        let merge_overflow = recent_merges.len().saturating_sub(merge_visible.len());
        let wave_height = 2;
        let active_height = 1 + active_visible.len() as u16 + if active_overflow > 0 { 1 } else { 0 };
        let merge_height = 1 + merge_visible.len() as u16 + if merge_overflow > 0 { 1 } else { 0 };

        let mut y = inner.y;
        render_palette_strip(buf, Rect::new(inner.x, y, inner.width, 1));
        y += 1;

        let title_rect = Rect::new(inner.x, y, inner.width, 1);
        render_title_row(buf, title_rect, &plan, live);
        y += 1;

        let counts_rect = Rect::new(inner.x, y, inner.width, 1);
        render_counts_row(buf, counts_rect, counts);
        y += 1;

        let active_rect = Rect::new(inner.x, y, inner.width, active_height);
        render_active_now(buf, active_rect, &active_now, active_overflow);
        y += active_height;

        let waves_rect = Rect::new(inner.x, y, inner.width, wave_height);
        render_waves(buf, waves_rect, &waves);
        y += wave_height;

        let merges_rect = Rect::new(inner.x, y, inner.width, merge_height);
        render_recent_merges(buf, merges_rect, &recent_merges, merge_overflow);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlanCounts {
    done: usize,
    claimed: usize,
    available: usize,
}

impl PlanCounts {
    fn from_plan(plan: &Plan) -> Self {
        let mut counts = Self::default();
        for task in &plan.tasks {
            match task.status.as_str() {
                "completed" => counts.done += 1,
                "claimed" => counts.claimed += 1,
                _ => counts.available += 1,
            }
        }
        counts
    }
}

fn render_palette_strip(buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let colors = [IOS_TINT, IOS_GREEN, IOS_ORANGE, IOS_DESTRUCTIVE, IOS_TINT];
    let mut spans = Vec::new();
    let mut remaining = area.width as usize;
    for (idx, color) in colors.iter().enumerate() {
        let slots_left = colors.len() - idx;
        let width = if slots_left == 1 {
            remaining
        } else {
            remaining / slots_left
        };
        let width = width.max(1).min(remaining);
        remaining = remaining.saturating_sub(width);
        spans.push(Span::styled(
            " ".repeat(width),
            Style::default().bg(*color),
        ));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_title_row(buf: &mut Buffer, area: Rect, plan: &Plan, live: LiveIndicator) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let live_width = live.width().min(area.width);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(live_width)])
        .split(area);

    let spans = vec![
        Span::styled("PLAN TREE ", Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD)),
        Span::styled(plan.plan_slug.clone(), Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
    ];
    Paragraph::new(Line::from(spans)).render(chunks[0], buf);
    live.render(chunks[1], buf);
}

fn render_counts_row(buf: &mut Buffer, area: Rect, counts: PlanCounts) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut lines = Vec::new();
    let chips = [
        summary_chip(counts.done.to_string(), "done", IOS_GREEN),
        summary_chip(counts.claimed.to_string(), "claimed", IOS_TINT),
        summary_chip(counts.available.to_string(), "available", IOS_ORANGE),
    ];
    let mut chip_spans = Vec::new();
    for (idx, chip) in chips.into_iter().enumerate() {
        if idx > 0 {
            chip_spans.push(Span::raw(" "));
        }
        chip_spans.extend(chip);
    }
    lines.push(Line::from(chip_spans));
    Paragraph::new(lines).render(area, buf);
}

fn render_active_now(
    buf: &mut Buffer,
    area: Rect,
    entries: &[ActiveNowEntry],
    overflow: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut lines = vec![section_heading(
        "ACTIVE NOW",
        "agents on Colony tasks",
        IOS_TINT,
    )];

    if entries.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  no active agents",
            Style::default().fg(IOS_FG_FAINT),
        )]));
    } else {
        for entry in entries {
            let mut spans = status_chip(entry.state.chip_kind());
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("{} · {}", entry.agent, entry.task),
                Style::default().fg(IOS_FG),
            ));
            lines.push(Line::from(spans));
        }
        if overflow > 0 {
            lines.push(Line::from(vec![Span::styled(
                format!("  + {overflow} more"),
                Style::default().fg(IOS_FG_MUTED),
            )]));
        }
    }

    Paragraph::new(lines).render(area, buf);
}

fn render_waves(buf: &mut Buffer, area: Rect, waves: &[WaveSummary]) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut lines = vec![section_heading("WAVES", "W1->Wn strip", IOS_GREEN)];
    if waves.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  no waves yet",
            Style::default().fg(IOS_FG_FAINT),
        )]));
    } else {
        let mut spans = Vec::new();
        for (idx, wave) in waves.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(" "));
            }
            spans.extend(wave_chip(wave));
        }
        lines.push(Line::from(spans));
    }

    Paragraph::new(lines).render(area, buf);
}

fn render_recent_merges(
    buf: &mut Buffer,
    area: Rect,
    merges: &[String],
    overflow: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let mut lines = vec![section_heading(
        "RECENT MERGES",
        "git log --oneline",
        IOS_TINT,
    )];

    if merges.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  no merge history yet",
            Style::default().fg(IOS_FG_FAINT),
        )]));
    } else {
        for merge in merges {
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(IOS_TINT)),
                Span::styled(
                    merge.to_string(),
                    Style::default()
                        .fg(IOS_FG)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        if overflow > 0 {
            lines.push(Line::from(vec![Span::styled(
                format!("│ + {overflow} more"),
                Style::default().fg(IOS_FG_MUTED),
            )]));
        }
    }

    Paragraph::new(lines).render(area, buf);
}

fn summary_chip(count: String, label: &str, color: ratatui::style::Color) -> Vec<Span<'static>> {
    let text = format!(" {count} {label} ");
    vec![
        Span::styled("◖", Style::default().fg(color).bg(IOS_BG_SOLID)),
        Span::styled(
            text,
            Style::default()
                .fg(IOS_FG)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("◗", Style::default().fg(color).bg(IOS_BG_SOLID)),
    ]
}

fn wave_chip(wave: &WaveSummary) -> Vec<Span<'static>> {
    let color = wave.state.accent();
    let text = format!(" {} {} ", wave.label, wave.detail);
    vec![
        Span::styled("◖", Style::default().fg(color).bg(IOS_BG_SOLID)),
        Span::styled(
            text,
            Style::default()
                .fg(IOS_FG)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("◗", Style::default().fg(color).bg(IOS_BG_SOLID)),
    ]
}

fn section_heading(title: &str, subtitle: &str, accent: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(subtitle.to_string(), Style::default().fg(IOS_FG_MUTED)),
    ])
}

#[cfg(test)]
fn plan_fixture(slug: &str, title: &str, statuses: &[&str]) -> Plan {
    use fleet_data::plan::Subtask;

    Plan {
        schema_version: 1,
        plan_slug: slug.to_string(),
        title: title.to_string(),
        problem: "fixture".to_string(),
        acceptance_criteria: vec![],
        roles: vec![],
        tasks: statuses
            .iter()
            .enumerate()
            .map(|(idx, status)| Subtask {
                subtask_index: idx as u32,
                title: format!("Task {idx}"),
                description: format!("Task {idx}"),
                file_scope: vec![],
                depends_on: vec![],
                capability_hint: Some("ui_work".to_string()),
                spec_row_id: None,
                status: (*status).to_string(),
                claimed_by_session_id: None,
                claimed_by_agent: None,
                completed_summary: None,
            })
            .collect(),
        created_at: None,
        updated_at: None,
        published: None,
    }
}

#[cfg(test)]
fn render_snapshot(widget: IosPageDesign, width: u16, height: u16) -> String {
    use ratatui::{backend::TestBackend, Terminal};

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| frame.render_widget(widget.clone(), frame.area())).unwrap();
    format!("{}", terminal.backend())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[macro_export]
    macro_rules! assert_snapshot {
        ($actual:expr, $expected:expr $(,)?) => {{
            assert_eq!($actual, $expected);
        }};
    }

    pub mod insta {
        pub use crate::assert_snapshot;
    }

    #[test]
    fn live_indicator_classifies_fresh_and_stale() {
        let fresh = LiveIndicator::from_age_secs(4, 1);
        let idle = LiveIndicator::from_age_secs(5, 4);
        let stale = LiveIndicator::from_age_secs(6, 11);
        assert!(matches!(fresh.freshness, LiveFreshness::Fresh { .. }));
        assert!(matches!(idle.freshness, LiveFreshness::Idle { .. }));
        assert!(matches!(stale.freshness, LiveFreshness::Stale { .. }));
    }

    #[test]
    fn small_plan_fresh_snapshot_matches() {
        let plan = plan_fixture(
            "codex-fleet-ios-pages-design-pass-2026-05-15",
            "iOS background chrome + design polish + live indicators",
            &["completed", "claimed", "available"],
        );
        let widget = IosPageDesign::new(plan)
            .active_now(vec![
                ActiveNowEntry::new("codex-ada", "plan-tree-anim", EntryState::Working),
                ActiveNowEntry::new("codex-brian", "wave-strip polish", EntryState::Idle),
                ActiveNowEntry::new("codex-cora", "recent-merges tail", EntryState::Done),
            ])
            .waves(vec![
                WaveSummary::new("W1", "1/1", EntryState::Done),
                WaveSummary::new("W2", "1/1", EntryState::Working),
                WaveSummary::new("W3", "0/1", EntryState::Idle),
            ])
            .recent_merges(vec![
                "a1b2c3d merge plan header polish".to_string(),
                "b2c3d4e merge active-now rows".to_string(),
                "c3d4e5f merge live indicator".to_string(),
            ])
            .live(LiveIndicator::from_age_secs(3, 1));

        let rendered = render_snapshot(widget, 100, 30);
        insta::assert_snapshot!(rendered, SMALL_FRESH_SNAPSHOT);
    }

    #[test]
    fn small_plan_stale_snapshot_matches() {
        let plan = plan_fixture(
            "codex-fleet-ios-pages-design-pass-2026-05-15",
            "iOS background chrome + design polish + live indicators",
            &["completed", "claimed", "available"],
        );
        let widget = IosPageDesign::new(plan)
            .active_now(vec![
                ActiveNowEntry::new("codex-ada", "plan-tree-anim", EntryState::Working),
                ActiveNowEntry::new("codex-brian", "wave-strip polish", EntryState::Idle),
                ActiveNowEntry::new("codex-cora", "recent-merges tail", EntryState::Done),
            ])
            .waves(vec![
                WaveSummary::new("W1", "1/1", EntryState::Done),
                WaveSummary::new("W2", "1/1", EntryState::Working),
                WaveSummary::new("W3", "0/1", EntryState::Idle),
            ])
            .recent_merges(vec![
                "a1b2c3d merge plan header polish".to_string(),
                "b2c3d4e merge active-now rows".to_string(),
                "c3d4e5f merge live indicator".to_string(),
            ])
            .live(LiveIndicator::from_age_secs(8, 11));

        let rendered = render_snapshot(widget, 100, 30);
        insta::assert_snapshot!(rendered, SMALL_STALE_SNAPSHOT);
    }

    #[test]
    fn large_plan_fresh_snapshot_matches() {
        let statuses = [
            "completed", "completed", "completed", "completed", "claimed", "claimed", "claimed",
            "available", "available", "available", "available", "available", "available",
            "available", "available", "available", "available", "available",
        ];
        let plan = plan_fixture(
            "codex-fleet-ios-pages-design-pass-2026-05-15",
            "iOS background chrome + design polish + live indicators",
            &statuses,
        );
        let widget = IosPageDesign::new(plan)
            .active_now(vec![
                ActiveNowEntry::new("codex-ada", "plan-tree-anim", EntryState::Working),
                ActiveNowEntry::new("codex-brian", "active-now refresh", EntryState::Working),
                ActiveNowEntry::new("codex-cora", "recent-merges tail", EntryState::Idle),
                ActiveNowEntry::new("codex-drew", "wave strip", EntryState::Done),
                ActiveNowEntry::new("codex-ella", "header pills", EntryState::Working),
                ActiveNowEntry::new("codex-finn", "summary counts", EntryState::Idle),
            ])
            .waves(vec![
                WaveSummary::new("W1", "4/4", EntryState::Done),
                WaveSummary::new("W2", "3/3", EntryState::Done),
                WaveSummary::new("W3", "2/3", EntryState::Working),
                WaveSummary::new("W4", "1/2", EntryState::Working),
                WaveSummary::new("W5", "0/2", EntryState::Idle),
                WaveSummary::new("W6", "0/1", EntryState::Idle),
                WaveSummary::new("W7", "1/1", EntryState::Done),
                WaveSummary::new("W8", "2/2", EntryState::Done),
                WaveSummary::new("W9", "0/1", EntryState::Idle),
            ])
            .recent_merges(vec![
                "f1e2d3c merge review page polish".to_string(),
                "e2d3c4b merge watcher chrome".to_string(),
                "d3c4b5a merge waves strip".to_string(),
                "c4b5a6d merge plan tree live".to_string(),
                "b5a6d7e merge palette polish".to_string(),
            ])
            .live(LiveIndicator::from_age_secs(14, 1));

        let rendered = render_snapshot(widget, 100, 30);
        insta::assert_snapshot!(rendered, LARGE_FRESH_SNAPSHOT);
    }

    #[test]
    fn large_plan_stale_snapshot_matches() {
        let statuses = [
            "completed", "completed", "completed", "completed", "claimed", "claimed", "claimed",
            "available", "available", "available", "available", "available", "available",
            "available", "available", "available", "available", "available",
        ];
        let plan = plan_fixture(
            "codex-fleet-ios-pages-design-pass-2026-05-15",
            "iOS background chrome + design polish + live indicators",
            &statuses,
        );
        let widget = IosPageDesign::new(plan)
            .active_now(vec![
                ActiveNowEntry::new("codex-ada", "plan-tree-anim", EntryState::Working),
                ActiveNowEntry::new("codex-brian", "active-now refresh", EntryState::Working),
                ActiveNowEntry::new("codex-cora", "recent-merges tail", EntryState::Idle),
                ActiveNowEntry::new("codex-drew", "wave strip", EntryState::Done),
                ActiveNowEntry::new("codex-ella", "header pills", EntryState::Working),
                ActiveNowEntry::new("codex-finn", "summary counts", EntryState::Idle),
            ])
            .waves(vec![
                WaveSummary::new("W1", "4/4", EntryState::Done),
                WaveSummary::new("W2", "3/3", EntryState::Done),
                WaveSummary::new("W3", "2/3", EntryState::Working),
                WaveSummary::new("W4", "1/2", EntryState::Working),
                WaveSummary::new("W5", "0/2", EntryState::Idle),
                WaveSummary::new("W6", "0/1", EntryState::Idle),
                WaveSummary::new("W7", "1/1", EntryState::Done),
                WaveSummary::new("W8", "2/2", EntryState::Done),
                WaveSummary::new("W9", "0/1", EntryState::Idle),
            ])
            .recent_merges(vec![
                "f1e2d3c merge review page polish".to_string(),
                "e2d3c4b merge watcher chrome".to_string(),
                "d3c4b5a merge waves strip".to_string(),
                "c4b5a6d merge plan tree live".to_string(),
                "b5a6d7e merge palette polish".to_string(),
            ])
            .live(LiveIndicator::from_age_secs(22, 11));

        let rendered = render_snapshot(widget, 100, 30);
        insta::assert_snapshot!(rendered, LARGE_STALE_SNAPSHOT);
    }
}

#[cfg(test)]
const SMALL_FRESH_SNAPSHOT: &str = "\"╭ PLAN TREE ───────────────────────────────────────────────────────────────────────────────────────╮\"\n\"│                                                                                                  │\"\n\"│PLAN TREE codex-fleet-ios-pages-design-pass-2026-05-15                             ◖ ● live · 1s ◗│\"\n\"│◖ 1 done ◗ ◖ 1 claimed ◗ ◖ 1 available ◗                                                          │\"\n\"│ACTIVE NOW agents on Colony tasks                                                                 │\"\n\"│◖ ● working ◗ codex-ada · plan-tree-anim                                                          │\"\n\"│◖ ◌ idle    ◗ codex-brian · wave-strip polish                                                     │\"\n\"│◖ ● done    ◗ codex-cora · recent-merges tail                                                     │\"\n\"│WAVES W1->Wn strip                                                                                │\"\n\"│◖ W1 1/1 ◗ ◖ W2 1/1 ◗ ◖ W3 0/1 ◗                                                                  │\"\n\"│RECENT MERGES git log --oneline                                                                   │\"\n\"││ a1b2c3d merge plan header polish                                                                │\"\n\"││ b2c3d4e merge active-now rows                                                                   │\"\n\"││ c3d4e5f merge live indicator                                                                    │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"╰──────────────────────────────────────────────────────────────────────────────────────────────────╯\"\n";
#[cfg(test)]
const SMALL_STALE_SNAPSHOT: &str = "\"╭ PLAN TREE ───────────────────────────────────────────────────────────────────────────────────────╮\"\n\"│                                                                                                  │\"\n\"│PLAN TREE codex-fleet-ios-pages-design-pass-2026-05-15                           ◖ ◎ stale · 11s ◗│\"\n\"│◖ 1 done ◗ ◖ 1 claimed ◗ ◖ 1 available ◗                                                          │\"\n\"│ACTIVE NOW agents on Colony tasks                                                                 │\"\n\"│◖ ● working ◗ codex-ada · plan-tree-anim                                                          │\"\n\"│◖ ◌ idle    ◗ codex-brian · wave-strip polish                                                     │\"\n\"│◖ ● done    ◗ codex-cora · recent-merges tail                                                     │\"\n\"│WAVES W1->Wn strip                                                                                │\"\n\"│◖ W1 1/1 ◗ ◖ W2 1/1 ◗ ◖ W3 0/1 ◗                                                                  │\"\n\"│RECENT MERGES git log --oneline                                                                   │\"\n\"││ a1b2c3d merge plan header polish                                                                │\"\n\"││ b2c3d4e merge active-now rows                                                                   │\"\n\"││ c3d4e5f merge live indicator                                                                    │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"╰──────────────────────────────────────────────────────────────────────────────────────────────────╯\"\n";
#[cfg(test)]
const LARGE_FRESH_SNAPSHOT: &str = "\"╭ PLAN TREE ───────────────────────────────────────────────────────────────────────────────────────╮\"\n\"│                                                                                                  │\"\n\"│PLAN TREE codex-fleet-ios-pages-design-pass-2026-05-15                             ◖ ◎ live · 1s ◗│\"\n\"│◖ 4 done ◗ ◖ 3 claimed ◗ ◖ 11 available ◗                                                         │\"\n\"│ACTIVE NOW agents on Colony tasks                                                                 │\"\n\"│◖ ● working ◗ codex-ada · plan-tree-anim                                                          │\"\n\"│◖ ● working ◗ codex-brian · active-now refresh                                                    │\"\n\"│◖ ◌ idle    ◗ codex-cora · recent-merges tail                                                     │\"\n\"│◖ ● done    ◗ codex-drew · wave strip                                                             │\"\n\"│◖ ● working ◗ codex-ella · header pills                                                           │\"\n\"│◖ ◌ idle    ◗ codex-finn · summary counts                                                         │\"\n\"│WAVES W1->Wn strip                                                                                │\"\n\"│◖ W1 4/4 ◗ ◖ W2 3/3 ◗ ◖ W3 2/3 ◗ ◖ W4 1/2 ◗ ◖ W5 0/2 ◗ ◖ W6 0/1 ◗ ◖ W7 1/1 ◗ ◖ W8 2/2 ◗ ◖ W9 0/1 ◗│\"\n\"│RECENT MERGES git log --oneline                                                                   │\"\n\"││ f1e2d3c merge review page polish                                                                │\"\n\"││ e2d3c4b merge watcher chrome                                                                    │\"\n\"││ d3c4b5a merge waves strip                                                                       │\"\n\"││ c4b5a6d merge plan tree live                                                                    │\"\n\"││ b5a6d7e merge palette polish                                                                    │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"╰──────────────────────────────────────────────────────────────────────────────────────────────────╯\"\n";
#[cfg(test)]
const LARGE_STALE_SNAPSHOT: &str = "\"╭ PLAN TREE ───────────────────────────────────────────────────────────────────────────────────────╮\"\n\"│                                                                                                  │\"\n\"│PLAN TREE codex-fleet-ios-pages-design-pass-2026-05-15                           ◖ ◉ stale · 11s ◗│\"\n\"│◖ 4 done ◗ ◖ 3 claimed ◗ ◖ 11 available ◗                                                         │\"\n\"│ACTIVE NOW agents on Colony tasks                                                                 │\"\n\"│◖ ● working ◗ codex-ada · plan-tree-anim                                                          │\"\n\"│◖ ● working ◗ codex-brian · active-now refresh                                                    │\"\n\"│◖ ◌ idle    ◗ codex-cora · recent-merges tail                                                     │\"\n\"│◖ ● done    ◗ codex-drew · wave strip                                                             │\"\n\"│◖ ● working ◗ codex-ella · header pills                                                           │\"\n\"│◖ ◌ idle    ◗ codex-finn · summary counts                                                         │\"\n\"│WAVES W1->Wn strip                                                                                │\"\n\"│◖ W1 4/4 ◗ ◖ W2 3/3 ◗ ◖ W3 2/3 ◗ ◖ W4 1/2 ◗ ◖ W5 0/2 ◗ ◖ W6 0/1 ◗ ◖ W7 1/1 ◗ ◖ W8 2/2 ◗ ◖ W9 0/1 ◗│\"\n\"│RECENT MERGES git log --oneline                                                                   │\"\n\"││ f1e2d3c merge review page polish                                                                │\"\n\"││ e2d3c4b merge watcher chrome                                                                    │\"\n\"││ d3c4b5a merge waves strip                                                                       │\"\n\"││ c4b5a6d merge plan tree live                                                                    │\"\n\"││ b5a6d7e merge palette polish                                                                    │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"│                                                                                                  │\"\n\"╰──────────────────────────────────────────────────────────────────────────────────────────────────╯\"\n";
