use fleet_data::{fleet::WorkerRow, panes::PaneState};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
    rail::{progress_rail, RailAxis},
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use std::time::Duration;

const WIDE_BREAKPOINT: u16 = 180;
const FOOTER_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Freshness {
    Fresh,
    Idle,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveIndicator {
    tick: u64,
    age_secs: u64,
}

impl LiveIndicator {
    pub fn from_elapsed(tick: u64, age: Duration) -> Self {
        Self {
            tick,
            age_secs: age.as_secs(),
        }
    }

    pub fn fresh(tick: u64) -> Self {
        Self { tick, age_secs: 0 }
    }

    fn freshness(self) -> Freshness {
        if self.age_secs <= 2 {
            Freshness::Fresh
        } else if self.age_secs >= 10 {
            Freshness::Stale
        } else {
            Freshness::Idle
        }
    }

    fn glyph(self) -> &'static str {
        match self.tick % 3 {
            0 => "●",
            1 => "◉",
            _ => "◎",
        }
    }

    fn label(self) -> &'static str {
        match self.freshness() {
            Freshness::Fresh => "live",
            Freshness::Idle => "idle",
            Freshness::Stale => "stale",
        }
    }

    fn color(self) -> Color {
        match self.freshness() {
            Freshness::Fresh => IOS_GREEN,
            Freshness::Idle => IOS_ORANGE,
            Freshness::Stale => IOS_DESTRUCTIVE,
        }
    }

    pub fn width(self) -> u16 {
        visible_width(&self.text()) + 2
    }

    fn text(self) -> String {
        format!(" {} {} · {}s ", self.glyph(), self.label(), self.age_secs)
    }
}

impl Widget for LiveIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let fill = self.color();
        let fg = if matches!(self.freshness(), Freshness::Stale) {
            IOS_FG
        } else {
            Color::Rgb(10, 36, 21)
        };
        Paragraph::new(Line::from(vec![
            Span::styled("◖", Style::default().fg(fill).bg(IOS_BG_SOLID)),
            Span::styled(
                self.text(),
                Style::default()
                    .fg(fg)
                    .bg(fill)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("◗", Style::default().fg(fill).bg(IOS_BG_SOLID)),
        ]))
        .render(area, buf);
    }
}

#[derive(Clone, Debug)]
pub struct IosPageDesign {
    rows: Vec<WorkerRow>,
    live: LiveIndicator,
    refresh_secs: u64,
}

impl IosPageDesign {
    pub fn new(rows: Vec<WorkerRow>) -> Self {
        Self {
            rows,
            live: LiveIndicator::fresh(0),
            refresh_secs: 1,
        }
    }

    pub fn live(mut self, live: LiveIndicator) -> Self {
        self.live = live;
        self
    }

    pub fn refresh_secs(mut self, refresh_secs: u64) -> Self {
        self.refresh_secs = refresh_secs;
        self
    }
}

impl Widget for IosPageDesign {
    fn render(self, area: Rect, buf: &mut Buffer) {
        fill(area, IOS_BG_SOLID, buf);
        if area.width < 50 || area.height < 14 {
            return;
        }

        let block = card(Some("FLEET"), false);
        let inner = block.inner(area);
        block.render(area, buf);
        fill(inner, IOS_BG_SOLID, buf);

        if inner.width == 0 || inner.height < 8 {
            return;
        }

        let active: Vec<&WorkerRow> = self.rows.iter().filter(|row| row.is_live()).collect();
        let reserve: Vec<&WorkerRow> = self.rows.iter().filter(|row| !row.is_live()).collect();
        let summary = Summary::from_rows(&self.rows);

        let mut y = inner.y;
        render_palette_strip(Rect::new(inner.x, y, inner.width, 1), buf);
        y += 1;
        render_header(
            Rect::new(inner.x, y, inner.width, 2),
            buf,
            summary,
            self.live,
        );
        y += 2;

        let footer_y = inner.y + inner.height.saturating_sub(FOOTER_HEIGHT);
        let body = Rect::new(
            inner.x,
            y,
            inner.width,
            footer_y.saturating_sub(y).saturating_sub(1),
        );

        if body.width >= WIDE_BREAKPOINT {
            let left_w = body.width.saturating_sub(1) / 2;
            let right_w = body.width.saturating_sub(left_w + 1);
            render_section(
                Rect::new(body.x, body.y, left_w, body.height),
                buf,
                "ACTIVE",
                "live panes",
                &active,
            );
            render_section(
                Rect::new(body.x + left_w + 1, body.y, right_w, body.height),
                buf,
                "RESERVE",
                "available accounts",
                &reserve,
            );
        } else {
            let top_h = body.height.saturating_sub(1) / 2;
            let bottom_h = body.height.saturating_sub(top_h + 1);
            render_section(
                Rect::new(body.x, body.y, body.width, top_h),
                buf,
                "ACTIVE",
                "live panes",
                &active,
            );
            render_section(
                Rect::new(body.x, body.y + top_h + 1, body.width, bottom_h),
                buf,
                "RESERVE",
                "available accounts",
                &reserve,
            );
        }

        render_footer(
            Rect::new(inner.x, footer_y, inner.width, FOOTER_HEIGHT),
            buf,
            summary,
            self.refresh_secs,
            self.live.tick,
        );
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Summary {
    accounts: usize,
    live: usize,
    capped: usize,
    review: usize,
}

impl Summary {
    fn from_rows(rows: &[WorkerRow]) -> Self {
        Self {
            accounts: rows.len(),
            live: rows.iter().filter(|row| row.is_live()).count(),
            capped: rows.iter().filter(|row| row.is_capped()).count(),
            review: rows
                .iter()
                .filter(|row| matches!(row.state, Some(PaneState::Approval)))
                .count(),
        }
    }
}

fn render_header(area: Rect, buf: &mut Buffer, summary: Summary, live: LiveIndicator) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let live_w = live.width().min(area.width);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(live_w)])
        .split(rows[0]);

    Paragraph::new(Line::from(vec![
        Span::styled(
            "FLEET ",
            Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "cockpit",
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  {} accounts · {} live · {} review",
                summary.accounts, summary.live, summary.review
            ),
            Style::default().fg(IOS_FG_MUTED),
        ),
    ]))
    .render(columns[0], buf);
    live.render(columns[1], buf);

    Paragraph::new(Line::from(Span::styled(
        "ACTIVE / RESERVE / FOOTER",
        Style::default()
            .fg(IOS_FG_FAINT)
            .add_modifier(Modifier::BOLD),
    )))
    .render(rows[1], buf);
}

fn render_section(
    area: Rect,
    buf: &mut Buffer,
    title: &'static str,
    subtitle: &'static str,
    rows: &[&WorkerRow],
) {
    if area.width < 24 || area.height < 4 {
        return;
    }

    let block = card(Some(title), false);
    let inner = block.inner(area);
    block.render(area, buf);
    fill(inner, IOS_BG_GLASS, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let header = Line::from(vec![
        Span::styled(
            format!("{title} "),
            Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(subtitle, Style::default().fg(IOS_FG_MUTED)),
        Span::styled(
            format!(" · {}", rows.len()),
            Style::default().fg(IOS_FG_FAINT),
        ),
    ]);
    Paragraph::new(header).render(Rect::new(inner.x, inner.y, inner.width, 1), buf);

    let columns_y = inner.y + 1;
    render_section_columns(Rect::new(inner.x, columns_y, inner.width, 1), buf);

    let list = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let max_rows = (list.height / 3) as usize;
    for (idx, row) in rows.iter().take(max_rows).enumerate() {
        let y = list.y + idx as u16 * 3;
        render_worker_row(Rect::new(list.x, y, list.width, 2), buf, row, idx);
    }
    if rows.len() > max_rows && list.height > 0 {
        let overflow = rows.len() - max_rows;
        let y = list.y + list.height.saturating_sub(1);
        Paragraph::new(Line::from(Span::styled(
            format!("  + {overflow} more"),
            Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
        )))
        .render(Rect::new(list.x, y, list.width, 1), buf);
    }
}

fn render_section_columns(area: Rect, buf: &mut Buffer) {
    let labels = if area.width >= 96 {
        "  ACCOUNT                         WEEKLY     5H        STATUS       WORKING ON"
    } else {
        "  ACCOUNT                  CAPS       STATUS       WORKING ON"
    };
    Paragraph::new(Line::from(Span::styled(
        truncate_chars(labels, area.width as usize),
        Style::default()
            .fg(IOS_FG_FAINT)
            .bg(IOS_BG_GLASS)
            .add_modifier(Modifier::BOLD),
    )))
    .render(area, buf);
}

fn render_worker_row(area: Rect, buf: &mut Buffer, row: &WorkerRow, index: usize) {
    if area.width == 0 || area.height < 2 {
        return;
    }

    let bg = if index % 2 == 0 {
        IOS_CARD_BG
    } else {
        IOS_BG_GLASS
    };
    fill(area, bg, buf);

    let working = working_text(row);
    let agent = if row.is_current {
        format!("* {}", row.email)
    } else {
        row.email.clone()
    };
    let account_w = if area.width >= 100 { 30 } else { 24 }.min(area.width);
    let caps_w = if area.width >= 100 { 22 } else { 13 }.min(area.width);
    let status_w = 14.min(area.width);
    let fixed = account_w + caps_w + status_w + 6;
    let working_w = area.width.saturating_sub(fixed).max(12);

    let mut x = area.x;
    Paragraph::new(Line::from(Span::styled(
        format!("  {}", truncate_chars(&agent, account_w.saturating_sub(2) as usize)),
        Style::default()
            .fg(IOS_FG)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )))
    .render(Rect::new(x, area.y, account_w, 1), buf);
    x += account_w + 1;

    render_caps(Rect::new(x, area.y, caps_w, 1), buf, row, bg);
    x += caps_w + 1;

    render_status(Rect::new(x, area.y, status_w, 1), buf, row, bg);
    x += status_w + 1;

    Paragraph::new(Line::from(Span::styled(
        truncate_chars(&working, working_w as usize),
        Style::default().fg(IOS_FG).bg(bg),
    )))
    .render(Rect::new(x, area.y, working_w, 1), buf);

    let sub = if row.pane_subtext.is_empty() {
        row.agent_id.clone()
    } else {
        format!("{} · {}", row.agent_id, row.pane_subtext)
    };
    Paragraph::new(Line::from(Span::styled(
        format!("  {}", truncate_chars(&sub, area.width.saturating_sub(2) as usize)),
        Style::default().fg(IOS_FG_MUTED).bg(bg),
    )))
    .render(Rect::new(area.x, area.y + 1, area.width, 1), buf);
}

fn render_caps(area: Rect, buf: &mut Buffer, row: &WorkerRow, bg: Color) {
    if area.width < 8 {
        return;
    }

    let rail_w = if area.width >= 20 { 6 } else { 4 };
    let mut spans = progress_rail(row.weekly_pct, RailAxis::Usage, rail_w);
    if area.width >= 20 {
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        spans.extend(progress_rail(row.five_h_pct, RailAxis::Usage, rail_w));
    }
    for span in &mut spans {
        span.style = span.style.bg(bg);
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_status(area: Rect, buf: &mut Buffer, row: &WorkerRow, bg: Color) {
    let mut spans = status_chip(chip_kind(row.state));
    for span in &mut spans {
        span.style = span.style.bg(bg);
    }
    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer, summary: Summary, refresh_secs: u64, tick: u64) {
    if area.width < 10 || area.height < 2 {
        return;
    }

    let block = card(Some("FLEET FOOTER"), false);
    let inner = block.inner(area);
    block.render(area, buf);
    fill(inner, IOS_BG_GLASS, buf);
    Paragraph::new(Line::from(vec![
        Span::styled("live=", Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS)),
        Span::styled(
            summary.live.to_string(),
            Style::default()
                .fg(IOS_FG)
                .bg(IOS_BG_GLASS)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   accounts={}   capped(5h>=100%)={}   refresh={}s   tick={tick}", summary.accounts, summary.capped, refresh_secs),
            Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
        ),
    ]))
    .render(inner, buf);
}

fn render_palette_strip(area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let colors = [IOS_TINT, IOS_GREEN, IOS_ORANGE, IOS_DESTRUCTIVE, IOS_PURPLE];
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

fn chip_kind(state: Option<PaneState>) -> ChipKind {
    match state {
        Some(PaneState::Working) => ChipKind::Working,
        Some(PaneState::Idle) | None => ChipKind::Idle,
        Some(PaneState::Polling) => ChipKind::Polling,
        Some(PaneState::Capped) => ChipKind::Capped,
        Some(PaneState::Approval) => ChipKind::Approval,
        Some(PaneState::Boot) => ChipKind::Boot,
        Some(PaneState::Dead) => ChipKind::Dead,
    }
}

fn working_text(row: &WorkerRow) -> String {
    let trimmed = row.working_on.trim();
    if !trimmed.is_empty() {
        trimmed.to_string()
    } else if row.pane_id.is_some() {
        "idle prompt".to_string()
    } else {
        "reserve · no live pane".to_string()
    }
}

fn fill(area: Rect, color: Color, buf: &mut Buffer) {
    Block::default()
        .style(Style::default().bg(color))
        .render(area, buf);
}

fn visible_width(s: &str) -> u16 {
    s.chars().count() as u16
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    format!("{}…", s.chars().take(keep).collect::<String>())
}
