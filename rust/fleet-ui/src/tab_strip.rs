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

use crate::palette::*;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use serde::Deserialize;
use std::fs;
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
        // Glyphs chosen to read at terminal size and match the iOS design
        // mockup (image #10): a grid for the worker board, linked nodes for
        // fleet, a clipboard for plan, a heavier triple-wave, and a check.
        match self {
            Tab::Overview => "⊞", // overview: squared plus → worker grid
            Tab::Fleet => "⌬",    // fleet: linked-nodes / benzene ring
            Tab::Plan => "▤",     // plan: clipboard / lined sheet
            Tab::Waves => "≋",    // waves: triple wave (heavier than ≈)
            Tab::Review => "✓",   // review: check
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

/// Counter snapshot shape on disk. Missing keys render `–`.
#[derive(Debug, Default, Deserialize)]
struct CounterSnapshot {
    #[serde(default)]
    overview: Option<u64>,
    #[serde(default)]
    fleet: Option<u64>,
    #[serde(default)]
    plan: Option<u64>,
    #[serde(default)]
    waves: Option<u64>,
    #[serde(default)]
    review: Option<u64>,
    #[serde(default)]
    updated_at: Option<u64>,
}

impl CounterSnapshot {
    fn get(&self, tab: Tab) -> Option<u64> {
        match tab {
            Tab::Overview => self.overview,
            Tab::Fleet => self.fleet,
            Tab::Plan => self.plan,
            Tab::Waves => self.waves,
            Tab::Review => self.review,
        }
    }

    fn is_fresh(&self, now_unix: u64) -> bool {
        match self.updated_at {
            Some(ts) => now_unix.saturating_sub(ts) <= COUNTER_STALE_SECS,
            None => false,
        }
    }
}

/// Lazy read of the counters file. Returns `None` if missing or
/// unparseable; caller renders `–` in that case.
fn read_counters(path: &Path) -> Option<CounterSnapshot> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<CounterSnapshot>(&raw).ok()
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
        // Clamp the dock row to the smaller of the caller's `area.width` and
        // the explicit `self.width` budget so a bin can ask for a narrower
        // dock than the frame (e.g. when sharing the top row with a side
        // status). Most bins pass `area.width` and the two match.
        let dock_w = area.width.min(self.width.max(1));
        let row = Rect { x: area.x, y: area.y, width: dock_w, height: 1 };

        // Paint the dock background hairline so gaps between pills read as a
        // single floating bar rather than five disconnected pills.
        let bg = Paragraph::new(Span::styled(
            " ".repeat(row.width as usize),
            Style::default().bg(IOS_BG_SOLID),
        ));
        frame.render_widget(bg, row);

        let counters = read_counters(Path::new(COUNTERS_PATH));
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let counters_fresh = counters.as_ref().is_some_and(|c| c.is_fresh(now_unix));

        // ---------- Logo chip (left) ----------
        let clock = format_clock(now_unix);
        let logo_text = format!(" ◆ codex-fleet {clock} ");
        let logo_w = logo_text.chars().count() as u16;
        let logo_rect = Rect {
            x: row.x,
            y: row.y,
            width: logo_w.min(row.width),
            height: 1,
        };
        let logo_spans = vec![
            Span::styled(" ◆ ", Style::default().fg(IOS_TINT).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD)),
            Span::styled("codex-fleet ", Style::default().fg(IOS_FG).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{clock} "), Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS)),
        ];
        frame.render_widget(Paragraph::new(ratatui::text::Line::from(logo_spans)), logo_rect);

        // ---------- Live chip (right) ----------
        let live_text = format!(" ● live · {} ", self.tick);
        let live_w = live_text.chars().count() as u16;
        let live_rect = Rect {
            x: row.x + row.width.saturating_sub(live_w),
            y: row.y,
            width: live_w.min(row.width),
            height: 1,
        };
        let live_spans = vec![
            Span::styled(" ● ", Style::default().fg(IOS_GREEN).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD)),
            Span::styled("live ", Style::default().fg(IOS_FG).bg(IOS_BG_GLASS).add_modifier(Modifier::BOLD)),
            Span::styled(format!("· {} ", self.tick), Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS)),
        ];
        frame.render_widget(Paragraph::new(ratatui::text::Line::from(live_spans)), live_rect);

        // ---------- Tabs (centre) ----------
        let tabs_left = logo_rect.x + logo_rect.width + 1;
        let tabs_right = live_rect.x.saturating_sub(1);
        if tabs_right <= tabs_left {
            return Vec::new();
        }
        let avail = tabs_right - tabs_left;

        let pills: Vec<(Tab, String, u16, bool)> = Tab::ALL
            .iter()
            .map(|&t| {
                let counter = render_counter(counters.as_ref(), t, counters_fresh);
                let active = t == self.active;
                let label = format_pill_label(t, &counter, active);
                let w = label.chars().count() as u16;
                (t, label, w, active)
            })
            .collect();

        // Gap of 1 cell between pills, plus 2 cells of caps (◖ + ◗) per
        // pill. Total natural width = sum(label widths) + 2*n caps + (n-1) gaps.
        let natural_total: u16 = pills.iter().map(|(_, _, w, _)| *w).sum::<u16>()
            + (pills.len() as u16) * 2
            + (pills.len().saturating_sub(1) as u16);

        // Centre the bar within available space; if natural exceeds it, clip
        // from the right (still better than overlapping the live chip).
        let start_x = if natural_total < avail {
            tabs_left + (avail - natural_total) / 2
        } else {
            tabs_left
        };

        let mut hits = Vec::with_capacity(pills.len());
        let mut x = start_x;
        for (tab, label, w, active) in pills {
            // Each pill now renders as three spans: ◖ <label> ◗. Caps reuse
            // the iOS-half-circle glyphs already used by the fleet status
            // chips, so the pill reads as a continuous rounded shape instead
            // of a hard-edged rectangle. Each cap occupies one cell, so the
            // total visible width is `w + 2`.
            const CAP_LEFT: &str = "◖";
            const CAP_RIGHT: &str = "◗";
            let pill_w = w + 2;
            if x + pill_w > tabs_right + 1 {
                break;
            }
            let pill_rect = Rect { x, y: row.y, width: pill_w, height: 1 };
            let (fill, fg) = if active {
                (IOS_TINT, IOS_FG)
            } else {
                (IOS_BG_GLASS, IOS_FG_MUTED)
            };
            // Caps punch the fill colour out of the dock background. Label
            // span gets the fill as its bg so the inner content lies on a
            // continuous coloured pill. Active pills are bold; inactive
            // stay regular weight for hierarchy.
            let cap_style = Style::default().fg(fill).bg(IOS_BG_SOLID);
            let mut label_style = Style::default().fg(fg).bg(fill);
            if active {
                label_style = label_style.add_modifier(Modifier::BOLD);
            }
            let spans = vec![
                Span::styled(CAP_LEFT, cap_style),
                Span::styled(label, label_style),
                Span::styled(CAP_RIGHT, cap_style),
            ];
            frame.render_widget(
                Paragraph::new(ratatui::text::Line::from(spans)),
                pill_rect,
            );
            hits.push(TabHit { rect: pill_rect, tab, window_idx: tab.window_idx() });
            x += pill_w + 1; // 1-cell gap between pills
        }
        hits
    }
}

/// Build the pill text. Active pills get 1.3× width via extra padding on
/// both sides, and a leading numbered circle. Inactive pills stay compact.
fn format_pill_label(tab: Tab, counter: &str, active: bool) -> String {
    let idx = tab.window_idx();
    let icon = tab.icon();
    let label = tab.label();
    if active {
        // Active: "  ⓘ ⊙  Overview  7  "
        format!("  {idx} {icon}  {label}  {counter}  ")
    } else {
        // Inactive: " ⓘ ⊙ Overview 7 "
        format!(" {idx} {icon} {label} {counter} ")
    }
}

/// Stale data, missing key, or missing file → en-dash. Fresh count → decimal.
fn render_counter(snap: Option<&CounterSnapshot>, tab: Tab, fresh: bool) -> String {
    if !fresh {
        return "–".to_string();
    }
    match snap.and_then(|s| s.get(tab)) {
        Some(n) => n.to_string(),
        None => "–".to_string(),
    }
}

/// Format a Unix timestamp as a local-ish `HH:MM:SS` for the logo chip.
///
/// No `chrono` in the workspace, so we use `std::time` + the `TZ`
/// environment variable when set; otherwise the value is **UTC**. The exact
/// timezone is not load-bearing — this is a visual freshness cue, not a
/// scheduling source of truth. Re-renders happen each frame.
fn format_clock(now_unix: u64) -> String {
    // Apply a coarse TZ offset if the operator set `TZ_OFFSET_SECS` (used
    // by the existing fleet scripts). Otherwise display UTC.
    let offset: i64 = std::env::var("TZ_OFFSET_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let local = (now_unix as i64 + offset).max(0) as u64;
    let secs_of_day = local % 86_400;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
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
        // Same label + counter; active must render wider thanks to extra
        // padding spaces around the inner text.
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
        // updated_at older than threshold ⇒ not fresh ⇒ dash, even if a
        // value is present.
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
        // Missing key ⇒ dash even when fresh.
        assert_eq!(render_counter(Some(&snap), Tab::Review, true), "–");
    }

    #[test]
    fn clock_formats_zero_pad() {
        // 0 unix → 00:00:00 (UTC, no offset).
        assert_eq!(format_clock(0), "00:00:00");
        // 1 h 2 m 3 s past epoch.
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
