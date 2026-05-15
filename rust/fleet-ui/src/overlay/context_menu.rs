//! Context menu overlay widget.
//!
//! Widget-local types and rendering helpers for the iOS-style context
//! menu surface. Shared centering and shadow helpers live in
//! `super::{centered_overlay, card_shadow}`.

use super::{card_shadow, centered_overlay};
use crate::palette::*;
use ratatui::Frame;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

/// One row in a [`ContextMenu`] section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem<'a> {
    pub icon: &'a str,
    pub label: &'a str,
    pub shortcut: &'a str,
    pub destructive: bool,
}

impl<'a> MenuItem<'a> {
    pub fn new(icon: &'a str, label: &'a str, shortcut: &'a str) -> Self {
        Self {
            icon,
            label,
            shortcut,
            destructive: false,
        }
    }

    pub fn destructive(icon: &'a str, label: &'a str, shortcut: &'a str) -> Self {
        Self {
            icon,
            label,
            shortcut,
            destructive: true,
        }
    }
}

/// A visually separated group of menu rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    pub items: Vec<MenuItem<'a>>,
}

impl<'a> Section<'a> {
    pub fn new(items: Vec<MenuItem<'a>>) -> Self {
        Self { items }
    }
}

/// Reusable iOS-style context menu overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMenu<'a> {
    pub title: &'a str,
    pub status_dot: Color,
    pub badge: Option<(&'a str, Color, Color)>,
    pub sections: Vec<Section<'a>>,
}

impl<'a> ContextMenu<'a> {
    pub const WIDTH: u16 = 48;

    pub fn new(
        title: &'a str,
        status_dot: Color,
        badge: Option<(&'a str, Color, Color)>,
        sections: Vec<Section<'a>>,
    ) -> Self {
        Self {
            title,
            status_dot,
            badge,
            sections,
        }
    }

    pub fn height(&self) -> u16 {
        let item_count: u16 = self
            .sections
            .iter()
            .map(|section| section.items.len() as u16)
            .sum();
        let separators = (self.sections.len() as u16).saturating_sub(1);
        2 + 1 + item_count + separators + 1 + 2
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let rect = centered_overlay(area, Self::WIDTH, self.height());
        card_shadow(frame, rect, area);
        frame.render_widget(Clear, rect);
        frame.render_widget(glass_block(None, false), rect);

        let inner = Rect {
            x: rect.x + 2,
            y: rect.y + 1,
            width: rect.width.saturating_sub(4),
            height: rect.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut y = inner.y + 1;
        self.render_title(frame, inner, y);
        y += 1;

        render_hairline(frame, inner, y);
        y += 1;

        for (section_index, section) in self.sections.iter().enumerate() {
            if section_index > 0 {
                render_hairline(frame, inner, y);
                y += 1;
            }
            for item in &section.items {
                self.render_item(frame, inner, y, item);
                y += 1;
            }
        }
    }

    fn render_title(&self, frame: &mut Frame, inner: Rect, y: u16) {
        let mut title_spans = vec![status_dot(self.status_dot), Span::raw("  ")];
        if let Some((name, pct)) = self.title.rsplit_once("  %") {
            title_spans.push(Span::styled(
                name,
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            ));
            title_spans.push(Span::raw("  "));
            title_spans.push(Span::styled(
                format!("%{pct}"),
                Style::default().fg(IOS_FG_MUTED),
            ));
        } else {
            title_spans.push(Span::styled(
                self.title,
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            ));
        }
        frame.render_widget(
            Paragraph::new(Line::from(title_spans)),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );

        let Some((text, fg, bg)) = self.badge else {
            return;
        };
        let badge = format!(" {} ", text.trim());
        let badge_w = visible_width(&badge);
        if inner.width > badge_w {
            frame.render_widget(
                Paragraph::new(Line::from(badge_chip(text, fg, bg))),
                Rect {
                    x: inner.x + inner.width - badge_w,
                    y,
                    width: badge_w,
                    height: 1,
                },
            );
        }
    }

    fn render_item(&self, frame: &mut Frame, inner: Rect, y: u16, item: &MenuItem<'_>) {
        let fg = if item.destructive {
            IOS_DESTRUCTIVE
        } else {
            IOS_FG
        };
        let icon_bg = if item.destructive {
            Color::Rgb(58, 24, 24)
        } else {
            IOS_ICON_CHIP
        };
        let spans = vec![
            Span::styled(
                format!(" {} ", item.icon),
                Style::default().fg(fg).bg(icon_bg),
            ),
            Span::styled(format!("  {}", item.label), Style::default().fg(fg)),
        ];
        let chip_w = visible_width(item.shortcut).saturating_add(2).max(5);
        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                x: inner.x,
                y,
                width: inner.width.saturating_sub(chip_w + 1),
                height: 1,
            },
        );
        if inner.width > chip_w + 1 {
            frame.render_widget(
                Paragraph::new(Line::from(shortcut_chip(item.shortcut))),
                Rect {
                    x: inner.x + inner.width - chip_w,
                    y,
                    width: chip_w,
                    height: 1,
                },
            );
        }
    }
}

fn glass_block(title: Option<&str>, solid: bool) -> Block<'_> {
    let fill = if solid { IOS_BG_SOLID } else { IOS_BG_GLASS };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
        .style(Style::default().bg(fill).fg(IOS_FG));
    if let Some(title) = title {
        block = block.title(Span::styled(
            format!(" {title} "),
            Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
        ));
    }
    block
}

fn shortcut_chip(s: &str) -> Span<'static> {
    Span::styled(
        format!(" {s} "),
        Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
    )
}

fn status_dot(color: Color) -> Span<'static> {
    Span::styled("●", Style::default().fg(color))
}

fn badge_chip(text: &str, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", text.trim()),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

fn render_hairline(frame: &mut Frame, inner: Rect, y: u16) {
    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(hairline, Style::default().fg(IOS_HAIRLINE))),
        Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        },
    );
}

fn visible_width(s: &str) -> u16 {
    s.chars().count() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_chip_uses_requested_colors() {
        let span = badge_chip("● LIVE", Color::Rgb(10, 36, 21), IOS_GREEN);
        assert_eq!(span.content, " ● LIVE ");
        assert_eq!(span.style.fg, Some(Color::Rgb(10, 36, 21)));
        assert_eq!(span.style.bg, Some(IOS_GREEN));
    }
}
