use fleet_ui::rail::{progress_rail, RailAxis};
use ratatui::{backend::TestBackend, layout::Rect, text::Line, widgets::Paragraph, Terminal};

#[test]
fn rail_default_render() {
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    let area = Rect::new(4, 2, 12, 1);

    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(Line::from(progress_rail(65, RailAxis::Usage, 10))),
                area,
            )
        })
        .unwrap();

    insta::assert_snapshot!(format!("{}", terminal.backend()));
}
