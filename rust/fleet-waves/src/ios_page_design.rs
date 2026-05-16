use fleet_ui::palette::{
    IOS_DESTRUCTIVE as DANGER, IOS_GREEN as SUCCESS, IOS_ORANGE as WARNING, IOS_TINT as ACCENT,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

// Local surface tones — the wave timeline runs on its own deeper-than-
// `IOS_BG_SOLID` ramp so the timeline panel reads as a distinct context.
// Only the iOS-named accent colors above ride canonical `fleet-ui::palette`.
const BG: Color = Color::Rgb(14, 17, 24);
const SURFACE: Color = Color::Rgb(22, 26, 36);
const SURFACE_ELEVATED: Color = Color::Rgb(30, 35, 47);
const TRACK: Color = Color::Rgb(42, 48, 63);
const BORDER: Color = Color::Rgb(63, 71, 89);
const TEXT: Color = Color::Rgb(240, 244, 250);
const MUTED: Color = Color::Rgb(163, 171, 188);
const FAINT: Color = Color::Rgb(102, 111, 130);
const DARK_TEXT: Color = Color::Rgb(13, 15, 20);

const CHIP_LABEL_WIDTH: u16 = 7;
const CHIP_WIDTH: u16 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowState {
    Done,
    Active,
    Queued,
    Blocked,
}

impl RowState {
    fn color(self) -> Color {
        match self {
            RowState::Done => SUCCESS,
            RowState::Active => ACCENT,
            RowState::Queued => WARNING,
            RowState::Blocked => DANGER,
        }
    }

    fn label(self) -> &'static str {
        match self {
            RowState::Done => "done",
            RowState::Active => "active",
            RowState::Queued => "queued",
            RowState::Blocked => "blocked",
        }
    }

    fn fill(self) -> u16 {
        match self {
            RowState::Done => 100,
            RowState::Active => 70,
            RowState::Queued => 46,
            RowState::Blocked => 28,
        }
    }

    fn title_fg(self) -> Color {
        match self {
            RowState::Done => DARK_TEXT,
            _ => TEXT,
        }
    }

    fn note(self) -> &'static str {
        match self {
            RowState::Done => "graph sealed and ready for handoff",
            RowState::Active => "blue lane spawning now",
            RowState::Queued => "waiting for the next refresh",
            RowState::Blocked => "needs a clean approval tick",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimelineRow {
    label: &'static str,
    title: &'static str,
    state: RowState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickState {
    Fresh,
    Stale { lag: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveIndicator {
    tick: u64,
    state: TickState,
}

impl LiveIndicator {
    pub fn fresh(tick: u64) -> Self {
        Self {
            tick,
            state: TickState::Fresh,
        }
    }

    pub fn stale(tick: u64, lag: u32) -> Self {
        Self {
            tick,
            state: TickState::Stale { lag },
        }
    }

    pub fn width(self) -> u16 {
        visible_width(&self.content_text()) + 2
    }

    fn fill_color(self) -> Color {
        match self.state {
            TickState::Fresh => SUCCESS,
            TickState::Stale { .. } => WARNING,
        }
    }

    fn content_text(self) -> String {
        match self.state {
            TickState::Fresh => format!(" ● live · tick {}", self.tick),
            TickState::Stale { lag } => format!(" ● stale · tick {} · lag {}", self.tick, lag),
        }
    }

    fn spans(self) -> Vec<Span<'static>> {
        let fill = self.fill_color();
        let mut spans = vec![Span::styled("◖", Style::default().fg(fill).bg(BG))];

        match self.state {
            TickState::Fresh => {
                spans.push(Span::styled(
                    " ● ",
                    Style::default()
                        .fg(SUCCESS)
                        .bg(fill)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    "live",
                    Style::default()
                        .fg(TEXT)
                        .bg(fill)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(" · ", Style::default().fg(MUTED).bg(fill)));
                spans.push(Span::styled(
                    format!("tick {}", self.tick),
                    Style::default().fg(TEXT).bg(fill),
                ));
            }
            TickState::Stale { lag } => {
                spans.push(Span::styled(
                    " ● ",
                    Style::default()
                        .fg(DANGER)
                        .bg(fill)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    "stale",
                    Style::default()
                        .fg(DANGER)
                        .bg(fill)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(" · ", Style::default().fg(MUTED).bg(fill)));
                spans.push(Span::styled(
                    format!("tick {}", self.tick),
                    Style::default().fg(TEXT).bg(fill),
                ));
                spans.push(Span::styled(" · ", Style::default().fg(MUTED).bg(fill)));
                spans.push(Span::styled("lag ", Style::default().fg(WARNING).bg(fill)));
                spans.push(Span::styled(
                    format!("{}", lag),
                    Style::default()
                        .fg(DANGER)
                        .bg(fill)
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        spans.push(Span::styled("◗", Style::default().fg(fill).bg(BG)));
        spans
    }
}

impl Widget for LiveIndicator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(Line::from(self.spans())).render(area, buf);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IosPageDesign {
    live: LiveIndicator,
    rows: Vec<TimelineRow>,
}

impl Default for IosPageDesign {
    fn default() -> Self {
        Self::fresh_demo()
    }
}

impl IosPageDesign {
    pub fn fresh_demo() -> Self {
        Self::demo(LiveIndicator::fresh(128))
    }

    pub fn stale_demo() -> Self {
        Self::demo(LiveIndicator::stale(128, 6))
    }

    pub fn demo(live: LiveIndicator) -> Self {
        Self {
            live,
            rows: vec![
                TimelineRow {
                    label: "W1",
                    title: "seed plan graph",
                    state: RowState::Done,
                },
                TimelineRow {
                    label: "W2",
                    title: "spawn page polish",
                    state: RowState::Active,
                },
                TimelineRow {
                    label: "W3",
                    title: "review queue",
                    state: RowState::Queued,
                },
                TimelineRow {
                    label: "W4",
                    title: "merge gate",
                    state: RowState::Blocked,
                },
            ],
        }
    }

    fn header_border(&self) -> Color {
        match self.live.state {
            TickState::Fresh => ACCENT,
            TickState::Stale { .. } => WARNING,
        }
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let block = panel(Some("WAVES"), self.header_border(), SURFACE_ELEVATED);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 4 {
            return;
        }

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let caption = vec![
            Span::styled("◆ ", Style::default().fg(ACCENT).bg(SURFACE_ELEVATED)),
            Span::styled(
                "iOS PAGE DESIGN",
                Style::default()
                    .fg(MUTED)
                    .bg(SURFACE_ELEVATED)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        Paragraph::new(Line::from(caption)).render(parts[0], buf);

        let title = Span::styled(
            "spawn timeline",
            Style::default()
                .fg(TEXT)
                .bg(SURFACE_ELEVATED)
                .add_modifier(Modifier::BOLD),
        );
        let title_area = Rect {
            x: parts[1].x,
            y: parts[1].y,
            width: parts[1].width.saturating_sub(self.live.width()),
            height: 1,
        };
        Paragraph::new(title).render(title_area, buf);

        let live_width = self.live.width();
        let live_x = parts[1].x + parts[1].width.saturating_sub(live_width);
        self.live.render(
            Rect {
                x: live_x,
                y: parts[1].y,
                width: live_width.min(parts[1].width),
                height: 1,
            },
            buf,
        );

        let subtitle = vec![
            Span::styled("iOS bg", Style::default().fg(ACCENT).bg(SURFACE_ELEVATED)),
            Span::styled(" · ", Style::default().fg(FAINT).bg(SURFACE_ELEVATED)),
            Span::styled(
                "live refresh",
                Style::default().fg(SUCCESS).bg(SURFACE_ELEVATED),
            ),
            Span::styled(" · ", Style::default().fg(FAINT).bg(SURFACE_ELEVATED)),
            Span::styled(
                "handoff cues",
                Style::default().fg(WARNING).bg(SURFACE_ELEVATED),
            ),
        ];
        Paragraph::new(Line::from(subtitle)).render(parts[2], buf);

        let mut legend = Vec::new();
        legend.extend(status_chip(RowState::Done));
        legend.push(Span::styled(" ", Style::default().bg(SURFACE_ELEVATED)));
        legend.extend(status_chip(RowState::Active));
        legend.push(Span::styled(" ", Style::default().bg(SURFACE_ELEVATED)));
        legend.extend(status_chip(RowState::Queued));
        legend.push(Span::styled(" ", Style::default().bg(SURFACE_ELEVATED)));
        legend.extend(status_chip(RowState::Blocked));
        Paragraph::new(Line::from(legend)).render(parts[3], buf);
    }

    fn render_timeline(&self, area: Rect, buf: &mut Buffer) {
        let block = panel(Some("SPAWN TIMELINE"), BORDER, SURFACE);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 8 {
            return;
        }

        let row_height = 2;
        let needed = (self.rows.len() as u16) * row_height;
        let split_height = needed.min(inner.height);
        let mut constraints = Vec::with_capacity(self.rows.len());
        for _ in 0..self.rows.len() {
            constraints.push(Constraint::Length(row_height));
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: split_height,
            });

        for (row_area, row) in rows.into_iter().zip(self.rows.iter()) {
            render_timeline_row(*row_area, *row, buf);
        }
    }
}

impl Widget for IosPageDesign {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Block::default()
            .style(Style::default().bg(BG))
            .render(area, buf);

        let inner = area.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });
        if inner.width < 60 || inner.height < 16 {
            return;
        }

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner);

        self.render_header(parts[0], buf);
        self.render_timeline(parts[2], buf);
    }
}

fn render_timeline_row(area: Rect, row: TimelineRow, buf: &mut Buffer) {
    if area.height < 2 || area.width < 24 {
        return;
    }

    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(CHIP_WIDTH),
        ])
        .split(lines[0]);

    Paragraph::new(Span::styled(
        row.label,
        Style::default()
            .fg(row.state.color())
            .bg(SURFACE)
            .add_modifier(Modifier::BOLD),
    ))
    .render(cols[0], buf);

    render_timeline_bar(cols[1], row, buf);
    Paragraph::new(Line::from(status_chip(row.state))).render(cols[2], buf);

    let note = vec![
        Span::styled("  • ", Style::default().fg(row.state.color()).bg(SURFACE)),
        Span::styled(row.state.note(), Style::default().fg(MUTED).bg(SURFACE)),
    ];
    Paragraph::new(Line::from(note)).render(lines[1], buf);
}

fn render_timeline_bar(area: Rect, row: TimelineRow, buf: &mut Buffer) {
    if area.width == 0 {
        return;
    }

    let fill_width = ((area.width as u32 * row.state.fill() as u32) / 100) as u16;
    let mut fill_width = fill_width.max(1).min(area.width);
    let title_width = visible_width(row.title).saturating_add(2);
    if title_width <= area.width {
        fill_width = fill_width.max(title_width.min(area.width));
    }

    let fill_label = clip(row.title, fill_width.saturating_sub(2));
    let fill_text = pad_visible(&format!(" {} ", fill_label), fill_width);
    let track_width = area.width.saturating_sub(fill_width);

    let mut spans = vec![Span::styled(
        fill_text,
        Style::default()
            .fg(row.state.title_fg())
            .bg(row.state.color())
            .add_modifier(Modifier::BOLD),
    )];
    if track_width > 0 {
        spans.push(Span::styled(
            " ".repeat(track_width as usize),
            Style::default().bg(TRACK),
        ));
    }

    Paragraph::new(Line::from(spans)).render(area, buf);
}

fn status_chip(state: RowState) -> Vec<Span<'static>> {
    let fill = state.color();
    vec![
        Span::styled("◖", Style::default().fg(fill).bg(BG)),
        Span::styled(
            format!(
                " ● {:<width$} ",
                state.label(),
                width = CHIP_LABEL_WIDTH as usize
            ),
            Style::default()
                .fg(TEXT)
                .bg(fill)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("◗", Style::default().fg(fill).bg(BG)),
    ]
}

fn panel<'a>(title: Option<&'a str>, border: Color, surface: Color) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(surface));
    if let Some(title) = title {
        block = block.title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
    }
    block
}

fn visible_width(text: &str) -> u16 {
    text.chars().count() as u16
}

fn clip(text: &str, max: u16) -> String {
    if max == 0 {
        return String::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() as u16 <= max {
        return text.to_string();
    }

    if max == 1 {
        return "…".to_string();
    }

    let mut out: String = chars.into_iter().take((max - 1) as usize).collect();
    out.push('…');
    out
}

fn pad_visible(text: &str, width: u16) -> String {
    let cur = visible_width(text);
    if cur >= width {
        text.chars().take(width as usize).collect()
    } else {
        let mut out = text.to_string();
        for _ in cur..width {
            out.push(' ');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn render_page(page: IosPageDesign) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(page, frame.area()))
            .unwrap();
        format!("{}", terminal.backend())
    }

    #[test]
    fn fresh_tick_renders_ios_page() {
        insta::assert_snapshot!(render_page(IosPageDesign::fresh_demo()));
    }

    #[test]
    fn stale_tick_renders_ios_page() {
        insta::assert_snapshot!(render_page(IosPageDesign::stale_demo()));
    }
}
