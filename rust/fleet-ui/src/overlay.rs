//! Centered modal overlay helpers.
//!
//! Foundation for the Image-2 command surfaces (context menu, Spotlight
//! palette, action sheet, session switcher). Two pieces:
//!
//! 1. [`centered_overlay`] — geometry helper. Returns a `Rect` of
//!    `width × height` centred inside `area`, clamped so it never
//!    exceeds the host frame.
//! 2. [`render_overlay`] — paints a `Clear` to wipe whatever was beneath
//!    the popup, then draws a rounded [`crate::card::card`] block. The
//!    caller renders their own content inside `Block::inner(rect)`.
//!
//! Ported from `fleet-tui-poc::center_rect` and the per-overlay
//! `render_*` heads that all start with `frame.render_widget(Clear, rect);
//! frame.render_widget(glass_block(...), rect);`.

use crate::card::card;
use ratatui::layout::Rect;
use ratatui::widgets::Clear;
use ratatui::Frame;

/// Return a `Rect` of `width × height` centred inside `area`. If the
/// requested size exceeds `area`, the result is clamped to `area` (top-left
/// aligned in that degenerate case so nothing overflows the frame).
pub fn centered_overlay(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
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
