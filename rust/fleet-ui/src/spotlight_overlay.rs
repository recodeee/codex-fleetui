//! Search-first Spotlight overlay for the design-B artboard.
//!
//! The data model stays POC-shaped: a ranked catalogue plus interactive
//! state (`query`, `selected`, `tick`), but the chrome follows the darker
//! search palette from `images/B _ Spotlight _ search-first palette.html`.

use crate::{
    overlay::card_shadow, overlay::centered_overlay, palette::*, spotlight_filter::SpotlightFilter,
};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpotlightItem<'a> {
    pub group: &'a str,
    pub icon: &'a str,
    pub title: &'a str,
    pub sub: &'a str,
    pub kbd: &'a str,
}

impl<'a> SpotlightItem<'a> {
    pub fn new(group: &'a str, icon: &'a str, title: &'a str, sub: &'a str, kbd: &'a str) -> Self {
        Self {
            group,
            icon,
            title,
            sub,
            kbd,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotlightState {
    pub query: String,
    pub selected: usize,
    pub tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spotlight<'a> {
    pub items: Vec<SpotlightItem<'a>>,
}

impl<'a> Spotlight<'a> {
    pub const WIDTH: u16 = 78;
    pub const HEIGHT: u16 = 42;

    pub fn new(items: Vec<SpotlightItem<'a>>) -> Self {
        Self { items }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &SpotlightState) {
        let rect = centered_overlay(area, Self::WIDTH, Self::HEIGHT);
        card_shadow(frame, rect, area);
        frame.render_widget(Clear, rect);
        frame.render_widget(glass_block(), rect);

        let inner = Rect {
            x: rect.x + 2,
            y: rect.y + 1,
            width: rect.width.saturating_sub(4),
            height: rect.height.saturating_sub(2),
        };
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let (ranked, matched_any) = self.ranked_items(&state.query);
        let mut y = inner.y + 1;
        y = render_search_row(frame, inner, y, state);
        render_hairline(frame, inner, y);
        y += 2;

        if !matched_any {
            let msg = "no matches";
            let mw = text_width(msg);
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
            render_footer(frame, inner);
            return;
        }

        let selected = if ranked.is_empty() {
            0
        } else {
            state.selected.min(ranked.len() - 1)
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "TOP HIT",
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

        y = render_top_hit(frame, inner, y, ranked[0], selected == 0);

        let bottom_guard = inner.y + inner.height.saturating_sub(2);
        let mut last_group: Option<&str> = None;
        for (rank_index, item) in ranked.iter().enumerate().skip(1) {
            if y + 3 > bottom_guard {
                break;
            }
            if last_group != Some(item.group) {
                if last_group.is_some() {
                    y += 1;
                    if y + 3 > bottom_guard {
                        break;
                    }
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

            let row_bg = if rank_index == selected {
                IOS_TINT_DARK
            } else {
                IOS_CARD_BG
            };
            let title_fg = if rank_index == selected {
                Color::Rgb(255, 255, 255)
            } else {
                IOS_FG
            };
            let sub_fg = if rank_index == selected {
                IOS_TINT_SUB
            } else {
                IOS_FG_MUTED
            };

            let item_rect = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 2,
            };
            frame.render_widget(
                Block::default().style(Style::default().bg(row_bg)),
                item_rect,
            );

            let kbd = format!(" {} ", item.kbd);
            let kw = text_width(&kbd);
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
                Rect {
                    x: inner.x,
                    y: y + 1,
                    width: inner.width,
                    height: 1,
                },
            );

            y += 2;
        }

        render_footer(frame, inner);
    }

    fn ranked_items(&self, query: &str) -> (Vec<&SpotlightItem<'a>>, bool) {
        if query.is_empty() {
            return (self.items.iter().collect(), true);
        }
        let query_lc = query.to_lowercase();
        let keys: Vec<String> = self
            .items
            .iter()
            .map(|item| format!("{} {} {}", item.group, item.title, item.sub))
            .collect();
        let hits = SpotlightFilter::default().rank(query, &keys, self.items.len());
        if hits.is_empty() {
            return (Vec::new(), false);
        }
        let hit_scores: HashMap<usize, i64> =
            hits.into_iter().map(|hit| (hit.index, hit.score)).collect();
        let mut scored: Vec<(i64, usize, &SpotlightItem<'a>)> = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let mut score = hit_scores.get(&index).copied().unwrap_or(0);
                score += if item.group.starts_with("PANE") {
                    3_000
                } else if item.group.starts_with("SESSION") {
                    500
                } else if item.group.starts_with("FLEET") {
                    -500
                } else {
                    0
                };
                let title_lc = item.title.to_lowercase();
                let sub_lc = item.sub.to_lowercase();
                if title_lc.contains(&query_lc) {
                    score += 10_000;
                } else if sub_lc.contains(&query_lc) {
                    score += 5_000;
                    score -= (item.title.chars().count() as i64) * 10;
                }
                (score, index, item)
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        (scored.into_iter().map(|(_, _, item)| item).collect(), true)
    }
}

fn glass_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE_STRONG))
        .style(Style::default().bg(IOS_BG_GLASS).fg(IOS_FG))
}

fn render_search_row(frame: &mut Frame, inner: Rect, y: u16, state: &SpotlightState) -> u16 {
    let caret_on = (state.tick / 4) % 2 == 0;
    let caret_char = if caret_on { "▏" } else { " " };
    let query_display = if state.query.is_empty() {
        "type to filter…"
    } else {
        state.query.as_str()
    };
    let query_style = if state.query.is_empty() {
        Style::default().fg(IOS_FG_FAINT)
    } else {
        Style::default().fg(IOS_FG).add_modifier(Modifier::BOLD)
    };
    let query_spans = vec![
        Span::styled("⌕  ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled(query_display.to_string(), query_style),
        Span::styled(
            caret_char,
            Style::default().fg(IOS_TINT).add_modifier(Modifier::BOLD),
        ),
    ];
    let cmdk = " ⌘ K ";
    let cmdk_w = text_width(cmdk);
    frame.render_widget(
        Paragraph::new(Line::from(query_spans)),
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
        Rect {
            x: inner.x + inner.width - cmdk_w,
            y,
            width: cmdk_w,
            height: 1,
        },
    );
    y + 1
}

fn render_top_hit(
    frame: &mut Frame,
    inner: Rect,
    y: u16,
    item: &SpotlightItem<'_>,
    active: bool,
) -> u16 {
    let hit_bg = if active {
        IOS_TINT
    } else {
        Color::Rgb(8, 80, 180)
    };
    let hit_rect = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 3,
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(hit_bg)),
        hit_rect,
    );

    let icon_chip = Span::styled(
        format!(" {} ", item.icon),
        Style::default()
            .fg(IOS_FG)
            .bg(IOS_TINT_DARK)
            .add_modifier(Modifier::BOLD),
    );
    let badge = format!(" tmux · {} ", item.kbd);
    let badge_w = text_width(&badge);
    let chev = "  › ";
    let chev_w = text_width(chev);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ", Style::default().bg(hit_bg)),
            icon_chip,
            Span::styled(
                format!("  {}", item.title),
                Style::default()
                    .fg(IOS_FG)
                    .bg(hit_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        Rect {
            x: hit_rect.x,
            y: hit_rect.y + 1,
            width: hit_rect.width.saturating_sub(badge_w + chev_w + 1),
            height: 1,
        },
    );
    if hit_rect.width > badge_w + chev_w + 1 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                badge,
                Style::default()
                    .fg(IOS_FG)
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
            format!("      {}", item.sub),
            Style::default().fg(IOS_TINT_SUB).bg(hit_bg),
        ))),
        Rect {
            x: hit_rect.x,
            y: hit_rect.y + 2,
            width: hit_rect.width,
            height: 1,
        },
    );
    y + 3
}

fn render_footer(frame: &mut Frame, inner: Rect) {
    let fy = inner.y + inner.height - 1;
    let footer = Line::from(vec![
        Span::styled("↵", Style::default().fg(IOS_FG)),
        Span::styled(" open    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("⌥↵", Style::default().fg(IOS_FG)),
        Span::styled(" in all panes    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("esc", Style::default().fg(IOS_FG)),
        Span::styled(" cancel    ", Style::default().fg(IOS_FG_MUTED)),
        Span::styled("✦", Style::default().fg(IOS_PURPLE)),
        Span::styled(" 7 codex panes", Style::default().fg(IOS_FG_MUTED)),
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

fn render_hairline(frame: &mut Frame, inner: Rect, y: u16) {
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
}

fn text_width(s: &str) -> u16 {
    s.chars().count() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn poc_items() -> Vec<SpotlightItem<'static>> {
        vec![
            SpotlightItem::new(
                "PANE",
                "⊟",
                "Horizontal split",
                "Split active pane top/bottom",
                "h",
            ),
            SpotlightItem::new(
                "PANE",
                "⊞",
                "Vertical split",
                "Split active pane left/right",
                "v",
            ),
            SpotlightItem::new(
                "PANE",
                "⤢",
                "Zoom pane",
                "Toggle full-screen for this pane",
                "z",
            ),
            SpotlightItem::new(
                "PANE",
                "⇄",
                "Swap with marked pane",
                "codex-ricsi-zazrifka ⇄ marked",
                "s",
            ),
            SpotlightItem::new(
                "SESSION · codex-admin-kollarrobert",
                "⧉",
                "Copy whole session",
                "180 lines · transcript",
                "⇧C",
            ),
            SpotlightItem::new(
                "SESSION · codex-admin-kollarrobert",
                "☰",
                "Queue message",
                "Send to agent on next idle",
                "↹",
            ),
            SpotlightItem::new(
                "SESSION · codex-admin-kollarrobert",
                "⌚",
                "Search history…",
                "Across all 7 panes",
                "/",
            ),
            SpotlightItem::new(
                "FLEET",
                "+",
                "Spawn new codex worker",
                "codex-fleet · new agent",
                "Ctrl N",
            ),
            SpotlightItem::new(
                "FLEET",
                "⎇",
                "Switch worktree…",
                "codex-fleet-extract-p1…",
                "Ctrl B",
            ),
        ]
    }

    #[test]
    fn spotlight_default_render_design_b() {
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
        let spotlight = Spotlight::new(poc_items());
        let state = SpotlightState {
            query: "split".to_string(),
            selected: 0,
            tick: 0,
        };

        terminal
            .draw(|frame| spotlight.render(frame, frame.area(), &state))
            .unwrap();

        insta::assert_snapshot!(format!("{}", terminal.backend()));
    }
}
