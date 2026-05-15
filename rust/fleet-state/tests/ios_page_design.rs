#[path = "../src/ios_page_design.rs"]
mod ios_page_design;

use fleet_data::{fleet::WorkerRow, panes::PaneState};
use ios_page_design::{IosPageDesign, LiveIndicator};
use ratatui::{backend::TestBackend, Terminal};
use std::time::Duration;

fn row(
    email: &str,
    agent_id: &str,
    state: Option<PaneState>,
    working_on: &str,
    pane_id: Option<&str>,
    five_h_pct: u8,
) -> WorkerRow {
    WorkerRow {
        email: email.to_string(),
        agent_id: agent_id.to_string(),
        model_label: Some("gpt-5.5 high".to_string()),
        weekly_pct: 42,
        five_h_pct,
        state,
        working_on: working_on.to_string(),
        pane_subtext: pane_id
            .map(|id| format!("pane {id} · 10m 28s"))
            .unwrap_or_default(),
        pane_id: pane_id.map(str::to_string),
        is_current: false,
        quality: Some(91),
    }
}

fn fixture_rows() -> Vec<WorkerRow> {
    vec![
        row(
            "ada@example.test",
            "ada-example",
            Some(PaneState::Working),
            "claimed-task-text",
            Some("%1"),
            18,
        ),
        row(
            "brian@example.test",
            "brian-example",
            Some(PaneState::Approval),
            "review PR #117",
            Some("%2"),
            75,
        ),
        row("cora@example.test", "cora-example", None, "", None, 0),
        row(
            "drew@example.test",
            "drew-example",
            Some(PaneState::Dead),
            "",
            Some("%4"),
            100,
        ),
    ]
}

fn render(widget: IosPageDesign, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| frame.render_widget(widget.clone(), frame.area()))
        .unwrap();
    format!("{}", terminal.backend())
}

#[test]
fn wide_snapshot_places_active_and_reserve_side_by_side() {
    let out = render(
        IosPageDesign::new(fixture_rows()).live(LiveIndicator::from_elapsed(
            0,
            Duration::from_secs(1),
        )),
        200,
        32,
    );

    let row = out.lines().find(|line| line.contains("live panes")).unwrap();
    assert!(row.contains("RESERVE"), "{row}");
    assert!(out.contains("claimed-task-text"));
    assert!(out.contains("reserve · no live pane"));
}

#[test]
fn narrow_snapshot_stacks_active_above_reserve() {
    let out = render(IosPageDesign::new(fixture_rows()), 90, 36);
    let active_line = out.lines().position(|line| line.contains("live panes")).unwrap();
    let reserve_line = out
        .lines()
        .position(|line| line.contains("available accounts"))
        .unwrap();

    assert!(reserve_line > active_line + 2);
}

#[test]
fn fresh_tick_snapshot_uses_live_pulse() {
    let out = render(
        IosPageDesign::new(fixture_rows())
            .live(LiveIndicator::from_elapsed(0, Duration::from_secs(1)))
            .refresh_secs(1),
        120,
        30,
    );

    assert!(out.contains("● live · 1s"), "{out}");
}

#[test]
fn stale_tick_snapshot_uses_stale_pulse() {
    let out = render(
        IosPageDesign::new(fixture_rows()).live(LiveIndicator::from_elapsed(
            2,
            Duration::from_secs(11),
        )),
        120,
        30,
    );

    assert!(out.contains("◎ stale · 11s"), "{out}");
}
