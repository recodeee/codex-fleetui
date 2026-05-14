// fleet-watcher — drop-in replacement for `scripts/codex-fleet/watcher-board.sh`.
//
// Phase-4 minimal: renders the iOS chrome (in-binary tab strip + rounded card
// header + 4 stat cards + per-pane table placeholder) using fleet-ui widgets.
// Real cap-swap log scraping and live `fleet-data::panes` integration land in
// follow-up PRs (the bin compiles and ships first so wave-4 unblocks wave-5).
//
// In-binary tab strip handles its own MouseDown(Left) and shells out
// `tmux select-window` — eliminates the kitty+tmux click-routing class of
// bugs from #1927 / #1931 / PR #6 once and for all.

use std::{io, process::Command, time::Duration};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};

const TABS: &[(&str, &str)] = &[
    ("0", "watcher"),
    ("1", "overview"),
    ("2", "fleet"),
    ("3", "plan"),
    ("4", "waves"),
];

const ACTIVE_TAB: usize = 0;

struct App {
    tab_rects: Vec<(Rect, usize)>,
}

impl App {
    fn new() -> Self {
        Self { tab_rects: Vec::new() }
    }

    fn handle_click(&self, col: u16, row: u16) -> Option<usize> {
        self.tab_rects
            .iter()
            .find(|(r, _)| {
                col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
            })
            .map(|(_, idx)| *idx)
    }
}

fn render_tab_strip(frame: &mut ratatui::Frame, area: Rect, active: usize, app: &mut App) {
    app.tab_rects.clear();
    let mut x = area.x;
    for (i, (idx, name)) in TABS.iter().enumerate() {
        let label = format!(" {} {} ", idx, name);
        let w = label.chars().count() as u16;
        if x + w + 1 >= area.x + area.width { break; }
        let r = Rect { x, y: area.y, width: w, height: 1 };
        let style = if i == active {
            Style::default().fg(IOS_FG).bg(IOS_TINT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(IOS_FG_MUTED).bg(IOS_CHIP_BG)
        };
        frame.render_widget(Paragraph::new(Span::styled(label, style)), r);
        app.tab_rects.push((r, i));
        x += w + 1; // 1-cell gap; click-router treats gap as outside any tab
    }
}

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab strip
            Constraint::Length(3), // header banner
            Constraint::Length(5), // 4 stat cards
            Constraint::Min(0),    // per-pane table area
        ])
        .split(area);

    render_tab_strip(frame, rows[0], ACTIVE_TAB, app);

    let header = card(Some("WATCHER · all clear · live"), false);
    frame.render_widget(header, rows[1]);

    let stats = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(rows[2]);
    for (i, (label, value, kind)) in [
        ("PANES",   "8", ChipKind::Working),
        ("CAPPED",  "0", ChipKind::Idle),
        ("SWAPPED", "0", ChipKind::Done),
        ("RANKED", "20", ChipKind::Working),
    ].iter().enumerate() {
        let block = card(Some(label), false);
        let inner = block.inner(stats[i]);
        frame.render_widget(block, stats[i]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {value}  "), Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
            ])),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
        );
        let chip = status_chip(*kind);
        if inner.height >= 2 {
            frame.render_widget(
                Paragraph::new(Line::from(chip)),
                Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
            );
        }
    }

    let panes_block = card(Some("FLEET PANES — port of watcher-board.sh deferred to follow-up"), false);
    let inner = panes_block.inner(rows[3]);
    frame.render_widget(panes_block, rows[3]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  fleet-data::panes::list_panes(\"codex-fleet\", Some(\"overview\")) → PaneState classifier",
                Style::default().fg(IOS_FG_MUTED),
            )),
            Line::from(Span::styled(
                "  click any tab above to select-window via tmux (the canonical click-routing fix).",
                Style::default().fg(IOS_FG_MUTED),
            )),
        ]),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: inner.height.min(3) },
    );
}

fn select_window(idx: usize) {
    // Best-effort: ignore failure (e.g. running outside tmux). The whole
    // point of the bin's click handler is to do *something* even when
    // tmux's status-bar routing fails to fire.
    let _ = Command::new("tmux")
        .args(["select-window", "-t", &format!("codex-fleet:{}", idx)])
        .status();
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &mut app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => {
                        if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) { break; }
                    }
                    Event::Mouse(m) => {
                        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                            if let Some(idx) = app.handle_click(m.column, m.row) {
                                select_window(idx);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    result
}
