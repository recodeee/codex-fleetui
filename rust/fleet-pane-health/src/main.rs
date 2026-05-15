// fleet-pane-health — per-pane health dashboard for codex-fleet.
//
// Each row corresponds to a live tmux pane in the fleet session and shows:
//   - pane id (e.g. "%337")
//   - the pane's `@panel` user-option label (codex-fleet's per-pane agent tag)
//   - last-activity age, derived from the mtime of any
//     /tmp/claude-viz/{kiro,claude,codex}-worker-<id>.log file matching the
//     panel id (newest one wins)
//   - colony claim state, parsed best-effort from
//     /tmp/claude-viz/colony-claims.json (falls back to "unknown")
//   - cap-probe state, parsed best-effort from
//     /tmp/claude-viz/cap-probe-cache/<email>.json (verdict + freshness)
//
// All data sources are best-effort and READ-ONLY. We never write to /tmp.
// If tmux is unavailable or no fleet session is up, the dashboard renders
// an empty list with an explanatory message in the header.
//
// Refresh cadence: poll every 1s (spec). Quit on `q` or Esc.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use fleet_ui::palette::*;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{DefaultTerminal, Frame};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const VIZ_ROOT: &str = "/tmp/claude-viz";
const CAP_PROBE_DIR: &str = "/tmp/claude-viz/cap-probe-cache";
const COLONY_CLAIMS: &str = "/tmp/claude-viz/colony-claims.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum AgentKind {
    Codex,
    Kiro,
    Claude,
    Unknown,
}

impl AgentKind {
    fn classify(panel: &str, activity_source: &str) -> Self {
        let p = panel.to_ascii_lowercase();
        if p.contains("kiro") {
            return Self::Kiro;
        }
        if p.contains("codex") {
            return Self::Codex;
        }
        if p.contains("claude") {
            return Self::Claude;
        }
        if activity_source.starts_with("kiro-worker-") {
            return Self::Kiro;
        }
        if activity_source.starts_with("codex-worker-") {
            return Self::Codex;
        }
        if activity_source.starts_with("claude-worker-") {
            return Self::Claude;
        }
        Self::Unknown
    }

    fn badge(&self) -> &'static str {
        match self {
            Self::Codex => "CODX",
            Self::Kiro => "KIRO",
            Self::Claude => "CLAU",
            Self::Unknown => "—",
        }
    }

    fn color(&self) -> ratatui::style::Color {
        match self {
            Self::Codex => IOS_TINT,
            Self::Kiro => IOS_PURPLE,
            Self::Claude => IOS_ORANGE,
            Self::Unknown => IOS_FG_FAINT,
        }
    }

    fn sort_key(&self) -> u8 {
        match self {
            Self::Codex => 0,
            Self::Kiro => 1,
            Self::Claude => 2,
            Self::Unknown => 3,
        }
    }

    fn group_label(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Kiro => "kiro",
            Self::Claude => "claude",
            Self::Unknown => "other",
        }
    }
}

#[derive(Clone, Debug)]
struct PaneRow {
    pane_id: String,            // e.g. "%337"
    panel: String,              // @panel label or "—"
    kind: AgentKind,            // codex / kiro / claude / unknown
    last_activity: Option<u64>, // seconds since now (0 = just now)
    activity_source: String,    // file name for the freshest log
    colony_claim: String,       // "claimed:<task>" | "free" | "unknown"
    cap_probe: String,          // "ok" / "429" / "unknown"
    cap_probe_age: Option<u64>, // seconds since mtime
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    rows: Vec<PaneRow>,
    note: Option<String>, // diagnostic shown in header when fleet is empty
    captured_at: Option<SystemTime>,
}

/// View toggles. Driven by the keyboard loop, not from data sources, so it
/// survives across snapshot refreshes.
#[derive(Clone, Copy, Debug, Default)]
struct View {
    /// When true, rows are sorted by [`AgentKind`] and group-header rows are
    /// injected between kinds.
    grouped: bool,
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut snapshot = collect_snapshot();
    let mut view = View::default();
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|frame| render(frame, frame.area(), &snapshot, &view))?;

        // Poll for keys with a short timeout so we can refresh at POLL_INTERVAL.
        let remaining = POLL_INTERVAL.saturating_sub(last_refresh.elapsed());
        let wait = remaining.min(Duration::from_millis(100));
        if event::poll(wait)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('g') | KeyCode::Char('G') => view.grouped = !view.grouped,
                        _ => {}
                    }
                }
            }
        }

        if last_refresh.elapsed() >= POLL_INTERVAL {
            snapshot = collect_snapshot();
            last_refresh = Instant::now();
        }
    }
}

// ---------------------------------------------------------------------------
// Data gathering
// ---------------------------------------------------------------------------

fn collect_snapshot() -> Snapshot {
    let session = std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".into());
    let panes = tmux_panes(&session);

    let log_index = build_log_index();
    let claim_index = read_colony_claims();
    let cap_index = read_cap_probe_cache();

    let note = if panes.is_empty() {
        Some(format!(
            "no panes from tmux session '{session}' — set CODEX_FLEET_SESSION / CODEX_FLEET_TMUX_SOCKET or start the fleet"
        ))
    } else {
        None
    };

    let mut rows: Vec<PaneRow> = panes
        .into_iter()
        .map(|(pane_id, panel)| {
            let activity = log_index.get(&panel).cloned().or_else(|| {
                // Fall back to a best-effort substring match against any log key.
                log_index
                    .iter()
                    .find(|(k, _)| !panel.is_empty() && k.contains(&panel))
                    .map(|(_, v)| v.clone())
            });
            let (last_activity, activity_source) = match activity {
                Some(a) => (Some(a.age_secs), a.file_name),
                None => (None, String::from("—")),
            };

            let colony_claim = claim_index
                .get(&panel)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            // cap-probe is keyed by email — we don't have a direct pane→email map.
            // If panel happens to contain an "@" we treat it as an email; otherwise
            // we show the freshest probe across the cache.
            let cap_entry = if panel.contains('@') {
                cap_index.get(&panel).cloned()
            } else {
                cap_index
                    .values()
                    .min_by_key(|e| e.age_secs)
                    .cloned()
            };
            let (cap_probe, cap_probe_age) = match cap_entry {
                Some(e) => (e.verdict, Some(e.age_secs)),
                None => ("unknown".into(), None),
            };

            let kind = AgentKind::classify(&panel, &activity_source);
            PaneRow {
                pane_id,
                panel,
                kind,
                last_activity,
                activity_source,
                colony_claim,
                cap_probe,
                cap_probe_age,
            }
        })
        .collect();

    rows.sort_by(|a, b| a.panel.cmp(&b.panel));

    Snapshot {
        rows,
        note,
        captured_at: Some(SystemTime::now()),
    }
}

/// Build a `tmux` command, honoring `CODEX_FLEET_TMUX_SOCKET` so the binary
/// queries the same socket the fleet runs on (full-bringup.sh uses
/// `-L codex-fleet`). When the env var is unset or empty, tmux's default
/// socket is used — preserving prior behavior.
fn tmux_command() -> Command {
    let mut cmd = Command::new("tmux");
    if let Ok(socket) = std::env::var("CODEX_FLEET_TMUX_SOCKET") {
        if !socket.is_empty() {
            cmd.args(["-L", &socket]);
        }
    }
    cmd
}

/// `tmux list-panes -s -t <session> -F '#{pane_id}\t#{@panel}'` — returns
/// (pane_id, panel) tuples. Empty list when tmux is absent or session missing.
fn tmux_panes(session: &str) -> Vec<(String, String)> {
    let output = tmux_command()
        .args([
            "list-panes",
            "-s",
            "-t",
            session,
            "-F",
            "#{pane_id}\t#{@panel}",
        ])
        .output();
    let Ok(out) = output else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let pane_id = parts.next()?.trim().to_owned();
            let panel = parts.next().unwrap_or("").trim().to_owned();
            if pane_id.is_empty() {
                None
            } else {
                Some((
                    pane_id,
                    if panel.is_empty() {
                        "—".to_owned()
                    } else {
                        panel
                    },
                ))
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct LogActivity {
    age_secs: u64,
    file_name: String,
}

/// Walk /tmp/claude-viz for `{kiro,claude,codex}-worker-<id>.log` and index
/// the *freshest* match per worker id. The "<id>" portion is whatever follows
/// the worker prefix; we use it as a panel key for direct lookup.
fn build_log_index() -> HashMap<String, LogActivity> {
    let mut idx: HashMap<String, LogActivity> = HashMap::new();
    let now = SystemTime::now();
    let Ok(entries) = fs::read_dir(VIZ_ROOT) else {
        return idx;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        let Some(id) = parse_worker_log(name) else {
            continue;
        };
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let age_secs = now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0);
        let candidate = LogActivity {
            age_secs,
            file_name: name.to_owned(),
        };
        idx.entry(id)
            .and_modify(|cur| {
                if age_secs < cur.age_secs {
                    *cur = candidate.clone();
                }
            })
            .or_insert(candidate);
    }
    idx
}

/// Parse names like:
///   "claude-worker-claude-fleet-1.log"  -> Some("claude-fleet-1")
///   "kiro-worker-foo.log"               -> Some("foo")
///   "codex-worker-acct-3.log"           -> Some("acct-3")
/// Everything else returns None.
fn parse_worker_log(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".log")?;
    for prefix in ["claude-worker-", "kiro-worker-", "codex-worker-"] {
        if let Some(id) = stem.strip_prefix(prefix) {
            if id.is_empty() {
                return None;
            }
            return Some(id.to_owned());
        }
    }
    None
}

/// Read /tmp/claude-viz/colony-claims.json. Format is best-effort: we look
/// for an outer JSON object and try a couple of common shapes:
///   { "<pane-or-panel>": { "task": "...", ... }, ... }
///   { "<pane-or-panel>": "task-id-string", ... }
/// On any parse failure we return an empty map (callers fall back to
/// "unknown").
fn read_colony_claims() -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let Ok(text) = fs::read_to_string(COLONY_CLAIMS) else {
        return out;
    };
    // Tiny, dependency-free JSON object scanner: find each top-level
    // "key": <value> pair. This is intentionally conservative — anything
    // remotely tricky falls back to "unknown".
    let trimmed = text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return out;
    }
    let body = &trimmed[1..trimmed.len() - 1];
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut esc = false;
    let mut start = 0usize;
    let bytes = body.as_bytes();
    let mut segments: Vec<&str> = Vec::new();
    for (i, &c) in bytes.iter().enumerate() {
        if esc {
            esc = false;
            continue;
        }
        match c {
            b'\\' if in_str => esc = true,
            b'"' => in_str = !in_str,
            b'{' | b'[' if !in_str => depth += 1,
            b'}' | b']' if !in_str => depth -= 1,
            b',' if !in_str && depth == 0 => {
                segments.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < body.len() {
        segments.push(&body[start..]);
    }
    for seg in segments {
        let Some(colon) = seg.find(':') else { continue };
        let raw_key = seg[..colon].trim().trim_matches('"');
        let raw_val = seg[colon + 1..].trim();
        if raw_key.is_empty() {
            continue;
        }
        let label = if raw_val.starts_with('"') {
            let val = raw_val.trim_matches('"');
            format!("claimed:{val}")
        } else if raw_val.starts_with('{') {
            // Try to pluck a "task" field, otherwise just say "claimed".
            extract_field(raw_val, "task")
                .map(|t| format!("claimed:{t}"))
                .unwrap_or_else(|| "claimed".into())
        } else if raw_val.eq_ignore_ascii_case("null") {
            "free".into()
        } else {
            "claimed".into()
        };
        out.insert(raw_key.to_string(), label);
    }
    out
}

/// Extract a string field from a flat JSON object literal like
/// `{ "task": "abc", ... }`. Returns None on any parse hiccup.
fn extract_field(obj: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let idx = obj.find(&needle)?;
    let rest = &obj[idx + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_owned())
}

#[derive(Clone, Debug)]
struct CapProbe {
    verdict: String,
    age_secs: u64,
}

/// Walk /tmp/claude-viz/cap-probe-cache/*.json. Each file is one account.
/// We pull `"verdict"` and the file's mtime; anything that fails to parse is
/// reported as "unknown".
fn read_cap_probe_cache() -> HashMap<String, CapProbe> {
    let mut idx: HashMap<String, CapProbe> = HashMap::new();
    let now = SystemTime::now();
    let Ok(entries) = fs::read_dir(CAP_PROBE_DIR) else {
        return idx;
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let Some(name) = path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let age_secs = now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0);
        let verdict = read_verdict(&path).unwrap_or_else(|| "unknown".into());
        idx.insert(name.to_owned(), CapProbe { verdict, age_secs });
    }
    idx
}

fn read_verdict(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    extract_field(&text, "verdict")
        .map(|v| match v.as_str() {
            "healthy" | "ok" => "ok".into(),
            "rate_limited" | "throttled" | "429" => "429".into(),
            other => other.to_string(),
        })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render(frame: &mut Frame, area: Rect, snap: &Snapshot, view: &View) {
    // Solid background fill so the dashboard reads as a card on the page.
    frame.render_widget(
        Block::default().style(Style::default().bg(IOS_BG_SOLID)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header (caption + sub-line)
            Constraint::Length(1), // top hairline divider
            Constraint::Length(1), // column headings
            Constraint::Length(1), // hairline divider below headings
            Constraint::Min(0),    // pane rows
            Constraint::Length(1), // footer
        ])
        .split(area);

    render_header(frame, chunks[0], snap);
    hairline(frame, chunks[1]);
    render_column_headings(frame, chunks[2]);
    hairline(frame, chunks[3]);
    render_rows(frame, chunks[4], &snap.rows, view);
    render_footer(frame, chunks[5], snap, view);
}

fn render_header(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    if area.height == 0 {
        return;
    }
    let count = snap.rows.len();
    let title = Line::from(vec![
        Span::styled(
            "PANE HEALTH",
            Style::default()
                .fg(IOS_TINT)
                .bg(IOS_BG_SOLID)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   {count} pane{}", if count == 1 { "" } else { "s" }),
            Style::default().fg(IOS_FG).bg(IOS_BG_SOLID),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title),
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );

    if area.height >= 2 {
        let sub = match &snap.note {
            Some(note) => Line::from(Span::styled(
                clip(note, area.width.saturating_sub(1)),
                Style::default().fg(IOS_ORANGE).bg(IOS_BG_SOLID),
            )),
            None => Line::from(Span::styled(
                "polling tmux + /tmp/claude-viz every 1s · read-only",
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_SOLID),
            )),
        };
        frame.render_widget(
            Paragraph::new(sub),
            Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            },
        );
    }
}

fn hairline(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::TOP)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(IOS_HAIRLINE))
        .style(Style::default().bg(IOS_BG_SOLID));
    frame.render_widget(block, area);
}

fn render_column_headings(frame: &mut Frame, area: Rect) {
    let cols = column_layout(area);
    let style = Style::default()
        .fg(IOS_FG_MUTED)
        .bg(IOS_BG_SOLID)
        .add_modifier(Modifier::BOLD);
    let names = ["PANE", "KIND", "PANEL", "ACTIVITY", "COLONY", "CAP-PROBE"];
    for (rect, name) in cols.into_iter().zip(names.iter()) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                clip(name, rect.width.saturating_sub(1)),
                style,
            ))),
            rect,
        );
    }
}

/// What occupies a visible row in the dashboard body: either an actual pane
/// or a "── group: <kind> ──" header injected when grouping is on.
enum BodyLine<'a> {
    Pane(&'a PaneRow),
    GroupHeader(AgentKind),
}

fn body_lines<'a>(rows: &'a [PaneRow], view: &View) -> Vec<BodyLine<'a>> {
    if !view.grouped {
        return rows.iter().map(BodyLine::Pane).collect();
    }
    // Group: stable sort by kind, then keep the per-kind order intact (already
    // sorted by panel from `collect_snapshot`).
    let mut indices: Vec<usize> = (0..rows.len()).collect();
    indices.sort_by(|&a, &b| rows[a].kind.sort_key().cmp(&rows[b].kind.sort_key()));
    let mut out: Vec<BodyLine<'a>> = Vec::with_capacity(rows.len() + 4);
    let mut current: Option<AgentKind> = None;
    for i in indices {
        let row = &rows[i];
        if current != Some(row.kind) {
            out.push(BodyLine::GroupHeader(row.kind));
            current = Some(row.kind);
        }
        out.push(BodyLine::Pane(row));
    }
    out
}

fn render_rows(frame: &mut Frame, area: Rect, rows: &[PaneRow], view: &View) {
    if area.height == 0 {
        return;
    }
    if rows.is_empty() {
        let y = area.y + area.height / 2;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no panes to show — fleet session not running or empty",
                Style::default().fg(IOS_FG_FAINT).bg(IOS_BG_SOLID),
            ))),
            Rect {
                x: area.x + 1,
                y,
                width: area.width.saturating_sub(2),
                height: 1,
            },
        );
        return;
    }
    let lines = body_lines(rows, view);
    let limit = area.height as usize;
    let mut pane_zebra = 0usize;
    for (i, line) in lines.iter().take(limit).enumerate() {
        let y = area.y + i as u16;
        let row_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };

        match line {
            BodyLine::GroupHeader(kind) => {
                // Group headers render on the solid background, full-width.
                frame.render_widget(
                    Block::default().style(Style::default().bg(IOS_BG_SOLID)),
                    row_area,
                );
                let label = format!("── group: {} ──", kind.group_label());
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        clip(&label, row_area.width.saturating_sub(2)),
                        Style::default()
                            .fg(kind.color())
                            .bg(IOS_BG_SOLID)
                            .add_modifier(Modifier::BOLD),
                    ))),
                    Rect {
                        x: row_area.x + 1,
                        y: row_area.y,
                        width: row_area.width.saturating_sub(2),
                        height: 1,
                    },
                );
                // Don't advance the zebra-stripe counter so the next pane row
                // resumes the alternation pattern from where it left off.
            }
            BodyLine::Pane(row) => {
                // Alternating row background mirrors the iOS grouped-list look.
                let row_bg = if pane_zebra % 2 == 0 {
                    IOS_ROW_BG_DARK
                } else {
                    IOS_ROW_BG_LIGHT
                };
                pane_zebra += 1;
                frame.render_widget(
                    Block::default().style(Style::default().bg(row_bg)),
                    row_area,
                );

                let cols = column_layout(row_area);
                // 1. pane id
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        clip(&row.pane_id, cols[0].width.saturating_sub(1)),
                        Style::default()
                            .fg(IOS_TINT)
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
                    )),
                    cols[0],
                );
                // 2. kind badge
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        clip(row.kind.badge(), cols[1].width.saturating_sub(1)),
                        Style::default()
                            .fg(row.kind.color())
                            .bg(row_bg)
                            .add_modifier(Modifier::BOLD),
                    )),
                    cols[1],
                );
                // 3. panel label
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        clip(&row.panel, cols[2].width.saturating_sub(1)),
                        Style::default().fg(IOS_FG).bg(row_bg),
                    )),
                    cols[2],
                );
                // 4. activity age + file
                let (age_text, age_color) = activity_label(row.last_activity);
                let activity_line = Line::from(vec![
                    Span::styled(age_text, Style::default().fg(age_color).bg(row_bg)),
                    Span::styled(" ", Style::default().bg(row_bg)),
                    Span::styled(
                        clip(
                            &row.activity_source,
                            cols[3].width.saturating_sub(12),
                        ),
                        Style::default().fg(IOS_FG_FAINT).bg(row_bg),
                    ),
                ]);
                frame.render_widget(Paragraph::new(activity_line), cols[3]);
                // 5. colony claim
                let claim_color = if row.colony_claim.starts_with("claimed") {
                    IOS_PURPLE
                } else if row.colony_claim == "free" {
                    IOS_GREEN
                } else {
                    IOS_FG_FAINT
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        clip(&row.colony_claim, cols[4].width.saturating_sub(1)),
                        Style::default().fg(claim_color).bg(row_bg),
                    )),
                    cols[4],
                );
                // 6. cap-probe + age
                let cap_color = match row.cap_probe.as_str() {
                    "ok" => IOS_GREEN,
                    "429" => IOS_DESTRUCTIVE,
                    "unknown" => IOS_FG_FAINT,
                    _ => IOS_YELLOW,
                };
                let cap_text = match row.cap_probe_age {
                    Some(age) => format!("{} · {}", row.cap_probe, age_words(age)),
                    None => row.cap_probe.clone(),
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        clip(&cap_text, cols[5].width.saturating_sub(1)),
                        Style::default().fg(cap_color).bg(row_bg),
                    )),
                    cols[5],
                );
            }
        }
    }
}

fn render_footer(frame: &mut Frame, area: Rect, snap: &Snapshot, view: &View) {
    if area.height == 0 {
        return;
    }
    let captured = snap
        .captured_at
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| format!("captured {}s ago", elapsed_label(d.as_secs())))
        .unwrap_or_else(|| "captured —".into());
    let group_state = if view.grouped { "on" } else { "off" };
    let left = format!(" q / Esc quit · g group: {group_state} · 1s poll ");
    let right = format!("{captured} ");
    let right_w = right.chars().count() as u16;
    let left_w = area.width.saturating_sub(right_w);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                clip(&left, left_w),
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_SOLID),
            ),
            Span::styled(
                format!("{:>width$}", right, width = right_w as usize),
                Style::default().fg(IOS_FG_FAINT).bg(IOS_BG_SOLID),
            ),
        ])),
        area,
    );
}

fn column_layout(area: Rect) -> Vec<Rect> {
    // Reserve a 1-cell gutter on the left so column text isn't flush with the
    // edge. Constraints are roughly proportional and degrade gracefully on
    // narrow terminals.
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),       // pane id
            Constraint::Length(6),       // kind badge
            Constraint::Min(14),         // panel label (flex)
            Constraint::Length(28),      // activity
            Constraint::Length(22),      // colony claim
            Constraint::Length(20),      // cap-probe
        ])
        .split(inner)
        .to_vec()
}

fn activity_label(secs: Option<u64>) -> (String, ratatui::style::Color) {
    match secs {
        None => ("—".to_string(), IOS_FG_FAINT),
        Some(0) => ("just now".to_string(), IOS_GREEN),
        Some(s) if s < 60 => (format!("{s}s ago"), IOS_GREEN),
        Some(s) if s < 600 => (format!("{}m ago", s / 60), IOS_YELLOW),
        Some(s) if s < 3600 => (format!("{}m ago", s / 60), IOS_ORANGE),
        Some(s) => (format!("{}h ago", s / 3600), IOS_DESTRUCTIVE),
    }
}

fn age_words(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

fn elapsed_label(_epoch_secs: u64) -> String {
    // We don't carry "delta vs now" here — the snapshot is fresh by definition
    // (we re-render every tick), so this surfaces a `0s` heartbeat label.
    "0".to_string()
}

fn clip(input: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= width as usize {
        return input.to_owned();
    }
    if width == 1 {
        return "…".into();
    }
    let mut out: String = chars
        .into_iter()
        .take(width.saturating_sub(1) as usize)
        .collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worker_log_names() {
        assert_eq!(
            parse_worker_log("claude-worker-claude-fleet-1.log").as_deref(),
            Some("claude-fleet-1")
        );
        assert_eq!(
            parse_worker_log("kiro-worker-foo.log").as_deref(),
            Some("foo")
        );
        assert_eq!(
            parse_worker_log("codex-worker-acct-3.log").as_deref(),
            Some("acct-3")
        );
        assert_eq!(parse_worker_log("cap-probe.log"), None);
        assert_eq!(parse_worker_log("random.txt"), None);
    }

    #[test]
    fn extracts_verdict_field() {
        let body = r#"{"verdict": "healthy", "until_epoch": 0}"#;
        assert_eq!(extract_field(body, "verdict").as_deref(), Some("healthy"));
        let none = r#"{"other": 1}"#;
        assert_eq!(extract_field(none, "verdict"), None);
    }

    #[test]
    fn activity_label_buckets() {
        assert_eq!(activity_label(None).0, "—");
        assert_eq!(activity_label(Some(0)).0, "just now");
        assert_eq!(activity_label(Some(45)).0, "45s ago");
        assert_eq!(activity_label(Some(120)).0, "2m ago");
        assert_eq!(activity_label(Some(7200)).0, "2h ago");
    }

    #[test]
    fn agent_kind_classifies_from_panel() {
        assert_eq!(
            AgentKind::classify("codex-admin-mite", "—"),
            AgentKind::Codex
        );
        assert_eq!(AgentKind::classify("kiro-foo", "—"), AgentKind::Kiro);
        assert_eq!(
            AgentKind::classify("idle-claude-pane-3", "—"),
            AgentKind::Claude
        );
        assert_eq!(AgentKind::classify("—", "—"), AgentKind::Unknown);
    }

    #[test]
    fn agent_kind_classifies_from_log_when_panel_blank() {
        assert_eq!(
            AgentKind::classify("—", "claude-worker-claude-fleet-1.log"),
            AgentKind::Claude
        );
        assert_eq!(
            AgentKind::classify("—", "codex-worker-acct-3.log"),
            AgentKind::Codex
        );
        assert_eq!(
            AgentKind::classify("—", "kiro-worker-x.log"),
            AgentKind::Kiro
        );
    }

    #[test]
    fn agent_kind_panel_wins_over_log() {
        // codex panel paired with a stale claude-* log key still classifies as
        // codex — panel label is the authoritative signal.
        assert_eq!(
            AgentKind::classify("codex-admin-mite", "claude-worker-claude-fleet-1.log"),
            AgentKind::Codex
        );
    }

    #[test]
    fn grouped_body_lines_inject_headers_per_kind() {
        let rows = vec![
            row("%1", "idle-claude-pane-1", AgentKind::Claude),
            row("%2", "codex-admin-mite", AgentKind::Codex),
            row("%3", "kiro-foo", AgentKind::Kiro),
            row("%4", "codex-bia-zazrifka", AgentKind::Codex),
        ];
        let lines = body_lines(&rows, &View { grouped: true });
        let kinds: Vec<&str> = lines
            .iter()
            .map(|l| match l {
                BodyLine::GroupHeader(k) => k.group_label(),
                BodyLine::Pane(r) => match r.pane_id.as_str() {
                    "%1" => "pane:claude",
                    "%2" => "pane:codex1",
                    "%3" => "pane:kiro",
                    "%4" => "pane:codex2",
                    _ => "pane:?",
                },
            })
            .collect();
        // Sort key is Codex(0) < Kiro(1) < Claude(2); the two codex panes
        // share one header.
        assert_eq!(
            kinds,
            vec![
                "codex",
                "pane:codex1",
                "pane:codex2",
                "kiro",
                "pane:kiro",
                "claude",
                "pane:claude",
            ]
        );
    }

    #[test]
    fn ungrouped_body_lines_have_no_headers() {
        let rows = vec![
            row("%1", "idle-claude-pane-1", AgentKind::Claude),
            row("%2", "codex-admin-mite", AgentKind::Codex),
        ];
        let lines = body_lines(&rows, &View { grouped: false });
        assert_eq!(lines.len(), 2);
        assert!(matches!(lines[0], BodyLine::Pane(_)));
        assert!(matches!(lines[1], BodyLine::Pane(_)));
    }

    fn row(pane_id: &str, panel: &str, kind: AgentKind) -> PaneRow {
        PaneRow {
            pane_id: pane_id.into(),
            panel: panel.into(),
            kind,
            last_activity: None,
            activity_source: "—".into(),
            colony_claim: "unknown".into(),
            cap_probe: "unknown".into(),
            cap_probe_age: None,
        }
    }

    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area();
        let mut out = String::with_capacity((area.width as usize + 1) * area.height as usize);
        for y in 0..area.height {
            for x in 0..area.width {
                out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            out.push('\n');
        }
        out
    }

    fn render_to_string(snap: &Snapshot, view: &View, w: u16, h: u16) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame, frame.area(), snap, view))
            .expect("draw");
        buffer_to_string(terminal.backend().buffer())
    }

    fn three_kind_snapshot() -> Snapshot {
        Snapshot {
            rows: vec![
                row("%1", "codex-admin-mite", AgentKind::Codex),
                row("%2", "kiro-foo", AgentKind::Kiro),
                row("%3", "idle-claude-pane-3", AgentKind::Claude),
            ],
            note: None,
            captured_at: None,
        }
    }

    #[test]
    fn render_ungrouped_shows_kind_column_for_each_agent() {
        let out = render_to_string(&three_kind_snapshot(), &View::default(), 120, 10);
        // KIND column heading present.
        assert!(out.contains("KIND"), "missing KIND header in:\n{out}");
        // One badge per agent kind, on its row.
        assert!(out.contains("CODX"), "missing CODX badge in:\n{out}");
        assert!(out.contains("KIRO"), "missing KIRO badge in:\n{out}");
        assert!(out.contains("CLAU"), "missing CLAU badge in:\n{out}");
        // No group headers when grouping is off.
        assert!(
            !out.contains("group: codex"),
            "unexpected group header in ungrouped view:\n{out}"
        );
        // Footer reports group state.
        assert!(out.contains("group: off"), "missing footer state:\n{out}");
    }

    #[test]
    fn render_grouped_shows_one_header_per_kind() {
        let out = render_to_string(&three_kind_snapshot(), &View { grouped: true }, 120, 12);
        assert!(out.contains("── group: codex ──"), "no codex header:\n{out}");
        assert!(out.contains("── group: kiro ──"), "no kiro header:\n{out}");
        assert!(out.contains("── group: claude ──"), "no claude header:\n{out}");
        assert!(out.contains("group: on"), "missing footer state:\n{out}");
    }
}
