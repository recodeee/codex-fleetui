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
use ratatui::Frame;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Clear},
};

pub mod context_menu;
pub use context_menu::{ContextMenu, MenuItem, Section};

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
