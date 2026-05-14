// fleet-watcher — tuirealm port. Renders the watcher-board chrome (header
// banner + 4 stat cards + per-pane table placeholder) as a tuirealm
// `AppComponent`. Fifth binary in the codex-fleet ratatui → tuirealm
// migration after fleet-tab-strip (#50), fleet-state (#52),
// fleet-plan-tree (#53), fleet-waves (#54).

use std::io;
use std::process::Command;
use std::time::Duration;

use fleet_ui::{
    overlay::{card_shadow, centered_overlay, render_overlay},
    palette::*,
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MergeQueueItem {
    number: String,
    title: String,
    author: String,
    merge_state: String,
    review_decision: String,
    blocked_checks: String,
    url: String,
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

const REVIEW_BG: Color = Color::Rgb(11, 13, 18);
const REVIEW_PANEL: Color = Color::Rgb(31, 32, 36);
const REVIEW_PANEL_RAISED: Color = Color::Rgb(39, 40, 44);
const REVIEW_PANEL_MUTED: Color = Color::Rgb(45, 46, 50);
const REVIEW_BLUE: Color = Color::Rgb(22, 141, 255);
const REVIEW_GREEN_BG: Color = Color::Rgb(14, 61, 35);
const REVIEW_GREEN_FG: Color = Color::Rgb(50, 230, 109);
const REVIEW_YELLOW_BG: Color = Color::Rgb(84, 61, 18);
const REVIEW_YELLOW_FG: Color = Color::Rgb(255, 194, 85);
const REVIEW_RED_BG: Color = Color::Rgb(85, 34, 34);
const REVIEW_RED_FG: Color = Color::Rgb(255, 138, 130);
const REVIEW_GRAY_BORDER: Color = Color::Rgb(72, 74, 80);
const REVIEW_DARK_BORDER: Color = Color::Rgb(42, 44, 49);
const REVIEW_TEXT: Color = Color::Rgb(248, 248, 252);
const REVIEW_MUTED: Color = Color::Rgb(165, 166, 176);
const REVIEW_FAINT: Color = Color::Rgb(118, 119, 130);

fn fill(frame: &mut Frame, area: Rect, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Block::default().style(Style::default().bg(color)), area);
}

fn rounded_block(border: Color, bg: Color, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border).add_modifier(if active {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .style(Style::default().bg(bg))
}

fn text(frame: &mut Frame, area: Rect, line: Line<'static>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn fit(input: &str, width: u16) -> String {
    let w = width as usize;
    if w == 0 {
        return String::new();
    }
    let mut chars = input.chars();
    let mut out = String::new();
    for _ in 0..w {
        if let Some(c) = chars.next() {
            out.push(c);
        } else {
            return out;
        }
    }
    if chars.next().is_some() && w > 1 {
        out.pop();
        out.push('…');
    }
    out
}

fn pill_line(label: &'static str, fg: Color, bg: Color) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {label}  "),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    ))
}

fn render_pill(frame: &mut Frame, area: Rect, label: &'static str, fg: Color, bg: Color) {
    fill(frame, area, bg);
    text(frame, area, pill_line(label, fg, bg));
}

fn render_nav_pill(
    frame: &mut Frame,
    area: Rect,
    index: &'static str,
    icon: &'static str,
    label: &'static str,
    count: &'static str,
    active: bool,
) {
    let bg = if active { REVIEW_BLUE } else { REVIEW_PANEL };
    let border = if active {
        Color::Rgb(63, 166, 255)
    } else {
        REVIEW_DARK_BORDER
    };
    let block = rounded_block(border, bg, active);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let count_bg = if active {
        Color::Rgb(73, 164, 255)
    } else {
        IOS_CHIP_BG
    };
    text(
        frame,
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                format!(" {index} "),
                Style::default()
                    .fg(IOS_FG_MUTED)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                icon,
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {label} "),
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {count} "),
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(count_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    );
}

fn parse_merge_queue_tsv(tsv: &str) -> Vec<MergeQueueItem> {
    tsv.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(7, '\t');
            let item = MergeQueueItem {
                number: parts.next()?.trim().to_owned(),
                title: parts.next()?.trim().to_owned(),
                author: parts.next()?.trim().to_owned(),
                merge_state: parts.next()?.trim().to_owned(),
                review_decision: parts.next()?.trim().to_owned(),
                blocked_checks: parts.next()?.trim().to_owned(),
                url: parts.next()?.trim().to_owned(),
            };
            if item.number.is_empty() || !item.review_decision.eq_ignore_ascii_case("APPROVED") {
                None
            } else {
                Some(item)
            }
        })
        .collect()
}

fn load_merge_queue() -> (Vec<MergeQueueItem>, Option<String>) {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--search",
            "review:approved",
            "--limit",
            "6",
            "--json",
            "number,title,author,mergeStateStatus,reviewDecision,statusCheckRollup,url",
            "--jq",
            ".[] | [.number, .title, .author.login, (.mergeStateStatus // \"\"), (.reviewDecision // \"\"), (((.statusCheckRollup // []) | map(select((.conclusion // \"\") != \"SUCCESS\" and (.conclusion // \"\") != \"SKIPPED\")) | length) | tostring), .url] | @tsv",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            (parse_merge_queue_tsv(&stdout), None)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("gh pr list failed")
                .trim()
                .to_owned();
            (Vec::new(), Some(message))
        }
        Err(error) => (Vec::new(), Some(format!("gh unavailable: {error}"))),
    }
}

fn merge_queue_check_label(item: &MergeQueueItem) -> String {
    match item.blocked_checks.parse::<usize>() {
        Ok(0) => "CI green".to_owned(),
        Ok(1) => "1 check blocking".to_owned(),
        Ok(count) => format!("{count} checks blocking"),
        Err(_) => "CI unknown".to_owned(),
    }
}

fn merge_queue_state_label(item: &MergeQueueItem) -> String {
    if item.merge_state.is_empty() {
        "merge state pending".to_owned()
    } else {
        item.merge_state.replace('_', " ").to_lowercase()
    }
}

fn render_merge_queue(
    frame: &mut Frame,
    area: Rect,
    queue: &[MergeQueueItem],
    error: Option<&str>,
) {
    if area.width < 28 || area.height < 5 {
        return;
    }

    let block = rounded_block(IOS_HAIRLINE, IOS_BG_GLASS, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    fill(frame, inner, IOS_BG_GLASS);

    text(
        frame,
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                "MERGE QUEUE",
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_BG_GLASS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{:>width$}",
                    format!("{} approved open", queue.len()),
                    width = inner.width.saturating_sub(12) as usize
                ),
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
            ),
        ]),
    );

    let mut y = inner.y + 2;
    if let Some(error) = error {
        text(
            frame,
            Rect {
                x: inner.x + 1,
                y,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(vec![
                Span::styled("gh: ", Style::default().fg(IOS_TINT).bg(IOS_BG_GLASS)),
                Span::styled(
                    fit(error, inner.width.saturating_sub(6)),
                    Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
                ),
            ]),
        );
        return;
    }

    if queue.is_empty() {
        text(
            frame,
            Rect {
                x: inner.x + 1,
                y,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(Span::styled(
                "No approved open PRs from gh",
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
            )),
        );
        return;
    }

    for item in queue
        .iter()
        .take(((inner.height.saturating_sub(2)) / 2) as usize)
    {
        if y + 1 >= inner.y + inner.height {
            break;
        }
        let left_w = inner.width.saturating_sub(18);
        text(
            frame,
            Rect {
                x: inner.x + 1,
                y,
                width: left_w,
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    format!("#{} ", item.number),
                    Style::default()
                        .fg(IOS_TINT)
                        .bg(IOS_BG_GLASS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fit(&item.title, left_w.saturating_sub(6)),
                    Style::default().fg(IOS_FG).bg(IOS_BG_GLASS),
                ),
            ]),
        );
        text(
            frame,
            Rect {
                x: inner.x + inner.width.saturating_sub(16),
                y,
                width: 15,
                height: 1,
            },
            Line::from(Span::styled(
                "APPROVED",
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_TINT)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        text(
            frame,
            Rect {
                x: inner.x + 1,
                y: y + 1,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    format!("@{} · ", fit(&item.author, 16)),
                    Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
                ),
                Span::styled(
                    format!("{} · ", merge_queue_state_label(item)),
                    Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
                ),
                Span::styled(
                    merge_queue_check_label(item),
                    Style::default().fg(IOS_TINT).bg(IOS_BG_GLASS),
                ),
                Span::styled(" · ", Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS)),
                Span::styled(
                    fit(&item.url, 32),
                    Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_GLASS),
                ),
            ]),
        );
        y += 2;
    }
}

fn render_top_dock(frame: &mut Frame, area: Rect) {
    if area.width < 80 || area.height < 4 {
        return;
    }
    let dock = Rect {
        x: area.x + 3,
        y: area.y + 1,
        width: area.width.saturating_sub(6),
        height: 3,
    };
    let block = rounded_block(REVIEW_GRAY_BORDER, REVIEW_PANEL, false);
    let inner = block.inner(dock);
    frame.render_widget(block, dock);

    text(
        frame,
        Rect {
            x: inner.x + 2,
            y: inner.y,
            width: 24,
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                " ◆ ",
                Style::default()
                    .fg(Color::Rgb(255, 146, 28))
                    .bg(Color::Rgb(255, 105, 32)),
            ),
            Span::styled(
                " codex-fleet",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(REVIEW_PANEL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " 18:51:01",
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_PANEL),
            ),
        ]),
    );
    text(
        frame,
        Rect {
            x: inner.x + 28,
            y: inner.y,
            width: 1,
            height: 1,
        },
        Line::from(Span::styled(
            "│",
            Style::default().fg(REVIEW_DARK_BORDER).bg(REVIEW_PANEL),
        )),
    );

    let tabs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(23),
            Constraint::Length(19),
            Constraint::Length(19),
            Constraint::Length(20),
            Constraint::Length(23),
            Constraint::Length(11),
            Constraint::Min(0),
        ])
        .split(Rect {
            x: inner.x + 31,
            y: dock.y,
            width: inner.width.saturating_sub(31),
            height: dock.height,
        });
    render_nav_pill(frame, tabs[0], "0", "⌘", "Overview", "7", false);
    render_nav_pill(frame, tabs[1], "1", "↬", "Fleet", "7", false);
    render_nav_pill(frame, tabs[2], "2", "☑", "Plan", "12", false);
    render_nav_pill(frame, tabs[3], "3", "≋", "Waves", "3", false);
    render_nav_pill(frame, tabs[4], "4", "♢", "Review", "1", true);
    render_pill(frame, tabs[5], "● live", REVIEW_GREEN_FG, REVIEW_GREEN_BG);
}

fn render_subnav(frame: &mut Frame, area: Rect) {
    if area.width < 40 || area.height == 0 {
        return;
    }
    let nav = Rect {
        x: area.x + 4,
        y: area.y,
        width: area.width.saturating_sub(8),
        height: 3.min(area.height),
    };
    let block = rounded_block(REVIEW_DARK_BORDER, Color::Rgb(18, 20, 25), false);
    let inner = block.inner(nav);
    frame.render_widget(block, nav);
    text(
        frame,
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                "0 Watcher    ",
                Style::default().fg(REVIEW_FAINT).bg(Color::Rgb(18, 20, 25)),
            ),
            Span::styled(
                "1 Overview    ",
                Style::default().fg(REVIEW_FAINT).bg(Color::Rgb(18, 20, 25)),
            ),
            Span::styled(
                "2 Fleet    ",
                Style::default().fg(REVIEW_FAINT).bg(Color::Rgb(18, 20, 25)),
            ),
            Span::styled(
                "3 Plan    ",
                Style::default().fg(REVIEW_FAINT).bg(Color::Rgb(18, 20, 25)),
            ),
            Span::styled(
                " 4 Waves ",
                Style::default().fg(REVIEW_TEXT).bg(REVIEW_BLUE),
            ),
            Span::styled(
                "    REVIEW · 1 pending · auto-reviewer on",
                Style::default().fg(REVIEW_MUTED).bg(Color::Rgb(18, 20, 25)),
            ),
        ]),
    );
}

fn render_review_hero(frame: &mut Frame, area: Rect) {
    let left = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };
    text(
        frame,
        Rect {
            x: left.x,
            y: left.y,
            width: left.width,
            height: 1,
        },
        Line::from(Span::styled(
            "REVIEW",
            Style::default()
                .fg(REVIEW_MUTED)
                .add_modifier(Modifier::BOLD),
        )),
    );
    if left.height > 2 {
        text(
            frame,
            Rect {
                x: left.x,
                y: left.y + 1,
                width: left.width,
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    "1 awaiting",
                    Style::default()
                        .fg(REVIEW_TEXT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" · 124 approved", Style::default().fg(REVIEW_MUTED)),
            ]),
        );
    }
    if area.width > 110 && area.height > 2 {
        render_pill(
            frame,
            Rect {
                x: area.x + area.width - 36,
                y: area.y + 1,
                width: 23,
                height: 1,
            },
            "✦ Auto-reviewer on",
            REVIEW_GREEN_FG,
            REVIEW_GREEN_BG,
        );
        render_pill(
            frame,
            Rect {
                x: area.x + area.width - 12,
                y: area.y + 1,
                width: 10,
                height: 1,
            },
            "▣ Policy",
            REVIEW_TEXT,
            REVIEW_PANEL,
        );
    }
}

fn render_review_action(
    frame: &mut Frame,
    area: Rect,
    label: &'static str,
    icon: &'static str,
    fg: Color,
    bg: Color,
    border: Color,
    active: bool,
) {
    if area.width < 8 || area.height == 0 {
        return;
    }
    let block = rounded_block(border, bg, active);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{icon} "),
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                label,
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center),
        inner,
    );
}

fn render_review_card(
    frame: &mut Frame,
    area: Rect,
    merge_queue: &[MergeQueueItem],
    merge_queue_error: Option<&str>,
) {
    if area.width < 36 || area.height < 14 {
        return;
    }
    let block = rounded_block(REVIEW_BLUE, REVIEW_PANEL, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    fill(frame, inner, REVIEW_PANEL);

    let icon_rect = Rect {
        x: inner.x + 2,
        y: inner.y + 1,
        width: 7.min(inner.width),
        height: 3.min(inner.height),
    };
    let icon = rounded_block(REVIEW_YELLOW_FG, REVIEW_YELLOW_BG, false);
    let icon_inner = icon.inner(icon_rect);
    frame.render_widget(icon, icon_rect);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "⚡",
            Style::default()
                .fg(IOS_ORANGE)
                .bg(REVIEW_YELLOW_BG)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        icon_inner,
    );

    let meta_x = inner.x + 11;
    text(
        frame,
        Rect {
            x: meta_x,
            y: inner.y + 1,
            width: inner.width.saturating_sub(13),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                "REV-014",
                Style::default()
                    .fg(REVIEW_MUTED)
                    .bg(REVIEW_PANEL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  ●  9m 17s",
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_PANEL),
            ),
        ]),
    );
    text(
        frame,
        Rect {
            x: meta_x,
            y: inner.y + 2,
            width: inner.width.saturating_sub(13),
            height: 1,
        },
        Line::from(Span::styled(
            "apply_patch touching 3 files",
            Style::default()
                .fg(REVIEW_TEXT)
                .bg(REVIEW_PANEL)
                .add_modifier(Modifier::BOLD),
        )),
    );
    text(
        frame,
        Rect {
            x: meta_x,
            y: inner.y + 3,
            width: inner.width.saturating_sub(13),
            height: 1,
        },
        Line::from(Span::styled(
            "codex-ricsi-zazrifka · pane 4",
            Style::default().fg(REVIEW_MUTED).bg(REVIEW_PANEL),
        )),
    );

    if inner.width > 44 {
        let rx = inner.x + inner.width - 18;
        render_pill(
            frame,
            Rect {
                x: rx,
                y: inner.y + 1,
                width: 16,
                height: 1,
            },
            "● risk medium",
            REVIEW_YELLOW_FG,
            REVIEW_YELLOW_BG,
        );
        render_pill(
            frame,
            Rect {
                x: rx,
                y: inner.y + 3,
                width: 16,
                height: 1,
            },
            "● auth high",
            REVIEW_RED_FG,
            REVIEW_RED_BG,
        );
    }

    let rationale_y = inner.y + 6;
    let rationale_h = 4.min(inner.height.saturating_sub(10));
    if rationale_h > 0 {
        let rationale = Rect {
            x: inner.x + 2,
            y: rationale_y,
            width: inner.width.saturating_sub(4),
            height: rationale_h,
        };
        fill(frame, rationale, REVIEW_PANEL_RAISED);
        text(
            frame,
            Rect {
                x: rationale.x,
                y: rationale.y,
                width: 1,
                height: rationale.height,
            },
            Line::from(Span::styled(
                "┃",
                Style::default().fg(IOS_YELLOW).bg(REVIEW_PANEL_RAISED),
            )),
        );
        text(
            frame,
            Rect {
                x: rationale.x + 2,
                y: rationale.y,
                width: rationale.width.saturating_sub(4),
                height: 1,
            },
            Line::from(Span::styled(
                "AUTO-REVIEWER RATIONALE",
                Style::default()
                    .fg(REVIEW_MUTED)
                    .bg(REVIEW_PANEL_RAISED)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        frame.render_widget(
            Paragraph::new("Bounded local edits within the claimed task file scope on an isolated agent worktree, a reversible change explicitly authorized by the user's worker and repo workflow.")
                .style(Style::default().fg(REVIEW_TEXT).bg(REVIEW_PANEL_RAISED))
                .wrap(Wrap { trim: true }),
            Rect { x: rationale.x + 2, y: rationale.y + 1, width: rationale.width.saturating_sub(4), height: rationale.height.saturating_sub(1) },
        );
    }

    let files_y = rationale_y + rationale_h + 1;
    if files_y + 4 < inner.y + inner.height {
        text(
            frame,
            Rect {
                x: inner.x + 2,
                y: files_y,
                width: inner.width.saturating_sub(4),
                height: 1,
            },
            Line::from(Span::styled(
                "3 FILES TOUCHED",
                Style::default()
                    .fg(REVIEW_MUTED)
                    .bg(REVIEW_PANEL)
                    .add_modifier(Modifier::BOLD),
            )),
        );
        for (i, file) in [
            "↕ scripts/codex-fleet/lib/_env.sh",
            "↕ scripts/codex-fleet/down-kitty.sh",
            "↕ docs/cockpit.md",
        ]
        .iter()
        .enumerate()
        {
            let y = files_y + 1 + i as u16;
            let row = Rect {
                x: inner.x + 2,
                y,
                width: inner.width.saturating_sub(4),
                height: 1,
            };
            fill(frame, row, REVIEW_PANEL_RAISED);
            text(
                frame,
                Rect {
                    x: row.x + 1,
                    y,
                    width: row.width.saturating_sub(2),
                    height: 1,
                },
                Line::from(vec![
                    Span::styled(
                        "↕ ",
                        Style::default()
                            .fg(REVIEW_BLUE)
                            .bg(REVIEW_PANEL_RAISED)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        fit(file.trim_start_matches("↕ "), row.width.saturating_sub(4)),
                        Style::default().fg(REVIEW_TEXT).bg(REVIEW_PANEL_RAISED),
                    ),
                ]),
            );
        }
    }

    let button_y = inner.y + inner.height.saturating_sub(3);
    let queue_y = files_y + 5;
    if queue_y + 4 < button_y {
        render_merge_queue(
            frame,
            Rect {
                x: inner.x + 2,
                y: queue_y,
                width: inner.width.saturating_sub(4),
                height: button_y.saturating_sub(queue_y).saturating_sub(1),
            },
            merge_queue,
            merge_queue_error,
        );
    }

    if inner.height >= 5 {
        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(38),
                Constraint::Percentage(28),
            ])
            .split(Rect {
                x: inner.x + 2,
                y: button_y,
                width: inner.width.saturating_sub(4),
                height: 3,
            });
        render_review_action(
            frame,
            buttons[0],
            "Approve",
            "✓",
            REVIEW_TEXT,
            REVIEW_BLUE,
            Color::Rgb(63, 166, 255),
            true,
        );
        render_review_action(
            frame,
            buttons[1],
            "Request changes",
            "!",
            REVIEW_RED_FG,
            REVIEW_RED_BG,
            Color::Rgb(166, 58, 53),
            false,
        );
        render_review_action(
            frame,
            buttons[2],
            "Skip",
            "→",
            REVIEW_TEXT,
            REVIEW_PANEL_MUTED,
            REVIEW_GRAY_BORDER,
            false,
        );
    }
}

fn render_recent_decision_row(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    author: &'static str,
    age: &'static str,
    risk: &'static str,
    state: &'static str,
    state_fg: Color,
    state_bg: Color,
) {
    if area.width < 20 || area.height < 2 {
        return;
    }
    text(
        frame,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width.saturating_sub(15),
            height: 1,
        },
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(REVIEW_TEXT)
                .bg(REVIEW_PANEL)
                .add_modifier(Modifier::BOLD),
        )),
    );
    text(
        frame,
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width.saturating_sub(15),
            height: 1,
        },
        Line::from(vec![
            Span::styled(author, Style::default().fg(REVIEW_MUTED).bg(REVIEW_PANEL)),
            Span::styled(
                format!("  ● {age}  ● risk · {risk}"),
                Style::default().fg(REVIEW_FAINT).bg(REVIEW_PANEL),
            ),
        ]),
    );
    render_pill(
        frame,
        Rect {
            x: area.x + area.width.saturating_sub(13),
            y: area.y,
            width: 13,
            height: 1,
        },
        state,
        state_fg,
        state_bg,
    );
    if area.height > 2 {
        text(
            frame,
            Rect {
                x: area.x,
                y: area.y + 2,
                width: area.width,
                height: 1,
            },
            Line::from(Span::styled(
                "─".repeat(area.width as usize),
                Style::default().fg(REVIEW_DARK_BORDER).bg(REVIEW_PANEL),
            )),
        );
    }
}

fn render_recent_decisions(frame: &mut Frame, area: Rect) {
    if area.width < 28 || area.height < 12 {
        return;
    }
    let block = rounded_block(REVIEW_GRAY_BORDER, REVIEW_PANEL, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    fill(frame, inner, REVIEW_PANEL);
    text(
        frame,
        Rect {
            x: inner.x + 2,
            y: inner.y + 1,
            width: inner.width.saturating_sub(4),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                "Recent decisions",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(REVIEW_PANEL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{:>width$}",
                    "last 30m",
                    width = inner.width.saturating_sub(18) as usize
                ),
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_PANEL),
            ),
        ]),
    );
    let mut y = inner.y + 3;
    for (title, author, age, risk, state, fg, bg) in [
        (
            "sleep 60",
            "codex-admin-kollarrobert",
            "3m ago",
            "low",
            "● approved",
            REVIEW_GREEN_FG,
            REVIEW_GREEN_BG,
        ),
        (
            "openspec validate --spec",
            "codex-admin-magnolia",
            "7m ago",
            "low",
            "● approved",
            REVIEW_GREEN_FG,
            REVIEW_GREEN_BG,
        ),
        (
            "bash -lc 'ls scripts/'",
            "codex-matt-gg",
            "12m ago",
            "low",
            "● approved",
            REVIEW_GREEN_FG,
            REVIEW_GREEN_BG,
        ),
        (
            "git diff --no-index",
            "codex-fico-magnolia",
            "18m ago",
            "low",
            "● approved",
            REVIEW_GREEN_FG,
            REVIEW_GREEN_BG,
        ),
        (
            "rm -rf .cap-probe-cache",
            "codex-recodee-mite",
            "24m ago",
            "medium",
            "● escalated",
            IOS_YELLOW,
            REVIEW_YELLOW_BG,
        ),
        (
            "curl https://api.colony…",
            "codex-ricsi-zazrifka",
            "31m ago",
            "medium",
            "● denied",
            REVIEW_RED_FG,
            REVIEW_RED_BG,
        ),
    ] {
        if y + 2 >= inner.y + inner.height {
            break;
        }
        render_recent_decision_row(
            frame,
            Rect {
                x: inner.x + 2,
                y,
                width: inner.width.saturating_sub(4),
                height: 3,
            },
            title,
            author,
            age,
            risk,
            state,
            fg,
            bg,
        );
        y += 3;
    }
}

#[derive(Default)]
struct WatcherView {
    props: Props,
    overlay: Overlay,
    spotlight_query: String,
    spotlight_selected: usize,
    spotlight_tick: u64,
    merge_queue: Vec<MergeQueueItem>,
    merge_queue_error: Option<String>,
    merge_queue_loaded: bool,
}

impl WatcherView {
    fn ensure_merge_queue_loaded(&mut self) {
        if self.merge_queue_loaded {
            return;
        }
        let (queue, error) = load_merge_queue();
        self.merge_queue = queue;
        self.merge_queue_error = error;
        self.merge_queue_loaded = true;
    }

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

fn render_dashboard(
    frame: &mut Frame,
    area: Rect,
    merge_queue: &[MergeQueueItem],
    merge_queue_error: Option<&str>,
) {
    if area.width < 30 || area.height < 8 {
        return;
    }
    fill(frame, area, REVIEW_BG);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    render_top_dock(frame, rows[0]);
    render_subnav(frame, rows[1]);
    render_review_hero(frame, rows[2]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(Rect {
            x: area.x + 2,
            y: rows[3].y,
            width: area.width.saturating_sub(4),
            height: rows[3].height,
        });
    render_review_card(
        frame,
        Rect {
            x: content[0].x,
            y: content[0].y,
            width: content[0].width.saturating_sub(1),
            height: content[0].height,
        },
        merge_queue,
        merge_queue_error,
    );
    render_recent_decisions(
        frame,
        Rect {
            x: content[1].x + 1,
            y: content[1].y,
            width: content[1].width.saturating_sub(1),
            height: content[1].height,
        },
    );

    text(
        frame,
        Rect {
            x: area.x + 3,
            y: rows[4].y,
            width: area.width.saturating_sub(6),
            height: 1,
        },
        Line::from(vec![
            Span::styled(
                "A",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Approve   ",
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_BG),
            ),
            Span::styled(
                "R",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Request Changes   ",
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_BG),
            ),
            Span::styled(
                "S",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Skip   ", Style::default().fg(REVIEW_MUTED).bg(REVIEW_BG)),
            Span::styled(
                "D",
                Style::default()
                    .fg(REVIEW_TEXT)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " View diff",
                Style::default().fg(REVIEW_MUTED).bg(REVIEW_BG),
            ),
        ]),
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
        self.ensure_merge_queue_loaded();
        render_dashboard(
            frame,
            area,
            &self.merge_queue,
            self.merge_queue_error.as_deref(),
        );
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
    use tuirealm::ratatui::{backend::TestBackend, Terminal};

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

    #[test]
    fn parse_merge_queue_tsv_keeps_only_approved_open_prs() {
        let rows = "\
72\tPolish review queue\tcodex-one\tREADY_TO_MERGE\tAPPROVED\t0\thttps://example.test/72\n\
73\tNeeds another pass\tcodex-two\tBLOCKED\tCHANGES_REQUESTED\t2\thttps://example.test/73\n";

        let queue = parse_merge_queue_tsv(rows);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].number, "72");
        assert_eq!(queue[0].title, "Polish review queue");
        assert_eq!(queue[0].blocked_checks, "0");
    }

    #[test]
    fn review_queue_render_keeps_design_j_chrome() {
        let queue = [MergeQueueItem {
            number: "72".into(),
            title: "Polish review queue".into(),
            author: "codex-one".into(),
            merge_state: "READY_TO_MERGE".into(),
            review_decision: "APPROVED".into(),
            blocked_checks: "0".into(),
            url: "https://example.test/72".into(),
        }];
        let mut terminal = Terminal::new(TestBackend::new(140, 44)).unwrap();
        terminal
            .draw(|frame| render_dashboard(frame, frame.area(), &queue, None))
            .unwrap();
        let frame = format!("{}", terminal.backend());

        for needle in [
            "codex-fleet",
            "Review",
            "1 awaiting",
            "124 approved",
            "Auto-reviewer on",
            "apply_patch touching 3 files",
            "risk medium",
            "auth high",
            "AUTO-REVIEWER RATIONALE",
            "3 FILES TOUCHED",
            "Approve",
            "Request changes",
            "Skip",
            "MERGE QUEUE",
            "#72",
            "APPROVED",
            "ready to merge",
            "CI green",
            "Recent decisions",
            "approved",
            "escalated",
            "denied",
        ] {
            assert!(
                frame.contains(needle),
                "rendered review queue should contain {needle:?}\n{frame}"
            );
        }
    }
}
