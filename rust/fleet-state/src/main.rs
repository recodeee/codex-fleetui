// fleet-state — drop-in replacement for `fleet-state-anim.sh` + render half
// of `fleet-tick.sh`. First real consumer of `fleet-data::fleet`: calls
// `fleet::load_live` and renders the live join of accounts + panes.
//
// `cargo build -p fleet-state` exercises the full chain: tmux::list_panes
// → panes::list_panes → fleet::join.
//
// The pane runs inside the `codex-fleet` tmux session, whose status bar
// (`style-tabs.sh`) supplies the canonical tab strip; this binary
// therefore does not draw one of its own. Key input is routed through
// `fleet_input` (the dashboard-wide action matcher) — currently only the
// `q` / `Esc` quit bindings are registered, but the plumbing is in place
// so future bindings (refresh, jump-to-row, …) only need a `bind()` line
// rather than another inline `matches!` branch.
//
// The full "G · Fleet" artboard (per-row avatars, the PANE `#N >` column,
// Filter / New worker buttons) lands in follow-up PRs — this binary's job
// is the data path, not pixel parity.

use std::{io, time::Duration};

use crossterm::{
    event::{self, Event, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fleet_data::{
    fleet::{self, FleetSummary, WorkerRow},
    panes::PaneState,
};
use fleet_input::{Action, ContextStack, KeyPattern, Matcher};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
    rail::{progress_rail, RailAxis},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InputContext {
    Global,
}

fn input_context() -> ContextStack<InputContext> {
    let matcher = Matcher::new()
        .bind(KeyPattern::char('q'), Action::Quit)
        .bind(KeyPattern::esc(), Action::Quit);

    ContextStack::new(InputContext::Global, matcher)
}

/// tmux session + window the fleet's worker panes live in. Matches the
/// `codex-fleet:overview` target every dashboard binary uses; overridable via
/// env for parallel fleets (`codex-fleet-2`, …).
fn fleet_target() -> (String, String) {
    let session =
        std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".to_string());
    let window =
        std::env::var("CODEX_FLEET_WINDOW").unwrap_or_else(|_| "overview".to_string());
    (session, window)
}

struct App {
    input: ContextStack<InputContext>,
    /// The live join of accounts + panes. `None` until the first successful
    /// load; `Some(vec![])` is a valid "fleet is empty / not running" state.
    rows: Option<Vec<WorkerRow>>,
    /// Set when the last `fleet::load_live` returned an `io::Error` (tmux not
    /// on PATH, etc.) — rendered as a faint status line rather than a crash.
    load_error: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            input: input_context(),
            rows: None,
            load_error: None,
        }
    }

    /// Refresh the worker rows from the live fleet. Called once per tick.
    /// An `Err` from `load_live` is recorded, not propagated — a transient
    /// tmux failure should leave the last good frame on screen, not kill the
    /// dashboard.
    fn refresh(&mut self) {
        let (session, window) = fleet_target();
        match fleet::load_live(&session, Some(&window)) {
            Ok(rows) => {
                self.rows = Some(rows);
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
            }
        }
    }

    fn dispatch_key(&mut self, key: KeyEvent) -> Option<Action> {
        self.input.dispatch(key)
    }

    /// Apply an action dispatched from the input matcher. Returns `true`
    /// when the action requests the event loop exit. `SelectTab` is a no-op
    /// here — tab navigation is owned by tmux's status bar
    /// (`style-tabs.sh`), so this binary intentionally does not handle it
    /// even though the action exists in the shared `fleet_input::Action`
    /// enum.
    fn apply_action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => true,
            Action::Refresh => {
                self.refresh();
                false
            }
            Action::Noop | Action::SelectTab(_) => false,
        }
    }
}

/// Map the data-layer [`PaneState`] to the UI-layer [`ChipKind`]. Reserve
/// accounts (no live pane → `state: None`) render as `Idle`. Mirrors the
/// `classify` converter in `fleet-waves/src/main.rs`, but keyed off the
/// pane state rather than a plan-status string.
fn chip_kind(state: Option<PaneState>) -> ChipKind {
    match state {
        Some(PaneState::Working) => ChipKind::Working,
        Some(PaneState::Idle) | None => ChipKind::Idle,
        Some(PaneState::Polling) => ChipKind::Polling,
        Some(PaneState::Capped) => ChipKind::Capped,
        Some(PaneState::Approval) => ChipKind::Approval,
        Some(PaneState::Boot) => ChipKind::Boot,
        Some(PaneState::Dead) => ChipKind::Dead,
    }
}

/// Render one worker row: account + dim model label, the two usage rails,
/// the status chip, and the scraped "WORKING ON" text. Geometry is computed
/// inline (same approach as the old mock loop) — the constraint-based column
/// layout from the artboard lands with the full Fleet render.
fn render_worker_row(frame: &mut ratatui::Frame, area: Rect, row: &WorkerRow) {
    let mut spans: Vec<Span> = Vec::new();

    // ACCOUNT — email, starred when it's the codex-auth current account.
    let label = if row.is_current {
        format!("★{}", row.email)
    } else {
        row.email.clone()
    };
    spans.push(Span::styled(format!("  {:<26}", label), Style::default().fg(IOS_FG)));
    spans.push(Span::raw(" "));

    // WEEKLY · 5H rails — usage axis (green→orange→red as the number climbs).
    spans.extend(progress_rail(row.weekly_pct, RailAxis::Usage, 8));
    spans.push(Span::raw(" "));
    spans.extend(progress_rail(row.five_h_pct, RailAxis::Usage, 8));
    spans.push(Span::raw(" "));

    // STATUS chip.
    spans.extend(status_chip(chip_kind(row.state)));
    spans.push(Span::raw("  "));

    // WORKING ON — headline + dim subtext, or a faint placeholder for reserve.
    if row.working_on.is_empty() {
        spans.push(Span::styled(
            "—  reserve",
            Style::default().fg(IOS_FG_FAINT),
        ));
    } else {
        spans.push(Span::styled(
            row.working_on.clone(),
            Style::default().fg(IOS_FG),
        ));
        if !row.pane_subtext.is_empty() {
            spans.push(Span::styled(
                format!("   {}", row.pane_subtext),
                Style::default().fg(IOS_FG_MUTED),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header banner
            Constraint::Min(0),    // worker table
        ])
        .split(area);

    // ── Header banner: "FLEET · N workers · M live · K in review" ──────────
    let header_text = match &app.rows {
        Some(worker_rows) => {
            let s = FleetSummary::of(worker_rows);
            format!(
                "FLEET · {} workers · {} live · {} in review",
                s.workers, s.live, s.in_review
            )
        }
        None => "FLEET · loading…".to_string(),
    };
    frame.render_widget(card(Some(&header_text), false), rows[0]);

    // ── Worker table ──────────────────────────────────────────────────────
    let block = card(
        Some("ACCOUNT · WEEKLY · 5H · STATUS · WORKING ON"),
        false,
    );
    let inner = block.inner(rows[1]);
    frame.render_widget(block, rows[1]);

    match &app.rows {
        Some(worker_rows) if !worker_rows.is_empty() => {
            for (i, row) in worker_rows.iter().enumerate() {
                let y = inner.y + i as u16;
                if y + 1 > inner.y + inner.height {
                    break;
                }
                render_worker_row(
                    frame,
                    Rect { x: inner.x, y, width: inner.width, height: 1 },
                    row,
                );
            }
        }
        Some(_) => {
            // Empty fleet — successful load, just no workers.
            let msg = match &app.load_error {
                Some(_) => "  fleet unreachable — see status below",
                None => "  no workers — is the codex-fleet session up?",
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    msg,
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  loading fleet state…",
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
            );
        }
    }

    // Faint error line at the bottom of the table when the last load failed.
    if let Some(err) = &app.load_error {
        let y = inner.y + inner.height.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  load error: {err}"),
                Style::default().fg(IOS_FG_FAINT),
            ))),
            Rect { x: inner.x, y, width: inner.width, height: 1 },
        );
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    // Prime the first frame with real data before the event loop so the
    // dashboard never flashes the "loading…" state for a full tick.
    app.refresh();

    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &app))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(k) = event::read()? {
                    if let Some(action) = app.dispatch_key(k) {
                        if app.apply_action(action) {
                            break;
                        }
                    }
                }
            } else {
                // No input this tick — refresh the fleet state so the table
                // stays live. The 250ms poll doubles as the refresh cadence.
                app.refresh();
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}
