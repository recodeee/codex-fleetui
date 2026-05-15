//! Design-D session switcher overlay.
//!
//! This ports the `images/D _ Session switcher _ card stack.html` artboard
//! into a reusable ratatui component: dimmed full-screen surface, large
//! `CODEX-FLEET · SESSION SWITCHER` header, a top-right "New worker" pill,
//! horizontally stacked worker cards, and footer keyboard hints.

use crate::palette::*;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

const SCRIM: Color = Color::Rgb(2, 4, 7);
const HEADER_RULE: Color = Color::Rgb(12, 18, 24);
const CARD_GREEN: Color = Color::Rgb(36, 94, 59);
const CARD_AMBER: Color = Color::Rgb(56, 43, 18);
const CARD_BLUE: Color = Color::Rgb(28, 48, 60);
const CARD_PURPLE: Color = Color::Rgb(40, 31, 57);
const CARD_MAROON: Color = Color::Rgb(55, 32, 42);
const CARD_FOOTER: Color = Color::Rgb(64, 57, 46);
const GREEN_SOFT: Color = Color::Rgb(41, 135, 72);
const RED_SOFT: Color = Color::Rgb(90, 37, 30);
const NEW_WORKER_BG: Color = Color::Rgb(32, 32, 34);
const NEW_WORKER_BORDER: Color = Color::Rgb(72, 72, 76);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionKind {
    Working,
    Approved,
    Diffing,
    Idle,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCard<'a> {
    pub pane: u8,
    pub name: &'a str,
    pub kind: SessionKind,
    pub task: &'a str,
    pub model: &'a str,
    pub context: &'a str,
    pub runtime: &'a str,
    pub badge: Option<&'a str>,
}

impl<'a> SessionCard<'a> {
    pub fn new(
        pane: u8,
        name: &'a str,
        kind: SessionKind,
        task: &'a str,
        model: &'a str,
        context: &'a str,
        runtime: &'a str,
    ) -> Self {
        Self {
            pane,
            name,
            kind,
            task,
            model,
            context,
            runtime,
            badge: None,
        }
    }

    pub fn badge(mut self, badge: &'a str) -> Self {
        self.badge = Some(badge);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionSwitcherState {
    pub selected: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSwitcher<'a> {
    sessions: Vec<SessionCard<'a>>,
    awaiting_review: u16,
    title: &'a str,
}

impl<'a> SessionSwitcher<'a> {
    pub fn new(sessions: Vec<SessionCard<'a>>) -> Self {
        Self {
            sessions,
            awaiting_review: 0,
            title: "CODEX-FLEET · SESSION SWITCHER",
        }
    }

    pub fn awaiting_review(mut self, count: u16) -> Self {
        self.awaiting_review = count;
        self
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    pub fn sessions(&self) -> &[SessionCard<'a>] {
        &self.sessions
    }

    pub fn selected_index(&self, state: &SessionSwitcherState) -> usize {
        state.selected.min(self.sessions.len().saturating_sub(1))
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &SessionSwitcherState) {
        frame.render_widget(Clear, area);
        fill(frame, area, SCRIM);
        render_header(
            frame,
            area,
            self.title,
            self.sessions.len(),
            self.awaiting_review,
        );
        render_new_worker_pill(frame, area);
        render_footer(frame, area);

        if self.sessions.is_empty() || area.width < 42 || area.height < 16 {
            render_empty_state(frame, area);
            return;
        }

        let selected = self.selected_index(state);
        render_card_strip(frame, area, &self.sessions, selected);
    }
}

fn render_header(frame: &mut Frame, area: Rect, title: &str, workers: usize, awaiting: u16) {
    let header_h = 5.min(area.height);
    fill(
        frame,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: header_h,
        },
        SCRIM,
    );
    if area.width > 6 {
        text(
            frame,
            Rect {
                x: area.x + 4,
                y: area.y + 2,
                width: area.width.saturating_sub(8),
                height: 1,
            },
            Line::from(Span::styled(
                fit(title, area.width.saturating_sub(8)),
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    }
    if area.height > 4 && area.width > 6 {
        text(
            frame,
            Rect {
                x: area.x + 4,
                y: area.y + 4,
                width: area.width.saturating_sub(8),
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    format!("{workers} workers"),
                    Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {awaiting} awaiting review"),
                    Style::default().fg(IOS_FG_MUTED),
                ),
            ]),
        );
    }
    if area.height > header_h {
        fill(
            frame,
            Rect {
                x: area.x,
                y: area.y + header_h,
                width: area.width,
                height: 1,
            },
            HEADER_RULE,
        );
    }
}

fn render_new_worker_pill(frame: &mut Frame, area: Rect) {
    if area.width < 28 || area.height < 6 {
        return;
    }
    let w = 14.min(area.width.saturating_sub(2));
    let rect = Rect {
        x: area.x + area.width.saturating_sub(w + 2),
        y: area.y + 2,
        width: w,
        height: 3,
    };
    let block = rounded_block(NEW_WORKER_BORDER, NEW_WORKER_BG, false);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    text(
        frame,
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
        Line::from(vec![
            Span::styled("+ ", Style::default().fg(IOS_FG).bg(NEW_WORKER_BG)),
            Span::styled(
                "New",
                Style::default()
                    .fg(IOS_FG)
                    .bg(NEW_WORKER_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    );
    if inner.height > 1 {
        text(
            frame,
            Rect {
                x: inner.x + 2,
                y: inner.y + 1,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(Span::styled(
                "worker",
                Style::default().fg(IOS_FG).bg(NEW_WORKER_BG),
            )),
        );
    }
}

fn render_empty_state(frame: &mut Frame, area: Rect) {
    let msg = "No worker sessions";
    let w = msg.chars().count() as u16;
    text(
        frame,
        Rect {
            x: area.x + area.width.saturating_sub(w) / 2,
            y: area.y + area.height / 2,
            width: w,
            height: 1,
        },
        Line::from(Span::styled(
            msg,
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        )),
    );
}

fn render_card_strip(frame: &mut Frame, area: Rect, sessions: &[SessionCard<'_>], selected: usize) {
    let header_h = 6;
    let footer_h = 3;
    if area.height <= header_h + footer_h + 8 {
        return;
    }

    let strip_top = area.y + header_h;
    let strip_bottom = area.y + area.height.saturating_sub(footer_h + 1);
    let strip_h = strip_bottom.saturating_sub(strip_top).max(8);
    let card_h = strip_h.saturating_sub(2).max(8);
    let card_y = strip_top + (strip_h.saturating_sub(card_h) / 2);
    let base_w = (area.width / 5).clamp(24, 56);
    let active_w = (base_w + 14).min(area.width.saturating_sub(4));
    let gap = 2;
    let mut x = area.x + 2;

    for (index, session) in sessions.iter().enumerate() {
        let is_selected = index == selected;
        let card_w = if is_selected { active_w } else { base_w };
        if x >= area.x + area.width {
            break;
        }

        let visible_w = card_w.min(area.x + area.width - x);
        let rect = Rect {
            x,
            y: card_y,
            width: visible_w,
            height: card_h,
        };
        render_session_card(frame, rect, session, is_selected);
        x = x.saturating_add(card_w + gap);
    }
}

fn render_session_card(frame: &mut Frame, rect: Rect, session: &SessionCard<'_>, selected: bool) {
    if rect.width < 18 || rect.height < 9 {
        return;
    }
    let border = if selected {
        IOS_TINT
    } else {
        session_border(session.kind)
    };
    let fill_color = session_fill(session.kind);
    let block = rounded_block(border, fill_color, selected);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    fill(frame, inner, fill_color);

    render_card_header(frame, inner, session, selected, fill_color);
    if inner.height < 4 {
        return;
    }
    text(
        frame,
        Rect {
            x: inner.x + 2,
            y: inner.y + 2,
            width: inner.width.saturating_sub(4),
            height: 1,
        },
        Line::from(Span::styled(
            fit(session.name, inner.width.saturating_sub(4)),
            Style::default()
                .fg(IOS_FG)
                .bg(fill_color)
                .add_modifier(Modifier::BOLD),
        )),
    );

    let rule_y = inner.y + 4;
    render_rule(frame, inner, rule_y, fill_color);

    let task_y = rule_y + 2;
    let task_h = 3.min(inner.height.saturating_sub(12));
    if task_h > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                session.task,
                Style::default().fg(IOS_FG).bg(fill_color),
            ))
            .wrap(Wrap { trim: true }),
            Rect {
                x: inner.x + 2,
                y: task_y,
                width: inner.width.saturating_sub(4),
                height: task_h,
            },
        );
    }

    let metric_y = task_y + task_h + 2;
    render_metrics(frame, inner, metric_y, session, fill_color);
    render_card_actions(frame, inner, selected);
}

fn render_card_header(
    frame: &mut Frame,
    inner: Rect,
    session: &SessionCard<'_>,
    selected: bool,
    bg: Color,
) {
    let dot = status_dot(session.kind);
    let status = session_status(session.kind);
    let header_w = inner.width.saturating_sub(14);
    text(
        frame,
        Rect {
            x: inner.x + 2,
            y: inner.y + 1,
            width: header_w,
            height: 1,
        },
        Line::from(vec![
            Span::styled("●", Style::default().fg(dot).bg(bg)),
            Span::raw(" "),
            Span::styled(
                fit(
                    &format!("PANE {} · {status}", session.pane),
                    header_w.saturating_sub(2),
                ),
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    );

    if let Some(badge) = session.badge {
        let badge_w = (badge.chars().count() as u16 + 2).min(inner.width.saturating_sub(4));
        if inner.width > badge_w + 4 {
            let (fg, chip_bg) = if badge == "LIVE" {
                (Color::Rgb(7, 38, 18), IOS_GREEN)
            } else if badge.contains("REVIEW") {
                (IOS_FG, IOS_CHIP_BG)
            } else {
                (IOS_FG, IOS_ICON_CHIP)
            };
            text(
                frame,
                Rect {
                    x: inner.x + inner.width.saturating_sub(badge_w + 2),
                    y: inner.y + 1,
                    width: badge_w,
                    height: 1,
                },
                Line::from(Span::styled(
                    format!(" {badge} "),
                    Style::default()
                        .fg(fg)
                        .bg(chip_bg)
                        .add_modifier(Modifier::BOLD),
                )),
            );
        }
    } else if selected {
        text(
            frame,
            Rect {
                x: inner.x + inner.width.saturating_sub(8),
                y: inner.y + 1,
                width: 6,
                height: 1,
            },
            Line::from(Span::styled(
                "LIVE",
                Style::default()
                    .fg(Color::Rgb(7, 38, 18))
                    .bg(IOS_GREEN)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    }
}

fn render_metrics(
    frame: &mut Frame,
    inner: Rect,
    start_y: u16,
    session: &SessionCard<'_>,
    bg: Color,
) {
    if start_y + 2 >= inner.y + inner.height.saturating_sub(3) {
        return;
    }
    for (index, (label, value, value_color)) in [
        ("MODEL", session.model, IOS_FG),
        ("CONTEXT", session.context, context_color(session.context)),
        ("RUNTIME", session.runtime, IOS_FG),
    ]
    .iter()
    .enumerate()
    {
        let y = start_y + index as u16;
        if y >= inner.y + inner.height.saturating_sub(3) {
            break;
        }
        text(
            frame,
            Rect {
                x: inner.x + 2,
                y,
                width: inner.width.saturating_sub(4),
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    format!("{label:<8}"),
                    Style::default()
                        .fg(IOS_FG_FAINT)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fit(value, inner.width.saturating_sub(14)),
                    Style::default().fg(*value_color).bg(bg),
                ),
            ]),
        );
    }
}

fn render_card_actions(frame: &mut Frame, inner: Rect, selected: bool) {
    if inner.height < 5 || inner.width < 18 {
        return;
    }
    let footer_y = inner.y + inner.height.saturating_sub(2);
    fill(
        frame,
        Rect {
            x: inner.x,
            y: footer_y.saturating_sub(1),
            width: inner.width,
            height: 2,
        },
        CARD_FOOTER,
    );

    let spacious_actions = inner.width >= 40;
    let focus_w = if spacious_actions {
        10
    } else if inner.width > 34 {
        13
    } else {
        9
    };
    let button_y = footer_y;
    render_action_chip(
        frame,
        Rect {
            x: inner.x + 2,
            y: button_y,
            width: focus_w,
            height: 1,
        },
        "▣ Focus",
        if selected { IOS_TINT } else { IOS_CHIP_BG },
        IOS_FG,
    );
    let mut x = inner.x + 2 + focus_w + 2;
    let actions: [(&str, u16, Color, Color); 3] = if spacious_actions {
        [
            ("Queue", 7, IOS_CHIP_BG, IOS_FG),
            ("Pause", 7, IOS_CHIP_BG, IOS_FG),
            ("Kill", 6, RED_SOFT, IOS_DESTRUCTIVE),
        ]
    } else {
        [
            ("☰", 4, IOS_CHIP_BG, IOS_FG),
            ("Ⅱ", 4, IOS_CHIP_BG, IOS_FG),
            ("⌫", 4, RED_SOFT, IOS_DESTRUCTIVE),
        ]
    };
    for (label, width, bg, fg) in actions {
        if x + width > inner.x + inner.width.saturating_sub(1) {
            break;
        }
        render_action_chip(
            frame,
            Rect {
                x,
                y: button_y,
                width,
                height: 1,
            },
            label,
            bg,
            fg,
        );
        x = x.saturating_add(width + 2);
    }
}

fn render_footer(frame: &mut Frame, area: Rect) {
    if area.height < 2 || area.width < 32 {
        return;
    }
    let y = area.y + area.height.saturating_sub(2);
    let footer = Line::from(vec![
        Span::styled("← →", Style::default().fg(IOS_FG).bg(SCRIM)),
        Span::styled(" navigate    ", Style::default().fg(IOS_FG_MUTED).bg(SCRIM)),
        Span::styled("↵", Style::default().fg(IOS_FG).bg(SCRIM)),
        Span::styled(" focus    ", Style::default().fg(IOS_FG_MUTED).bg(SCRIM)),
        Span::styled("↑", Style::default().fg(IOS_FG).bg(SCRIM)),
        Span::styled(
            " dismiss worker    ",
            Style::default().fg(IOS_FG_MUTED).bg(SCRIM),
        ),
        Span::styled("⌘ N", Style::default().fg(IOS_FG).bg(SCRIM)),
        Span::styled(" new worker", Style::default().fg(IOS_FG_MUTED).bg(SCRIM)),
    ]);
    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
    );
}

fn render_action_chip(frame: &mut Frame, rect: Rect, label: &str, bg: Color, fg: Color) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            fit(label, rect.width),
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        rect,
    );
}

fn render_rule(frame: &mut Frame, inner: Rect, y: u16, bg: Color) {
    if y >= inner.y + inner.height {
        return;
    }
    text(
        frame,
        Rect {
            x: inner.x + 2,
            y,
            width: inner.width.saturating_sub(4),
            height: 1,
        },
        Line::from(Span::styled(
            "─".repeat(inner.width.saturating_sub(4) as usize),
            Style::default().fg(IOS_HAIRLINE).bg(bg),
        )),
    );
}

fn rounded_block(border: Color, bg: Color, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border).add_modifier(if active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .style(Style::default().bg(bg))
}

fn fill(frame: &mut Frame, area: Rect, color: Color) {
    if area.width > 0 && area.height > 0 {
        frame.render_widget(Block::default().style(Style::default().bg(color)), area);
    }
}

fn text(frame: &mut Frame, area: Rect, line: Line<'static>) {
    if area.width > 0 && area.height > 0 {
        frame.render_widget(Paragraph::new(line), area);
    }
}

fn fit(input: &str, width: u16) -> String {
    let width = width as usize;
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut chars = input.chars();
    for _ in 0..width {
        let Some(ch) = chars.next() else {
            return out;
        };
        out.push(ch);
    }
    if chars.next().is_some() && width > 1 {
        out.pop();
        out.push('…');
    }
    out
}

fn session_status(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Working => "WORKING",
        SessionKind::Approved => "APPROVED",
        SessionKind::Diffing => "DIFFING",
        SessionKind::Idle => "IDLE",
        SessionKind::Blocked => "BLOCKED",
    }
}

fn status_dot(kind: SessionKind) -> Color {
    match kind {
        SessionKind::Approved => IOS_ORANGE,
        SessionKind::Blocked => IOS_DESTRUCTIVE,
        SessionKind::Working | SessionKind::Diffing | SessionKind::Idle => IOS_GREEN,
    }
}

fn session_fill(kind: SessionKind) -> Color {
    match kind {
        SessionKind::Working => CARD_GREEN,
        SessionKind::Approved => CARD_AMBER,
        SessionKind::Diffing => CARD_BLUE,
        SessionKind::Idle => CARD_PURPLE,
        SessionKind::Blocked => CARD_MAROON,
    }
}

fn session_border(kind: SessionKind) -> Color {
    match kind {
        SessionKind::Working => GREEN_SOFT,
        SessionKind::Approved => Color::Rgb(92, 73, 37),
        SessionKind::Diffing => Color::Rgb(58, 88, 105),
        SessionKind::Idle => Color::Rgb(79, 61, 110),
        SessionKind::Blocked => Color::Rgb(104, 50, 55),
    }
}

fn context_color(context: &str) -> Color {
    let Some(raw) = context.trim().strip_suffix('%') else {
        return IOS_FG_MUTED;
    };
    match raw.parse::<u16>() {
        Ok(value) if value >= 52 => IOS_GREEN,
        Ok(_) => IOS_ORANGE,
        Err(_) => IOS_FG_MUTED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn sample_sessions() -> Vec<SessionCard<'static>> {
        vec![
            SessionCard::new(
                1,
                "codex-admin-kollarrobert",
                SessionKind::Working,
                "Run colony task ready --session 019e2685...",
                "gpt-5.5 xhigh",
                "54%",
                "10m 28s",
            )
            .badge("LIVE"),
            SessionCard::new(
                0,
                "codex-matt-gg",
                SessionKind::Working,
                "Patch lib/_env.sh -- env helper exporting",
                "gpt-5.5 xhigh",
                "47%",
                "10m 28s",
            ),
            SessionCard::new(
                2,
                "codex-ricsi-zazrifka",
                SessionKind::Approved,
                "apply_patch touching 3 files",
                "gpt-5.5 high",
                "49%",
                "9m 18s",
            )
            .badge("⚠ REVIEW"),
            SessionCard::new(
                3,
                "codex-fico-magnolia",
                SessionKind::Diffing,
                "git diff scripts/codex-fleet/probe-accounts.py",
                "gpt-5.5 high",
                "47%",
                "10m 30s",
            ),
        ]
    }

    #[test]
    fn selected_index_clamps_to_available_sessions() {
        let switcher = SessionSwitcher::new(sample_sessions());
        assert_eq!(
            switcher.selected_index(&SessionSwitcherState { selected: 99 }),
            3
        );
    }

    #[test]
    fn renders_design_d_session_switcher_chrome() {
        let switcher = SessionSwitcher::new(sample_sessions()).awaiting_review(1);
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();

        terminal
            .draw(|frame| {
                switcher.render(frame, frame.area(), &SessionSwitcherState { selected: 0 })
            })
            .unwrap();

        let frame = format!("{}", terminal.backend());
        for needle in [
            "CODEX-FLEET · SESSION SWITCHER",
            "4 workers · 1 awaiting review",
            "New",
            "worker",
            "PANE 1 · WORKING",
            "codex-admin-kollarrobert",
            "LIVE",
            "MODEL",
            "gpt-5.5 xhigh",
            "CONTEXT",
            "54%",
            "RUNTIME",
            "10m 28s",
            "▣ Focus",
            "Queue",
            "Pause",
            "Kill",
            "dismiss worker",
            "new worker",
        ] {
            assert!(
                frame.contains(needle),
                "session switcher should contain {needle:?}\n{frame}"
            );
        }
    }
}
