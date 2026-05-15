//! Centered modal overlay helpers.
//!
//! Shared geometry/chrome primitives used by the overlay modules. Two
//! pieces:
//!
//! 1. [`centered_overlay`] — geometry helper. Returns a `Rect` of
//!    `width × height` centred inside `area`, clamped so it never
//!    exceeds the host frame.
//! 2. [`render_overlay`] — paints a `Clear` to wipe whatever was beneath
//!    the popup, then draws a rounded [`crate::card::card`] block. The
//!    caller renders their own content inside `Block::inner(rect)`.

use crate::card::card;
use crate::palette::*;
use ratatui::Frame;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub mod context_menu;
pub use context_menu::{ContextMenu, MenuItem, Section};

/// One command row in the reusable Spotlight palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpotlightItem {
    pub group: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub sub: &'static str,
    pub kbd: &'static str,
}

impl SpotlightItem {
    pub const fn new(
        group: &'static str,
        icon: &'static str,
        title: &'static str,
        sub: &'static str,
        kbd: &'static str,
    ) -> Self {
        Self {
            group,
            icon,
            title,
            sub,
            kbd,
        }
    }
}

/// Interactive Spotlight palette state owned by the caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotlightState {
    pub query: String,
    pub selected: usize,
    pub tick: u64,
}

/// Return Spotlight items whose title or sub-line contains `query`.
pub fn filter<'a>(items: &'a [SpotlightItem], query: &str) -> Vec<&'a SpotlightItem> {
    if query.is_empty() {
        return items.iter().collect();
    }

    let query = query.to_lowercase();
    items
        .iter()
        .filter(|item| {
            item.title.to_lowercase().contains(&query) || item.sub.to_lowercase().contains(&query)
        })
        .collect()
}

/// Reusable iOS-style Spotlight command palette overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spotlight;

impl Spotlight {
    pub const WIDTH: u16 = 78;
    pub const HEIGHT: u16 = 42;

    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &SpotlightState,
        items: &[SpotlightItem],
    ) {
        let filtered = filter(items, &state.query);
        let total = filtered.len();
        let selected = if total == 0 {
            0
        } else {
            state.selected.min(total - 1)
        };

        let rect = centered_overlay(area, Self::WIDTH, Self::HEIGHT);
        card_shadow(frame, rect, area);
        frame.render_widget(Clear, rect);
        frame.render_widget(spotlight_glass_block(), rect);

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
        y = render_spotlight_search_row(frame, inner, y, state);
        render_spotlight_hairline(frame, inner, y);
        y += 2;

        if total == 0 {
            render_spotlight_empty(frame, inner, y);
            render_spotlight_footer(frame, inner, total);
            return;
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "TOP HIT",
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y += 1;

        y = render_spotlight_top_hit(frame, inner, y, filtered[0], selected == 0);
        y += 1;

        let bottom_guard = inner.y + inner.height.saturating_sub(2);
        let mut last_group: Option<&str> = None;
        for (rank_index, item) in filtered.iter().enumerate().skip(1) {
            if y + 3 > bottom_guard {
                break;
            }
            if last_group != Some(item.group) {
                if last_group.is_some() {
                    y += 1;
                    if y + 3 > bottom_guard {
                        break;
                    }
                }
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        format!(" {}", item.group),
                        Style::default()
                            .fg(IOS_FG_MUTED)
                            .add_modifier(Modifier::BOLD),
                    ))),
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: 1,
                    },
                );
                y += 1;
                last_group = Some(item.group);
            }

            render_spotlight_result(frame, inner, y, item, rank_index == selected);
            y += 2;
        }

        render_spotlight_footer(frame, inner, total);
    }
}

/// Return a `Rect` of `width × height` centred inside `area`. If the
/// requested size exceeds `area`, the result is clamped to `area` (top-left
/// aligned in that degenerate case so nothing overflows the frame).
pub fn centered_overlay(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Render the standard overlay chrome (Clear + rounded card) into `rect`.
/// Returns the inner content `Rect` the caller should paint into.
pub fn render_overlay(frame: &mut Frame, rect: Rect, title: Option<&str>) -> Rect {
    frame.render_widget(Clear, rect);
    let block = card(title, false);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// Paint the standard 3D-ish overlay shadow: a band below the card plus a
/// two-column right edge strip, clipped to the host frame.
pub fn card_shadow(frame: &mut Frame, card_rect: Rect, area: Rect) {
    let shadow = Color::Rgb(0, 0, 4);
    let by = card_rect.y + card_rect.height;
    if by < area.y + area.height {
        let bx = card_rect.x + 2;
        let area_right = area.x + area.width;
        if bx < area_right {
            let bw = card_rect.width.min(area_right - bx);
            frame.render_widget(
                Block::default().style(Style::default().bg(shadow)),
                Rect {
                    x: bx,
                    y: by,
                    width: bw,
                    height: 1,
                },
            );
        }
    }

    let rx = card_rect.x + card_rect.width;
    let area_right = area.x + area.width;
    if rx < area_right {
        let rw = 2u16.min(area_right - rx);
        let area_bottom = area.y + area.height;
        let ry = card_rect.y + 1;
        if ry < area_bottom {
            let rh = card_rect.height.saturating_sub(1).min(area_bottom - ry);
            frame.render_widget(
                Block::default().style(Style::default().bg(shadow)),
                Rect {
                    x: rx,
                    y: ry,
                    width: rw,
                    height: rh,
                },
            );
        }
    }
}

fn spotlight_glass_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
        .style(Style::default().bg(IOS_BG_SOLID).fg(IOS_FG))
}

fn render_spotlight_search_row(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    state: &SpotlightState,
) -> u16 {
    let caret_on = (state.tick / 4) % 2 == 0;
    let caret_char = if caret_on { "▏" } else { " " };
    let query_display = if state.query.is_empty() {
        "type to filter…"
    } else {
        state.query.as_str()
    };
    let query_style = if state.query.is_empty() {
        Style::default().fg(IOS_FG_FAINT)
    } else {
        Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)
    };
    let query_spans = vec![
        Span::styled("⌕  ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled(query_display.to_string(), query_style),
        Span::styled(
            caret_char,
            Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
        ),
    ];
    let cmdk = " Ctrl K ";
    let cmdk_w = text_width(cmdk);
    frame.render_widget(
        Paragraph::new(Line::from(query_spans)),
        Rect {
            x: inner.x,
            y,
            width: inner.width.saturating_sub(cmdk_w + 1),
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cmdk,
            Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
        ))),
        Rect {
            x: inner.x + inner.width - cmdk_w,
            y,
            width: cmdk_w,
            height: 1,
        },
    );
    y + 1
}

fn render_spotlight_empty(frame: &mut Frame, inner: Rect, y: u16) {
    let msg = "no matches";
    let msg_w = text_width(msg);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: inner.x + (inner.width.saturating_sub(msg_w)) / 2,
            y: y + 3,
            width: msg_w,
            height: 1,
        },
    );
}

fn render_spotlight_top_hit(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    item: &SpotlightItem,
    active: bool,
) -> u16 {
    let hit_bg = if active {
        IOS_TINT
    } else {
        Color::Rgb(8, 80, 180)
    };
    let hit_rect = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 3,
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(hit_bg)),
        hit_rect,
    );

    let badge = format!(" tmux · {} ", item.kbd);
    let badge_w = text_width(&badge);
    let chevron = "  › ";
    let chevron_w = text_width(chevron);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().bg(hit_bg)),
            Span::styled(
                format!(" {} ", item.icon),
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(IOS_TINT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.title),
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(hit_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect {
            x: hit_rect.x,
            y: hit_rect.y + 1,
            width: hit_rect.width.saturating_sub(badge_w + chevron_w + 1),
            height: 1,
        },
    );
    if hit_rect.width > badge_w + chevron_w + 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                badge,
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(IOS_TINT_DARK)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: hit_rect.x + hit_rect.width - badge_w - chevron_w,
                y: hit_rect.y + 1,
                width: badge_w,
                height: 1,
            },
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                chevron,
                Style::default()
                    .fg(IOS_TINT_SUB)
                    .bg(hit_bg)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: hit_rect.x + hit_rect.width - chevron_w,
                y: hit_rect.y + 1,
                width: chevron_w,
                height: 1,
            },
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("      {}", item.sub),
            Style::default().fg(IOS_TINT_SUB).bg(hit_bg),
        ))),
        Rect {
            x: hit_rect.x,
            y: hit_rect.y + 2,
            width: hit_rect.width,
            height: 1,
        },
    );
    y + 3
}

fn render_spotlight_result(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    item: &SpotlightItem,
    selected: bool,
) {
    let row_bg = if selected { IOS_TINT_DARK } else { IOS_CARD_BG };
    let title_fg = if selected {
        Color::Rgb(255, 255, 255)
    } else {
        IOS_FG
    };
    let sub_fg = if selected { IOS_TINT_SUB } else { IOS_FG_MUTED };
    let item_rect = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 2,
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(row_bg)),
        item_rect,
    );

    let kbd = format!(" {} ", item.kbd);
    let kbd_w = text_width(&kbd);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().bg(row_bg)),
            Span::styled(
                format!(" {} ", item.icon),
                Style::default()
                    .fg(title_fg)
                    .bg(IOS_ICON_CHIP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.title),
                Style::default()
                    .fg(title_fg)
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect {
            x: inner.x,
            y,
            width: inner.width.saturating_sub(kbd_w + 2),
            height: 1,
        },
    );
    if inner.width > kbd_w + 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                kbd,
                Style::default()
                    .fg(title_fg)
                    .bg(IOS_ICON_CHIP)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x + inner.width - kbd_w - 1,
                y,
                width: kbd_w,
                height: 1,
            },
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("       {}", item.sub),
            Style::default().fg(sub_fg).bg(row_bg),
        ))),
        Rect {
            x: inner.x,
            y: y + 1,
            width: inner.width,
            height: 1,
        },
    );
}

fn render_spotlight_footer(frame: &mut Frame, inner: Rect, total: usize) {
    let y = inner.y + inner.height - 1;
    let footer = Line::from(vec![
        Span::styled("↵", Style::default().fg(IOS_FG)),
        Span::styled(" open · ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("⌥↵", Style::default().fg(IOS_FG)),
        Span::styled(" all panes · ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("esc", Style::default().fg(IOS_FG)),
        Span::styled(" cancel · ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("✦", Style::default().fg(IOS_PURPLE)),
        Span::styled(format!(" {total} items"), Style::default().fg(IOS_FG_MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        },
    );
}

fn render_spotlight_hairline(frame: &mut Frame, inner: Rect, y: u16) {
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

fn text_width(s: &str) -> u16 {
    s.chars().count() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_in_square_area() {
        let area = Rect::new(0, 0, 100, 100);
        let r = centered_overlay(area, 40, 20);
        assert_eq!(r, Rect::new(30, 40, 40, 20));
    }

    #[test]
    fn clamps_oversized_to_area() {
        let area = Rect::new(0, 0, 20, 10);
        let r = centered_overlay(area, 40, 20);
        assert_eq!(r.width, 20);
        assert_eq!(r.height, 10);
    }

    #[test]
    fn offset_area_centers_correctly() {
        let area = Rect::new(10, 10, 80, 80);
        let r = centered_overlay(area, 20, 20);
        // 80-20 = 60; 60/2 = 30; 10+30 = 40
        assert_eq!(r, Rect::new(40, 40, 20, 20));
    }
}
