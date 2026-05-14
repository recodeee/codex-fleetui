use fleet_ui::{button::Button, overlay::centered_overlay};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

#[test]
fn button_default_render() {
    let button = Button::new("Launch").icon("▶").shortcut("Enter");

    let mut terminal = Terminal::new(TestBackend::new(32, 7)).unwrap();
    let area = centered_overlay(Rect::new(0, 0, 32, 7), button.width(), 3);

    terminal
        .draw(|frame| frame.render_widget(button, area))
        .unwrap();

    insta::assert_snapshot!(format!("{}", terminal.backend()));
}
