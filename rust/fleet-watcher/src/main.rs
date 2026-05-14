// fleet-watcher — tuirealm port. Renders the watcher-board chrome (header
// banner + 4 stat cards + per-pane table placeholder) as a tuirealm
// `AppComponent`. Fifth binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (#50), fleet-state (#52),
// fleet-plan-tree (#53), fleet-waves (#54).

use std::io;
use std::time::Duration;

use fleet_ui::{
    card::card,
    chip::{status_chip, ChipKind},
    overlay::{card_shadow, centered_overlay, render_overlay},
    palette::*,
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
use tuirealm::ratatui::widgets::{Block, Paragraph};
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Redraw,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Watcher,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Overlay {
    #[default]
    None,
    Spotlight,
}

#[derive(Clone, Copy, Debug)]
struct SpotlightItem {
    group: &'static str,
    icon: &'static str,
    title: &'static str,
    sub: &'static str,
    kbd: &'static str,
}

const SPOTLIGHT_ITEMS: &[SpotlightItem] = &[
    SpotlightItem {
        group: "PANE",
        icon: "⊟",
        title: "Horizontal split",
        sub: "Split active pane top/bottom",
        kbd: "h",
    },
    SpotlightItem {
        group: "PANE",
        icon: "⊞",
        title: "Vertical split",
        sub: "Split active pane left/right",
        kbd: "v",
    },
    SpotlightItem {
        group: "PANE",
        icon: "⤢",
        title: "Zoom pane",
        sub: "Toggle full-screen for this pane",
        kbd: "z",
    },
    SpotlightItem {
        group: "PANE",
        icon: "⇄",
        title: "Swap with marked pane",
        sub: "Swap active and marked panes",
        kbd: "s",
    },
    SpotlightItem {
        group: "SESSION",
        icon: "⧉",
        title: "Copy whole session",
        sub: "Copy the current transcript",
        kbd: "⇧C",
    },
    SpotlightItem {
        group: "SESSION",
        icon: "☰",
        title: "Queue message",
        sub: "Send a message on next idle",
        kbd: "↹",
    },
    SpotlightItem {
        group: "SESSION",
        icon: "⌚",
        title: "Search history…",
        sub: "Search the current session",
        kbd: "/",
    },
    SpotlightItem {
        group: "FLEET",
        icon: "+",
        title: "Spawn new codex worker",
        sub: "Open another worker pane",
        kbd: "Ctrl N",
    },
    SpotlightItem {
        group: "FLEET",
        icon: "⎇",
        title: "Switch worktree…",
        sub: "Choose another branch/worktree",
        kbd: "Ctrl B",
    },
];

fn spotlight_filter(query: &str) -> Vec<&'static SpotlightItem> {
    if query.is_empty() {
        return SPOTLIGHT_ITEMS.iter().collect();
    }

    let q = query.to_lowercase();
    SPOTLIGHT_ITEMS
        .iter()
        .filter(|it| {
            it.title.to_lowercase().contains(&q)
                || it.sub.to_lowercase().contains(&q)
                || it.group.to_lowercase().contains(&q)
        })
        .collect()
}

fn spotlight_group_count(items: &[&'static SpotlightItem]) -> usize {
    let mut groups = 0;
    let mut last_group: Option<&str> = None;
    for item in items {
        if last_group != Some(item.group) {
            groups += 1;
            last_group = Some(item.group);
        }
    }
    groups
}

fn spotlight_selected(total: usize, selected: usize) -> usize {
    if total == 0 {
        0
    } else {
        selected.min(total - 1)
    }
}

#[derive(Default)]
struct WatcherView {
    props: Props,
    overlay: Overlay,
    spotlight_query: String,
    spotlight_selected: usize,
    spotlight_tick: u64,
}

impl WatcherView {
    fn open_spotlight(&mut self) {
        self.overlay = Overlay::Spotlight;
        self.spotlight_query.clear();
        self.spotlight_selected = 0;
    }

    fn close_spotlight(&mut self) {
        self.overlay = Overlay::None;
        self.spotlight_query.clear();
        self.spotlight_selected = 0;
    }
}

fn render_dashboard(frame: &mut Frame, area: Rect) {
    if area.width < 30 || area.height < 8 {
        return;
    }
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
        ("PANES", "8", ChipKind::Working),
        ("CAPPED", "0", ChipKind::Idle),
        ("SWAPPED", "0", ChipKind::Done),
        ("RANKED", "20", ChipKind::Working),
    ]
    .iter()
    .enumerate()
    {
        let block = card(Some(label), false);
        let inner = block.inner(stats[i]);
        frame.render_widget(block, stats[i]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!("  {value}  "),
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            )])),
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            },
        );
        let chip = status_chip(*kind);
        if inner.height >= 2 {
            frame.render_widget(
                Paragraph::new(Line::from(chip)),
                Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: 1,
                },
            );
        }
    }

    let panes_block = card(
        Some("FLEET PANES — port of watcher-board.sh deferred to follow-up"),
        false,
    );
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
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.min(3),
        },
    );
}

fn render_spotlight(frame: &mut Frame, area: Rect, view: &WatcherView) {
    let filtered = spotlight_filter(&view.spotlight_query);
    let total = filtered.len();
    let overlay_h = if total == 0 {
        12
    } else {
        (9 + total as u16 + spotlight_group_count(&filtered) as u16)
            .min(area.height.saturating_sub(2))
            .max(12)
    };
    let rect = centered_overlay(area, 76, overlay_h);
    card_shadow(frame, rect, area);
    let inner = render_overlay(frame, rect, Some("SPOTLIGHT"));
    if inner.width == 0 || inner.height < 6 {
        return;
    }

    let mut y = inner.y + 1;

    // Search row.
    let caret_on = (view.spotlight_tick / 4) % 2 == 0;
    let caret_char = if caret_on { "▏" } else { " " };
    let query_display = if view.spotlight_query.is_empty() {
        "type to filter…"
    } else {
        view.spotlight_query.as_str()
    };
    let query_style = if view.spotlight_query.is_empty() {
        Style::default().fg(IOS_FG_FAINT)
    } else {
        Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)
    };
    let hint = " /? ";
    let hint_w = hint.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⌕  ", Style::default().fg(IOS_FG_MUTED)),
            Span::styled(query_display.to_string(), query_style),
            Span::styled(
                caret_char,
                Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect {
            x: inner.x,
            y,
            width: inner.width.saturating_sub(hint_w + 1),
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
        ))),
        Rect {
            x: inner.x + inner.width - hint_w,
            y,
            width: hint_w,
            height: 1,
        },
    );
    y += 1;

    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(hairline, Style::default().fg(IOS_HAIRLINE))),
        Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 1,
        },
    );
    y += 2;

    if total == 0 {
        let msg = "no matches";
        let mw = msg.chars().count() as u16;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x + (inner.width.saturating_sub(mw)) / 2,
                y: y + 1,
                width: mw,
                height: 1,
            },
        );
        render_spotlight_footer(frame, inner);
        return;
    }

    let selected = spotlight_selected(total, view.spotlight_selected);
    let mut last_group: Option<&str> = None;
    let bottom_guard = inner.y + inner.height - 2;

    for (index, item) in filtered.iter().enumerate() {
        if y >= bottom_guard {
            break;
        }
        if last_group != Some(item.group) {
            if last_group.is_some() {
                if y >= bottom_guard {
                    break;
                }
                y += 1;
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {}", item.group),
                    Style::default()
                        .fg(IOS_FG_MUTED)
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width,
                    height: 1,
                },
            );
            y += 1;
            last_group = Some(item.group);
        }

        if y >= bottom_guard {
            break;
        }
        render_spotlight_row(frame, inner, y, item, index == selected);
        y += 1;
    }

    render_spotlight_footer(frame, inner);
}

fn render_spotlight_row(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    item: &SpotlightItem,
    selected: bool,
) {
    let row_bg = if selected { IOS_TINT_DARK } else { IOS_CARD_BG };
    let title_fg = if selected {
        Color::Rgb(255, 255, 255)
    } else {
        IOS_FG
    };
    let sub_fg = if selected { IOS_TINT_SUB } else { IOS_FG_MUTED };
    let kbd = format!(" {} ", item.kbd);
    let kbd_w = kbd.chars().count() as u16;
    let row_rect = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(row_bg)),
        row_rect,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", item.icon),
                Style::default()
                    .fg(title_fg)
                    .bg(IOS_ICON_CHIP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.title),
                Style::default()
                    .fg(title_fg)
                    .bg(row_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", item.sub),
                Style::default().fg(sub_fg).bg(row_bg),
            ),
        ])),
        Rect {
            x: inner.x,
            y,
            width: inner.width.saturating_sub(kbd_w + 1),
            height: 1,
        },
    );
    if inner.width > kbd_w + 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                kbd,
                Style::default()
                    .fg(title_fg)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x + inner.width - kbd_w,
                y,
                width: kbd_w,
                height: 1,
            },
        );
    }
}

fn render_spotlight_footer(frame: &mut Frame, inner: Rect) {
    let fy = inner.y + inner.height - 1;
    let footer = Line::from(vec![
        Span::styled("↵", Style::default().fg(IOS_FG)),
        Span::styled(" close    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("esc", Style::default().fg(IOS_FG)),
        Span::styled(" dismiss    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("↑↓", Style::default().fg(IOS_FG)),
        Span::styled(" move    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("/ ?", Style::default().fg(IOS_PURPLE)),
        Span::styled(" search", Style::default().fg(IOS_FG_MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect {
            x: inner.x,
            y: fy,
            width: inner.width,
            height: 1,
        },
    );
}

impl Component for WatcherView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        render_dashboard(frame, area);
        if self.overlay == Overlay::Spotlight {
            render_spotlight(frame, area, self);
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

impl AppComponent<Msg, NoUserEvent> for WatcherView {
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
                Some(Msg::Redraw)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => {
                if self.overlay == Overlay::Spotlight {
                    self.close_spotlight();
                    Some(Msg::Redraw)
                } else {
                    Some(Msg::Quit)
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) if self.overlay == Overlay::Spotlight => {
                self.close_spotlight();
                Some(Msg::Redraw)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) if self.overlay == Overlay::Spotlight => {
                self.spotlight_query.pop();
                self.spotlight_selected = 0;
                Some(Msg::Redraw)
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
                if self.overlay == Overlay::Spotlight =>
            {
                self.spotlight_selected = self.spotlight_selected.saturating_sub(1);
                Some(Msg::Redraw)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) if self.overlay == Overlay::Spotlight => {
                let max = spotlight_filter(&self.spotlight_query)
                    .len()
                    .saturating_sub(1);
                self.spotlight_selected = (self.spotlight_selected + 1).min(max);
                Some(Msg::Redraw)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(c), ..
            }) if self.overlay == Overlay::Spotlight && !c.is_control() => {
                self.spotlight_query.push(*c);
                self.spotlight_selected = 0;
                Some(Msg::Redraw)
            }
            Event::Tick => {
                self.spotlight_tick = self.spotlight_tick.wrapping_add(1);
                Some(Msg::Tick)
            }
            _ => None,
        }
    }
}

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
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .crossterm_input_listener(Duration::from_millis(100), 3)
                .tick_interval(Duration::from_millis(500)),
        );
        app.mount(
            Id::Watcher,
            Box::new(WatcherView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Watcher)?;
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
            let _ = self.app.view(&Id::Watcher, frame, area);
        });
    }

    fn update(&mut self, msg: Msg) {
        self.redraw = true;
        match msg {
            Msg::Quit => self.quit = true,
            Msg::Tick | Msg::Redraw => {}
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotlight_filter_keeps_catalog_order_for_empty_query() {
        let hits = spotlight_filter("");
        assert_eq!(hits.len(), SPOTLIGHT_ITEMS.len());
        assert_eq!(hits[0].title, "Horizontal split");
        assert_eq!(hits[hits.len() - 1].title, "Switch worktree…");
    }

    #[test]
    fn spotlight_filter_matches_group_title_and_subtext_case_insensitively() {
        let hits = spotlight_filter("split");
        let titles: Vec<&str> = hits.iter().map(|hit| hit.title).collect();
        assert!(titles.contains(&"Horizontal split"));
        assert!(titles.contains(&"Vertical split"));
        let query_hits = spotlight_filter("SESSION");
        assert!(query_hits
            .iter()
            .any(|hit| hit.title == "Copy whole session"));
    }

    #[test]
    fn spotlight_selection_clamps_to_available_results() {
        assert_eq!(spotlight_selected(0, 99), 0);
        assert_eq!(spotlight_selected(3, 99), 2);
        assert_eq!(spotlight_selected(3, 1), 1);
    }

    #[test]
    fn spotlight_open_and_close_reset_state() {
        let mut view = WatcherView::default();
        view.spotlight_query = "zoom".into();
        view.spotlight_selected = 2;
        view.open_spotlight();
        assert_eq!(view.overlay, Overlay::Spotlight);
        assert!(view.spotlight_query.is_empty());
        assert_eq!(view.spotlight_selected, 0);

        view.spotlight_query = "split".into();
        view.spotlight_selected = 1;
        view.close_spotlight();
        assert_eq!(view.overlay, Overlay::None);
        assert!(view.spotlight_query.is_empty());
        assert_eq!(view.spotlight_selected, 0);
    }
}
