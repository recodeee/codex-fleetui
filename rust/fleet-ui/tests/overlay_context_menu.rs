use fleet_ui::{
    overlay::{ContextMenu, MenuItem, Section},
    palette::{IOS_GREEN, IOS_ORANGE},
};
use ratatui::{backend::TestBackend, style::Color, Terminal};

fn poc_sections() -> Vec<Section<'static>> {
    vec![
        Section::new(vec![
            MenuItem::new("⧉", "Copy whole session", "C"),
            MenuItem::new("▤", "Copy visible", "c"),
            MenuItem::new("≡", "Copy this line", "l"),
        ]),
        Section::new(vec![
            MenuItem::new("⌕", "Search history…", "/"),
            MenuItem::new("↑", "Scroll to top", "<"),
            MenuItem::new("↓", "Scroll to bottom", ">"),
        ]),
        Section::new(vec![
            MenuItem::new("⊟", "Horizontal split", "h"),
            MenuItem::new("⊞", "Vertical split", "v"),
            MenuItem::new("⤢", "Zoom pane", "z"),
        ]),
        Section::new(vec![
            MenuItem::new("↥", "Swap up", "u"),
            MenuItem::new("↧", "Swap down", "d"),
            MenuItem::new("⇄", "Swap with marked", "s"),
            MenuItem::new("◆", "Mark pane", "m"),
        ]),
        Section::new(vec![
            MenuItem::new("↻", "Respawn pane", "R"),
            MenuItem::destructive("✕", "Kill pane", "X"),
        ]),
    ]
}

#[test]
fn context_menu_default_render() {
    let mut terminal = Terminal::new(TestBackend::new(80, 40)).unwrap();
    let menu = ContextMenu::new(
        "pane 1  %47",
        IOS_ORANGE,
        Some(("● LIVE", Color::Rgb(10, 36, 21), IOS_GREEN)),
        poc_sections(),
    );

    terminal
        .draw(|frame| menu.render(frame, frame.area()))
        .unwrap();

    insta::assert_snapshot!(format!("{}", terminal.backend()));
}
