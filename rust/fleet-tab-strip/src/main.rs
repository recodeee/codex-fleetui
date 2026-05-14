// fleet-tab-strip — tuirealm port. Renders the codex-fleet glass-dock tab
// strip as an `AppComponent`, routing `MouseEvent::Down(Left)` through
// tuirealm's M-V-U cycle to dispatch `tmux select-window`.
//
// First binary in the codex-fleet ratatui → tuirealm migration. The
// existing `fleet_ui::tab_strip::TabStrip` widget stays as the rendering
// backend (now usable directly thanks to the workspace ratatui bump
// 0.28 → 0.30 in this same PR); the binary wraps it in a `Component` +
// `AppComponent<Msg, NoUserEvent>` pair so the click handler, tick
// counter, and active-tab resolution flow through tuirealm's update cycle
// instead of an ad-hoc crossterm event loop.
//
// Why migrate at all: the codex-fleet binaries each grow their own
// hand-rolled crossterm event loop + state. tuirealm gives us a uniform
// (state, update, view) shape so future binaries (fleet-state,
// fleet-plan-tree, fleet-waves, fleet-watcher, fleet-tui-poc) can be
// re-implemented with the same mental model.

use std::fs;
use std::io;
// std::process::Command moved into fleet_components::select_tmux_window.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, MouseButton, MouseEvent, MouseEventKind, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::Paragraph;
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

use fleet_ui::tab_strip::{Tab, TabHit};

// ---------- Messages and component IDs ----------

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    TabClicked(usize),
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Strip,
}

// ---------- The Strip component ----------

/// Wraps `fleet_ui::tab_strip::TabStrip` as a tuirealm `Component` +
/// `AppComponent<Msg, NoUserEvent>`. Owns:
///
///   - the wall-clock tick counter shown in the live chip,
///   - the most recent hit-test rects so `on(Event::Mouse(..))` can map a
///     click coordinate to a tmux window index without re-rendering.
///
/// Active tab is resolved on each `view()` from `tmux display-message` —
/// when the operator switches windows, the active pill follows.
struct StripView {
    tick: u64,
    last_hits: Vec<TabHit>,
    props: Props,
}

impl Default for StripView {
    fn default() -> Self {
        Self {
            tick: 0,
            last_hits: Vec::new(),
            props: Props::default(),
        }
    }
}

impl Component for StripView {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        let active = current_tab();
        self.last_hits = render_design_e_dock(frame, area, active, self.tick);
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

impl AppComponent<Msg, NoUserEvent> for StripView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => self
                .last_hits
                .iter()
                .find(|h| {
                    *column >= h.rect.x
                        && *column < h.rect.x + h.rect.width
                        && *row >= h.rect.y
                        && *row < h.rect.y + h.rect.height
                })
                .map(|h| Msg::TabClicked(h.window_idx)),
            Event::Tick => Some(Msg::Tick),
            _ => None,
        }
    }
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
        let mut app: Application<Id, Msg, NoUserEvent> = Application::init(
            EventListenerCfg::default()
                .crossterm_input_listener(Duration::from_millis(100), 3)
                .tick_interval(Duration::from_millis(500)),
        );
        app.mount(
            Id::Strip,
            Box::new(StripView::default()),
            vec![Sub::new(EventClause::Tick, SubClause::Always)],
        )?;
        app.active(&Id::Strip)?;
        Ok(app)
    }

    fn init_adapter() -> Result<CrosstermTerminalAdapter, Box<dyn std::error::Error>> {
        // Delegate to the shared helper. `true` opts into
        // EnableMouseCapture because fleet-tab-strip's pills are
        // clickable.
        Ok(fleet_components::init_crossterm_adapter(true)?)
    }
}

impl<T: TerminalAdapter> Model<T> {
    fn view(&mut self) {
        let _ = self.terminal.draw(|frame| {
            let area = frame.area();
            let _ = self.app.view(&Id::Strip, frame, area);
        });
    }

    fn update(&mut self, msg: Msg) {
        self.redraw = true;
        match msg {
            Msg::Quit => self.quit = true,
            Msg::TabClicked(idx) => select_window(idx),
            Msg::Tick => {}
        }
    }
}

// ---------- Design E local render path ----------

// Reference: images/E _ Glass dock _ floating top nav.html / .png.
// Kept in this binary because this lane is scoped to fleet-tab-strip only.
const BG: Color = Color::Rgb(0, 0, 0);
const BG_DIM: Color = Color::Rgb(13, 17, 23);
const GLASS: Color = Color::Rgb(44, 44, 46);
const GLASS_EDGE: Color = Color::Rgb(72, 72, 74);
const GLASS_SHADOW: Color = Color::Rgb(8, 20, 34);
const FG: Color = Color::Rgb(242, 242, 247);
const FG_MUTED: Color = Color::Rgb(174, 174, 184);
const FG_DIM: Color = Color::Rgb(99, 99, 108);
const BLUE: Color = Color::Rgb(10, 132, 255);
const BLUE_EDGE: Color = Color::Rgb(64, 156, 255);
const BLUE_CHIP: Color = Color::Rgb(93, 173, 255);
const GREEN: Color = Color::Rgb(48, 209, 88);
const GREEN_DIM: Color = Color::Rgb(30, 115, 54);
const GREEN_GLASS: Color = Color::Rgb(28, 69, 42);
const ORANGE: Color = Color::Rgb(255, 127, 39);
const COUNTERS_PATH: &str = "/tmp/claude-viz/fleet-tab-counters.json";
const COUNTER_STALE_SECS: u64 = 30;

#[derive(Clone, Copy)]
struct PillSpec {
    tab: Tab,
    width: u16,
}

fn render_design_e_dock(frame: &mut Frame, area: Rect, active: Tab, tick: u64) -> Vec<TabHit> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    fill_rect(frame, area, BG);

    if area.height >= 5 {
        render_ghost_strip(
            frame,
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            },
        );
    }

    let full_height = area.height >= 3;
    let dock_height = if full_height { 3 } else { 1 };
    let desired_width = 250_u16;
    let dock_width = desired_width.min(area.width);
    let dock_x = area.x + area.width.saturating_sub(dock_width) / 2;
    let dock_y = if area.height >= 5 { area.y + 2 } else { area.y };
    let dock_end = dock_x.saturating_add(dock_width);

    if full_height && dock_y + dock_height < area.y + area.height {
        fill_rect(
            frame,
            Rect {
                x: dock_x + 2,
                y: dock_y + dock_height,
                width: dock_width.saturating_sub(4),
                height: 1,
            },
            GLASS_SHADOW,
        );
    }

    let mut x = dock_x;
    render_logo_pill(
        frame,
        Rect {
            x,
            y: dock_y,
            width: 34.min(dock_width),
            height: dock_height,
        },
    );
    x += 36;
    render_separator(frame, x, dock_y, dock_height);
    x += 3;

    let specs = [
        PillSpec {
            tab: Tab::Overview,
            width: 40,
        },
        PillSpec {
            tab: Tab::Fleet,
            width: 34,
        },
        PillSpec {
            tab: Tab::Plan,
            width: 34,
        },
        PillSpec {
            tab: Tab::Waves,
            width: 36,
        },
        PillSpec {
            tab: Tab::Review,
            width: 38,
        },
    ];

    let mut hits = Vec::with_capacity(specs.len());
    for (idx, spec) in specs.iter().enumerate() {
        let w = spec.width.min(dock_end.saturating_sub(x));
        if w < 10 {
            break;
        }
        let rect = Rect {
            x,
            y: dock_y,
            width: w,
            height: dock_height,
        };
        render_tab_pill(frame, rect, spec.tab, spec.tab == active);
        hits.push(TabHit {
            rect,
            tab: spec.tab,
            window_idx: spec.tab.window_idx(),
        });
        x += w;
        if idx + 1 < specs.len() {
            x += 2;
        }
        if x >= dock_end {
            break;
        }
    }

    let live_w = 14_u16.min(dock_end.saturating_sub(x).saturating_sub(3));
    if live_w >= 10 {
        x += 3;
        render_separator(frame, x - 2, dock_y, dock_height);
        render_live_pill(
            frame,
            Rect {
                x,
                y: dock_y,
                width: live_w,
                height: dock_height,
            },
            tick,
        );
    }

    hits
}

fn render_ghost_strip(frame: &mut Frame, rect: Rect) {
    let clock = clock_hms();
    let left = " ◆ codex-fleet   0 overview   1 fleet   2 plan   3 waves   4 review   5 watch>";
    let right = format!("  ● live                                  {clock}");
    let mut text = String::from(left);
    if rect.width as usize > right.chars().count() + text.chars().count() {
        text.push_str(
            &" ".repeat(rect.width as usize - right.chars().count() - text.chars().count()),
        );
        text.push_str(&right);
    }
    let clipped = clip_to_width(&text, rect.width);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            clipped,
            Style::default()
                .fg(FG_DIM)
                .bg(BG_DIM)
                .add_modifier(Modifier::DIM),
        ))),
        rect,
    );
}

fn render_logo_pill(frame: &mut Frame, rect: Rect) {
    let clock = clock_hms();
    let content = vec![
        Span::styled(
            " ◆ ",
            Style::default()
                .fg(BG)
                .bg(ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " codex-fleet ",
            Style::default()
                .fg(FG)
                .bg(GLASS)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(clock, Style::default().fg(FG_MUTED).bg(GLASS)),
    ];
    render_glass_pill(frame, rect, GLASS, GLASS_EDGE, content);
}

fn render_tab_pill(frame: &mut Frame, rect: Rect, tab: Tab, active: bool) {
    let (fill, edge, chip, fg, label_mod) = if active {
        (BLUE, BLUE_EDGE, BLUE_CHIP, FG, Modifier::BOLD)
    } else {
        (
            GLASS,
            GLASS_EDGE,
            Color::Rgb(86, 86, 92),
            FG,
            Modifier::BOLD,
        )
    };
    let counter = tab_counter(tab);
    let content = vec![
        Span::styled(
            format!(" {} ", tab.window_idx()),
            Style::default()
                .fg(FG)
                .bg(chip)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}  {}  ", tab_icon(tab), tab_label(tab)),
            Style::default().fg(fg).bg(fill).add_modifier(label_mod),
        ),
        Span::styled(
            format!(" {} ", counter),
            Style::default()
                .fg(FG)
                .bg(chip)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    render_glass_pill(frame, rect, fill, edge, content);
}

fn render_live_pill(frame: &mut Frame, rect: Rect, tick: u64) {
    let pulse_on = tick % 4 < 2;
    let dot = if pulse_on { GREEN } else { GREEN_DIM };
    let edge = if pulse_on {
        Color::Rgb(61, 220, 104)
    } else {
        GREEN_DIM
    };
    let content = vec![
        Span::styled(
            " ● ",
            Style::default()
                .fg(dot)
                .bg(GREEN_GLASS)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "live ",
            Style::default()
                .fg(GREEN)
                .bg(GREEN_GLASS)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    render_glass_pill(frame, rect, GREEN_GLASS, edge, content);
}

fn render_separator(frame: &mut Frame, x: u16, y: u16, height: u16) {
    if height >= 3 {
        let rect = Rect {
            x,
            y,
            width: 1,
            height,
        };
        fill_rect(frame, rect, Color::Rgb(34, 34, 38));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "│",
                Style::default().fg(GLASS_EDGE).bg(BG),
            ))),
            Rect {
                x,
                y: y + 1,
                width: 1,
                height: 1,
            },
        );
    }
}

fn render_glass_pill(
    frame: &mut Frame,
    rect: Rect,
    fill: Color,
    edge: Color,
    content: Vec<Span<'static>>,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    if rect.height >= 3 && rect.width >= 4 {
        let horizontal = "─".repeat(rect.width.saturating_sub(2) as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("╭{horizontal}╮"),
                Style::default().fg(edge).bg(BG),
            ))),
            Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: 1,
            },
        );
        render_pill_middle(
            frame,
            Rect {
                x: rect.x,
                y: rect.y + 1,
                width: rect.width,
                height: 1,
            },
            fill,
            edge,
            content,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("╰{horizontal}╯"),
                Style::default().fg(edge).bg(BG),
            ))),
            Rect {
                x: rect.x,
                y: rect.y + 2,
                width: rect.width,
                height: 1,
            },
        );
    } else {
        let compact = Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: 1,
        };
        render_pill_middle(frame, compact, fill, edge, content);
    }
}

fn render_pill_middle(
    frame: &mut Frame,
    rect: Rect,
    fill: Color,
    edge: Color,
    content: Vec<Span<'static>>,
) {
    if rect.width < 2 {
        return;
    }
    let inner = rect.width.saturating_sub(2) as usize;
    let content_width: usize = content.iter().map(span_width).sum();
    let left_pad = inner.saturating_sub(content_width) / 2;
    let right_pad = inner.saturating_sub(content_width + left_pad);
    let mut spans = vec![Span::styled("│", Style::default().fg(edge).bg(BG))];
    if left_pad > 0 {
        spans.push(Span::styled(
            " ".repeat(left_pad),
            Style::default().bg(fill),
        ));
    }
    spans.extend(content);
    if right_pad > 0 {
        spans.push(Span::styled(
            " ".repeat(right_pad),
            Style::default().bg(fill),
        ));
    }
    spans.push(Span::styled("│", Style::default().fg(edge).bg(BG)));
    frame.render_widget(Paragraph::new(Line::from(spans)), rect);
}

fn fill_rect(frame: &mut Frame, rect: Rect, bg: Color) {
    for row in rect.y..rect.y.saturating_add(rect.height) {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " ".repeat(rect.width as usize),
                Style::default().bg(bg),
            ))),
            Rect {
                x: rect.x,
                y: row,
                width: rect.width,
                height: 1,
            },
        );
    }
}

fn span_width(span: &Span<'_>) -> usize {
    span.content.chars().count()
}

fn clip_to_width(text: &str, width: u16) -> String {
    text.chars().take(width as usize).collect()
}

fn tab_icon(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "⌘",
        Tab::Fleet => "⌬",
        Tab::Plan => "▣",
        Tab::Waves => "≋",
        Tab::Review => "♢",
    }
}

fn tab_label(tab: Tab) -> &'static str {
    match tab {
        Tab::Overview => "Overview",
        Tab::Fleet => "Fleet",
        Tab::Plan => "Plan",
        Tab::Waves => "Waves",
        Tab::Review => "Review",
    }
}

fn tab_counter(tab: Tab) -> String {
    let key = match tab {
        Tab::Overview => "overview",
        Tab::Fleet => "fleet",
        Tab::Plan => "plan",
        Tab::Waves => "waves",
        Tab::Review => "review",
    };
    read_fresh_counter(key).unwrap_or_else(|| {
        match tab {
            Tab::Overview => 7,
            Tab::Fleet => 7,
            Tab::Plan => 12,
            Tab::Waves => 3,
            Tab::Review => 1,
        }
        .to_string()
    })
}

fn read_fresh_counter(key: &str) -> Option<String> {
    let raw = fs::read_to_string(COUNTERS_PATH).ok()?;
    if let Some(updated_at) = json_number(&raw, "updated_at") {
        let now = now_unix_secs();
        if now.saturating_sub(updated_at) > COUNTER_STALE_SECS {
            return None;
        }
    }
    json_number(&raw, key).map(|n| n.to_string())
}

fn json_number(raw: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let after_key = raw.split_once(&needle)?.1;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let digits: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn clock_hms() -> String {
    let secs = now_unix_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- tmux integration helpers ----------

fn current_tab() -> Tab {
    let idx: usize = std::process::Command::new("tmux")
        .args(["display-message", "-p", "-F", "#{window_index}"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0);
    match idx {
        0 => Tab::Overview,
        1 => Tab::Fleet,
        2 => Tab::Plan,
        3 => Tab::Waves,
        _ => Tab::Review,
    }
}

fn select_window(idx: usize) {
    // Delegated to the shared helper so every dashboard's tmux
    // click-routing semantics match.
    fleet_components::select_tmux_window(idx);
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

    fleet_components::shutdown_adapter(&mut model.terminal);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::ratatui::backend::TestBackend;
    use tuirealm::ratatui::Terminal;

    #[test]
    fn design_e_dock_renders_all_tabs_and_live_chip() {
        let mut terminal = Terminal::new(TestBackend::new(274, 6)).unwrap();
        let mut hits = Vec::new();

        terminal
            .draw(|frame| {
                hits = render_design_e_dock(
                    frame,
                    Rect {
                        x: 0,
                        y: 0,
                        width: 274,
                        height: 6,
                    },
                    Tab::Overview,
                    2,
                );
            })
            .unwrap();

        let rendered = format!("{}", terminal.backend());
        assert!(rendered.contains("codex-fleet"));
        assert!(rendered.contains("Overview"));
        assert!(rendered.contains("Fleet"));
        assert!(rendered.contains("Plan"));
        assert!(rendered.contains("Waves"));
        assert!(rendered.contains("Review"));
        assert!(rendered.contains("live"));
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0].window_idx, 0);
        assert_eq!(hits[4].window_idx, 4);
        assert!(hits[0].rect.width > hits[1].rect.width);
    }

    #[test]
    fn design_e_dock_degrades_to_one_row_header() {
        let mut terminal = Terminal::new(TestBackend::new(220, 1)).unwrap();
        let mut hits = Vec::new();

        terminal
            .draw(|frame| {
                hits = render_design_e_dock(
                    frame,
                    Rect {
                        x: 0,
                        y: 0,
                        width: 220,
                        height: 1,
                    },
                    Tab::Plan,
                    3,
                );
            })
            .unwrap();

        let rendered = format!("{}", terminal.backend());
        assert!(rendered.contains("codex-fleet"));
        assert!(rendered.contains("Plan"));
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|hit| hit.window_idx == 2));
    }

    #[test]
    fn json_number_extracts_counter_without_json_dependency() {
        let raw = r#"{ "overview": 7, "fleet": 2, "updated_at": 1715712986 }"#;
        assert_eq!(json_number(raw, "overview"), Some(7));
        assert_eq!(json_number(raw, "fleet"), Some(2));
        assert_eq!(json_number(raw, "review"), None);
    }
}
