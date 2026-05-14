use fleet_layout::{Layout, SplitSize};

pub const OVERVIEW_HEADER_TILE: &str = "overview-header-tile";

pub fn overview_header_tile(header_rows: u16, workers: u16) -> Layout {
    let workers = worker_container(workers);
    if header_rows == 0 {
        workers
    } else {
        Layout::SplitV(vec![
            (SplitSize::Lines(header_rows), Layout::Leaf),
            (SplitSize::Fill, workers),
        ])
    }
}

fn worker_container(workers: u16) -> Layout {
    match workers {
        0 | 1 => Layout::Leaf,
        n => {
            let left = (n + 1) / 2;
            let right = n - left;
            Layout::SplitH(vec![
                (SplitSize::Percent(50), worker_column(left)),
                (SplitSize::Fill, worker_column(right)),
            ])
        }
    }
}

fn worker_column(rows: u16) -> Layout {
    match rows {
        0 | 1 => Layout::Leaf,
        n => Layout::SplitV((0..n).map(|_| (SplitSize::Fill, Layout::Leaf)).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_preset_places_header_first_then_two_worker_columns() {
        assert_eq!(
            overview_header_tile(1, 4),
            Layout::SplitV(vec![
                (SplitSize::Lines(1), Layout::Leaf),
                (
                    SplitSize::Fill,
                    Layout::SplitH(vec![
                        (
                            SplitSize::Percent(50),
                            Layout::SplitV(vec![
                                (SplitSize::Fill, Layout::Leaf),
                                (SplitSize::Fill, Layout::Leaf),
                            ]),
                        ),
                        (
                            SplitSize::Fill,
                            Layout::SplitV(vec![
                                (SplitSize::Fill, Layout::Leaf),
                                (SplitSize::Fill, Layout::Leaf),
                            ]),
                        ),
                    ]),
                ),
            ])
        );
    }

    #[test]
    fn overview_preset_splits_odd_workers_into_left_heavy_columns() {
        assert_eq!(
            overview_header_tile(2, 5),
            Layout::SplitV(vec![
                (SplitSize::Lines(2), Layout::Leaf),
                (
                    SplitSize::Fill,
                    Layout::SplitH(vec![
                        (
                            SplitSize::Percent(50),
                            Layout::SplitV(vec![
                                (SplitSize::Fill, Layout::Leaf),
                                (SplitSize::Fill, Layout::Leaf),
                                (SplitSize::Fill, Layout::Leaf),
                            ]),
                        ),
                        (
                            SplitSize::Fill,
                            Layout::SplitV(vec![
                                (SplitSize::Fill, Layout::Leaf),
                                (SplitSize::Fill, Layout::Leaf),
                            ]),
                        ),
                    ]),
                ),
            ])
        );
    }

    #[test]
    fn overview_preset_can_skip_header() {
        assert_eq!(
            overview_header_tile(0, 2),
            Layout::SplitH(vec![
                (SplitSize::Percent(50), Layout::Leaf),
                (SplitSize::Fill, Layout::Leaf),
            ])
        );
    }
}
