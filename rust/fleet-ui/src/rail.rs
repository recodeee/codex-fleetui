//! Segmented progress rail — `▕███░░░▏`.
//!
//! Ported from `scripts/codex-fleet/fleet-tick.sh::ios_progress_rail` plus
//! the `ios_axis_color` thresholding. A rail is a single-row gauge bracketed
//! by `▕` / `▏` end-caps with `█` (filled) / `░` (empty) cells inside. The
//! cell-colour shifts green → orange → red as `pct` declines for the
//! [`RailAxis::Usage`] axis, and inverts for [`RailAxis::Done`] (so the
//! "complete" axis goes red → orange → green).

use crate::palette::*;
use ratatui::style::{Color, Style};
use ratatui::text::Span;

const RAIL_LEFT: &str = "▕";
const RAIL_RIGHT: &str = "▏";
const FILL: &str = "█";
const EMPTY: &str = "░";

/// Which way the colour ramp runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailAxis {
    /// Capacity-used axis (≥75% → red, ≥50% → orange, otherwise green).
    /// Use for 5h-cap, weekly-cap, context-used.
    Usage,
    /// Completeness axis (inverted: ≥75% → green, ≥50% → orange, else red).
    /// Use for "tasks done", "tests passing".
    Done,
    /// Capacity-available axis. Identical thresholds to `Done`.
    Available,
}

/// Map a percentage + axis to its accent colour, matching
/// `fleet-tick.sh::ios_axis_color`.
pub fn rail_color(pct: u8, axis: RailAxis) -> Color {
    let pct = pct.min(100);
    let bucket: u8 = match axis {
        RailAxis::Usage => {
            if pct >= 75 { 2 } else if pct >= 50 { 1 } else { 0 }
        }
        RailAxis::Done | RailAxis::Available => {
            if pct >= 75 { 0 } else if pct >= 50 { 1 } else { 2 }
        }
    };
    match bucket {
        0 => IOS_GREEN,
        1 => IOS_ORANGE,
        _ => IOS_DESTRUCTIVE,
    }
}

/// Render the rail as a Span sequence: ` ▕<filled █s><empty ░s>▏ `. Caller
/// places it on a Line. `width` is the cell count between the end-caps;
/// total rendered width is `width + 2` (caps).
pub fn progress_rail(pct: u8, axis: RailAxis, width: u16) -> Vec<Span<'static>> {
    let pct = pct.min(100) as u32;
    let w = width.max(1) as u32;
    let filled = ((pct * w) / 100).min(w) as usize;
    let empty = (w as usize).saturating_sub(filled);
    let color = rail_color(pct as u8, axis);
    let mut spans = Vec::with_capacity(3);
    spans.push(Span::styled(RAIL_LEFT, Style::default().fg(color)));
    spans.push(Span::styled(
        FILL.repeat(filled),
        Style::default().fg(color),
    ));
    spans.push(Span::styled(
        EMPTY.repeat(empty),
        Style::default().fg(IOS_HAIRLINE),
    ));
    spans.push(Span::styled(RAIL_RIGHT, Style::default().fg(color)));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rail_has_zero_fill() {
        let s = progress_rail(0, RailAxis::Usage, 10);
        // s[0]=left cap, s[1]=filled (empty), s[2]=empty (10 ░s), s[3]=right cap
        assert_eq!(s[1].content, "");
        assert_eq!(s[2].content.chars().count(), 10);
    }

    #[test]
    fn full_rail_has_all_filled() {
        let s = progress_rail(100, RailAxis::Usage, 10);
        assert_eq!(s[1].content.chars().count(), 10);
        assert_eq!(s[2].content, "");
    }

    #[test]
    fn half_rail_splits_evenly() {
        let s = progress_rail(50, RailAxis::Usage, 10);
        assert_eq!(s[1].content.chars().count(), 5);
        assert_eq!(s[2].content.chars().count(), 5);
    }

    #[test]
    fn usage_color_thresholds() {
        assert_eq!(rail_color(10, RailAxis::Usage), IOS_GREEN);
        assert_eq!(rail_color(60, RailAxis::Usage), IOS_ORANGE);
        assert_eq!(rail_color(90, RailAxis::Usage), IOS_DESTRUCTIVE);
    }

    #[test]
    fn done_axis_inverts_ramp() {
        // Done at 10% is bad → red. Done at 90% is good → green.
        assert_eq!(rail_color(10, RailAxis::Done), IOS_DESTRUCTIVE);
        assert_eq!(rail_color(60, RailAxis::Done), IOS_ORANGE);
        assert_eq!(rail_color(90, RailAxis::Done), IOS_GREEN);
    }

    #[test]
    fn pct_clamps_above_100() {
        let s = progress_rail(200, RailAxis::Usage, 10);
        assert_eq!(s[1].content.chars().count(), 10, "pct > 100 must clamp to full");
    }
}
