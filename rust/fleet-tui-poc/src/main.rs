// fleet-tui-poc — validates the three risks called out in
// openspec/changes/fleet-tui-ratatui-port-2026-05-14/design.md before the
// fleet-ui crate scaffolds:
//
//   1. Truecolor (Color::Rgb(0, 122, 255) = systemBlue) renders inside tmux
//      without colour-quantisation against `style-tabs.sh` chrome.
//   2. ratatui's BorderType::Rounded does NOT double-frame against tmux's
//      `pane-border-status top` + `pane-border-format ' #[…] ▭ #{@panel} '`.
//   3. crossterm mouse-click events reach the binary through tmux's
//      `mouse on` pass-through.
//
// Also previews Phase 5 (overlays) by porting the four iOS palette
// artboards from the `terminal-ios-style` design handoff:
//   1 = context menu  ·  2 = spotlight  ·  3 = action sheet  ·  4 = session switcher
//   0 / Esc dismisses the overlay back to the validation harness.
//
// Run: `cargo run -p fleet-tui-poc`. `q` quits.

use std::{
    io::{self, stdout, Stdout},
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Terminal,
};

// ─── iOS dark-glass palette (mapped from design GLASS object) ──────────────
// rgba() values are pre-flattened against the terminal backdrop since cells
// don't blend. The numbers come straight from terminal-ios-style/palettes.jsx.
const IOS_BG_GLASS: Color = Color::Rgb(38, 38, 40);
const IOS_BG_SOLID: Color = Color::Rgb(28, 28, 30);
const IOS_HAIRLINE: Color = Color::Rgb(60, 60, 65);
const IOS_HAIRLINE_STRONG: Color = Color::Rgb(85, 85, 90);
const IOS_CHIP_BG: Color = Color::Rgb(54, 54, 58);
// Nested group cards inside a palette — slightly lighter than the palette bg,
// approximates the JSX `rgba(255,255,255,0.04)` block under each group.
const IOS_CARD_BG: Color = Color::Rgb(44, 44, 48);
// Icon chip inside the top-hit / group rows — sits on top of cards.
const IOS_ICON_CHIP: Color = Color::Rgb(70, 70, 76);
// Tint helpers for the systemBlue top-hit pill so subtitle + badge contrast
// against the bar fill without bleaching to pure white.
const IOS_TINT_DARK: Color = Color::Rgb(7, 100, 220);
const IOS_TINT_SUB: Color = Color::Rgb(210, 224, 255);
const IOS_FG: Color = Color::Rgb(242, 242, 247);
const IOS_FG_MUTED: Color = Color::Rgb(160, 160, 170);
const IOS_FG_FAINT: Color = Color::Rgb(110, 110, 120);
const IOS_TINT: Color = Color::Rgb(10, 132, 255);
const IOS_DESTRUCTIVE: Color = Color::Rgb(255, 69, 58);
const IOS_GREEN: Color = Color::Rgb(48, 209, 88);
const IOS_ORANGE: Color = Color::Rgb(255, 159, 10);
const IOS_PURPLE: Color = Color::Rgb(191, 90, 242);

// ─── codex terminal backdrop (TERM_COLORS in terminal.jsx) ─────────────────
const TERM_BG: Color = Color::Rgb(13, 17, 23);
const TERM_BG2: Color = Color::Rgb(10, 14, 19);
const TERM_BORDER: Color = Color::Rgb(31, 39, 49);
const TERM_BORDER_ACTIVE: Color = Color::Rgb(46, 160, 67);
const TERM_FG: Color = Color::Rgb(201, 209, 217);
const TERM_FG_MUTED: Color = Color::Rgb(125, 133, 144);
const TERM_FG_DIM: Color = Color::Rgb(72, 79, 88);
const TERM_GREEN: Color = Color::Rgb(86, 211, 100);
const TERM_BLUE: Color = Color::Rgb(88, 166, 255);
const TERM_ORANGE: Color = Color::Rgb(219, 137, 80);
const TERM_CREAM: Color = Color::Rgb(240, 216, 168);
const TERM_RED: Color = Color::Rgb(248, 81, 73);
const TERM_YELLOW: Color = Color::Rgb(210, 153, 34);

// Chip glyphs (verified single-cell wide against test-status-chips.sh).
const CHIP_LEFT_CAP: &str = "◖";
const CHIP_RIGHT_CAP: &str = "◗";
const CHIP_DOT: &str = "●";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    ContextMenu,
    Spotlight,
    ActionSheet,
    SessionSwitcher,
    /// Tab-triggered ⌘K-style jump grid: pick a tmux window inside the
    /// active codex-fleet session.
    SectionJump,
}

// Spotlight catalogue — all commands the palette filters over. Each row is
// matched against the user's query (case-insensitive substring on title + sub).
struct SpotlightItem {
    group: &'static str,
    icon: &'static str,
    title: &'static str,
    sub: &'static str,
    kbd: &'static str,
}

const SPOTLIGHT_ITEMS: &[SpotlightItem] = &[
    SpotlightItem { group: "PANE", icon: "⊟", title: "Horizontal split",   sub: "Split active pane top/bottom",       kbd: "h" },
    SpotlightItem { group: "PANE", icon: "⊞", title: "Vertical split",     sub: "Split active pane left/right",       kbd: "v" },
    SpotlightItem { group: "PANE", icon: "⤢", title: "Zoom pane",          sub: "Toggle full-screen for this pane",   kbd: "z" },
    SpotlightItem { group: "PANE", icon: "⇄", title: "Swap with marked pane", sub: "codex-ricsi-zazrifka ⇄ marked",   kbd: "s" },
    SpotlightItem { group: "SESSION · codex-admin-kollarrobert", icon: "⧉", title: "Copy whole session", sub: "180 lines · transcript",      kbd: "⇧C" },
    SpotlightItem { group: "SESSION · codex-admin-kollarrobert", icon: "☰", title: "Queue message",      sub: "Send to agent on next idle",  kbd: "↹" },
    SpotlightItem { group: "SESSION · codex-admin-kollarrobert", icon: "⌚", title: "Search history…",    sub: "Across all 7 panes",          kbd: "/" },
    SpotlightItem { group: "FLEET", icon: "+",  title: "Spawn new codex worker", sub: "codex-fleet · new agent",        kbd: "⌘N" },
    SpotlightItem { group: "FLEET", icon: "⎇", title: "Switch worktree…",       sub: "codex-fleet-extract-p1…",         kbd: "⌘B" },
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

#[derive(Clone, Copy, Debug)]
enum CardAction {
    Focus(usize),
    Queue(usize),
    Pause(usize),
    Kill(usize),
    NewWorker,
}

struct App {
    events: Vec<String>,
    chip_rect: Option<Rect>,
    overlay: Overlay,
    /// (rect, shortcut_char) for each rendered context-menu item — populated
    /// each frame in render_context_menu so the mouse handler can look up
    /// which row was clicked and dispatch the same tmux command as the
    /// keyboard shortcut.
    ctx_menu_items: Vec<(Rect, char)>,
    // Spotlight state — query the user is typing + which result row is
    // selected (0 = top hit, 1..N = grouped results).
    spotlight_query: String,
    spotlight_selected: usize,
    spotlight_tick: u64,
    // Session-switcher hit-testing. render_session_switcher rebuilds this
    // every frame; the mouse handler consults it on each MouseDown(Left)
    // and dispatches the matching CardAction into `last_action`, which the
    // footer flashes as visual feedback on the next frame.
    card_buttons: Vec<(Rect, CardAction)>,
    last_action: Option<String>,
    /// When the section-jump overlay opens, this is the tmux window the
    /// invoker says is currently focused — render_section_jump uses it to
    /// highlight the matching card in iOS-blue. None falls back to "the
    /// first card", matching the screenshot's default state.
    section_active: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            events: vec!["click the systemBlue chip — coords land here".into()],
            chip_rect: None,
            overlay: Overlay::None,
            ctx_menu_items: Vec::new(),
            spotlight_query: String::new(),
            spotlight_selected: 0,
            spotlight_tick: 0,
            card_buttons: Vec::new(),
            last_action: None,
            section_active: None,
        }
    }

    fn open_spotlight(&mut self) {
        self.overlay = Overlay::Spotlight;
        self.spotlight_query.clear();
        self.spotlight_selected = 0;
    }

    fn dispatch_card_click(&mut self, col: u16, row: u16) -> bool {
        // Walk in reverse so a button rendered on top wins ties (e.g. Kill
        // button overlapping a card edge).
        for (r, action) in self.card_buttons.iter().rev() {
            if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
                let line = match action {
                    CardAction::Focus(i) => format!("✓ Focus → pane {} (would: tmux select-pane -t codex-fleet:overview.{})", i, i),
                    CardAction::Queue(i) => format!("✓ Queue → pane {} (would: tmux send-keys after next idle)", i),
                    CardAction::Pause(i) => format!("✓ Pause → pane {} (would: SIGSTOP codex; SIGCONT on next click)", i),
                    CardAction::Kill(i)  => format!("✕ Kill  → pane {} (would: tmux kill-pane -t codex-fleet:overview.{})", i, i),
                    CardAction::NewWorker => "+ New worker (would: spawn a new pane via full-bringup --n+1)".to_string(),
                };
                self.last_action = Some(line);
                return true;
            }
        }
        false
    }

    fn record_mouse(&mut self, ev: MouseEvent) {
        let inside = self
            .chip_rect
            .map(|r| {
                ev.column >= r.x
                    && ev.column < r.x + r.width
                    && ev.row >= r.y
                    && ev.row < r.y + r.height
            })
            .unwrap_or(false);
        let tag = if inside { "✓ ON CHIP" } else { "off chip" };
        let line = format!("  ({}, {})  {:?}  {}", ev.column, ev.row, ev.kind, tag);
        self.events.push(line);
        if self.events.len() > 8 {
            self.events.remove(0);
        }
    }
}

// ────────────────────────── shared widget helpers ──────────────────────────

fn ios_chip(label: &str, bg: Color) -> Vec<Span<'static>> {
    let label_text = format!("  {}  ", label);
    vec![
        Span::styled(CHIP_LEFT_CAP, Style::default().fg(bg)),
        Span::styled(
            format!(" {} ", CHIP_DOT),
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label_text,
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(CHIP_RIGHT_CAP, Style::default().fg(bg)),
    ]
}

// Render a translucent-feeling card block: rounded border, hairline grey,
// solid dark-grey fill that reads as glass on top of the dim backdrop.
fn glass_block(title: Option<&str>, accent: Color, solid: bool) -> Block<'_> {
    let fill = if solid { IOS_BG_SOLID } else { IOS_BG_GLASS };
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
        .style(Style::default().bg(fill).fg(IOS_FG));
    if let Some(t) = title {
        b = b.title(Span::styled(
            format!(" {t} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    }
    b
}

// Right-aligned monospace shortcut chip ` X `.
fn shortcut_chip(s: &str) -> Span<'static> {
    Span::styled(
        format!(" {s} "),
        Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
    )
}

// Small status dot with a halo via reversed FG, single-cell.
fn status_dot(c: Color) -> Span<'static> {
    Span::styled("●", Style::default().fg(c))
}

// Centre a width×height rect inside `area`, clamped.
fn center_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

// ───────────────────── codex terminal backdrop (dim) ───────────────────────

struct PaneMock {
    agent: &'static str,
    accent: Color,
    active: bool,
    lines: &'static [(Color, &'static str)],
    footer: Option<(&'static str, &'static str)>,
}

fn render_term_topbar(frame: &mut ratatui::Frame, area: Rect) {
    let bg = Block::default().style(Style::default().bg(TERM_BG2));
    frame.render_widget(bg, area);

    let tabs: &[(&str, &str, Color, Color)] = &[
        ("◆", "codex-fleet", TERM_ORANGE, Color::Rgb(58, 42, 24)),
        ("0", "overview", Color::Rgb(157, 199, 255), Color::Rgb(15, 58, 114)),
        ("1", "fleet", TERM_FG_DIM, Color::Rgb(22, 27, 34)),
        ("2", "plan", TERM_FG_DIM, Color::Rgb(22, 27, 34)),
        ("3", "waves", TERM_FG_DIM, Color::Rgb(22, 27, 34)),
        ("4", "review", TERM_FG_DIM, Color::Rgb(22, 27, 34)),
        ("5", "watch>", TERM_FG_DIM, Color::Rgb(22, 27, 34)),
    ];

    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (idx, label, fg, bg) in tabs {
        spans.push(Span::styled(
            format!(" {idx} {label} "),
            Style::default().fg(*fg).bg(*bg).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        " ● live ",
        Style::default()
            .fg(TERM_GREEN)
            .bg(Color::Rgb(26, 42, 34))
            .add_modifier(Modifier::BOLD),
    ));

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);

    // right-side clock
    let clock_w = 10u16;
    if area.width > clock_w + 2 {
        let clock_rect = Rect {
            x: area.x + area.width - clock_w - 1,
            y: area.y,
            width: clock_w,
            height: 1,
        };
        let clock = Paragraph::new(Span::styled(
            "14:56:26",
            Style::default().fg(TERM_FG),
        ));
        frame.render_widget(clock, clock_rect);
    }
}

fn render_term_pane(frame: &mut ratatui::Frame, area: Rect, pane: &PaneMock) {
    if area.width < 6 || area.height < 3 {
        return;
    }
    let border_color = if pane.active { pane.accent } else { TERM_BORDER };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(TERM_BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Header: [agent-name]
    let header = Line::from(vec![
        Span::styled("[", Style::default().fg(TERM_FG_MUTED)),
        Span::styled(pane.agent, Style::default().fg(TERM_CREAM)),
        Span::styled("]", Style::default().fg(TERM_FG_MUTED)),
    ]);
    let header_rect = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(header), header_rect);

    // Body lines (truncated to available height)
    let body_h = inner.height.saturating_sub(2);
    let mut body_lines: Vec<Line> = Vec::new();
    for (color, text) in pane.lines.iter().take(body_h as usize) {
        body_lines.push(Line::from(Span::styled(*text, Style::default().fg(*color))));
    }
    let body_rect = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: body_h,
    };
    frame.render_widget(Paragraph::new(body_lines), body_rect);

    // Footer (last row of inner)
    if let Some((left, right)) = pane.footer {
        if inner.height >= 3 {
            let fy = inner.y + inner.height - 1;
            let fw = inner.width;
            let left_w = (fw / 2).min(left.chars().count() as u16);
            let left_rect = Rect { x: inner.x, y: fy, width: left_w, height: 1 };
            frame.render_widget(
                Paragraph::new(Span::styled(left, Style::default().fg(TERM_FG_MUTED))),
                left_rect,
            );
            let right_chars = right.chars().count() as u16;
            if fw > right_chars + 1 {
                let right_rect = Rect {
                    x: inner.x + fw - right_chars,
                    y: fy,
                    width: right_chars,
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(right, Style::default().fg(TERM_FG_MUTED))),
                    right_rect,
                );
            }
        }
    }
}

fn render_terminal_backdrop(frame: &mut ratatui::Frame, area: Rect) {
    // Solid wash
    frame.render_widget(
        Block::default().style(Style::default().bg(TERM_BG)),
        area,
    );

    // Top bar
    let topbar_h = 1u16;
    let topbar = Rect { x: area.x, y: area.y, width: area.width, height: topbar_h };
    render_term_topbar(frame, topbar);

    let body = Rect {
        x: area.x,
        y: area.y + topbar_h,
        width: area.width,
        height: area.height.saturating_sub(topbar_h),
    };
    if body.height < 6 {
        return;
    }

    // Two rows: main grid 65%, bottom strip 35%
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(body);

    // Top: 3 columns. Middle col has the active codex-admin-kollarrobert.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[0]);

    // Left column: matt-gg over fico-magnolia
    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cols[0]);
    render_term_pane(frame, left_split[0], &PANE_MATT);
    render_term_pane(frame, left_split[1], &PANE_FICO);

    // Middle: kollar (active)
    render_term_pane(frame, cols[1], &PANE_KOLLAR);

    // Right column: ricsi over magnolia
    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cols[2]);
    render_term_pane(frame, right_split[0], &PANE_RICSI);
    render_term_pane(frame, right_split[1], &PANE_MAGNOLIA);

    // Bottom strip: narrow admin + wide recodee
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(15), Constraint::Percentage(85)])
        .split(rows[1]);
    render_term_pane(frame, bottom[0], &PANE_ADMIN);
    render_term_pane(frame, bottom[1], &PANE_RECODEE);
}

// Canned pane content. Kept short — the backdrop is meant to read as
// "plausible codex sessions", not be transcript-accurate.
static PANE_MATT: PaneMock = PaneMock {
    agent: "codex-matt-gg",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_FG, "Patch now: source scripts/codex-fleet/"),
        (TERM_BLUE, "lib/_env.sh, replace duplicated defaults"),
        (TERM_FG, "with exported fleet variables, add -c"),
        (TERM_YELLOW, "\"$CODEX_FLEET_REPO_ROOT\" where tmux"),
        (TERM_FG, "starts worker panes."),
        (TERM_FG_MUTED, ""),
        (TERM_GREEN, "● Working (10m 28s · esc to interrupt)"),
        (TERM_FG_MUTED, "› OVERRIDE current plan pinning."),
    ],
    footer: Some(("gpt-5.5 xhigh · 37% left", "47% context")),
};

static PANE_FICO: PaneMock = PaneMock {
    agent: "codex-fico-magnolia",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_FG_MUTED, "  └ diff --git a/scripts/codex-fleet/"),
        (TERM_FG_MUTED, "  … +106 lines (ctrl + t to view)"),
        (TERM_RED, "● Ran git diff --no-index"),
        (TERM_GREEN, "● Ran git status --short"),
        (TERM_FG_MUTED, "  └ ?? openspec/changes/"),
        (TERM_GREEN, "● Working (10m 30s)"),
    ],
    footer: Some(("tab to queue", "47% context")),
};

static PANE_KOLLAR: PaneMock = PaneMock {
    agent: "codex-admin-kollarrobert",
    accent: TERM_BORDER_ACTIVE,
    active: true,
    lines: &[
        (TERM_FG, "Ran colony task ready --session"),
        (TERM_YELLOW, "  019e2685-a80f-7e72-8461-88d413c4d746"),
        (TERM_FG_MUTED, "  … +180 lines (ctrl + t to view)"),
        (TERM_FG_MUTED, ""),
        (TERM_GREEN, "● Working (10m 28s · esc to interrupt)"),
        (TERM_FG_MUTED, ""),
        (TERM_YELLOW, "⚠ Automatic approval review approved"),
        (TERM_FG, "(risk: low, authorization: high):"),
        (TERM_FG, "Sleeping 60 seconds is a non-destructive"),
        (TERM_GREEN, "✓ Auto-reviewer approved codex to run"),
        (TERM_FG, "sleep 60 this time"),
    ],
    footer: Some(("tab to queue", "54% context")),
};

static PANE_RICSI: PaneMock = PaneMock {
    agent: "codex-ricsi-zazrifka",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_YELLOW, "⚠ Automatic approval review approved"),
        (TERM_FG, "(risk: medium, authorization: high)"),
        (TERM_GREEN, "✓ Request approved for apply_patch"),
        (TERM_FG, "touching 3 files"),
        (TERM_GREEN, "● Working (10m 28s)"),
        (TERM_FG_MUTED, "› OVERRIDE current plan pinning."),
    ],
    footer: Some(("tab to queue", "49% context")),
};

static PANE_MAGNOLIA: PaneMock = PaneMock {
    agent: "codex-admin-magnolia",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_FG_MUTED, "… +37 lines (ctrl + t)"),
        (TERM_FG, "ub-1-full-bringu-2026-05-14-14-52/"),
        (TERM_GREEN, "● Ran git status --short"),
        (TERM_FG_MUTED, "  └ M openspec/plans/"),
        (TERM_GREEN, "● Working (10m 28s)"),
        (TERM_FG_MUTED, "› OVERRIDE current plan pinning."),
    ],
    footer: Some(("tab to queue", "46% context")),
};

static PANE_ADMIN: PaneMock = PaneMock {
    agent: "codex-admin-…",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_GREEN, "● Ran openspec"),
        (TERM_FG, "  validate"),
        (TERM_FG, "  --spe"),
        (TERM_FG_MUTED, "  … +35 l"),
        (TERM_GREEN, "✓ Auto-reviewe"),
        (TERM_FG, "  approved codex"),
    ],
    footer: None,
};

static PANE_RECODEE: PaneMock = PaneMock {
    agent: "codex-recodee-mite",
    accent: TERM_BORDER_ACTIVE,
    active: false,
    lines: &[
        (TERM_FG_MUTED, "83  +printf '%s\\n' \"$*\" >\"$CAP_PROBE_MARKER\""),
        (TERM_FG_MUTED, "84  +shift"),
        (TERM_FG_MUTED, "85  +printf '%s\\n' \"$1\""),
        (TERM_FG_MUTED, "86  +EOF"),
        (TERM_FG_MUTED, "87  +chmod +x \"$REPO/scripts/codex-fleet/cap-probe.sh\""),
        (TERM_FG_MUTED, "89  +rank_candidates() {"),
        (TERM_FG_MUTED, "90  +  printf '%s\\n' pool-a@example.com probe-a@example.com"),
        (TERM_FG_MUTED, "91  +}"),
        (TERM_FG, "cap-swap daemon warm-pool tests passed"),
    ],
    footer: Some(("", "[0/845]")),
};

// Soft dim overlay to focus the palette — single-pass tint by drawing a
// translucent-feeling block at low intensity over the backdrop.
fn dim_backdrop(frame: &mut ratatui::Frame, area: Rect) {
    // Approximates rgba(0,0,0,0.55) over TERM_BG by blending toward black.
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(2, 4, 7))),
        area,
    );
    // re-render backdrop at "dimmed" intensity by writing it again with a
    // muted FG style — kept as a wash so the palette stays readable.
}

// 3D-ish drop shadow for floating cards: paints a near-black band 1 row below
// (offset 2 cols right) and a 2-col strip down the right edge. Approximates an
// iOS card-shadow on top of the dimmed backdrop.
fn card_shadow(frame: &mut ratatui::Frame, card_rect: Rect, area: Rect) {
    let shadow = Color::Rgb(0, 0, 4);
    let by = card_rect.y + card_rect.height;
    if by < area.y + area.height {
        let bx = card_rect.x + 2;
        let aw_end = area.x + area.width;
        if bx < aw_end {
            let bw = card_rect.width.min(aw_end - bx);
            frame.render_widget(
                Block::default().style(Style::default().bg(shadow)),
                Rect { x: bx, y: by, width: bw, height: 1 },
            );
        }
    }
    let rx = card_rect.x + card_rect.width;
    let aw_end = area.x + area.width;
    if rx < aw_end {
        let rw = 2u16.min(aw_end - rx);
        let ah_end = area.y + area.height;
        let ry = card_rect.y + 1;
        if ry < ah_end {
            let rh = card_rect.height.saturating_sub(1).min(ah_end - ry);
            frame.render_widget(
                Block::default().style(Style::default().bg(shadow)),
                Rect { x: rx, y: ry, width: rw, height: rh },
            );
        }
    }
}

// ───────────────────────── 1 · iOS context menu ────────────────────────────

fn render_context_menu(frame: &mut ratatui::Frame, area: Rect) {
    let sections: &[&[(&str, &str, &str, bool)]] = &[
        &[
            ("⧉", "Copy whole session", "C", false),
            ("▤", "Copy visible", "c", false),
            ("≡", "Copy this line", "l", false),
        ],
        &[
            ("⌕", "Search history…", "/", false),
            ("↑", "Scroll to top", "<", false),
            ("↓", "Scroll to bottom", ">", false),
        ],
        &[
            ("⊟", "Horizontal split", "h", false),
            ("⊞", "Vertical split", "v", false),
            ("⤢", "Zoom pane", "z", false),
        ],
        &[
            ("↥", "Swap up", "u", false),
            ("↧", "Swap down", "d", false),
            ("⇄", "Swap with marked", "s", false),
            ("◆", "Mark pane", "m", false),
        ],
        &[
            ("↻", "Respawn pane", "R", false),
            ("✕", "Kill pane", "X", true),
        ],
    ];

    let menu_w: u16 = 48;
    let item_count: u16 = sections.iter().map(|s| s.len() as u16).sum();
    // 1 pad + 1 title + 1 hairline + items + (sections-1) section padding rows + 1 pad + 2 border
    let menu_h: u16 = 2 + 1 + item_count + (sections.len() as u16 - 1) + 1 + 2;

    let rect = center_rect(area, menu_w, menu_h);
    card_shadow(frame, rect, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(glass_block(None, IOS_TINT, false), rect);

    let inner = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };

    let mut y = inner.y + 1; // top padding

    // ── Title row: ● pane 1   %47                      ● LIVE ──────────
    let title_spans: Vec<Span> = vec![
        status_dot(IOS_ORANGE),
        Span::raw("  "),
        Span::styled(
            "pane 1",
            Style::default()
                .fg(IOS_FG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("%47", Style::default().fg(IOS_FG_MUTED)),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(title_spans)),
        Rect { x: inner.x, y, width: inner.width, height: 1 },
    );
    let live = " ● LIVE ";
    let live_w = live.chars().count() as u16;
    if inner.width > live_w {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                live,
                Style::default()
                    .fg(Color::Rgb(10, 36, 21))
                    .bg(IOS_GREEN)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x + inner.width - live_w,
                y,
                width: live_w,
                height: 1,
            },
        );
    }
    y += 1;

    // Hairline below title
    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(
            hairline.clone(),
            Style::default().fg(IOS_HAIRLINE),
        )),
        Rect { x: inner.x, y, width: inner.width, height: 1 },
    );
    y += 1;

    // ── Sections ─────────────────────────────────────────────────────────
    for (si, sec) in sections.iter().enumerate() {
        if si > 0 {
            // Hairline as section separator with a half-row of padding above
            frame.render_widget(
                Paragraph::new(Span::styled(
                    hairline.clone(),
                    Style::default().fg(IOS_HAIRLINE),
                )),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
            y += 1;
        }
        for (icon, label, sub, destructive) in sec.iter() {
            let fg = if *destructive { IOS_DESTRUCTIVE } else { IOS_FG };
            let icon_bg = if *destructive {
                Color::Rgb(58, 24, 24)
            } else {
                IOS_ICON_CHIP
            };
            let spans = vec![
                Span::styled(
                    format!(" {} ", icon),
                    Style::default().fg(fg).bg(icon_bg),
                ),
                Span::styled(
                    format!("  {}", label),
                    Style::default().fg(fg),
                ),
            ];
            let chip_w = 5u16;
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect {
                    x: inner.x,
                    y,
                    width: inner.width.saturating_sub(chip_w + 1),
                    height: 1,
                },
            );
            if inner.width > chip_w + 1 {
                frame.render_widget(
                    Paragraph::new(Line::from(shortcut_chip(sub))),
                    Rect {
                        x: inner.x + inner.width - chip_w,
                        y,
                        width: chip_w,
                        height: 1,
                    },
                );
            }
            y += 1;
        }
    }
}

// ─────────────────────────── 2 · iOS spotlight ─────────────────────────────

fn render_spotlight(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let filtered = spotlight_filter(&app.spotlight_query);
    let total = filtered.len();
    let selected = if total == 0 { 0 } else { app.spotlight_selected.min(total - 1) };

    let w: u16 = 78;
    let h: u16 = 42;
    let rect = center_rect(area, w, h);
    card_shadow(frame, rect, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(glass_block(None, IOS_TINT, true), rect);

    // Generous horizontal padding inside the palette — iOS surfaces don't
    // hug the edges. Vertical padding handled inline as we walk down y.
    let inner = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };

    let mut y = inner.y + 1; // top padding row

    // ── Search bar ────────────────────────────────────────────────────────
    // The caret blinks at ~2 Hz off the tick counter (120ms poll × 4 ≈ 500ms).
    let caret_on = (app.spotlight_tick / 4) % 2 == 0;
    let caret_char = if caret_on { "▏" } else { " " };
    let query_display = if app.spotlight_query.is_empty() {
        "type to filter…"
    } else {
        app.spotlight_query.as_str()
    };
    let query_style = if app.spotlight_query.is_empty() {
        Style::default().fg(IOS_FG_FAINT)
    } else {
        Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)
    };
    let q_spans: Vec<Span> = vec![
        Span::styled("⌕  ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled(query_display.to_string(), query_style),
        Span::styled(caret_char, Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD)),
    ];
    let cmdk = " ⌘ K ";
    let cmdk_w = cmdk.chars().count() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(q_spans)),
        Rect {
            x: inner.x,
            y,
            width: inner.width.saturating_sub(cmdk_w + 1),
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cmdk,
            Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
        ))),
        Rect { x: inner.x + inner.width - cmdk_w, y, width: cmdk_w, height: 1 },
    );
    y += 1;

    // Hairline under the search bar
    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(
            hairline.clone(),
            Style::default().fg(IOS_HAIRLINE),
        )),
        Rect { x: inner.x, y, width: inner.width, height: 1 },
    );
    y += 2; // blank breathing row

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
                y: y + 3,
                width: mw,
                height: 1,
            },
        );
        render_spotlight_footer(frame, inner);
        return;
    }

    // ── TOP HIT label ─────────────────────────────────────────────────────
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "TOP HIT",
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ))),
        Rect { x: inner.x, y, width: inner.width, height: 1 },
    );
    y += 1;

    // ── Top-hit pill (3 rows of systemBlue) — always filtered[0] ──────────
    let top = filtered[0];
    let hit_active = selected == 0;
    let hit_bg = if hit_active { IOS_TINT } else { Color::Rgb(8, 80, 180) };
    let hit_rect = Rect { x: inner.x, y, width: inner.width, height: 3 };
    frame.render_widget(
        Block::default().style(Style::default().bg(hit_bg)),
        hit_rect,
    );
    let icon_chip = Span::styled(
        format!(" {} ", top.icon),
        Style::default()
            .fg(Color::Rgb(255, 255, 255))
            .bg(IOS_TINT_DARK)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().bg(hit_bg)),
            icon_chip,
            Span::styled(
                format!("  {}", top.title),
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(hit_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect { x: hit_rect.x, y: hit_rect.y + 1, width: hit_rect.width, height: 1 },
    );
    let badge = format!(" tmux · {} ", top.kbd);
    let chev = "  › ";
    let badge_w = badge.chars().count() as u16;
    let chev_w = chev.chars().count() as u16;
    if hit_rect.width > badge_w + chev_w + 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                badge,
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(IOS_TINT_DARK)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: hit_rect.x + hit_rect.width - badge_w - chev_w,
                y: hit_rect.y + 1,
                width: badge_w,
                height: 1,
            },
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                chev,
                Style::default()
                    .fg(IOS_TINT_SUB)
                    .bg(hit_bg)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: hit_rect.x + hit_rect.width - chev_w,
                y: hit_rect.y + 1,
                width: chev_w,
                height: 1,
            },
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("      {}", top.sub),
            Style::default().fg(IOS_TINT_SUB).bg(hit_bg),
        ))),
        Rect { x: hit_rect.x, y: hit_rect.y + 2, width: hit_rect.width, height: 1 },
    );
    y += 4;

    // ── Remaining items (filtered[1..]), grouped, 2 rows each ────────────
    let bottom_guard = inner.y + inner.height - 2;
    let remaining: Vec<(usize, &SpotlightItem)> =
        filtered.iter().enumerate().skip(1).map(|(i, it)| (i, *it)).collect();

    let mut last_group: Option<&str> = None;
    for (gi, item) in remaining.iter() {
        if y + 3 > bottom_guard {
            break;
        }
        if last_group != Some(item.group) {
            if last_group.is_some() {
                y += 1;
                if y + 3 > bottom_guard { break; }
            }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {}", item.group),
                    Style::default()
                        .fg(IOS_FG_MUTED)
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
            y += 1;
            last_group = Some(item.group);
        }

        let selected_here = *gi == selected;
        let row_bg = if selected_here { IOS_TINT_DARK } else { IOS_CARD_BG };
        let title_fg = if selected_here {
            Color::Rgb(255, 255, 255)
        } else {
            IOS_FG
        };
        let sub_fg = if selected_here { IOS_TINT_SUB } else { IOS_FG_MUTED };

        let item_rect = Rect { x: inner.x, y, width: inner.width, height: 2 };
        frame.render_widget(
            Block::default().style(Style::default().bg(row_bg)),
            item_rect,
        );

        let row1 = Line::from(vec![
            Span::styled(" ", Style::default().bg(row_bg)),
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
        ]);
        let kbd = format!(" {} ", item.kbd);
        let kw = kbd.chars().count() as u16;
        frame.render_widget(
            Paragraph::new(row1),
            Rect {
                x: inner.x,
                y,
                width: inner.width.saturating_sub(kw + 2),
                height: 1,
            },
        );
        if inner.width > kw + 1 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    kbd,
                    Style::default()
                        .fg(title_fg)
                        .bg(IOS_ICON_CHIP)
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect {
                    x: inner.x + inner.width - kw - 1,
                    y,
                    width: kw,
                    height: 1,
                },
            );
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("       {}", item.sub),
                Style::default().fg(sub_fg).bg(row_bg),
            ))),
            Rect { x: inner.x, y: y + 1, width: inner.width, height: 1 },
        );

        y += 2;
    }

    render_spotlight_footer(frame, inner);
}

fn render_spotlight_footer(frame: &mut ratatui::Frame, inner: Rect) {
    let fy = inner.y + inner.height - 1;
    let footer = Line::from(vec![
        Span::styled("↵", Style::default().fg(IOS_FG)),
        Span::styled(" open    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("⌥↵", Style::default().fg(IOS_FG)),
        Span::styled(" all panes    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("esc", Style::default().fg(IOS_FG)),
        Span::styled(" cancel    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("✦", Style::default().fg(IOS_PURPLE)),
        Span::styled(" 7 codex panes", Style::default().fg(IOS_FG_MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect { x: inner.x, y: fy, width: inner.width, height: 1 },
    );
}

// ────────────────────────── 3 · iOS action sheet ───────────────────────────

fn render_action_sheet(frame: &mut ratatui::Frame, area: Rect) {
    struct Item(&'static str, &'static str, &'static str, Option<Color>, bool);
    let groups: &[(&str, Option<&str>, &[Item])] = &[
        (
            "Active pane · codex-admin-kollarrobert",
            Some("pane 1 · %47 · 54% context left"),
            &[
                Item("⧉", "Copy whole session", "⇧C", None, false),
                Item("⌕", "Search history", "/", None, false),
                Item("☰", "Queue message for next idle", "↹", None, false),
            ],
        ),
        (
            "Layout",
            None,
            &[
                Item("⊟", "Horizontal split", "h", None, false),
                Item("⊞", "Vertical split", "v", None, false),
                Item("⤢", "Zoom pane", "z", None, false),
                Item("⇄", "Swap with marked pane", "s", None, false),
            ],
        ),
        (
            "Worker",
            None,
            &[
                Item("↻", "Respawn worker", "R", Some(IOS_ORANGE), false),
                Item("✕", "Kill pane", "X", None, true),
            ],
        ),
    ];

    let card_w: u16 = 64;
    let item_count: u16 = groups.iter().map(|(_, _, items)| items.len() as u16).sum();
    let group_count: u16 = groups.len() as u16;
    let captioned: u16 = groups.iter().filter(|(_, c, _)| c.is_some()).count() as u16;
    let sep_count: u16 = group_count.saturating_sub(1);
    // per group: 1 title + maybe 1 caption + items, separators between groups,
    // plus one pad row top and bottom inside the rounded card.
    // Items render 2 rows tall (icon+title row + breathing row) for an iOS feel.
    let card_h: u16 = item_count * 2 + group_count + captioned + sep_count + 2 /*pad*/ + 2 /*borders*/;
    let cancel_h: u16 = 3;

    // Anchor near bottom: leave 1 row gap, card stacked over cancel button.
    let total_h = card_h + cancel_h + 1;
    let card_y = if area.height > total_h + 1 {
        area.y + area.height - total_h - 1
    } else {
        area.y
    };

    let card_rect = Rect {
        x: area.x + (area.width.saturating_sub(card_w)) / 2,
        y: card_y,
        width: card_w.min(area.width),
        height: card_h.min(area.height.saturating_sub(cancel_h + 1)),
    };

    card_shadow(frame, card_rect, area);
    frame.render_widget(Clear, card_rect);
    frame.render_widget(glass_block(None, IOS_TINT, true), card_rect);

    let inner = Rect {
        x: card_rect.x + 2,
        y: card_rect.y + 1,
        width: card_rect.width.saturating_sub(4),
        height: card_rect.height.saturating_sub(2),
    };

    let mut y = inner.y + 1; // top padding
    for (gi, (title, caption, items)) in groups.iter().enumerate() {
        if gi > 0 && y < inner.y + inner.height {
            let hairline = "─".repeat(inner.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    hairline,
                    Style::default().fg(IOS_HAIRLINE),
                )),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
            y += 1;
        }
        if y >= inner.y + inner.height { break; }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {title}"),
                Style::default()
                    .fg(IOS_FG)
                    .add_modifier(Modifier::BOLD),
            ))),
            Rect { x: inner.x, y, width: inner.width, height: 1 },
        );
        y += 1;

        if let Some(cap) = caption {
            if y >= inner.y + inner.height { break; }
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {cap}"),
                    Style::default().fg(IOS_FG_MUTED),
                ))),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
            y += 1;
        }

        for it in *items {
            if y + 1 >= inner.y + inner.height { break; }
            let fg = if it.4 {
                IOS_DESTRUCTIVE
            } else {
                it.3.unwrap_or(IOS_FG)
            };
            let icon_bg = if it.4 {
                Color::Rgb(58, 24, 24)
            } else if it.3.is_some() {
                Color::Rgb(58, 44, 24)
            } else {
                IOS_ICON_CHIP
            };

            // 2-row icon chip — paint a 3-wide × 2-tall block as the bg, then
            // render the glyph centered on row 1.
            frame.render_widget(
                Block::default().style(Style::default().bg(icon_bg)),
                Rect { x: inner.x + 1, y, width: 3, height: 2 },
            );
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {} ", it.0),
                    Style::default().fg(fg).bg(icon_bg).add_modifier(Modifier::BOLD),
                )),
                Rect { x: inner.x + 1, y, width: 3, height: 1 },
            );

            // Title on row 1, right of icon chip.
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    it.1,
                    Style::default()
                        .fg(fg)
                        .add_modifier(Modifier::BOLD),
                ))),
                Rect {
                    x: inner.x + 6,
                    y,
                    width: inner.width.saturating_sub(12),
                    height: 1,
                },
            );

            // Keyboard chip right-aligned on row 1.
            let kbd = format!(" {} ", it.2);
            let kw = kbd.chars().count() as u16;
            if inner.width > kw + 1 {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        kbd,
                        Style::default().fg(IOS_FG_FAINT).bg(IOS_CHIP_BG),
                    ))),
                    Rect {
                        x: inner.x + inner.width - kw - 1,
                        y,
                        width: kw,
                        height: 1,
                    },
                );
            }
            // Row 2 is the breathing row — left empty; icon chip bg already
            // painted there.
            y += 2;
        }
    }

    // Cancel button — iOS hallmark
    let cancel_rect = Rect {
        x: card_rect.x,
        y: card_rect.y + card_rect.height,
        width: card_rect.width,
        height: cancel_h.min(area.height.saturating_sub(card_rect.y + card_rect.height)),
    };
    if cancel_rect.height > 0 {
        card_shadow(frame, cancel_rect, area);
        frame.render_widget(Clear, cancel_rect);
        let cancel_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
            .style(Style::default().bg(Color::Rgb(58, 58, 60)));
        let cancel_inner = cancel_block.inner(cancel_rect);
        frame.render_widget(cancel_block, cancel_rect);
        let label = "Cancel";
        let lw = label.chars().count() as u16;
        if cancel_inner.width >= lw {
            let lx = cancel_inner.x + (cancel_inner.width - lw) / 2;
            frame.render_widget(
                Paragraph::new(Span::styled(
                    label,
                    Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
                )),
                Rect { x: lx, y: cancel_inner.y, width: lw, height: 1 },
            );
        }
    }
}

// ────────────────────── 4 · iOS session switcher ───────────────────────────

struct SessionCard {
    name: &'static str,
    pane: &'static str,
    dot: Color,
    status: &'static str,
    task: &'static str,
    ctx: &'static str,
    time: &'static str,
    model: &'static str,
    tint: Color,
    active: bool,
    badge: Option<&'static str>,
}

const SESSIONS: &[SessionCard] = &[
    SessionCard {
        name: "codex-admin-kollarrobert",
        pane: "1",
        dot: IOS_GREEN,
        status: "Working",
        task: "Run colony task ready --session 019e2685…",
        ctx: "54%",
        time: "10m 28s",
        model: "gpt-5.5 xhigh",
        tint: Color::Rgb(44, 94, 63),
        active: true,
        badge: Some("LIVE"),
    },
    SessionCard {
        name: "codex-matt-gg",
        pane: "0",
        dot: IOS_GREEN,
        status: "Working",
        task: "Patch lib/_env.sh — env helper exporting",
        ctx: "47%",
        time: "10m 28s",
        model: "gpt-5.5 xhigh",
        tint: Color::Rgb(58, 46, 34),
        active: false,
        badge: None,
    },
    SessionCard {
        name: "codex-ricsi-zazrifka",
        pane: "2",
        dot: IOS_ORANGE,
        status: "Approved",
        task: "apply_patch touching 3 files",
        ctx: "49%",
        time: "9m 18s",
        model: "gpt-5.5 high",
        tint: Color::Rgb(58, 46, 26),
        active: false,
        badge: Some("⚠ REVIEW"),
    },
    SessionCard {
        name: "codex-fico-magnolia",
        pane: "3",
        dot: IOS_GREEN,
        status: "Diffing",
        task: "git diff scripts/codex-fleet/probe-accounts.py",
        ctx: "47%",
        time: "10m 30s",
        model: "gpt-5.5 high",
        tint: Color::Rgb(31, 47, 58),
        active: false,
        badge: None,
    },
    SessionCard {
        name: "codex-admin-magnolia",
        pane: "4",
        dot: IOS_GREEN,
        status: "Working",
        task: "OVERRIDE current plan pinning. Claim sub…",
        ctx: "46%",
        time: "10m 28s",
        model: "gpt-5.5 high",
        tint: Color::Rgb(42, 35, 58),
        active: false,
        badge: None,
    },
    SessionCard {
        name: "codex-recodee-mite",
        pane: "5",
        dot: IOS_PURPLE,
        status: "Reviewing",
        task: "cap-swap daemon warm-pool tests passed",
        ctx: "—",
        time: "9m 17s",
        model: "gpt-5.5 xhigh",
        tint: Color::Rgb(31, 31, 47),
        active: false,
        badge: Some("845 LINES"),
    },
];

fn render_session_switcher(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    // Wipe last frame's hit-rects; every button registers fresh below.
    app.card_buttons.clear();
    // Full-area scrim
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(2, 4, 7))),
        area,
    );

    // Header
    let header_h: u16 = 4;
    let header_rect = Rect { x: area.x, y: area.y, width: area.width, height: header_h };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                " CODEX-FLEET · SESSION SWITCHER",
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled(
                    " 6 workers ",
                    Style::default()
                        .fg(IOS_FG)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "· 1 awaiting review",
                    Style::default().fg(IOS_FG_MUTED),
                ),
            ]),
        ]),
        header_rect,
    );

    // "New worker" pill — top-right, clickable.
    let pill = " + New worker ";
    let pill_w = pill.chars().count() as u16;
    if area.width > pill_w + 2 {
        let pill_rect = Rect {
            x: area.x + area.width - pill_w - 1,
            y: area.y + 1,
            width: pill_w,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                pill,
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_BG_GLASS)
                    .add_modifier(Modifier::BOLD),
            )),
            pill_rect,
        );
        app.card_buttons.push((pill_rect, CardAction::NewWorker));
    }

    // Footer hints + last-action flash (single row, then nav line).
    let footer_h: u16 = 2;
    let footer_y = area.y + area.height - footer_h;
    if let Some(msg) = &app.last_action {
        let truncated: String = msg.chars().take(area.width.saturating_sub(4) as usize).collect();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    truncated,
                    Style::default().fg(IOS_GREEN).add_modifier(Modifier::BOLD),
                ),
            ])),
            Rect { x: area.x, y: footer_y, width: area.width, height: 1 },
        );
    }
    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("← →", Style::default().fg(IOS_FG)),
        Span::styled(" navigate    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("↵", Style::default().fg(IOS_FG)),
        Span::styled(" focus    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("↑", Style::default().fg(IOS_FG)),
        Span::styled(" dismiss    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("⌘ N", Style::default().fg(IOS_FG)),
        Span::styled(" new worker    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("click", Style::default().fg(IOS_FG)),
        Span::styled(" buttons work →", Style::default().fg(IOS_FG_MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(footer),
        Rect { x: area.x, y: footer_y + 1, width: area.width, height: 1 },
    );

    // Card strip — cap height at ~70% of available vertical so cards read
    // as cards (artboard D shows ~78%; 70% leaves room for the action-feedback
    // flash row that sits between the strip and the nav footer). Center the
    // strip vertically inside the remaining gap.
    let strip_area_h = area.height.saturating_sub(header_h + footer_h);
    let strip_y_origin = area.y + header_h;
    if strip_area_h < 8 || area.width < 14 {
        return;
    }
    let strip_h = ((strip_area_h as u32 * 70 / 100) as u16).max(8);
    let strip_y = strip_y_origin + (strip_area_h.saturating_sub(strip_h) / 2);
    let card_w: u16 = 28;
    let gap: u16 = 1;
    let pad: u16 = 2;
    let max_cards = ((area.width.saturating_sub(pad * 2) + gap) / (card_w + gap)).max(1);
    let visible = (SESSIONS.len() as u16).min(max_cards) as usize;

    for (i, s) in SESSIONS.iter().take(visible).enumerate() {
        let x = area.x + pad + i as u16 * (card_w + gap);
        if x + card_w > area.x + area.width {
            break;
        }
        let rect = Rect { x, y: strip_y, width: card_w, height: strip_h };
        render_session_card(frame, rect, s, i, app);
    }
}

fn render_session_card(
    frame: &mut ratatui::Frame,
    rect: Rect,
    s: &SessionCard,
    card_index: usize,
    app: &mut App,
) {
    let border = if s.active { IOS_TINT } else { IOS_HAIRLINE_STRONG };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::default()
                .fg(border)
                .add_modifier(if s.active { Modifier::BOLD } else { Modifier::empty() }),
        )
        .style(Style::default().bg(s.tint));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // Header row: ● PANE x · STATUS                BADGE
    let header = Line::from(vec![
        status_dot(s.dot),
        Span::raw(" "),
        Span::styled(
            format!("PANE {} · {}", s.pane, s.status.to_uppercase()),
            Style::default()
                .fg(IOS_FG_MUTED)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header),
        Rect { x: inner.x, y: inner.y, width: inner.width.saturating_sub(11), height: 1 },
    );
    if let Some(badge) = s.badge {
        let bw = badge.chars().count() as u16 + 2;
        if inner.width > bw {
            // Colour the badge by its content: LIVE = green, ⚠ = orange,
            // anything else = chip-gray.
            let (fg, bg) = if badge == "LIVE" {
                (Color::Rgb(10, 36, 21), IOS_GREEN)
            } else if badge.starts_with('⚠') {
                (Color::Rgb(48, 28, 6), IOS_ORANGE)
            } else {
                (IOS_FG, IOS_CHIP_BG)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(" {badge} "),
                    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                )),
                Rect {
                    x: inner.x + inner.width - bw,
                    y: inner.y,
                    width: bw,
                    height: 1,
                },
            );
        }
    }

    // Name
    if inner.height < 3 { return; }
    let name = Line::from(Span::styled(
        s.name,
        Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(name),
        Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
    );

    // Hairline
    if inner.height < 5 { return; }
    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(hairline.clone(), Style::default().fg(IOS_HAIRLINE))),
        Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: 1 },
    );

    // Task (wrapped, up to 3 lines)
    let task_rect = Rect {
        x: inner.x,
        y: inner.y + 3,
        width: inner.width,
        height: inner.height.saturating_sub(8).min(3),
    };
    let task = Paragraph::new(Span::styled(s.task, Style::default().fg(IOS_FG)))
        .wrap(Wrap { trim: true });
    frame.render_widget(task, task_rect);

    // Footer rows: model / context / runtime
    let y_meta = task_rect.y + task_rect.height + 1;
    if y_meta + 3 < inner.y + inner.height {
        for (i, (k, v, accent)) in [
            ("model", s.model, IOS_FG),
            (
                "context",
                s.ctx,
                if s.ctx.starts_with('—') {
                    IOS_FG_MUTED
                } else if s
                    .ctx
                    .trim_end_matches('%')
                    .parse::<u32>()
                    .map(|n| n < 50)
                    .unwrap_or(false)
                {
                    IOS_ORANGE
                } else {
                    IOS_GREEN
                },
            ),
            ("runtime", s.time, IOS_FG),
        ]
        .iter()
        .enumerate()
        {
            let yy = y_meta + i as u16;
            if yy >= inner.y + inner.height - 2 {
                break;
            }
            let row = Line::from(vec![
                Span::styled(
                    format!("{:<8}", k.to_uppercase()),
                    Style::default().fg(IOS_FG_FAINT),
                ),
                Span::styled(*v, Style::default().fg(*accent)),
            ]);
            frame.render_widget(
                Paragraph::new(row),
                Rect { x: inner.x, y: yy, width: inner.width, height: 1 },
            );
        }
    }

    // Actions row at bottom. Each button gets a Rect that registers in
    // app.card_buttons so the mouse handler can resolve clicks to
    // CardAction::Focus / Queue / Pause / Kill (card_index = pane index in
    // the fleet). Geometry: Focus button takes the leftmost 9 cells (
    // " ❯ Focus "), then 3-cell Queue (" ☰ "), 3-cell Pause (" ‖ "),
    // remaining space, then 3-cell Kill (" ✕ ") right-aligned.
    let action_y = inner.y + inner.height - 1;
    if action_y > inner.y && inner.width > 14 {
        let focus_w: u16 = 9;
        let icon_w: u16 = 3;

        let focus_rect = Rect { x: inner.x, y: action_y, width: focus_w, height: 1 };
        frame.render_widget(
            Paragraph::new(Span::styled(
                " ❯ Focus ",
                Style::default()
                    .fg(if s.active { Color::Rgb(255, 255, 255) } else { IOS_FG })
                    .bg(if s.active { IOS_TINT } else { IOS_CHIP_BG })
                    .add_modifier(Modifier::BOLD),
            )),
            focus_rect,
        );
        app.card_buttons.push((focus_rect, CardAction::Focus(card_index)));

        let queue_rect = Rect { x: inner.x + focus_w + 1, y: action_y, width: icon_w, height: 1 };
        frame.render_widget(
            Paragraph::new(Span::styled(" ☰ ", Style::default().fg(IOS_FG).bg(IOS_CHIP_BG))),
            queue_rect,
        );
        app.card_buttons.push((queue_rect, CardAction::Queue(card_index)));

        let pause_rect = Rect { x: inner.x + focus_w + 1 + icon_w + 1, y: action_y, width: icon_w, height: 1 };
        frame.render_widget(
            Paragraph::new(Span::styled(" ‖ ", Style::default().fg(IOS_FG).bg(IOS_CHIP_BG))),
            pause_rect,
        );
        app.card_buttons.push((pause_rect, CardAction::Pause(card_index)));

        let kill_rect = Rect {
            x: inner.x + inner.width.saturating_sub(icon_w),
            y: action_y,
            width: icon_w,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                " ✕ ",
                Style::default().fg(IOS_DESTRUCTIVE).bg(Color::Rgb(58, 24, 24)),
            )),
            kill_rect,
        );
        app.card_buttons.push((kill_rect, CardAction::Kill(card_index)));
    }
}

// ────────────────────────── 5 · section-jump grid ──────────────────────────
// ⌘K-style window-jump overlay. Tab opens this; number keys 1–5 select a
// card and dispatch `tmux select-window -t <session>:<window>`; Esc / 0 / q
// dismiss. The card metadata mirrors the live codex-fleet tabs (Overview,
// Fleet, Plan, Waves, Review). The active section can be marked by the
// caller via `--active <name>` so the matching card renders in iOS-blue.

struct Section {
    /// Number key (1-5) that selects this card. Also drawn as the in-card
    /// badge in the upper-right.
    key: char,
    /// Display title.
    title: &'static str,
    /// One-line description shown under the title.
    sub: &'static str,
    /// Single-line footer ("7 workers", "12 tasks", "1 pending", …).
    footer: &'static str,
    /// tmux window name to target via `select-window -t <session>:<name>`.
    window: &'static str,
    /// Single-glyph icon drawn in the upper-left badge of the card.
    icon: &'static str,
}

const SECTIONS: &[Section] = &[
    Section {
        key: '1',
        title: "Overview",
        sub: "7 workers · 1 awaiting review",
        footer: "7 workers",
        window: "overview",
        icon: "▦",
    },
    Section {
        key: '2',
        title: "Fleet",
        sub: "Live worker panes & tmux layout",
        footer: "7 workers",
        window: "fleet",
        icon: "◫",
    },
    Section {
        key: '3',
        title: "Plan",
        sub: "codex-fleet-extract-p1-2026-05-14",
        footer: "12 tasks",
        window: "plan",
        icon: "≡",
    },
    Section {
        key: '4',
        title: "Waves",
        sub: "Spawn cycles & rebalancing",
        footer: "3 cycles",
        window: "waves",
        icon: "∿",
    },
    Section {
        key: '5',
        title: "Review",
        sub: "Approval queue · auto-reviewer log",
        footer: "1 pending",
        window: "review",
        icon: "✓",
    },
];

fn section_jump_tmux_args(key: char, session: &str) -> Option<Vec<String>> {
    let s = SECTIONS.iter().find(|s| s.key == key)?;
    Some(vec![
        "select-window".into(),
        "-t".into(),
        format!("{}:{}", session, s.window),
    ])
}

fn render_section_jump(frame: &mut ratatui::Frame, area: Rect, active_window: Option<&str>) {
    // 3 columns × 2 rows grid; the 6th cell sits empty. Card geometry
    // matches the screenshot reference (rounded corners via card_shadow +
    // glass_block, ~22-wide cards, 6-high cards).
    let card_w: u16 = 22;
    let card_h: u16 = 6;
    let gap: u16 = 1;
    let cols: u16 = 3;
    let rows: u16 = 2;

    let title_block_h: u16 = 3; // title row + sub row + hairline pad
    let footer_h: u16 = 1;
    let menu_w: u16 = 2 + (card_w * cols) + (gap * (cols - 1)) + 2;
    let menu_h: u16 = 2 + title_block_h + (card_h * rows) + (gap * (rows - 1)) + 1 + footer_h + 2;

    let rect = center_rect(area, menu_w, menu_h);
    card_shadow(frame, rect, area);
    frame.render_widget(Clear, rect);
    frame.render_widget(glass_block(None, IOS_TINT, false), rect);

    let inner = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };

    // ── Title row: "codex-fleet"  +  "Jump to section" (muted) ────────────
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "codex-fleet",
                Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Jump to section",
            Style::default().fg(IOS_FG_MUTED),
        ))),
        Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 },
    );

    // Hairline under the title block
    let hairline = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(
            hairline.clone(),
            Style::default().fg(IOS_HAIRLINE),
        )),
        Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: 1 },
    );

    // ── Card grid ─────────────────────────────────────────────────────────
    let grid_y0 = inner.y + 3 + 1; // +1 for breathing room under hairline
    for (idx, sec) in SECTIONS.iter().enumerate() {
        let col = (idx as u16) % cols;
        let row = (idx as u16) / cols;
        let cx = inner.x + col * (card_w + gap);
        let cy = grid_y0 + row * (card_h + gap);
        let cr = Rect { x: cx, y: cy, width: card_w, height: card_h };
        let is_active = active_window.map(|w| w == sec.window).unwrap_or(idx == 0);
        render_jump_card(frame, cr, sec, is_active);
    }

    // ── Footer: "1-5 jump   ↵ open   esc close              ● Live" ──────
    let footer_y = inner.y + inner.height.saturating_sub(1);
    let hints: Vec<Span> = vec![
        Span::styled("1–5", Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
        Span::styled(" jump   ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("↵", Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
        Span::styled(" open   ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("esc", Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)),
        Span::styled(" close", Style::default().fg(IOS_FG_MUTED)),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(hints)),
        Rect { x: inner.x, y: footer_y, width: inner.width.saturating_sub(8), height: 1 },
    );
    let live = " ● Live ";
    let live_w = live.chars().count() as u16;
    if inner.width > live_w {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                live,
                Style::default().fg(IOS_GREEN).add_modifier(Modifier::BOLD),
            ))),
            Rect {
                x: inner.x + inner.width - live_w,
                y: footer_y,
                width: live_w,
                height: 1,
            },
        );
    }
}

fn render_jump_card(frame: &mut ratatui::Frame, rect: Rect, sec: &Section, active: bool) {
    let (bg, fg, sub_fg, badge_bg, badge_fg) = if active {
        (IOS_TINT, IOS_FG, Color::Rgb(220, 232, 255), IOS_TINT_DARK, IOS_FG)
    } else {
        (IOS_CARD_BG, IOS_FG, IOS_FG_MUTED, IOS_ICON_CHIP, IOS_FG_MUTED)
    };
    let card = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(bg));
    frame.render_widget(card, rect);

    let inner = Rect {
        x: rect.x + 1,
        y: rect.y,
        width: rect.width.saturating_sub(2),
        height: rect.height,
    };

    // Top row: icon badge (left) + key badge (right).
    let icon_span = Span::styled(
        format!(" {} ", sec.icon),
        Style::default().fg(fg).bg(badge_bg).add_modifier(Modifier::BOLD),
    );
    frame.render_widget(
        Paragraph::new(Line::from(icon_span)),
        Rect { x: inner.x, y: inner.y, width: 3, height: 1 },
    );
    let key_span = Span::styled(
        format!(" {} ", sec.key),
        Style::default().fg(badge_fg).bg(badge_bg).add_modifier(Modifier::BOLD),
    );
    let key_w = 3u16;
    if inner.width > key_w {
        frame.render_widget(
            Paragraph::new(Line::from(key_span)),
            Rect {
                x: inner.x + inner.width - key_w,
                y: inner.y,
                width: key_w,
                height: 1,
            },
        );
    }

    // Title (bold) + subtitle on the middle rows.
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sec.title,
            Style::default().fg(fg).add_modifier(Modifier::BOLD),
        ))),
        Rect { x: inner.x, y: inner.y + 2, width: inner.width, height: 1 },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sec.sub,
            Style::default().fg(sub_fg),
        ))),
        Rect { x: inner.x, y: inner.y + 3, width: inner.width, height: 1 },
    );

    // Footer line at the bottom of the card.
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            sec.footer,
            Style::default().fg(sub_fg).add_modifier(Modifier::ITALIC),
        ))),
        Rect {
            x: inner.x,
            y: inner.y + rect.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        },
    );
}

// ─────────────────────────── validation harness ────────────────────────────
// Original Phase-0 POC view — chip on top, hint line, event log. Stays as
// the default (`0` / Esc) so the three risk checks still drive the binary.

fn render_validation_harness(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_FG_MUTED))
        .title(Span::styled(
            " ◆  fleet-tui-poc  (1·2·3·4 palettes  ·  q quit) ",
            Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));
    let inner = card.inner(area);
    frame.render_widget(card, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let chip_spans = ios_chip("working", IOS_TINT);
    let chip_width: u16 = chip_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();
    let chip_para = Paragraph::new(Line::from(chip_spans));
    let chip_rect = Rect {
        x: rows[0].x,
        y: rows[0].y,
        width: chip_width.min(rows[0].width),
        height: 1,
    };
    frame.render_widget(chip_para, chip_rect);
    app.chip_rect = Some(chip_rect);

    let hint = Paragraph::new(Line::from(Span::styled(
        "click the chip; coords appear below. expect ✓ ON CHIP when click lands inside.",
        Style::default().fg(IOS_FG_MUTED),
    )));
    frame.render_widget(hint, rows[1]);

    let log_lines: Vec<Line> = app
        .events
        .iter()
        .map(|e| Line::from(Span::styled(e.clone(), Style::default().fg(IOS_FG))))
        .collect();
    let log = Paragraph::new(log_lines);
    frame.render_widget(log, rows[2]);
}

// ─────────────────────────────── routing ───────────────────────────────────

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 4 || area.height < 4 {
        return;
    }

    match app.overlay {
        Overlay::None => {
            render_validation_harness(frame, area, app);
            // Clear chip rect mid-frame for non-default overlays — only the
            // harness wants mouse hit-testing.
        }
        Overlay::SessionSwitcher => {
            // Full-screen iOS surface; no dim of terminal backdrop because
            // the switcher *is* the surface (matches the JSX artboard D).
            render_session_switcher(frame, area, app);
            app.chip_rect = None;
        }
        Overlay::ContextMenu | Overlay::Spotlight | Overlay::ActionSheet | Overlay::SectionJump => {
            render_terminal_backdrop(frame, area);
            dim_backdrop(frame, area);
            match app.overlay {
                Overlay::ContextMenu => render_context_menu(frame, area),
                Overlay::Spotlight => render_spotlight(frame, area, app),
                Overlay::ActionSheet => render_action_sheet(frame, area),
                Overlay::SectionJump => {
                    render_section_jump(frame, area, app.section_active.as_deref());
                }
                _ => unreachable!(),
            }
            app.chip_rect = None;
        }
    }
}

fn run(
    initial: Overlay,
    single_shot: bool,
    pane_id: Option<String>,
    session: String,
    active_section: Option<String>,
) -> io::Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let mut app = App::new();
    if initial != Overlay::None {
        app.overlay = initial;
    }
    app.section_active = active_section;
    // Action dispatched on Enter / a shortcut key in single-shot mode. We run
    // the tmux subprocess *after* tearing down raw mode so the spawned cmd
    // sees a clean terminal state.
    let mut pending_tmux: Option<Vec<String>> = None;
    loop {
        terminal.draw(|frame| render(frame, &mut app))?;
        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                Event::Key(k) => {
                    if app.overlay == Overlay::Spotlight {
                        // Spotlight captures all printable keys + arrows. Esc
                        // dismisses; the harness's `q` / `0` / `1-4` switching
                        // is parked until the palette is closed.
                        match k.code {
                            KeyCode::Esc => {
                                app.overlay = Overlay::None;
                                app.spotlight_query.clear();
                                app.spotlight_selected = 0;
                            }
                            KeyCode::Char(c) => {
                                app.spotlight_query.push(c);
                                app.spotlight_selected = 0;
                            }
                            KeyCode::Backspace => {
                                app.spotlight_query.pop();
                                app.spotlight_selected = 0;
                            }
                            KeyCode::Up => {
                                app.spotlight_selected =
                                    app.spotlight_selected.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                let max =
                                    spotlight_filter(&app.spotlight_query)
                                        .len()
                                        .saturating_sub(1);
                                app.spotlight_selected =
                                    (app.spotlight_selected + 1).min(max);
                            }
                            KeyCode::Enter => {
                                // Visual feedback only — no command bus in the POC.
                            }
                            _ => {}
                        }
                    } else if app.overlay == Overlay::SectionJump {
                        // Tab-triggered window jumper. Number keys 1–5 dispatch
                        // `tmux select-window` then exit (single-shot) or close
                        // the overlay (preview mode). Esc / 0 / q / Tab close.
                        match k.code {
                            KeyCode::Esc
                            | KeyCode::Char('q')
                            | KeyCode::Char('0')
                            | KeyCode::Tab => {
                                if single_shot {
                                    break;
                                }
                                app.overlay = Overlay::None;
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() => {
                                if let Some(args) =
                                    section_jump_tmux_args(c, &session)
                                {
                                    pending_tmux = Some(args);
                                    break;
                                }
                            }
                            KeyCode::Enter => {
                                // Enter confirms the currently-highlighted card;
                                // falls back to section #1 when no caller-marked
                                // active window is known.
                                let key = app
                                    .section_active
                                    .as_deref()
                                    .and_then(|w| {
                                        SECTIONS
                                            .iter()
                                            .find(|s| s.window == w)
                                            .map(|s| s.key)
                                    })
                                    .unwrap_or('1');
                                if let Some(args) =
                                    section_jump_tmux_args(key, &session)
                                {
                                    pending_tmux = Some(args);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    } else if single_shot && app.overlay == Overlay::ContextMenu {
                        // Live-fleet mode: render the iOS context menu, dispatch
                        // a tmux command on the matching shortcut, then exit.
                        match k.code {
                            KeyCode::Esc | KeyCode::Char('q') => break,
                            KeyCode::Char(c) => {
                                if let Some(cmd) =
                                    context_menu_tmux_args(c, pane_id.as_deref())
                                {
                                    pending_tmux = Some(cmd);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match k.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Esc | KeyCode::Char('0') => {
                                if app.overlay == Overlay::None {
                                    break;
                                }
                                app.overlay = Overlay::None;
                            }
                            KeyCode::Tab => app.overlay = Overlay::SectionJump,
                            KeyCode::Char('1') => app.overlay = Overlay::ContextMenu,
                            KeyCode::Char('2') => app.open_spotlight(),
                            KeyCode::Char('3') => app.overlay = Overlay::ActionSheet,
                            KeyCode::Char('4') => app.overlay = Overlay::SessionSwitcher,
                            KeyCode::Char('5') => app.overlay = Overlay::SectionJump,
                            _ => {}
                        }
                    }
                }
                Event::Mouse(m) => {
                    if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                        match app.overlay {
                            Overlay::None => app.record_mouse(m),
                            Overlay::SessionSwitcher => {
                                app.dispatch_card_click(m.column, m.row);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        app.spotlight_tick = app.spotlight_tick.wrapping_add(1);
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    if let Some(args) = pending_tmux {
        // Best-effort — if tmux isn't on PATH we silently fall through (the
        // harness still works for the standalone preview).
        let _ = std::process::Command::new("tmux").args(&args).status();
    }
    Ok(())
}

// Maps a context-menu shortcut letter to its tmux argv. Mirrors the dispatch
// table in scripts/codex-fleet/bin/pane-context-menu.sh so swapping the
// binary into the display-popup is behavioural parity, not just visual.
fn context_menu_tmux_args(c: char, pane: Option<&str>) -> Option<Vec<String>> {
    let p = pane.unwrap_or("");
    let push_t = |args: &mut Vec<String>| {
        if !p.is_empty() {
            args.push("-t".into());
            args.push(p.into());
        }
    };
    let mut v: Vec<String> = match c {
        'h' => vec!["split-window".into(), "-h".into()],
        'v' => vec!["split-window".into(), "-v".into()],
        'z' => vec!["resize-pane".into(), "-Z".into()],
        'u' => vec!["swap-pane".into(), "-U".into()],
        'd' => vec!["swap-pane".into(), "-D".into()],
        's' => vec!["swap-pane".into()],
        'm' => vec!["select-pane".into(), "-m".into()],
        'R' => vec!["respawn-pane".into(), "-k".into()],
        'X' => vec!["kill-pane".into()],
        _ => return None,
    };
    push_t(&mut v);
    Some(v)
}

fn parse_overlay(name: &str) -> Overlay {
    match name {
        "context-menu" | "context_menu" | "ctx" => Overlay::ContextMenu,
        "spotlight" | "search" => Overlay::Spotlight,
        "action-sheet" | "action_sheet" | "sheet" => Overlay::ActionSheet,
        "session-switcher" | "switcher" => Overlay::SessionSwitcher,
        "section-jump" | "section_jump" | "jump" => Overlay::SectionJump,
        _ => Overlay::None,
    }
}

fn main() -> io::Result<()> {
    // Minimal CLI: --overlay <name>  --pane <id>  --session <name>  --active <window>
    let mut overlay = Overlay::None;
    let mut single_shot = false;
    let mut pane_id: Option<String> = None;
    let mut session: String = std::env::var("CODEX_FLEET_TMUX_SESSION")
        .unwrap_or_else(|_| "codex-fleet".to_string());
    let mut active_section: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if let Some(v) = a.strip_prefix("--overlay=") {
            overlay = parse_overlay(v);
            single_shot = overlay != Overlay::None;
        } else if a == "--overlay" {
            if let Some(v) = args.next() {
                overlay = parse_overlay(&v);
                single_shot = overlay != Overlay::None;
            }
        } else if let Some(v) = a.strip_prefix("--pane=") {
            pane_id = Some(v.to_string());
        } else if a == "--pane" {
            pane_id = args.next();
        } else if let Some(v) = a.strip_prefix("--session=") {
            session = v.to_string();
        } else if a == "--session" {
            if let Some(v) = args.next() {
                session = v;
            }
        } else if let Some(v) = a.strip_prefix("--active=") {
            active_section = Some(v.to_string());
        } else if a == "--active" {
            active_section = args.next();
        }
    }
    run(overlay, single_shot, pane_id, session, active_section)
}
