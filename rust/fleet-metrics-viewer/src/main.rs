//! fleet-metrics-viewer — live-tail viewer for the supervisor metrics TSV.
//!
//! Lane 9 of the codex-fleet-tui-improvements-2026-05-15 plan. Renders the
//! last 30 rows of `/tmp/claude-viz/supervisor-metrics.tsv` (or any TSV passed
//! via `--path`) in a scrollable ratatui table wrapped in the canonical iOS
//! card chrome from `fleet-ui`. Auto-refreshes every 500ms by polling the
//! file's mtime — only re-reads when the mtime changes.
//!
//! Keybindings:
//! - `q` / Esc        → quit
//! - `PgUp` / `PgDn`  → scroll back / forward through the buffered rows
//!
//! Behaviour notes:
//! - When `--path` is missing **and** the default path
//!   `/tmp/claude-viz/supervisor-metrics.tsv` does not exist, the binary
//!   prints `idle (no metrics file yet)` to stderr and exits 0 without
//!   touching the terminal. This matches the supervisor tab convention used
//!   elsewhere in codex-fleet.
//! - First line of the TSV is treated as column headers; every subsequent
//!   line is split on `\t`. Short rows are padded with empty cells so the
//!   table never panics on ragged input.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use fleet_ui::card::card;
use fleet_ui::palette::{
    IOS_BG_SOLID, IOS_FG, IOS_FG_FAINT, IOS_FG_MUTED, IOS_HAIRLINE, IOS_TINT,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::{DefaultTerminal, Frame};

/// Maximum number of TSV data rows we buffer for display. The viewer is a
/// tail, not a log archive — older rows just scroll off the visible window.
const MAX_ROWS: usize = 30;
/// Poll interval for the crossterm event loop. Also gates how often the
/// background mtime check fires.
const TICK: Duration = Duration::from_millis(500);
/// Default TSV location, written by `scripts/codex-fleet/claude-supervisor.sh`.
const DEFAULT_PATH: &str = "/tmp/claude-viz/supervisor-metrics.tsv";

/// In-memory snapshot of the parsed TSV. Owned strings so the table widget
/// can borrow `&str` slices freely.
#[derive(Default, Debug)]
struct TsvData {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    /// `mtime` we last loaded — used to skip re-parsing when nothing changed.
    mtime: Option<SystemTime>,
    /// True when we successfully loaded at least once.
    loaded: bool,
    /// Human-readable status line (empty when everything is fine).
    status: String,
}

impl TsvData {
    /// Re-parse the TSV at `path` if its `mtime` changed since the last load.
    /// Returns `true` when the in-memory snapshot was updated.
    fn refresh(&mut self, path: &Path) -> bool {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(err) => {
                let next_status = format!("{} unreadable: {}", path.display(), err);
                if next_status != self.status {
                    self.status = next_status;
                    return true;
                }
                return false;
            }
        };
        let mtime = meta.modified().ok();
        if self.loaded && mtime == self.mtime {
            return false;
        }
        match fs::read_to_string(path) {
            Ok(body) => {
                let mut lines = body.lines();
                let headers: Vec<String> = match lines.next() {
                    Some(h) => h.split('\t').map(|s| s.to_string()).collect(),
                    None => Vec::new(),
                };
                let all_rows: Vec<Vec<String>> = lines
                    .filter(|l| !l.is_empty())
                    .map(|l| l.split('\t').map(|s| s.to_string()).collect())
                    .collect();
                // Keep only the last MAX_ROWS so the viewer tails the file.
                let start = all_rows.len().saturating_sub(MAX_ROWS);
                self.rows = all_rows[start..].to_vec();
                self.headers = headers;
                self.mtime = mtime;
                self.loaded = true;
                self.status.clear();
                true
            }
            Err(err) => {
                let next_status = format!("{} read error: {}", path.display(), err);
                if next_status != self.status {
                    self.status = next_status;
                    return true;
                }
                false
            }
        }
    }

    /// Width of the widest known row (used to compute column count when the
    /// header is absent).
    fn column_count(&self) -> usize {
        let header_cols = self.headers.len();
        let row_cols = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        header_cols.max(row_cols)
    }
}

/// CLI options parsed from `argv`. Tiny by design — only one flag.
struct Cli {
    path: Option<PathBuf>,
}

fn parse_cli() -> Result<Cli, String> {
    let mut args = env::args().skip(1);
    let mut path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--path requires a value".to_string())?;
                path = Some(PathBuf::from(val));
            }
            "-h" | "--help" => {
                println!(
                    "fleet-metrics-viewer — live-tail a supervisor metrics TSV\n\
                     \n\
                     USAGE:\n    \
                     fleet-metrics-viewer [--path <tsv>]\n\
                     \n\
                     When --path is omitted, defaults to {DEFAULT_PATH}.\n\
                     When neither the override nor the default exists, prints\n\
                     'idle (no metrics file yet)' to stderr and exits 0.\n\
                     \n\
                     KEYS: q/Esc quit  PgUp/PgDn scroll"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Cli { path })
}

fn main() -> ExitCode {
    let cli = match parse_cli() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("fleet-metrics-viewer: {err}");
            return ExitCode::from(2);
        }
    };

    // Resolve the effective path. Whether the path comes from --path or
    // from the supervisor default, if it doesn't exist we don't touch the
    // terminal — we just print the canonical idle marker on stderr and
    // exit 0. This matches the supervisor-tab convention used elsewhere
    // and avoids alternate-screen flicker when the metrics file hasn't
    // been seeded yet.
    let path = cli.path.unwrap_or_else(|| PathBuf::from(DEFAULT_PATH));
    if !path.exists() {
        eprintln!("idle (no metrics file yet)");
        return ExitCode::SUCCESS;
    }

    if let Err(err) = run(&path) {
        eprintln!("fleet-metrics-viewer: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Set up the alternate-screen ratatui terminal, run the event loop, then
/// always restore on the way out (panic-safe via `ratatui::restore`).
fn run(path: &Path) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, path);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, path: &Path) -> io::Result<()> {
    let mut data = TsvData::default();
    // Prime once so the first frame isn't blank.
    data.refresh(path);

    let mut scroll: usize = 0;
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| draw(frame, path, &data, scroll))?;

        // Poll with a timeout so we wake up at least once per TICK to check
        // the file's mtime even when the user is idle.
        let timeout = TICK
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::PageUp => {
                            scroll = scroll.saturating_add(visible_rows_hint());
                        }
                        KeyCode::PageDown => {
                            scroll = scroll.saturating_sub(visible_rows_hint());
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= TICK {
            data.refresh(path);
            last_tick = Instant::now();
            // Clamp scroll back into range whenever the row count shrinks
            // (e.g. file truncated by `: > tsv` from the supervisor).
            scroll = scroll.min(data.rows.len().saturating_sub(1));
        }
    }
}

/// Rough viewport size used by PgUp / PgDn before we know the true table
/// height. The real clamp happens in `draw` against the rendered row slice.
fn visible_rows_hint() -> usize {
    10
}

fn draw(frame: &mut Frame<'_>, path: &Path, data: &TsvData, scroll: usize) {
    let area = frame.area();
    // Background wash so the chrome matches the rest of the iOS dashboards.
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(IOS_BG_SOLID)),
        area,
    );

    // Header banner (rounded card) on top, table card below, status footer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, chunks[0], path, data);
    draw_table(frame, chunks[1], data, scroll);
    draw_footer(frame, chunks[2], data);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, path: &Path, data: &TsvData) {
    let block = card(Some("FLEET METRICS — LIVE TAIL"), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_count = data.rows.len();
    let header_count = data.headers.len();
    let mtime_label = data
        .mtime
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| format!("mtime={}s", d.as_secs()))
        .unwrap_or_else(|| "mtime=?".to_string());

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", path.display()),
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" rows={row_count}/{MAX_ROWS}"),
            Style::default().fg(IOS_FG_MUTED),
        ),
        Span::styled(
            format!("  cols={header_count}"),
            Style::default().fg(IOS_FG_MUTED),
        ),
        Span::styled(
            format!("  {mtime_label}"),
            Style::default().fg(IOS_FG_FAINT),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

fn draw_table(frame: &mut Frame<'_>, area: Rect, data: &TsvData, scroll: usize) {
    let block = card(Some("ROWS"), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.rows.is_empty() && data.headers.is_empty() {
        let msg = if data.status.is_empty() {
            "no rows yet — waiting for supervisor to append".to_string()
        } else {
            data.status.clone()
        };
        let para = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().fg(IOS_FG_MUTED).add_modifier(Modifier::ITALIC),
        )));
        frame.render_widget(para, inner);
        return;
    }

    let col_count = data.column_count().max(1);
    // Compute scroll window: render from `start..end` of `data.rows`,
    // newest at the bottom so the file tails naturally. `scroll` shifts the
    // window backwards (PgUp).
    let total = data.rows.len();
    let visible = (inner.height as usize).saturating_sub(1).max(1); // 1 row for header
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible);
    let window = &data.rows[start..end];

    let header_cells: Vec<Cell<'_>> = (0..col_count)
        .map(|i| {
            let label = data.headers.get(i).map(String::as_str).unwrap_or("");
            Cell::from(label).style(
                Style::default()
                    .fg(IOS_TINT)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    let header_row = Row::new(header_cells).style(Style::default().bg(IOS_BG_SOLID));

    let body_rows: Vec<Row<'_>> = window
        .iter()
        .map(|r| {
            let cells: Vec<Cell<'_>> = (0..col_count)
                .map(|i| {
                    let v = r.get(i).map(String::as_str).unwrap_or("");
                    Cell::from(v).style(Style::default().fg(IOS_FG))
                })
                .collect();
            Row::new(cells)
        })
        .collect();

    // Equal-weight columns; ratatui re-flows them to fit `inner.width`. This
    // keeps the viewer schema-agnostic — any TSV produced by the supervisor
    // (or another writer) renders without code changes.
    let widths: Vec<Constraint> =
        (0..col_count).map(|_| Constraint::Ratio(1, col_count as u32)).collect();

    let table = Table::new(body_rows, widths)
        .header(header_row)
        .style(Style::default().bg(IOS_BG_SOLID).fg(IOS_FG))
        .column_spacing(1);
    frame.render_widget(table, inner);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, data: &TsvData) {
    let mut spans: Vec<Span<'_>> = vec![
        Span::styled(" q/Esc ", Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
        Span::styled("quit  ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("PgUp/PgDn ", Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
        Span::styled("scroll  ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled(
            format!("refresh={}ms", TICK.as_millis()),
            Style::default().fg(IOS_HAIRLINE),
        ),
    ];
    if !data.status.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            data.status.clone(),
            Style::default().fg(IOS_FG_FAINT).add_modifier(Modifier::ITALIC),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(IOS_BG_SOLID)),
        area,
    );
}
