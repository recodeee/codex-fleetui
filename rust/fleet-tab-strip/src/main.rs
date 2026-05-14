// fleet-tab-strip — minimal ratatui binary that renders only the in-binary
// tab strip from `fleet_ui::tab_strip`. Designed for windows that aren't
// themselves a full ratatui dashboard but should still carry the same top
// navigation row. The overview window (a tmux pane grid of N codex CLI
// workers) is the first such consumer: a 1-row header pane at the top of
// the window runs this binary so the strip is consistent with the
// dashboards in windows 1-5.
//
// Behaviour:
// - Renders fleet_ui::tab_strip::TabStrip into the top row of the frame.
// - Highlights the tab matching the current tmux window (looked up via
//   `tmux display-message`) — when the operator switches windows, the
//   active pill follows.
// - Mouse clicks on a pill exec `tmux select-window -t <session>:<idx>`.
// - Refreshes every 500ms; the TabStrip widget picks up counter updates
//   from /tmp/claude-viz/fleet-tab-counters.json on each render.
// - Never quits on its own; the pane is a chrome surface. Ctrl+C still
//   exits cleanly for manual restarts.

use std::io;
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use fleet_ui::tab_strip::{Tab, TabStrip};

/// tmux session this binary belongs to. Overridable so a parallel fleet
/// (codex-fleet-2, etc.) can spawn its own header pane with the right
/// click target.
fn fleet_session() -> String {
    std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".to_string())
}

/// Look up the tmux window the strip is hosted in — used so the binary
/// can pick the right *active* tab pill. Returns the window index of the
/// pane this binary runs in, or `0` if tmux isn't reachable (which is the
/// only sensible default — overview is window 0).
fn current_window_index() -> usize {
    Command::new("tmux")
        .args(["display-message", "-p", "-F", "#{window_index}"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

/// Best-effort `tmux select-window`. Mirrors the posture of every other
/// fleet click router: failures are silent because a dashboard outside
/// tmux should render a frame, not crash.
fn select_window(session: &str, idx: usize) {
    let target = format!("{session}:{idx}");
    let _ = Command::new("tmux")
        .args(["select-window", "-t", &target])
        .status();
}

/// Map a tmux window index to one of the five canonical [`Tab`]s. Returns
/// `Tab::Overview` for any unknown index — keeps the active highlight on
/// a sensible default rather than dropping back to "no tab active".
fn tab_for_index(idx: usize) -> Tab {
    match idx {
        0 => Tab::Overview,
        1 => Tab::Fleet,
        2 => Tab::Plan,
        3 => Tab::Waves,
        4 => Tab::Review,
        _ => Tab::Overview,
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let session = fleet_session();
    let mut tick: u64 = 0;
    let mut last_hits: Vec<fleet_ui::tab_strip::TabHit> = Vec::new();

    let result: io::Result<()> = (|| {
        loop {
            // Re-resolve active window each render so the highlighted pill
            // tracks `prefix N` window switches without extra signaling.
            let active = tab_for_index(current_window_index());
            terminal.draw(|f| {
                let area = f.area();
                let row = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
                let strip = TabStrip::new(active, row.width).with_tick(tick);
                last_hits = strip.render(f, row);
            })?;

            tick = tick.wrapping_add(1);

            // 500ms tick: cheap enough for live counters, slow enough that
            // the binary stays well under 1% CPU.
            if event::poll(Duration::from_millis(500))? {
                match event::read()? {
                    Event::Mouse(MouseEvent { kind: MouseEventKind::Down(_), column, row, .. }) => {
                        if let Some(hit) = last_hits.iter().find(|h| {
                            column >= h.rect.x
                                && column < h.rect.x + h.rect.width
                                && row >= h.rect.y
                                && row < h.rect.y + h.rect.height
                        }) {
                            select_window(&session, hit.window_idx);
                        }
                    }
                    Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) => {
                        // Ctrl+C is the only explicit exit — operators may
                        // need to relaunch the binary after upgrades.
                        if matches!(code, KeyCode::Char('c')) && modifiers.contains(KeyModifiers::CONTROL) {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    result
}
