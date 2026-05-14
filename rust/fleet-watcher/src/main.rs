// fleet-watcher — drop-in replacement for `scripts/codex-fleet/watcher-board.sh`.
//
// Renders the iOS chrome (rounded card header + 4 stat cards + per-pane
// table placeholder) using fleet-ui widgets. The pane itself runs inside
// the `codex-fleet` tmux session, whose status bar (`style-tabs.sh`)
// supplies the canonical tab strip — this binary therefore does not draw
// one of its own. See the PR that introduced this convention for the
// duplication bug that motivated dropping the in-pane strips.

use std::{io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode},
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

fn render(frame: &mut ratatui::Frame) {
    let area = frame.area();
    if area.width < 30 || area.height < 8 { return; }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header banner
            Constraint::Length(5), // 4 stat cards
            Constraint::Min(0),    // per-pane table area
        ])
        .split(area);

    let header = card(Some("WATCHER · all clear · live"), false);
    frame.render_widget(header, rows[0]);

    let stats = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(rows[1]);
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
    let inner = panes_block.inner(rows[2]);
    frame.render_widget(panes_block, rows[2]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  fleet-data::panes::list_panes(\"codex-fleet\", Some(\"overview\")) → PaneState classifier",
                Style::default().fg(IOS_FG_MUTED),
            )),
            Line::from(Span::styled(
                "  use tmux's status-bar tabs (style-tabs.sh) to switch windows.",
                Style::default().fg(IOS_FG_MUTED),
            )),
        ]),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: inner.height.min(3) },
    );
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let result: io::Result<()> = (|| {
        loop {
            terminal.draw(render)?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(k) = event::read()? {
                    if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) { break; }
                }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}
