use fleet_ui::tab_strip::{Tab, TabStrip, COUNTERS_PATH};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn tab_strip_default_render() {
    let _fixture = CounterFixture::install();
    let mut terminal = Terminal::new(TestBackend::new(128, 3)).unwrap();
    let mut hits = Vec::new();

    terminal
        .draw(|frame| {
            hits = TabStrip::new(Tab::Plan, 128)
                .with_tick(42)
                .render(frame, Rect::new(0, 1, 128, 1));
        })
        .unwrap();

    assert_eq!(hits.len(), Tab::ALL.len());
    assert_eq!(hits[2].tab, Tab::Plan);

    insta::assert_snapshot!(normalize_clock(format!("{}", terminal.backend())));
}

struct CounterFixture {
    previous: Option<Vec<u8>>,
}

impl CounterFixture {
    fn install() -> Self {
        let path = Path::new(COUNTERS_PATH);
        let previous = fs::read(path).ok();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        fs::write(
            path,
            format!(
                r#"{{"overview":7,"fleet":12,"plan":3,"waves":1,"review":0,"updated_at":{updated_at}}}"#
            ),
        )
        .unwrap();

        Self { previous }
    }
}

impl Drop for CounterFixture {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => {
                let _ = fs::write(COUNTERS_PATH, previous);
            }
            None => {
                let _ = fs::remove_file(COUNTERS_PATH);
            }
        }
    }
}

fn normalize_clock(rendered: String) -> String {
    rendered
        .lines()
        .map(|line| {
            let Some(start) = line.find("codex-fleet ") else {
                return line.to_string();
            };
            let mut line = line.to_string();
            let clock_start = start + "codex-fleet ".len();
            line.replace_range(clock_start..clock_start + "HH:MM:SS".len(), "HH:MM:SS");
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}
