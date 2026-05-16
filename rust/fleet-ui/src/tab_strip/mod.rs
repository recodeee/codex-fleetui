//! Glass-dock in-binary tab strip — the top-of-frame pill row each codex-fleet
//! bin renders to match the new tmux-level tab strip.
//!
//! Anatomy (single row, height = 1):
//!
//! ```text
//! [◆ codex-fleet HH:MM:SS]  ( 0 ⊙ Overview 7 ) [ 1 ⊙ Fleet 12 ] [ 2 ⊙ Plan 3 ] [ 3 ⊙ Waves 1 ] [ 4 ⊙ Review – ]   [● live · 1234]
//!  └─ logo chip ──────────┘  └─ active pill ──┘ └─ inactive pills ────────────────────────────────────────────┘   └─ live chip ─┘
//! ```
//!
//! - **Logo chip** (left): `◆ codex-fleet` + a muted `HH:MM:SS` from
//!   `std::time::SystemTime::now()` (no `chrono` in workspace; we format the
//!   local wall clock by hand).
//! - **Tabs** (centre): fixed 5-tab order `Overview / Fleet / Plan / Waves /
//!   Review`. Each pill carries `<idx> <icon> <label> <counter>`. The active
//!   tab fills `IOS_TINT` (systemBlue, `#0a84ff`) with bold `IOS_FG` text and
//!   renders ~1.3× wider than inactives (extra padding spaces). Inactive
//!   pills are `IOS_BG_GLASS` with `IOS_FG_MUTED` (`#a0a0aa`) labels.
//! - **Counter badge**: pulled from
//!   `/tmp/claude-viz/fleet-tab-counters.json` (snapshot written every 5 s by
//!   tmux side). Missing file, parse failure, or `updated_at` older than 30 s
//!   renders the en-dash `–` so the operator can spot stale data.
//! - **Live chip** (right): `●` in `IOS_GREEN` + `live` + a short tick count
//!   (frame counter passed in by the caller; bins increment per render).
//!
//! Hit testing: [`TabStrip::render`] returns a `Vec<TabHit>` with the screen
//! `Rect` of each pill and its tmux window index, so the bin's existing
//! `MouseEvent::Down(Left)` handler keeps firing
//! `tmux select-window -t codex-fleet:<idx>` unchanged.

mod layout;
mod render;

use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Path the tmux side writes counters to every 5 s.
pub const COUNTERS_PATH: &str = "/tmp/claude-viz/fleet-tab-counters.json";

/// Stale threshold: if `updated_at` is older than this, render `–`.
const COUNTER_STALE_SECS: u64 = 30;

/// The five canonical tabs, in their fixed dock order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Fleet,
    Plan,
    Waves,
    Review,
}

impl Tab {
    /// tmux window index (`codex-fleet:<idx>`).
    pub fn window_idx(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Fleet => 1,
            Tab::Plan => 2,
            Tab::Waves => 3,
            Tab::Review => 4,
        }
    }

    /// Short single-glyph icon for the pill. Uses generic Unicode so it
    /// renders without Nerd Fonts; falls back to ASCII visually if the
    /// terminal lacks the glyph.
    pub fn icon(self) -> &'static str {
        match self {
            Tab::Overview => "⊞",
            Tab::Fleet => "⌬",
            Tab::Plan => "▤",
            Tab::Waves => "≋",
            Tab::Review => "✓",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Fleet => "Fleet",
            Tab::Plan => "Plan",
            Tab::Waves => "Waves",
            Tab::Review => "Review",
        }
    }

    /// Key in the counters JSON.
    pub fn counter_key(self) -> &'static str {
        match self {
            Tab::Overview => "overview",
            Tab::Fleet => "fleet",
            Tab::Plan => "plan",
            Tab::Waves => "waves",
            Tab::Review => "review",
        }
    }

    pub const ALL: [Tab; 5] = [Tab::Overview, Tab::Fleet, Tab::Plan, Tab::Waves, Tab::Review];
}

/// One pill's hit-test rectangle + tmux window index. The bin's mouse
/// handler walks these on `MouseEvent::Down(Left)`.
#[derive(Clone, Copy, Debug)]
pub struct TabHit {
    pub rect: Rect,
    pub tab: Tab,
    pub window_idx: usize,
}

/// Glass-dock tab strip. Build with [`TabStrip::new`], paint with
/// [`TabStrip::render`].
pub struct TabStrip {
    active: Tab,
    width: u16,
    /// Frame / tick counter shown in the live chip. Increments per render
    /// from the bin's loop.
    pub tick: u64,
}

impl TabStrip {
    pub fn new(active: Tab, width: u16) -> Self {
        Self { active, width, tick: 0 }
    }

    pub fn with_tick(mut self, tick: u64) -> Self {
        self.tick = tick;
        self
    }

    /// Render the dock into `area` (typically a 1-row strip at the very top
    /// of the frame). Returns the hit-test rects for each pill so the bin's
    /// mouse handler can fire `tmux select-window`.
    pub fn render(&self, frame: &mut Frame, area: Rect) -> Vec<TabHit> {
        if area.height == 0 || area.width == 0 {
            return Vec::new();
        }
        let dock_w = area.width.min(self.width.max(1));
        let row = Rect { x: area.x, y: area.y, width: dock_w, height: 1 };

        render::paint_background(frame, row);

        let counters = layout::read_counters(Path::new(COUNTERS_PATH));
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let counters_fresh = counters.as_ref().is_some_and(|c| c.is_fresh(now_unix));

        let clock = layout::format_clock(now_unix);
        let logo_rect = render::paint_logo_chip(frame, row, &clock);
        let live_rect = render::paint_live_chip(frame, row, self.tick);

        let tabs_left = logo_rect.x + logo_rect.width + 1;
        let tabs_right = live_rect.x.saturating_sub(1);
        if tabs_right <= tabs_left {
            return Vec::new();
        }
        let avail = tabs_right - tabs_left;

        let pills = layout::build_pills(self.active, counters.as_ref(), counters_fresh);
        let natural_total = layout::natural_total_width(&pills);

        let start_x = if natural_total < avail {
            tabs_left + (avail - natural_total) / 2
        } else {
            tabs_left
        };

        let mut hits = Vec::with_capacity(pills.len());
        let mut x = start_x;
        for pill in pills {
            let pill_w = pill.label_w + 2;
            if x + pill_w > tabs_right + 1 {
                break;
            }
            let hit = render::render_pill(frame, x, row.y, pill);
            hits.push(hit);
            x += pill_w + 1;
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::layout::{format_clock, format_pill_label, render_counter, CounterSnapshot};
    use super::*;

    #[test]
    fn tab_window_idx_is_fixed_order() {
        assert_eq!(Tab::Overview.window_idx(), 0);
        assert_eq!(Tab::Fleet.window_idx(), 1);
        assert_eq!(Tab::Plan.window_idx(), 2);
        assert_eq!(Tab::Waves.window_idx(), 3);
        assert_eq!(Tab::Review.window_idx(), 4);
    }

    #[test]
    fn active_pill_is_wider_than_inactive() {
        let active = format_pill_label(Tab::Overview, "7", true);
        let inactive = format_pill_label(Tab::Overview, "7", false);
        assert!(
            active.chars().count() > inactive.chars().count(),
            "active pill must be wider; got active={} inactive={}",
            active.chars().count(),
            inactive.chars().count(),
        );
    }

    #[test]
    fn stale_snapshot_renders_dash() {
        let snap = CounterSnapshot {
            overview: Some(7),
            updated_at: Some(0),
            ..Default::default()
        };
        assert!(!snap.is_fresh(COUNTER_STALE_SECS + 100));
        assert_eq!(render_counter(Some(&snap), Tab::Overview, false), "–");
    }

    #[test]
    fn fresh_snapshot_renders_count() {
        let snap = CounterSnapshot {
            overview: Some(7),
            updated_at: Some(1_000),
            ..Default::default()
        };
        assert!(snap.is_fresh(1_000));
        assert_eq!(render_counter(Some(&snap), Tab::Overview, true), "7");
        assert_eq!(render_counter(Some(&snap), Tab::Review, true), "–");
    }

    #[test]
    fn clock_formats_zero_pad() {
        assert_eq!(format_clock(0), "00:00:00");
        assert_eq!(format_clock(3_723), "01:02:03");
    }

    #[test]
    fn json_parses_canonical_snapshot() {
        let raw = r#"{ "overview": 7, "fleet": 7, "plan": 12, "waves": 3, "review": 1, "updated_at": 1715712986 }"#;
        let snap: CounterSnapshot = serde_json::from_str(raw).expect("parse");
        assert_eq!(snap.overview, Some(7));
        assert_eq!(snap.plan, Some(12));
        assert_eq!(snap.review, Some(1));
        assert_eq!(snap.updated_at, Some(1_715_712_986));
    }
}
