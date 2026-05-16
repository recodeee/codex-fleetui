use fleet_ui::palette::{
    IOS_DESTRUCTIVE as IOS_RED, IOS_GREEN, IOS_ORANGE, IOS_TINT as IOS_BLUE,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap},
};

// Local surface tones — this mock dashboard runs on a deeper-than-`IOS_BG_SOLID`
// dark surface and keeps its own greyscale ramp so the panels read as a
// separate visual context from the production fleet board. Only the iOS-named
// accent colors above ride the canonical `fleet-ui::palette` values.
const BG: Color = Color::Rgb(0x10, 0x12, 0x16);
const SURFACE: Color = Color::Rgb(0x1a, 0x1d, 0x24);
const SURFACE_ALT: Color = Color::Rgb(0x20, 0x24, 0x2d);
const SURFACE_RAISED: Color = Color::Rgb(0x27, 0x2c, 0x37);
const BORDER: Color = Color::Rgb(0x3a, 0x40, 0x4d);
const TEXT: Color = Color::Rgb(0xf3, 0xf5, 0xf8);
const MUTED: Color = Color::Rgb(0xa7, 0xae, 0xbb);
const FAINT: Color = Color::Rgb(0x78, 0x80, 0x8e);

// Tinted soft-fill washes derived from the accent colors — kept local because
// they are intentionally dark muted bg tints not present in the canonical palette.
const BLUE_SOFT: Color = Color::Rgb(0x0f, 0x24, 0x42);
const ORANGE_SOFT: Color = Color::Rgb(0x35, 0x27, 0x11);
const RED_SOFT: Color = Color::Rgb(0x37, 0x16, 0x14);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveState {
    Fresh,
    Warning,
    Stale,
}

impl LiveState {
    fn from_age(age_secs: u64) -> Self {
        match age_secs {
            0..=12 => LiveState::Fresh,
            13..=35 => LiveState::Warning,
            _ => LiveState::Stale,
        }
    }

    fn label(self) -> &'static str {
        match self {
            LiveState::Fresh => "LIVE",
            LiveState::Warning => "LAG",
            LiveState::Stale => "STALE",
        }
    }

    fn accent(self) -> Color {
        match self {
            LiveState::Fresh => IOS_BLUE,
            LiveState::Warning => IOS_ORANGE,
            LiveState::Stale => IOS_RED,
        }
    }

    fn soft_bg(self) -> Color {
        match self {
            LiveState::Fresh => BLUE_SOFT,
            LiveState::Warning => ORANGE_SOFT,
            LiveState::Stale => RED_SOFT,
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            LiveState::Fresh => "tick current",
            LiveState::Warning => "tick slipping",
            LiveState::Stale => "poll stalled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveIndicator {
    tick_age_secs: u64,
}

impl LiveIndicator {
    pub fn new(tick_age_secs: u64) -> Self {
        Self { tick_age_secs }
    }

    fn state(self) -> LiveState {
        LiveState::from_age(self.tick_age_secs)
    }
}

impl Widget for LiveIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 18 || area.height < 5 {
            return;
        }

        let state = self.state();
        let block = panel(Some("LIVE TICK"), state.accent(), state.soft_bg(), true);
        let inner = block.inner(area);
        block.render(area, buf);
        fill(inner, state.soft_bg(), buf);

        let columns = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        line(
            buf,
            columns[0],
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default()
                        .fg(state.accent())
                        .bg(state.soft_bg())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    state.label(),
                    Style::default()
                        .fg(TEXT)
                        .bg(state.soft_bg())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {}s", self.tick_age_secs),
                    Style::default().fg(MUTED).bg(state.soft_bg()),
                ),
            ]),
        );

        line(
            buf,
            columns[1],
            Line::from(Span::styled(
                format!("{}s ago", self.tick_age_secs),
                Style::default()
                    .fg(state.accent())
                    .bg(state.soft_bg())
                    .add_modifier(Modifier::BOLD),
            )),
        );

        line(
            buf,
            columns[2],
            Line::from(Span::styled(
                state.subtitle(),
                Style::default().fg(MUTED).bg(state.soft_bg()),
            )),
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IosPageDesign {
    tick_age_secs: u64,
    cap_pool_pct: u8,
}

impl IosPageDesign {
    pub fn new(tick_age_secs: u64, cap_pool_pct: u8) -> Self {
        Self {
            tick_age_secs,
            cap_pool_pct: cap_pool_pct.min(100),
        }
    }

    fn cap_pool_bar(self, width: u16) -> String {
        progress_rail(self.cap_pool_pct, width)
    }
}

impl Widget for IosPageDesign {
    fn render(self, area: Rect, buf: &mut Buffer) {
        fill(area, BG, buf);

        if area.width < 72 || area.height < 24 {
            return;
        }

        let root = area.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(9),
                Constraint::Min(0),
            ])
            .split(root);

        self.render_hero(rows[0], buf);
        self.render_summary(rows[1], buf);
        self.render_recent(rows[2], buf);
    }
}

impl IosPageDesign {
    fn render_hero(self, area: Rect, buf: &mut Buffer) {
        let block = panel(Some("WATCHER"), IOS_BLUE, SURFACE, true);
        let inner = block.inner(area);
        block.render(area, buf);
        fill(inner, SURFACE, buf);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(inner);

        let left = cols[0].inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        line(
            buf,
            Rect {
                x: left.x,
                y: left.y,
                width: left.width,
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    "iOS page design",
                    Style::default()
                        .fg(TEXT)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ·  ", Style::default().fg(FAINT).bg(SURFACE)),
                Span::styled(
                    "cap pool + live tick",
                    Style::default().fg(MUTED).bg(SURFACE),
                ),
            ]),
        );

        line(
            buf,
            Rect {
                x: left.x,
                y: left.y + 1,
                width: left.width,
                height: 1,
            },
            Line::from(Span::styled(
                "watcher board in iOS chrome",
                Style::default()
                    .fg(TEXT)
                    .bg(SURFACE)
                    .add_modifier(Modifier::BOLD),
            )),
        );

        line(
            buf,
            Rect {
                x: left.x,
                y: left.y + 3,
                width: left.width,
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    "cap pool",
                    Style::default()
                        .fg(IOS_BLUE)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}% used", self.cap_pool_pct),
                    Style::default().fg(TEXT).bg(SURFACE),
                ),
                Span::styled(
                    format!("  ·  {} spare", 100u8.saturating_sub(self.cap_pool_pct)),
                    Style::default().fg(MUTED).bg(SURFACE),
                ),
            ]),
        );

        line(
            buf,
            Rect {
                x: left.x,
                y: left.y + 4,
                width: left.width,
                height: 1,
            },
            Line::from(Span::styled(
                self.cap_pool_bar(left.width.min(28)),
                Style::default()
                    .fg(state_for_pool(self.cap_pool_pct).accent())
                    .bg(SURFACE),
            )),
        );

        LiveIndicator::new(self.tick_age_secs).render(cols[1], buf);
    }

    fn render_summary(self, area: Rect, buf: &mut Buffer) {
        let cards = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(36),
                Constraint::Percentage(32),
                Constraint::Percentage(32),
            ])
            .split(area);

        stat_card(
            buf,
            cards[0],
            "CAP POOL",
            format!("{}% used", self.cap_pool_pct),
            format!("{} spare", 100u8.saturating_sub(self.cap_pool_pct)),
            self.cap_pool_bar(cards[0].width.saturating_sub(6).min(24)),
            IOS_BLUE,
        );

        stat_card(
            buf,
            cards[1],
            "FRESH TICKS",
            "18 live workers",
            "median 4s",
            "● 0 skipped",
            IOS_GREEN,
        );

        let alert_accent = LiveState::from_age(self.tick_age_secs).accent();
        stat_card(
            buf,
            cards[2],
            "ALERTS",
            if self.tick_age_secs <= 12 {
                "1 warning"
            } else {
                "3 warnings"
            },
            if self.tick_age_secs <= 12 {
                "0 blocked"
            } else {
                "1 stale"
            },
            if self.tick_age_secs <= 12 {
                "0 red lanes"
            } else {
                "refresh due"
            },
            alert_accent,
        );
    }

    fn render_recent(self, area: Rect, buf: &mut Buffer) {
        let block = panel(Some("RECENT LANES"), BORDER, SURFACE_ALT, false);
        let inner = block.inner(area);
        block.render(area, buf);
        fill(inner, SURFACE_ALT, buf);

        line(
            buf,
            Rect {
                x: inner.x + 1,
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    "lane",
                    Style::default()
                        .fg(TEXT)
                        .bg(SURFACE_ALT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "    state    age    note",
                    Style::default().fg(MUTED).bg(SURFACE_ALT),
                ),
            ]),
        );

        let rows = [
            (
                "ios-pages-sub-1",
                "ready",
                "1m",
                "watcher board clean",
                IOS_GREEN,
            ),
            (
                "ios-pages-sub-2",
                "live",
                "9s",
                "review queue synced",
                IOS_BLUE,
            ),
            (
                "ios-pages-sub-3",
                if self.tick_age_secs <= 12 {
                    "live"
                } else {
                    "stale"
                },
                if self.tick_age_secs <= 12 {
                    "4s"
                } else {
                    "51s"
                },
                if self.tick_age_secs <= 12 {
                    "fresh tick lane"
                } else {
                    "refresh needed"
                },
                LiveState::from_age(self.tick_age_secs).accent(),
            ),
        ];

        for (idx, (lane, state, age, note, accent)) in rows.into_iter().enumerate() {
            let y = inner.y + 2 + idx as u16;
            if y >= inner.y + inner.height {
                break;
            }
            line(
                buf,
                Rect {
                    x: inner.x + 1,
                    y,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                },
                Line::from(vec![
                    Span::styled(
                        fit(lane, 18),
                        Style::default()
                            .fg(TEXT)
                            .bg(SURFACE_ALT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {state:<6}"),
                        Style::default().fg(accent).bg(SURFACE_ALT),
                    ),
                    Span::styled(
                        format!("  {age:<5}"),
                        Style::default().fg(MUTED).bg(SURFACE_ALT),
                    ),
                    Span::styled(
                        format!("  {note}"),
                        Style::default().fg(FAINT).bg(SURFACE_ALT),
                    ),
                ]),
            );
        }
    }
}

fn panel<'a>(title: Option<&'a str>, accent: Color, bg: Color, bold: bool) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent).add_modifier(if bold {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .style(Style::default().bg(bg));

    if let Some(title) = title {
        block = block.title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(TEXT)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ));
    }

    block
}

fn fill(area: Rect, color: Color, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Block::default()
        .style(Style::default().bg(color))
        .render(area, buf);
}

fn line(buf: &mut Buffer, area: Rect, line: Line<'static>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Paragraph::new(line)
        .wrap(Wrap { trim: true })
        .render(area, buf);
}

fn fit(input: &str, width: u16) -> String {
    let width = width as usize;
    if width == 0 {
        return String::new();
    }

    let mut chars = input.chars();
    let mut out = String::new();
    for _ in 0..width {
        if let Some(ch) = chars.next() {
            out.push(ch);
        } else {
            return out;
        }
    }

    if chars.next().is_some() && width > 1 {
        out.pop();
        out.push('…');
    }

    out
}

fn progress_rail(pct: u8, width: u16) -> String {
    let width = width.max(1) as usize;
    let filled = ((pct.min(100) as usize) * width) / 100;
    format!(
        "▕{}{}▏",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

fn state_for_pool(pct: u8) -> LiveState {
    if pct >= 75 {
        LiveState::Stale
    } else if pct >= 50 {
        LiveState::Warning
    } else {
        LiveState::Fresh
    }
}

fn stat_card(
    buf: &mut Buffer,
    area: Rect,
    title: &'static str,
    headline: impl Into<String>,
    second: impl Into<String>,
    third: impl Into<String>,
    accent: Color,
) {
    let block = panel(Some(title), accent, SURFACE_RAISED, true);
    let inner = block.inner(area);
    block.render(area, buf);
    fill(inner, SURFACE_RAISED, buf);

    line(
        buf,
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(Span::styled(
            headline.into(),
            Style::default()
                .fg(TEXT)
                .bg(SURFACE_RAISED)
                .add_modifier(Modifier::BOLD),
        )),
    );
    line(
        buf,
        Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(Span::styled(
            second.into(),
            Style::default().fg(accent).bg(SURFACE_RAISED),
        )),
    );
    line(
        buf,
        Rect {
            x: inner.x + 1,
            y: inner.y + 2,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(Span::styled(
            third.into(),
            Style::default()
                .fg(accent)
                .bg(SURFACE_RAISED)
                .add_modifier(Modifier::BOLD),
        )),
    );
}

#[cfg(test)]
mod insta {
    pub use crate::assert_snapshot;

    #[macro_export]
    macro_rules! assert_snapshot {
        ($actual:expr, @$expected:literal $(,)?) => {{
            let actual = $actual;
            let expected: &str = $expected;
            assert_eq!(actual, expected);
        }};
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_snapshot(design: IosPageDesign) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(design, frame.area()))
            .unwrap();
        normalize(format!("{}", terminal.backend()))
    }

    fn normalize(text: String) -> String {
        text.lines()
            .map(|line| line.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn fresh_tick_snapshot() {
        let rendered = render_snapshot(IosPageDesign::new(4, 62));
        insta::assert_snapshot!(
            rendered,
            @r###""                                                                                                    "
"  ╭ WATCHER ─────────────────────────────────────────────────────────────────────────────────────╮  "
"  │                                                            ╭ LIVE TICK ─────────────────────╮│  "
"  │ iOS page design  ·  cap pool + live tick                   │● LIVE · 4s                     ││  "
"  │ watcher board in iOS chrome                                │4s ago                          ││  "
"  │                                                            │tick current                    ││  "
"  │ cap pool  62% used  ·  38 spare                            ╰────────────────────────────────╯│  "
"  ╰─▕█████████████████░░░░░░░░░░░▏───────────────────────────────────────────────────────────────╯  "
"  ╭ CAP POOL ───────────────────────╮╭ FRESH TICKS ───────────────╮╭ ALERTS ─────────────────────╮  "
"  │ 62% used                        ││ 18 live workers            ││ 1 warning                   │  "
"  │ 38 spare                        ││ median 4s                  ││ 0 blocked                   │  "
"  │ ▕██████████████░░░░░░░░░░▏      ││ ● 0 skipped                ││ 0 red lanes                 │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  ╰─────────────────────────────────╯╰────────────────────────────╯╰─────────────────────────────╯  "
"  ╭ RECENT LANES ────────────────────────────────────────────────────────────────────────────────╮  "
"  │ lane    state    age    note                                                                 │  "
"  │                                                                                              │  "
"  │ ios-pages-sub-1  ready   1m     watcher board clean                                          │  "
"  │ ios-pages-sub-2  live    9s     review queue synced                                          │  "
"  │ ios-pages-sub-3  live    4s     fresh tick lane                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  ╰──────────────────────────────────────────────────────────────────────────────────────────────╯  "
"                                                                                                    ""###
        );
    }

    #[test]
    fn stale_tick_snapshot() {
        let rendered = render_snapshot(IosPageDesign::new(52, 62));
        insta::assert_snapshot!(
            rendered,
            @r###""                                                                                                    "
"  ╭ WATCHER ─────────────────────────────────────────────────────────────────────────────────────╮  "
"  │                                                            ╭ LIVE TICK ─────────────────────╮│  "
"  │ iOS page design  ·  cap pool + live tick                   │● STALE · 52s                   ││  "
"  │ watcher board in iOS chrome                                │52s ago                         ││  "
"  │                                                            │poll stalled                    ││  "
"  │ cap pool  62% used  ·  38 spare                            ╰────────────────────────────────╯│  "
"  ╰─▕█████████████████░░░░░░░░░░░▏───────────────────────────────────────────────────────────────╯  "
"  ╭ CAP POOL ───────────────────────╮╭ FRESH TICKS ───────────────╮╭ ALERTS ─────────────────────╮  "
"  │ 62% used                        ││ 18 live workers            ││ 3 warnings                  │  "
"  │ 38 spare                        ││ median 4s                  ││ 1 stale                     │  "
"  │ ▕██████████████░░░░░░░░░░▏      ││ ● 0 skipped                ││ refresh due                 │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  │                                 ││                            ││                             │  "
"  ╰─────────────────────────────────╯╰────────────────────────────╯╰─────────────────────────────╯  "
"  ╭ RECENT LANES ────────────────────────────────────────────────────────────────────────────────╮  "
"  │ lane    state    age    note                                                                 │  "
"  │                                                                                              │  "
"  │ ios-pages-sub-1  ready   1m     watcher board clean                                          │  "
"  │ ios-pages-sub-2  live    9s     review queue synced                                          │  "
"  │ ios-pages-sub-3  stale   51s    refresh needed                                               │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  │                                                                                              │  "
"  ╰──────────────────────────────────────────────────────────────────────────────────────────────╯  "
"                                                                                                    ""###
        );
    }
}
