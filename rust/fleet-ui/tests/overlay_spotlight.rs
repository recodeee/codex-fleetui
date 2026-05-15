use fleet_ui::overlay::{filter, Spotlight, SpotlightItem, SpotlightState};
use ratatui::{backend::TestBackend, Terminal};

fn poc_items() -> Vec<SpotlightItem> {
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
fn spotlight_filter_is_case_insensitive_substring() {
    let items = poc_items();
    let filtered = filter(&items, "SPLIT");
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].title, "Horizontal split");
    assert_eq!(filtered[1].title, "Vertical split");
}

#[test]
fn spotlight_default_render() {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let spotlight = Spotlight::new();
    let state = SpotlightState {
        query: "split".to_string(),
        selected: 0,
        tick: 0,
    };
    let items = poc_items();

    terminal
        .draw(|frame| spotlight.render(frame, frame.area(), &state, &items))
        .unwrap();

    insta::assert_snapshot!(format!("{}", terminal.backend()));
}
