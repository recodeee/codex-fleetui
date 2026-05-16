// fleet-state — tuirealm port. Renders the live fleet table (accounts +
// panes via `fleet_data::fleet::load_live`) inside a tuirealm
// `AppComponent`. Second binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (PR #50).
//
// Pattern (mirrors fleet-tab-strip):
//   - `FleetView` is the Component. It owns `rows: Option<Vec<WorkerRow>>`,
//     a `load_error: Option<String>`, and refreshes on every Tick.
//   - `Msg::Tick` drives a `refresh()` call and keeps the Spotlight
//     caret animated; `Msg::Quit` terminates the loop. q / Esc quit when
//     Spotlight is closed, and dismiss it when Spotlight is open.
//   - The existing render functions (`render`, `render_worker_row`,
//     `chip_kind`) are unchanged — the tuirealm wrapper only owns the
//     event loop, not the rendering.
//
// `cargo build -p fleet-state` exercises the full chain:
// tmux::list_panes → panes::list_panes → fleet::join.

use std::io;
use std::time::Duration;

use fleet_data::{
    fleet::{self, FleetSummary, WorkerRow},
    panes::PaneState,
};
use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    palette::*,
    rail::{progress_rail, RailAxis},
    spotlight_overlay::{
        shared_spotlight_filter, Spotlight, SpotlightState, SHARED_SPOTLIGHT_ITEMS,
    },
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

// ---------- Messages and component IDs ----------

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Fleet,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Overlay {
    #[default]
    None,
    Spotlight,
}

// ---------- The Fleet component ----------

/// tmux session + window the fleet's worker panes live in. Matches the
/// `codex-fleet:overview` target every dashboard binary uses; overridable
/// via env for parallel fleets (`codex-fleet-2`, …).
fn fleet_target() -> (String, String) {
    let session =
        std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".to_string());
    let window = std::env::var("CODEX_FLEET_WINDOW").unwrap_or_else(|_| "overview".to_string());
    (session, window)
}

struct FleetView {
    rows: Option<Vec<WorkerRow>>,
    load_error: Option<String>,
    overlay: Overlay,
    spotlight: Spotlight<'static>,
    spotlight_state: SpotlightState,
    props: Props,
}

impl Default for FleetView {
    fn default() -> Self {
        let mut view = Self {
            rows: None,
            load_error: None,
            overlay: Overlay::None,
            spotlight: Spotlight::new(SHARED_SPOTLIGHT_ITEMS.to_vec()),
            spotlight_state: SpotlightState::default(),
            props: Props::default(),
        };
        view.refresh();
        view
    }
}

impl FleetView {
    fn open_spotlight(&mut self) {
        self.overlay = Overlay::Spotlight;
        self.spotlight_state.query.clear();
        self.spotlight_state.selected = 0;
    }

    fn close_spotlight(&mut self) {
        self.overlay = Overlay::None;
        self.spotlight_state.query.clear();
        self.spotlight_state.selected = 0;
    }

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
}

impl Component for FleetView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 30 || area.height < 8 {
            return;
        }
        frame.render_widget(
            Block::default().style(Style::default().bg(IOS_BG_GLASS)),
            area,
        );

        // Design G layout:
        //   Row 0..=4   header block (FLEET caption / big stat / button row)
        //   Row 5       column header row (ACCOUNT, WEEKLY · 5H, WORKER · 5H, STATUS, WORKING ON, PANE)
        //   Row 6..N    worker row cards, 2 content lines each (avatar+email / sub+rails+status+working+pane)
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // header card
                Constraint::Length(3), // column header strip card
                Constraint::Min(0),    // worker rows card
            ])
            .split(area);

        // ── Header card ───────────────────────────────────────────────────
        // Matches Design G's two-line title block: small "FLEET" caption
        // above a big "N workers · M live · K in review" line. Right side
        // carries the Filter + "+ New worker" pills (the latter accent-blue).
        let summary = self
            .rows
            .as_ref()
            .map(|rows| FleetSummary::of(rows.as_slice()));
        render_header(frame, layout[0], summary.as_ref());

        // ── Column header strip ───────────────────────────────────────────
        let column_block = card(Some("COLUMNS"), false);
        let column_inner = column_block.inner(layout[1]);
        frame.render_widget(column_block, layout[1]);
        render_column_headers(frame, column_inner);

        // ── Worker rows ───────────────────────────────────────────────────
        let rows_block = card(Some("WORKERS"), false);
        let body = rows_block.inner(layout[2]);
        frame.render_widget(rows_block, layout[2]);
        match &self.rows {
            Some(worker_rows) if !worker_rows.is_empty() => {
                // 2 content lines inside a rounded hairline row card, plus a
                // 1-line gap between rows, capped by area.
                let row_h: u16 = 4;
                let gap: u16 = 1;
                let unit = row_h + gap;
                let max_rows = (body.height / unit) as usize;
                for (i, row) in worker_rows.iter().take(max_rows).enumerate() {
                    let y = body.y + (i as u16) * unit;
                    if y + row_h > body.y + body.height {
                        break;
                    }
                    render_worker_row(
                        frame,
                        Rect {
                            x: body.x,
                            y,
                            width: body.width,
                            height: row_h,
                        },
                        row,
                        i,
                    );
                }
            }
            Some(_) => {
                let msg = match &self.load_error {
                    Some(_) => "  fleet unreachable — see status below",
                    None => "  no workers — is the codex-fleet session up?",
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        msg,
                        Style::default().fg(IOS_FG_MUTED),
                    ))),
                    Rect {
                        x: body.x,
                        y: body.y,
                        width: body.width,
                        height: 1,
                    },
                );
            }
            None => {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "  loading fleet state…",
                        Style::default().fg(IOS_FG_MUTED),
                    ))),
                    Rect {
                        x: body.x,
                        y: body.y,
                        width: body.width,
                        height: 1,
                    },
                );
            }
        }

        let inner = body; // alias used by the error-line render below

        if let Some(err) = &self.load_error {
            let y = inner.y + inner.height.saturating_sub(1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("  load error: {err}"),
                    Style::default().fg(IOS_FG_FAINT),
                ))),
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
            );
        }

        if self.overlay == Overlay::Spotlight {
            self.spotlight.render(frame, area, &self.spotlight_state);
        }
    }

    fn query(&self, attr: Attribute) -> Option<QueryResult<'_>> {
        self.props.get(attr).map(|v| QueryResult::from(v.clone()))
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<Msg, NoUserEvent> for FleetView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Char('/'),
                ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('?'),
                ..
            }) => {
                self.open_spotlight();
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                if self.overlay == Overlay::Spotlight {
                    self.close_spotlight();
                    Some(Msg::Tick)
                } else {
                    Some(Msg::Quit)
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) if self.overlay == Overlay::Spotlight => {
                self.close_spotlight();
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) if self.overlay == Overlay::Spotlight => {
                self.spotlight_state.query.pop();
                self.spotlight_state.selected = 0;
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
                if self.overlay == Overlay::Spotlight =>
            {
                self.spotlight_state.selected = self.spotlight_state.selected.saturating_sub(1);
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) if self.overlay == Overlay::Spotlight => {
                let max = shared_spotlight_filter(&self.spotlight_state.query)
                    .len()
                    .saturating_sub(1);
                self.spotlight_state.selected = (self.spotlight_state.selected + 1).min(max);
                Some(Msg::Tick)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(c), ..
            }) if self.overlay == Overlay::Spotlight && !c.is_control() => {
                self.spotlight_state.query.push(*c);
                self.spotlight_state.selected = 0;
                Some(Msg::Tick)
            }
            Event::Tick => {
                // Refresh under the hood so the next frame sees fresh rows.
                self.refresh();
                self.spotlight_state.tick = self.spotlight_state.tick.wrapping_add(1);
                Some(Msg::Tick)
            }
            _ => None,
        }
    }
}

// ---------- Render helpers (unchanged from pre-tuirealm) ----------

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

// ── Column widths (cell-based) ─────────────────────────────────────────────
// Tuned for a 274-col tmux pane (the typical codex-fleet overview width).
// Sum = ACCOUNT(34) + sep(2) + WEEKLY(15) + sep(2) + WORKER(15) + sep(2)
//     + STATUS(14) + sep(2) + WORKING(~rest) + sep(2) + PANE(8) ≈ 274.
const COL_ACCOUNT: u16 = 34;
const COL_RAIL_DUAL: u16 = 15; // for both WEEKLY · 5H and WORKER · 5H
const COL_STATUS: u16 = 14;
const COL_PANE: u16 = 8;
const COL_SEP: u16 = 2;
// Temporary row tones until fleet-ui exposes IOS_ROW_BG_LIGHT / DARK.
const IOS_ROW_BG_LIGHT: Color = Color::Rgb(44, 44, 48);
const IOS_ROW_BG_DARK: Color = Color::Rgb(38, 38, 40);

/// Header card matching design G — small caption + big stat + Filter/New pills.
fn render_header(frame: &mut Frame, area: Rect, summary: Option<&FleetSummary>) {
    // Outer card chrome (border + title slot).
    let block = card(None, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 10 || inner.height < 2 {
        return;
    }
    // Line 1: small "FLEET" caption.
    let caption_y = inner.y;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  FLEET",
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: inner.x,
            y: caption_y,
            width: inner.width,
            height: 1,
        },
    );
    // Line 2: big "N workers · M live · K in review" + right-aligned action
    // pills. Both halves share the same row so the layout reads like the PNG.
    let stat_y = inner.y + 1;
    let big = match summary {
        Some(s) => format!(
            "  {} workers  ·  {} live  ·  {} in review",
            s.workers, s.live, s.in_review
        ),
        None => "  loading fleet…".to_string(),
    };
    let big_w = visible_width(&big) as u16;
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            big,
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: inner.x,
            y: stat_y,
            width: big_w.min(inner.width),
            height: 1,
        },
    );
    // Right-side Filter + "+ New worker" pills.
    let filter_pill = "  Filter  ";
    let new_pill = "  + New worker  ";
    let pill_total = visible_width(filter_pill) + 1 + visible_width(new_pill);
    if (inner.width as usize) > pill_total + 4 {
        let pill_x = inner.x + inner.width - pill_total as u16 - 2;
        // Filter (outline / muted bg)
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                filter_pill,
                Style::default().fg(IOS_FG).bg(IOS_BG_GLASS),
            ))),
            Rect {
                x: pill_x,
                y: stat_y,
                width: visible_width(filter_pill) as u16,
                height: 1,
            },
        );
        // + New worker (accent fill)
        let new_x = pill_x + visible_width(filter_pill) as u16 + 1;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                new_pill,
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_TINT)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: new_x,
                y: stat_y,
                width: visible_width(new_pill) as u16,
                height: 1,
            },
        );
    }
}

/// Column-header strip — `ACCOUNT  WEEKLY · 5H  WORKER · 5H  STATUS  WORKING ON  PANE`.
fn render_column_headers(frame: &mut Frame, area: Rect) {
    if area.width < 60 || area.height == 0 {
        return;
    }
    let y = area.y;
    let mut x = area.x + 2;
    let style = Style::default()
        .fg(IOS_FG_MUTED)
        .add_modifier(Modifier::BOLD);
    let put = |frame: &mut Frame, x: u16, w: u16, label: &str| {
        let rect = Rect {
            x,
            y,
            width: w.min(area.width.saturating_sub(x - area.x)),
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.to_string(), style))),
            rect,
        );
    };
    put(frame, x, COL_ACCOUNT, "ACCOUNT");
    x += COL_ACCOUNT + COL_SEP;
    put(frame, x, COL_RAIL_DUAL, "WEEKLY · 5H");
    x += COL_RAIL_DUAL + COL_SEP;
    put(frame, x, COL_RAIL_DUAL, "WORKER · 5H");
    x += COL_RAIL_DUAL + COL_SEP;
    put(frame, x, COL_STATUS, "STATUS");
    x += COL_STATUS + COL_SEP;
    let working_w = area.width.saturating_sub(x - area.x + COL_PANE + COL_SEP);
    put(frame, x, working_w, "WORKING ON");
    x += working_w + COL_SEP;
    put(frame, x, COL_PANE, "PANE");
}

/// 2-character avatar initials derived from the agent_id (e.g. `bia-zazrifka`
/// → `BZ`, `admin-magnolia` → `AM`). Used in the per-row avatar block.
fn avatar_initials(agent_id: &str) -> String {
    let mut parts = agent_id.split(|c: char| c == '-' || c == '_' || c == '.');
    let first = parts.next().and_then(|s| s.chars().next()).unwrap_or('?');
    let second = parts.next().and_then(|s| s.chars().next()).unwrap_or(first);
    format!(
        "{}{}",
        first.to_uppercase().next().unwrap_or('?'),
        second.to_uppercase().next().unwrap_or('?')
    )
}

/// Stable per-agent avatar colour drawn from a small UIColor-system palette.
/// FNV-1a of the agent id mod 6 picks one of 6 named accents so the same
/// agent gets the same colour run-to-run (no global state).
fn avatar_color(agent_id: &str) -> Color {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in agent_id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    match h % 6 {
        0 => IOS_TINT,                 // systemBlue
        1 => IOS_GREEN,                // systemGreen
        2 => IOS_DESTRUCTIVE,          // systemRed
        3 => Color::Rgb(255, 149, 0),  // systemOrange
        4 => Color::Rgb(175, 82, 222), // systemPurple
        _ => Color::Rgb(255, 204, 0),  // systemYellow
    }
}

/// 2-line worker row matching Design G:
///   Line 1: [avatar]  email                     ▕rail▏  ▕rail▏      [chip]    working_on            #N >
///   Line 2:           got X.X high              5h sub-text          (pane sub)
fn render_worker_row(frame: &mut Frame, area: Rect, row: &WorkerRow, row_index: usize) {
    if area.height < 4 || area.width < 60 {
        return;
    }
    let row_bg = if row_index % 2 == 0 {
        IOS_ROW_BG_LIGHT
    } else {
        IOS_ROW_BG_DARK
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
        .style(Style::default().bg(row_bg).fg(IOS_FG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 2 || inner.width < 58 {
        return;
    }
    let y1 = inner.y;
    let y2 = inner.y + 1;

    // ── ACCOUNT (avatar + email + sub) ────────────────────────────────────
    let mut x = inner.x + 1;
    // Avatar block: 4 cells wide, colored bg, 2-char initials centered.
    let avatar = format!(" {} ", avatar_initials(&row.agent_id));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            avatar.clone(),
            Style::default()
                .fg(IOS_FG)
                .bg(avatar_color(&row.agent_id))
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x,
            y: y1,
            width: visible_width(&avatar) as u16,
            height: 1,
        },
    );
    let email_x = x + visible_width(&avatar) as u16 + 1;
    let email_w = COL_ACCOUNT.saturating_sub(visible_width(&avatar) as u16 + 2);
    // Email line.
    let email_label = if row.is_current {
        format!("★ {}", row.email)
    } else {
        row.email.clone()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_chars(&email_label, email_w as usize),
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: email_x,
            y: y1,
            width: email_w,
            height: 1,
        },
    );
    // Sub-line: model label (e.g. "gpt-5.5 xhigh") or "got X high" placeholder.
    let sub_text = match &row.model_label {
        Some(m) => format!("got {}", m),
        None => "got — ".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_chars(&sub_text, email_w as usize),
            Style::default().fg(IOS_FG_MUTED),
        ))),
        Rect {
            x: email_x,
            y: y2,
            width: email_w,
            height: 1,
        },
    );
    x = inner.x + 1 + COL_ACCOUNT + COL_SEP;

    // ── WEEKLY · 5H dual rails ────────────────────────────────────────────
    // Two stacked rails — WEEKLY on row 1, 5H label on row 2 ("X% / Y%"
    // style of the design's sub-bar isn't replicable so we put the rail on
    // line 1 and the numeric on line 2 for parity).
    let rail_w: u16 = COL_RAIL_DUAL.saturating_sub(2);
    let weekly_rail: Vec<Span> = progress_rail(row.weekly_pct, RailAxis::Usage, rail_w);
    frame.render_widget(
        Paragraph::new(Line::from(weekly_rail)),
        Rect {
            x,
            y: y1,
            width: COL_RAIL_DUAL,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}%", row.weekly_pct),
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x,
            y: y2,
            width: COL_RAIL_DUAL,
            height: 1,
        },
    );
    x += COL_RAIL_DUAL + COL_SEP;

    // ── WORKER · 5H rails (the 5h budget, displayed as second dual rail) ─
    let worker_rail: Vec<Span> = progress_rail(row.five_h_pct, RailAxis::Usage, rail_w);
    frame.render_widget(
        Paragraph::new(Line::from(worker_rail)),
        Rect {
            x,
            y: y1,
            width: COL_RAIL_DUAL,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}%", row.five_h_pct),
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x,
            y: y2,
            width: COL_RAIL_DUAL,
            height: 1,
        },
    );
    x += COL_RAIL_DUAL + COL_SEP;

    // ── STATUS chip ───────────────────────────────────────────────────────
    let chip_spans: Vec<Span> = status_chip(chip_kind(row.state));
    frame.render_widget(
        Paragraph::new(Line::from(chip_spans)),
        Rect {
            x,
            y: y1,
            width: COL_STATUS,
            height: 1,
        },
    );
    x += COL_STATUS + COL_SEP;

    // ── WORKING ON (2-line) ──────────────────────────────────────────────
    let working_w = inner
        .width
        .saturating_sub(x - inner.x + COL_PANE + COL_SEP + 1);
    if row.working_on.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "—  reserve",
                Style::default().fg(IOS_FG_FAINT),
            ))),
            Rect {
                x,
                y: y1,
                width: working_w,
                height: 1,
            },
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate_chars(&row.working_on, working_w as usize),
                Style::default().fg(IOS_FG),
            ))),
            Rect {
                x,
                y: y1,
                width: working_w,
                height: 1,
            },
        );
        if !row.pane_subtext.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    truncate_chars(&row.pane_subtext, working_w as usize),
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect {
                    x,
                    y: y2,
                    width: working_w,
                    height: 1,
                },
            );
        }
    }
    x += working_w + COL_SEP;

    // ── PANE pill (#N >) ─────────────────────────────────────────────────
    let pane_label = row
        .pane_id
        .as_deref()
        .map(|p| {
            // tmux pane ids are `%47`; the design's pill renders `#7 >` style
            // by stripping the `%`. Fall back to the raw id when stripping fails.
            let stripped = p.trim_start_matches('%');
            format!(" #{stripped} > ")
        })
        .unwrap_or_else(|| "        ".to_string());
    let pane_style = if row.pane_id.is_some() {
        Style::default().fg(IOS_FG).bg(IOS_BG_GLASS)
    } else {
        Style::default().fg(IOS_FG_FAINT)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(pane_label, pane_style))),
        Rect {
            x,
            y: y1,
            width: COL_PANE,
            height: 1,
        },
    );
}

/// Width-in-cells approximation for ASCII strings. We don't pull in
/// `unicode-width` here — every glyph used by this binary is single-cell.
fn visible_width(s: &str) -> usize {
    s.chars().count()
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut chars = 0;
    for c in s.chars() {
        if chars + 1 > n {
            out.push('…');
            break;
        }
        out.push(c);
        chars += 1;
    }
    out
}

// ---------- Model (tuirealm's M in M-V-U) ----------

struct Model<T: TerminalAdapter> {
    app: Application<Id, Msg, NoUserEvent>,
    terminal: T,
    quit: bool,
    redraw: bool,
}

impl Model<CrosstermTerminalAdapter> {
    fn new() -> io::Result<Self> {
        let app = Self::init_app().map_err(|e| io::Error::other(format!("init app: {e:?}")))?;
        let terminal =
            Self::init_adapter().map_err(|e| io::Error::other(format!("init adapter: {e:?}")))?;
        Ok(Self {
            app,
            terminal,
            quit: false,
            redraw: true,
        })
    }

    fn init_app() -> Result<Application<Id, Msg, NoUserEvent>, Box<dyn std::error::Error>> {
        // 250ms tick interval matches the pre-tuirealm refresh cadence.
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .crossterm_input_listener(Duration::from_millis(100), 3)
                .tick_interval(Duration::from_millis(250)),
        );
        app.mount(
            Id::Fleet,
            Box::new(FleetView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Fleet)?;
        Ok(app)
    }

    fn init_adapter() -> Result<CrosstermTerminalAdapter, Box<dyn std::error::Error>> {
        let mut adapter = CrosstermTerminalAdapter::new()?;
        adapter.enable_raw_mode()?;
        adapter.enter_alternate_screen()?;
        Ok(adapter)
    }
}

impl<T: TerminalAdapter> Model<T> {
    fn view(&mut self) {
        let _ = self.terminal.draw(|frame| {
            let area = frame.area();
            let _ = self.app.view(&Id::Fleet, frame, area);
        });
    }

    fn update(&mut self, msg: Msg) {
        self.redraw = true;
        match msg {
            Msg::Quit => self.quit = true,
            Msg::Tick => {}
        }
    }
}

// ---------- Entry point ----------

fn main() -> io::Result<()> {
    let mut model = Model::<CrosstermTerminalAdapter>::new()?;

    let result = (|| -> io::Result<()> {
        while !model.quit {
            if let Ok(messages) = model
                .app
                .tick(PollStrategy::Once(Duration::from_millis(100)))
            {
                for msg in messages {
                    model.update(msg);
                }
            }
            if model.redraw {
                model.view();
                model.redraw = false;
            }
        }
        Ok(())
    })();

    let _ = model.terminal.disable_raw_mode();
    let _ = model.terminal.leave_alternate_screen();
    result
}
