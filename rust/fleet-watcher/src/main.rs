// fleet-watcher — live review approval queue.
//
// Data sources are intentionally best-effort: `gh pr list` drives counts,
// recent decisions, and merged-PR diff histograms; `colony task_messages
// kind=review` is sampled for reviewer events when the CLI exists. If either
// side is missing, the pane renders a deterministic fixture instead of
// panicking, so bare development shells still show the intended board.

use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

use fleet_ui::{
    chip::{status_chip, ChipKind},
    palette::*,
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, Props, QueryResult};
use tuirealm::ratatui::layout::{Constraint, Direction, Layout, Rect};
use tuirealm::ratatui::style::{Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tuirealm::ratatui::Frame;
use tuirealm::state::State;
use tuirealm::subscription::{EventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Msg {
    Tick,
    Quit,
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Id {
    Watcher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewOutcome {
    Pending,
    Approved,
    ChangesRequested,
    Merged,
}

impl ReviewOutcome {
    fn chip_kind(self) -> ChipKind {
        match self {
            Self::Pending => ChipKind::Working,
            Self::Approved => ChipKind::Done,
            Self::ChangesRequested => ChipKind::Blocked,
            Self::Merged => ChipKind::Working,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes",
            Self::Merged => "merged",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReviewStats {
    pending: usize,
    approved_today: usize,
    changes_requested: usize,
    merged_last_hour: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewEvent {
    hhmm: String,
    reviewer: String,
    title: String,
    outcome: ReviewOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReviewData {
    stats: ReviewStats,
    events: Vec<ReviewEvent>,
    diff_bins: Vec<i32>,
    source: String,
    degraded: bool,
}

impl ReviewData {
    fn demo(source: impl Into<String>) -> Self {
        Self {
            stats: ReviewStats {
                pending: 3,
                approved_today: 8,
                changes_requested: 1,
                merged_last_hour: 5,
            },
            events: vec![
                ReviewEvent {
                    hhmm: "13:00".into(),
                    reviewer: "auto-reviewer".into(),
                    title: "fleet-watcher live review board".into(),
                    outcome: ReviewOutcome::Approved,
                },
                ReviewEvent {
                    hhmm: "12:56".into(),
                    reviewer: "claude".into(),
                    title: "waves idle rows need lively state".into(),
                    outcome: ReviewOutcome::Pending,
                },
                ReviewEvent {
                    hhmm: "12:49".into(),
                    reviewer: "codex".into(),
                    title: "request tighter diff sparkline contrast".into(),
                    outcome: ReviewOutcome::ChangesRequested,
                },
                ReviewEvent {
                    hhmm: "12:43".into(),
                    reviewer: "merge-bot".into(),
                    title: "approved review queue chrome".into(),
                    outcome: ReviewOutcome::Merged,
                },
            ],
            diff_bins: demo_diff_bins(),
            source: source.into(),
            degraded: true,
        }
    }

    fn queue_clear(source: impl Into<String>) -> Self {
        Self {
            stats: ReviewStats::default(),
            events: Vec::new(),
            diff_bins: vec![0; 60],
            source: source.into(),
            degraded: false,
        }
    }
}

trait PrFeed {
    fn load(&mut self) -> ReviewData;
}

#[derive(Default)]
struct CommandPrFeed {
    cache: Option<(Instant, ReviewData)>,
}

impl PrFeed for CommandPrFeed {
    fn load(&mut self) -> ReviewData {
        if let Some((loaded_at, data)) = &self.cache {
            if loaded_at.elapsed() < Duration::from_secs(30) {
                return data.clone();
            }
        }

        let data = load_command_data()
            .unwrap_or_else(|| ReviewData::demo("demo fixture · gh/colony unavailable"));
        self.cache = Some((Instant::now(), data.clone()));
        data
    }
}

struct WatcherView<F: PrFeed> {
    props: Props,
    feed: F,
    data: ReviewData,
    tick: u64,
}

impl Default for WatcherView<CommandPrFeed> {
    fn default() -> Self {
        Self::with_feed(CommandPrFeed::default())
    }
}

impl<F: PrFeed> WatcherView<F> {
    fn with_feed(feed: F) -> Self {
        Self {
            props: Props::default(),
            feed,
            data: ReviewData::demo("loading live review feed"),
            tick: 0,
        }
    }

    fn refresh(&mut self) {
        self.data = self.feed.load();
    }
}

fn load_command_data() -> Option<ReviewData> {
    let today = command_stdout("date", &["-u", "+%Y-%m-%d"])?;
    let one_hour_ago = command_stdout("date", &["-u", "-d", "1 hour ago", "+%Y-%m-%dT%H:%M:%SZ"])
        .unwrap_or_else(|| today.clone());
    let thirty_min_ago = command_stdout(
        "date",
        &["-u", "-d", "30 minutes ago", "+%Y-%m-%dT%H:%M:%SZ"],
    )
    .unwrap_or_else(|| today.clone());

    let pending = gh_count("is:open is:pr review:pending")?;
    let approved_today = gh_count(&format!("is:pr review:approved updated:>={today}"))?;
    let changes_requested = gh_count("is:open is:pr review:changes_requested")?;
    let merged_last_hour = gh_count(&format!("is:merged is:pr merged:>={one_hour_ago}"))?;

    let mut events = gh_recent_events(&thirty_min_ago);
    events.extend(colony_review_events());
    events.sort_by(|a, b| b.hhmm.cmp(&a.hhmm));
    events.truncate(10);

    let diff_bins = gh_diff_bins(&one_hour_ago).unwrap_or_else(|| vec![0; 60]);
    let stats = ReviewStats {
        pending,
        approved_today,
        changes_requested,
        merged_last_hour,
    };

    if stats == ReviewStats::default() && events.is_empty() {
        Some(ReviewData::queue_clear("gh pr list · colony task_messages"))
    } else {
        Some(ReviewData {
            stats,
            events,
            diff_bins,
            source: "gh pr list · colony task_messages".into(),
            degraded: false,
        })
    }
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn gh_count(search: &str) -> Option<usize> {
    command_stdout(
        "gh",
        &[
            "pr", "list", "--state", "all", "--limit", "200", "--search", search, "--json",
            "number", "--jq", "length",
        ],
    )?
    .parse()
    .ok()
}

fn gh_recent_events(since: &str) -> Vec<ReviewEvent> {
    let jq =
        ".[] | [(.updatedAt // .mergedAt // \"\"), (.author.login // \"reviewer\"), .number, .title, (.reviewDecision // (if .mergedAt then \"MERGED\" else \"PENDING\" end))] | @tsv";
    let Some(out) = command_stdout(
        "gh",
        &[
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "20",
            "--search",
            &format!("is:pr updated:>={since}"),
            "--json",
            "number,title,author,reviewDecision,updatedAt,mergedAt",
            "--jq",
            jq,
        ],
    ) else {
        return Vec::new();
    };
    out.lines().filter_map(parse_gh_event).take(10).collect()
}

fn parse_gh_event(line: &str) -> Option<ReviewEvent> {
    let mut parts = line.splitn(5, '\t');
    let when = parts.next()?.trim();
    let reviewer = parts.next()?.trim();
    let number = parts.next()?.trim();
    let title = parts.next()?.trim();
    let outcome = parse_outcome(parts.next()?.trim());
    Some(ReviewEvent {
        hhmm: hhmm_from_timestamp(when),
        reviewer: reviewer.to_owned(),
        title: format!("#{number} {title}"),
        outcome,
    })
}

fn parse_outcome(raw: &str) -> ReviewOutcome {
    match raw.to_ascii_uppercase().as_str() {
        "APPROVED" => ReviewOutcome::Approved,
        "CHANGES_REQUESTED" => ReviewOutcome::ChangesRequested,
        "MERGED" => ReviewOutcome::Merged,
        _ => ReviewOutcome::Pending,
    }
}

fn colony_review_events() -> Vec<ReviewEvent> {
    let Some(out) = command_stdout(
        "colony",
        &["task_messages", "--kind", "review", "--limit", "10"],
    ) else {
        return Vec::new();
    };
    out.lines()
        .filter(|line| !line.trim().is_empty())
        .take(10)
        .map(|line| ReviewEvent {
            hhmm: clock_hhmm(),
            reviewer: "colony".into(),
            title: line.trim().to_owned(),
            outcome: ReviewOutcome::Pending,
        })
        .collect()
}

fn gh_diff_bins(since: &str) -> Option<Vec<i32>> {
    let jq = ".[] | [.mergedAt, (.additions // 0), (.deletions // 0)] | @tsv";
    let out = command_stdout(
        "gh",
        &[
            "pr",
            "list",
            "--state",
            "merged",
            "--limit",
            "100",
            "--search",
            &format!("is:merged is:pr merged:>={since}"),
            "--json",
            "mergedAt,additions,deletions",
            "--jq",
            jq,
        ],
    )?;
    let now = epoch_now()?;
    let mut bins = vec![0; 60];
    for line in out.lines() {
        let mut parts = line.splitn(3, '\t');
        let merged_at = parts.next().unwrap_or_default();
        let additions = parts
            .next()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        let deletions = parts
            .next()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        let Some(epoch) = epoch_for_timestamp(merged_at) else {
            continue;
        };
        let delta_minutes = ((now - epoch) / 60).clamp(0, 59) as usize;
        let idx = 59usize.saturating_sub(delta_minutes);
        bins[idx] += additions + deletions;
    }
    Some(bins)
}

fn epoch_now() -> Option<i64> {
    command_stdout("date", &["+%s"])?.parse().ok()
}

fn epoch_for_timestamp(ts: &str) -> Option<i64> {
    command_stdout("date", &["-u", "-d", ts, "+%s"])?
        .parse()
        .ok()
}

fn clock_hms() -> String {
    command_stdout("date", &["+%H:%M:%S"]).unwrap_or_else(|| "--:--:--".into())
}

fn clock_hhmm() -> String {
    command_stdout("date", &["+%H:%M"]).unwrap_or_else(|| "--:--".into())
}

fn hhmm_from_timestamp(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[11..16].to_owned()
    } else {
        clock_hhmm()
    }
}

fn demo_diff_bins() -> Vec<i32> {
    (0..60)
        .map(|i| {
            if i % 11 == 0 {
                96
            } else if i % 7 == 0 {
                48
            } else if i % 5 == 0 {
                20
            } else {
                0
            }
        })
        .collect()
}

fn fill(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(IOS_BG_SOLID)),
        area,
    );
}

fn card_block(title: Option<&str>) -> Block<'static> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(IOS_HAIRLINE))
        .style(Style::default().bg(IOS_CARD_BG));
    if let Some(title) = title {
        block = block.title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(IOS_FG)
                .bg(IOS_CARD_BG)
                .add_modifier(Modifier::BOLD),
        ));
    }
    block
}

fn render_line(frame: &mut Frame, area: Rect, line: Line<'static>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(IOS_CARD_BG)),
        area,
    );
}

fn clip(input: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= width as usize {
        return input.to_owned();
    }
    if width == 1 {
        return "…".into();
    }
    let mut out: String = chars
        .into_iter()
        .take(width.saturating_sub(1) as usize)
        .collect();
    out.push('…');
    out
}

fn right_text(text: &str, width: u16) -> String {
    format!("{:>width$}", clip(text, width), width = width as usize)
}

fn shimmer(tick: u64) -> &'static str {
    match tick % 4 {
        0 => "·",
        1 => "•",
        2 => "∙",
        _ => " ",
    }
}

fn render_header(frame: &mut Frame, area: Rect, data: &ReviewData) {
    if area.height == 0 {
        return;
    }
    let title = format!(
        "WATCHER · {} pending · auto-reviewer on",
        data.stats.pending
    );
    let clock = clock_hms();
    let right_w = clock.chars().count() as u16;
    let title_w = area.width.saturating_sub(right_w + 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                clip(&title, title_w),
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_BG_SOLID)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                right_text(&clock, area.width.saturating_sub(title_w)),
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_SOLID),
            ),
        ])),
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );

    if area.height > 1 {
        let state = if data.degraded {
            "fixture mode · gh/colony unavailable"
        } else if data.stats == ReviewStats::default() && data.events.is_empty() {
            "queue clear · auto-reviewer caught up"
        } else {
            "live queue · refreshed every 30s"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                state,
                Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_SOLID),
            )),
            Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            },
        );
    }
}

struct StatCard {
    title: &'static str,
    count: usize,
    kind: ChipKind,
}

fn visible_stats(data: &ReviewData, width: u16) -> Vec<StatCard> {
    let mut cards = vec![
        StatCard {
            title: "PENDING",
            count: data.stats.pending,
            kind: ChipKind::Working,
        },
        StatCard {
            title: "APPROVED-TODAY",
            count: data.stats.approved_today,
            kind: ChipKind::Done,
        },
        StatCard {
            title: "CHANGES-REQUESTED",
            count: data.stats.changes_requested,
            kind: ChipKind::Idle,
        },
        StatCard {
            title: "MERGED-LAST-1H",
            count: data.stats.merged_last_hour,
            kind: ChipKind::Working,
        },
    ];
    let min_card_width = 23;
    if width < (cards.len() as u16 * min_card_width) {
        cards.retain(|card| card.count > 0);
        if cards.is_empty() {
            cards.push(StatCard {
                title: "PENDING",
                count: 0,
                kind: ChipKind::Working,
            });
        }
    }
    cards
}

fn render_stats(frame: &mut Frame, area: Rect, data: &ReviewData, tick: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let cards = visible_stats(data, area.width);
    let constraints = vec![Constraint::Ratio(1, cards.len() as u32); cards.len()];
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (card, chunk) in cards.iter().zip(chunks.iter()) {
        let area = Rect {
            x: chunk.x,
            y: chunk.y,
            width: chunk.width.saturating_sub(1),
            height: chunk.height,
        };
        render_stat_card(frame, area, card, tick);
    }
}

fn render_stat_card(frame: &mut Frame, area: Rect, card: &StatCard, tick: u64) {
    if area.width < 6 || area.height < 3 {
        return;
    }
    let block = card_block(None);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_line(
        frame,
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(2),
            height: 1,
        },
        Line::from(Span::styled(
            clip(card.title, inner.width.saturating_sub(2)),
            Style::default()
                .fg(IOS_FG_MUTED)
                .bg(IOS_CARD_BG)
                .add_modifier(Modifier::BOLD),
        )),
    );
    if inner.height > 1 {
        render_line(
            frame,
            Rect {
                x: inner.x + 1,
                y: inner.y + 1,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(vec![
                Span::styled(
                    format!("{:>3}", card.count),
                    Style::default()
                        .fg(if card.count == 0 {
                            IOS_FG_FAINT
                        } else {
                            IOS_FG
                        })
                        .bg(IOS_CARD_BG)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default().bg(IOS_CARD_BG)),
                Span::styled(
                    shimmer(tick),
                    Style::default().fg(IOS_HAIRLINE).bg(IOS_CARD_BG),
                ),
            ]),
        );
    }
    if inner.height > 2 {
        let mut spans = status_chip(card.kind);
        spans.insert(0, Span::styled(" ", Style::default().bg(IOS_CARD_BG)));
        render_line(
            frame,
            Rect {
                x: inner.x,
                y: inner.y + 2,
                width: inner.width,
                height: 1,
            },
            Line::from(spans),
        );
    }
}

fn render_feed(frame: &mut Frame, area: Rect, data: &ReviewData, tick: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = card_block(Some("RECENT DECISIONS · last 30m"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.events.is_empty() {
        let y = inner.y + inner.height.saturating_sub(1) / 2;
        render_line(
            frame,
            Rect {
                x: inner.x + 2,
                y,
                width: inner.width.saturating_sub(4),
                height: 1,
            },
            Line::from(vec![
                Span::styled(shimmer(tick), Style::default().fg(IOS_TINT).bg(IOS_CARD_BG)),
                Span::styled(
                    " queue clear · auto-reviewer caught up",
                    Style::default()
                        .fg(IOS_FG)
                        .bg(IOS_CARD_BG)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        );
        return;
    }

    let max_rows = inner.height.saturating_sub(1) as usize;
    for (idx, event) in data.events.iter().take(max_rows).enumerate() {
        let y = inner.y + idx as u16;
        let title_budget = inner.width.saturating_sub(38);
        let mut spans = vec![
            Span::styled(
                format!("{} · ", event.hhmm),
                Style::default().fg(IOS_FG_MUTED).bg(IOS_CARD_BG),
            ),
            Span::styled(
                format!(" @{} ", clip(&event.reviewer, 14)),
                Style::default()
                    .fg(IOS_FG)
                    .bg(IOS_CHIP_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default().bg(IOS_CARD_BG)),
            Span::styled(
                clip(&event.title, title_budget),
                Style::default().fg(IOS_FG).bg(IOS_CARD_BG),
            ),
            Span::styled(" ", Style::default().bg(IOS_CARD_BG)),
        ];
        spans.extend(status_chip(event.outcome.chip_kind()));
        spans.push(Span::styled(
            format!(" {}", event.outcome.label()),
            Style::default().fg(IOS_FG_MUTED).bg(IOS_CARD_BG),
        ));
        render_line(
            frame,
            Rect {
                x: inner.x + 1,
                y,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            Line::from(spans),
        );
    }
}

fn render_diff_sparkline(frame: &mut Frame, area: Rect, data: &ReviewData, tick: u64) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = card_block(Some("DIFF SPARKLINE · merged PRs · last 60m"));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let bars = sparkline(&data.diff_bins, inner.width.saturating_sub(4));
    render_line(
        frame,
        Rect {
            x: inner.x + 2,
            y: inner.y,
            width: inner.width.saturating_sub(4),
            height: 1,
        },
        Line::from(Span::styled(
            bars,
            Style::default()
                .fg(if data.diff_bins.iter().any(|v| *v > 0) {
                    IOS_TINT
                } else {
                    IOS_FG_FAINT
                })
                .bg(IOS_CARD_BG)
                .add_modifier(Modifier::BOLD),
        )),
    );
    if inner.height > 1 {
        let total: i32 = data.diff_bins.iter().sum();
        let label = if total == 0 {
            format!("{} no merged diff activity yet", shimmer(tick))
        } else {
            format!("+/- {total} lines merged in the last hour")
        };
        render_line(
            frame,
            Rect {
                x: inner.x + 2,
                y: inner.y + 1,
                width: inner.width.saturating_sub(4),
                height: 1,
            },
            Line::from(Span::styled(
                clip(&label, inner.width.saturating_sub(4)),
                Style::default().fg(IOS_FG_MUTED).bg(IOS_CARD_BG),
            )),
        );
    }
}

fn sparkline(values: &[i32], width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let chars = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let mut sampled = Vec::new();
    for x in 0..width as usize {
        let idx = x * values.len().max(1) / width as usize;
        let value = values.get(idx).copied().unwrap_or(0);
        let level = if value <= 0 {
            0
        } else {
            ((value as f32 / max as f32) * 7.0).ceil() as usize
        };
        sampled.push(chars[level.min(7)]);
    }
    sampled.join("")
}

fn render_footer(frame: &mut Frame, area: Rect, data: &ReviewData) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let left = " q quit · / filter soon · 2s tick ";
    let source = format!("source: {}", data.source);
    let source_w = area.width.saturating_sub(left.chars().count() as u16);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, Style::default().fg(IOS_FG_MUTED).bg(IOS_BG_SOLID)),
            Span::styled(
                right_text(&source, source_w),
                Style::default().fg(IOS_FG_FAINT).bg(IOS_BG_SOLID),
            ),
        ])),
        area,
    );
}

fn render_dashboard(frame: &mut Frame, area: Rect, data: &ReviewData, tick: u64) {
    if area.width < 30 || area.height < 12 {
        fill(frame, area);
        return;
    }
    fill(frame, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, rows[0], data);
    render_stats(frame, rows[1], data, tick);

    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(rows[2]);
    render_feed(frame, content[0], data, tick);
    render_diff_sparkline(frame, content[1], data, tick);
    render_footer(frame, rows[3], data);
}

impl<F: PrFeed> Component for WatcherView<F> {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.refresh();
        render_dashboard(frame, area, &self.data, self.tick);
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

impl<F: PrFeed + 'static> AppComponent<Msg, NoUserEvent> for WatcherView<F> {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::Quit),
            Event::Tick => {
                self.tick = self.tick.wrapping_add(1);
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
                .tick_interval(Duration::from_millis(2_000)),
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
            Msg::Tick => {}
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

    #[derive(Clone)]
    struct StubPrFeed {
        data: ReviewData,
    }

    impl StubPrFeed {
        fn fixture() -> Self {
            Self {
                data: ReviewData {
                    stats: ReviewStats {
                        pending: 2,
                        approved_today: 4,
                        changes_requested: 0,
                        merged_last_hour: 3,
                    },
                    events: vec![
                        ReviewEvent {
                            hhmm: "12:59".into(),
                            reviewer: "auto-reviewer".into(),
                            title: "#91 Replace watcher placeholder".into(),
                            outcome: ReviewOutcome::Approved,
                        },
                        ReviewEvent {
                            hhmm: "12:54".into(),
                            reviewer: "codex".into(),
                            title: "#90 Idle rows need motion".into(),
                            outcome: ReviewOutcome::Pending,
                        },
                    ],
                    diff_bins: demo_diff_bins(),
                    source: "stub snapshot feed".into(),
                    degraded: false,
                },
            }
        }
    }

    impl PrFeed for StubPrFeed {
        fn load(&mut self) -> ReviewData {
            self.data.clone()
        }
    }

    #[test]
    fn parse_gh_event_maps_review_decision() {
        let event = parse_gh_event(
            "2026-05-15T10:42:03Z\treview-bot\t91\tReplace watcher placeholder\tAPPROVED",
        )
        .unwrap();
        assert_eq!(event.hhmm, "10:42");
        assert_eq!(event.reviewer, "review-bot");
        assert_eq!(event.title, "#91 Replace watcher placeholder");
        assert_eq!(event.outcome, ReviewOutcome::Approved);
    }

    #[test]
    fn zero_cards_hide_only_when_row_would_overflow() {
        let data = ReviewData::queue_clear("test");
        assert_eq!(visible_stats(&data, 120).len(), 4);
        assert_eq!(visible_stats(&data, 40).len(), 1);
    }

    #[test]
    fn stub_feed_renders_live_board_at_sixty_rows() {
        let mut view = WatcherView::with_feed(StubPrFeed::fixture());
        let mut terminal = Terminal::new(TestBackend::new(120, 60)).unwrap();
        terminal
            .draw(|frame| view.view(frame, frame.area()))
            .unwrap();
        let frame = format!("{}", terminal.backend());

        for needle in [
            "WATCHER · 2 pending · auto-reviewer on",
            "PENDING",
            "APPROVED-TODAY",
            "CHANGES-REQUESTED",
            "MERGED-LAST-1H",
            "RECENT DECISIONS · last 30m",
            "#91 Replace watcher placeholder",
            "DIFF SPARKLINE · merged PRs · last 60m",
            "source: stub snapshot feed",
        ] {
            assert!(frame.contains(needle), "missing {needle:?}\n{frame}");
        }
    }

    #[test]
    fn queue_clear_fallback_still_fills_sixty_rows() {
        let data = ReviewData::queue_clear("fixture");
        let mut terminal = Terminal::new(TestBackend::new(100, 60)).unwrap();
        terminal
            .draw(|frame| render_dashboard(frame, frame.area(), &data, 2))
            .unwrap();
        let frame = format!("{}", terminal.backend());

        assert!(frame.contains("queue clear · auto-reviewer caught up"));
        assert!(frame.contains("DIFF SPARKLINE"));
        assert!(frame.contains("no merged diff activity yet"));
    }

    #[test]
    fn render_sixty_row_snapshot_for_pr_body() {
        let mut view = WatcherView::with_feed(StubPrFeed::fixture());
        let mut terminal = Terminal::new(TestBackend::new(120, 60)).unwrap();
        terminal
            .draw(|frame| view.view(frame, frame.area()))
            .unwrap();
        println!("--- fleet-watcher 120x60 snapshot ---");
        println!("{}", terminal.backend());
    }
}
