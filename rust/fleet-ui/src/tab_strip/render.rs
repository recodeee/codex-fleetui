use super::layout::PillSpec;
use super::TabHit;
use crate::palette::*;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const CAP_LEFT: &str = "◖";
const CAP_RIGHT: &str = "◗";

pub(super) fn paint_background(frame: &mut Frame, row: Rect) {
    let bg = Paragraph::new(Span::styled(
        " ".repeat(row.width as usize),
        Style::default().bg(IOS_BG_SOLID),
    ));
    frame.render_widget(bg, row);
}

pub(super) fn paint_logo_chip(frame: &mut Frame, row: Rect, clock: &str) -> Rect {
    let logo_text = format!(" ◆ codex-fleet {clock} ");
    let logo_w = logo_text.chars().count() as u16;
    let logo_rect = Rect {
        x: row.x,
        y: row.y,
        width: logo_w.min(row.width),
        height: 1,
    };
    let logo_spans = vec![
        Span::styled(
            " ◆ ",
            Style::default().fg(IOS_TINT).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "codex-fleet ",
            Style::default().fg(IOS_FG).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{clock} "),
            Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(logo_spans)), logo_rect);
    logo_rect
}

pub(super) fn paint_live_chip(frame: &mut Frame, row: Rect, tick: u64) -> Rect {
    let live_text = format!(" ● live · {tick} ");
    let live_w = live_text.chars().count() as u16;
    let live_rect = Rect {
        x: row.x + row.width.saturating_sub(live_w),
        y: row.y,
        width: live_w.min(row.width),
        height: 1,
    };
    let live_spans = vec![
        Span::styled(
            " ● ",
            Style::default().fg(IOS_GREEN).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "live ",
            Style::default().fg(IOS_FG).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {tick} "),
            Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(live_spans)), live_rect);
    live_rect
}

/// Paint a single pill at `x` on row `y` and return its hit-test entry.
///
/// Caps occupy one cell each on top of the label cell count, so the pill's
/// total visible width is `pill.label_w + 2`.
pub(super) fn render_pill(frame: &mut Frame, x: u16, y: u16, pill: PillSpec) -> TabHit {
    let pill_w = pill.label_w + 2;
    let pill_rect = Rect { x, y, width: pill_w, height: 1 };
    let (fill, fg) = if pill.active {
        (IOS_TINT, IOS_FG)
    } else {
        (IOS_BG_GLASS, IOS_FG_MUTED)
    };
    let cap_style = Style::default().fg(fill).bg(IOS_BG_SOLID);
    let mut label_style = Style::default().fg(fg).bg(fill);
    if pill.active {
        label_style = label_style.add_modifier(Modifier::BOLD);
    }
    let spans = vec![
        Span::styled(CAP_LEFT, cap_style),
        Span::styled(pill.label, label_style),
        Span::styled(CAP_RIGHT, cap_style),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), pill_rect);
    TabHit {
        rect: pill_rect,
        tab: pill.tab,
        window_idx: pill.tab.window_idx(),
    }
}
