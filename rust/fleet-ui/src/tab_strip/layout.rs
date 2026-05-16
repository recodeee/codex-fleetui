use super::{Tab, COUNTER_STALE_SECS};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub(super) struct CounterSnapshot {
    #[serde(default)]
    pub(super) overview: Option<u64>,
    #[serde(default)]
    pub(super) fleet: Option<u64>,
    #[serde(default)]
    pub(super) plan: Option<u64>,
    #[serde(default)]
    pub(super) waves: Option<u64>,
    #[serde(default)]
    pub(super) review: Option<u64>,
    #[serde(default)]
    pub(super) updated_at: Option<u64>,
}

impl CounterSnapshot {
    pub(super) fn get(&self, tab: Tab) -> Option<u64> {
        match tab {
            Tab::Overview => self.overview,
            Tab::Fleet => self.fleet,
            Tab::Plan => self.plan,
            Tab::Waves => self.waves,
            Tab::Review => self.review,
        }
    }

    pub(super) fn is_fresh(&self, now_unix: u64) -> bool {
        match self.updated_at {
            Some(ts) => now_unix.saturating_sub(ts) <= COUNTER_STALE_SECS,
            None => false,
        }
    }
}

pub(super) fn read_counters(path: &Path) -> Option<CounterSnapshot> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<CounterSnapshot>(&raw).ok()
}

/// Build the pill text. Active pills get 1.3× width via extra padding on
/// both sides, and a leading numbered circle. Inactive pills stay compact.
pub(super) fn format_pill_label(tab: Tab, counter: &str, active: bool) -> String {
    let idx = tab.window_idx();
    let icon = tab.icon();
    let label = tab.label();
    if active {
        format!("  {idx} {icon}  {label}  {counter}  ")
    } else {
        format!(" {idx} {icon} {label} {counter} ")
    }
}

/// Stale data, missing key, or missing file → en-dash. Fresh count → decimal.
pub(super) fn render_counter(snap: Option<&CounterSnapshot>, tab: Tab, fresh: bool) -> String {
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
/// No `chrono` in the workspace, so we use `std::time` + the `TZ_OFFSET_SECS`
/// environment variable when set; otherwise the value is **UTC**.
pub(super) fn format_clock(now_unix: u64) -> String {
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

/// One pill ready to paint: tab identity, finished label string, label cell
/// width (excluding caps), and active flag.
pub(super) struct PillSpec {
    pub(super) tab: Tab,
    pub(super) label: String,
    pub(super) label_w: u16,
    pub(super) active: bool,
}

pub(super) fn build_pills(
    active: Tab,
    counters: Option<&CounterSnapshot>,
    counters_fresh: bool,
) -> Vec<PillSpec> {
    Tab::ALL
        .iter()
        .map(|&t| {
            let counter = render_counter(counters, t, counters_fresh);
            let is_active = t == active;
            let label = format_pill_label(t, &counter, is_active);
            let w = label.chars().count() as u16;
            PillSpec { tab: t, label, label_w: w, active: is_active }
        })
        .collect()
}

/// Natural width sum of all pills (label widths + 2 caps each + 1-cell
/// gaps between adjacent pills).
pub(super) fn natural_total_width(pills: &[PillSpec]) -> u16 {
    pills.iter().map(|p| p.label_w).sum::<u16>()
        + (pills.len() as u16) * 2
        + (pills.len().saturating_sub(1) as u16)
}
