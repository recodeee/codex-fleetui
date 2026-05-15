//! Rounded card widget — the canonical surface for grouped content.
//!
//! Ported from `fleet-tui-poc::glass_block` plus the bash convention used
//! across every dashboard (`╭─ TITLE ───╮` rounded headers, 2-space padding
//! inside, hairline borders). All consumers compose their own content
//! inside `Block::inner(rect)`; this module is purely the wrapper.

use crate::palette::*;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders};
use ratatui::Frame;

/// Build a rounded `Block` with the canonical iOS chrome.
///
/// - `BorderType::Rounded` ( `╭─╮╰─╯` ).
/// - Border colour: `IOS_HAIRLINE_STRONG` (or `IOS_TINT` when `active`).
/// - Background: `IOS_BG_SOLID` so the card visually pops from a darker
///   surround.
/// - Title (if Some) renders bold white in the top border (`╭─ TITLE ──╮`).
///
/// `active = true` thickens the border modifier with `BOLD` and recolours to
/// the systemBlue tint, matching the focus state on the session-switcher D
/// artboard.
pub fn card<'a>(title: Option<&'a str>, active: bool) -> Block<'a> {
    let border_color = if active {
        IOS_TINT
    } else {
        IOS_HAIRLINE_STRONG
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color).add_modifier(if active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .style(Style::default().bg(IOS_BG_SOLID));
    if let Some(t) = title {
        block = block.title(ratatui::text::Span::styled(
            format!(" {} ", t),
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ));
    }
    block
}

/// Same as `card` but explicitly accepts a custom accent colour for the
/// border (used by session-switcher cards where each card tints its border
/// to its worker's status colour).
pub fn card_accent<'a>(title: Option<&'a str>, accent: Color) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(IOS_BG_SOLID));
    if let Some(t) = title {
        block = block.title(ratatui::text::Span::styled(
            format!(" {} ", t),
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ));
    }
    block
}

/// Paint an inset iOS glass surface inside a card content area.
///
/// Dashboards use this after `card(...).inner(area)` when the card needs a
/// tinted sub-surface for grouped rows while keeping the public card chrome
/// stable.
pub fn card_inner(frame: &mut Frame, area: Rect, tint: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(IOS_HAIRLINE_BORDER).bg(tint))
            .style(Style::default().bg(tint)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;
    use ratatui::Terminal;

    #[test]
    fn renders_rounded_corners() {
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(area);
        card(Some("TITLE"), false).render(area, &mut buf);
        // Top-left rounded corner glyph ╭
        let s = buf.get(0, 0).symbol();
        assert_eq!(s, "╭", "top-left must be rounded; got {:?}", s);
        let s = buf.get(19, 0).symbol();
        assert_eq!(s, "╮", "top-right must be rounded; got {:?}", s);
        let s = buf.get(0, 4).symbol();
        assert_eq!(s, "╰", "bottom-left must be rounded; got {:?}", s);
        let s = buf.get(19, 4).symbol();
        assert_eq!(s, "╯", "bottom-right must be rounded; got {:?}", s);
    }

    #[test]
    fn inner_provides_padding() {
        let area = Rect::new(0, 0, 20, 5);
        let inner = card(None, false).inner(area);
        // ratatui Block reserves 1 cell on each side for the border.
        assert_eq!(inner, Rect::new(1, 1, 18, 3));
    }

    #[test]
    fn card_inner_paints_tinted_hairline_surface() {
        let mut terminal = Terminal::new(TestBackend::new(12, 6)).unwrap();
        terminal
            .draw(|frame| card_inner(frame, Rect::new(1, 1, 10, 4), IOS_BG_GLASS))
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].symbol(), "╭");
        assert_eq!(buffer[(10, 1)].symbol(), "╮");
        assert_eq!(buffer[(1, 4)].symbol(), "╰");
        assert_eq!(buffer[(10, 4)].symbol(), "╯");
        assert_eq!(buffer[(1, 1)].fg, IOS_HAIRLINE_BORDER);
        assert_eq!(buffer[(5, 2)].bg, IOS_BG_GLASS);
    }
}
