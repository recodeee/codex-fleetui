use fleet_ui::chip::{status_chip, ChipKind, CHIP_WIDTH};
use ratatui::{backend::TestBackend, layout::Rect, text::Line, widgets::Paragraph, Terminal};

#[test]
fn chip_default_render() {
    let mut terminal = Terminal::new(TestBackend::new(24, 5)).unwrap();
    let area = Rect::new(6, 2, CHIP_WIDTH, 1);

    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(Line::from(status_chip(ChipKind::Working))),
                area,
            )
        })
        .unwrap();

    insta::assert_snapshot!(format!("{}", terminal.backend()));
}
