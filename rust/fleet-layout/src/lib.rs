// Design inspired by zellij (https://github.com/zellij-org/zellij, MIT).

mod tmux;

use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use tmux::{SplitAxis, Tmux};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    Leaf,
    SplitV(Vec<(SplitSize, Layout)>),
    SplitH(Vec<(SplitSize, Layout)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitSize {
    Lines(u16),
    Percent(u8),
    Fill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applier {
    session: String,
    window: String,
}

impl Applier {
    pub fn new(session: impl Into<String>, window: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            window: window.into(),
        }
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    pub fn window(&self) -> &str {
        &self.window
    }

    pub fn apply(&self, layout: &Layout) -> Result<Vec<String>, LayoutError> {
        self.apply_with(&mut tmux::CommandTmux, layout)
    }

    fn apply_with<T: Tmux>(
        &self,
        tmux: &mut T,
        layout: &Layout,
    ) -> Result<Vec<String>, LayoutError> {
        let target = self.target_window();
        let panes = tmux
            .list_panes(&target)
            .map_err(|source| LayoutError::ListPanes { target, source })?;
        let root = panes.first().cloned().ok_or_else(|| LayoutError::NoPanes {
            target: self.target_window(),
        })?;

        apply_node(tmux, &self.target_window(), root, layout)
    }

    fn target_window(&self) -> String {
        format!("{}:{}", self.session, self.window)
    }
}

#[derive(Debug)]
pub enum LayoutError {
    EmptySplit,
    NoPanes {
        target: String,
    },
    ListPanes {
        target: String,
        source: std::io::Error,
    },
    SplitFailed {
        target: String,
    },
    SplitDidNotCreatePane {
        target: String,
    },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::EmptySplit => write!(f, "split layout must contain at least one child"),
            LayoutError::NoPanes { target } => {
                write!(f, "tmux target {target} did not contain a root pane")
            }
            LayoutError::ListPanes { target, source } => {
                write!(f, "failed to list panes for {target}: {source}")
            }
            LayoutError::SplitFailed { target } => {
                write!(f, "tmux split-window failed for target {target}")
            }
            LayoutError::SplitDidNotCreatePane { target } => {
                write!(f, "tmux split-window for {target} did not create a pane")
            }
        }
    }
}

impl Error for LayoutError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LayoutError::ListPanes { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn apply_node<T: Tmux>(
    tmux: &mut T,
    window_target: &str,
    root_pane: String,
    layout: &Layout,
) -> Result<Vec<String>, LayoutError> {
    match layout {
        Layout::Leaf => Ok(vec![root_pane]),
        Layout::SplitH(children) => apply_split(
            tmux,
            window_target,
            root_pane,
            SplitAxis::Horizontal,
            children,
        ),
        Layout::SplitV(children) => apply_split(
            tmux,
            window_target,
            root_pane,
            SplitAxis::Vertical,
            children,
        ),
    }
}

fn apply_split<T: Tmux>(
    tmux: &mut T,
    window_target: &str,
    root_pane: String,
    axis: SplitAxis,
    children: &[(SplitSize, Layout)],
) -> Result<Vec<String>, LayoutError> {
    if children.is_empty() {
        return Err(LayoutError::EmptySplit);
    }

    let mut child_roots = vec![root_pane.clone()];
    let mut known: HashSet<String> = tmux
        .list_panes(window_target)
        .map_err(|source| LayoutError::ListPanes {
            target: window_target.to_string(),
            source,
        })?
        .into_iter()
        .collect();
    known.insert(root_pane.clone());

    let mut split_target = root_pane;
    for (size, _) in children.iter().skip(1) {
        if !tmux.split_window(&split_target, axis, *size) {
            return Err(LayoutError::SplitFailed {
                target: split_target,
            });
        }

        let panes = tmux
            .list_panes(window_target)
            .map_err(|source| LayoutError::ListPanes {
                target: window_target.to_string(),
                source,
            })?;
        let new_pane = panes
            .into_iter()
            .find(|pane_id| !known.contains(pane_id))
            .ok_or_else(|| LayoutError::SplitDidNotCreatePane {
                target: split_target.clone(),
            })?;
        known.insert(new_pane.clone());
        split_target = new_pane.clone();
        child_roots.push(new_pane);
    }

    let mut ordered = Vec::new();
    for ((_, child), pane_id) in children.iter().zip(child_roots) {
        ordered.extend(apply_node(tmux, window_target, pane_id, child)?);
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::{split_window_args, SplitAxis};

    #[derive(Debug)]
    struct FakeTmux {
        panes: Vec<String>,
        next_pane: usize,
        splits: Vec<Vec<String>>,
        fail_next_split: bool,
    }

    impl FakeTmux {
        fn with_root() -> Self {
            Self {
                panes: vec!["%0".to_string()],
                next_pane: 1,
                splits: Vec::new(),
                fail_next_split: false,
            }
        }

        fn fail_splits() -> Self {
            Self {
                fail_next_split: true,
                ..Self::with_root()
            }
        }
    }

    impl Tmux for FakeTmux {
        fn list_panes(&mut self, _target: &str) -> std::io::Result<Vec<String>> {
            Ok(self.panes.clone())
        }

        fn split_window(&mut self, target: &str, axis: SplitAxis, size: SplitSize) -> bool {
            self.splits.push(split_window_args(target, axis, size));
            if self.fail_next_split {
                self.fail_next_split = false;
                return false;
            }

            let pane_id = format!("%{}", self.next_pane);
            self.next_pane += 1;
            self.panes.push(pane_id);
            true
        }
    }

    fn apply_with_fake(layout: &Layout) -> (Result<Vec<String>, LayoutError>, FakeTmux) {
        let applier = Applier::new("codex-fleet", "overview");
        let mut fake = FakeTmux::with_root();
        let result = applier.apply_with(&mut fake, layout);
        (result, fake)
    }

    #[test]
    fn leaf_layout_returns_one_pane() {
        let (panes, _) = apply_with_fake(&Layout::Leaf);
        assert_eq!(panes.unwrap(), vec!["%0"]);
    }

    #[test]
    fn single_horizontal_split_returns_two_panes_left_to_right() {
        let layout = Layout::SplitH(vec![
            (SplitSize::Fill, Layout::Leaf),
            (SplitSize::Fill, Layout::Leaf),
        ]);

        let (panes, _) = apply_with_fake(&layout);

        assert_eq!(panes.unwrap(), vec!["%0", "%1"]);
    }

    #[test]
    fn single_vertical_split_returns_two_panes_top_to_bottom() {
        let layout = Layout::SplitV(vec![
            (SplitSize::Fill, Layout::Leaf),
            (SplitSize::Fill, Layout::Leaf),
        ]);

        let (panes, _) = apply_with_fake(&layout);

        assert_eq!(panes.unwrap(), vec!["%0", "%1"]);
    }

    #[test]
    fn nested_splits_preserve_depth_first_tree_order() {
        let layout = Layout::SplitH(vec![
            (
                SplitSize::Fill,
                Layout::SplitV(vec![
                    (SplitSize::Fill, Layout::Leaf),
                    (SplitSize::Fill, Layout::Leaf),
                ]),
            ),
            (SplitSize::Fill, Layout::Leaf),
        ]);

        let (panes, _) = apply_with_fake(&layout);

        assert_eq!(panes.unwrap(), vec!["%0", "%2", "%1"]);
    }

    #[test]
    fn split_sizes_are_translated_to_tmux_args() {
        let applier = Applier::new("codex-fleet", "overview");
        let mut fake = FakeTmux::with_root();
        let layout = Layout::SplitH(vec![
            (SplitSize::Fill, Layout::Leaf),
            (SplitSize::Lines(5), Layout::Leaf),
            (SplitSize::Percent(25), Layout::Leaf),
            (SplitSize::Fill, Layout::Leaf),
        ]);

        applier.apply_with(&mut fake, &layout).unwrap();

        assert!(fake.splits[0].windows(2).any(|args| args == ["-l", "5"]));
        assert!(fake.splits[1].windows(2).any(|args| args == ["-p", "25"]));
        assert!(!fake.splits[2].iter().any(|arg| arg == "-l" || arg == "-p"));
    }

    #[test]
    fn split_failure_returns_error() {
        let applier = Applier::new("codex-fleet", "overview");
        let mut fake = FakeTmux::fail_splits();
        let layout = Layout::SplitH(vec![
            (SplitSize::Fill, Layout::Leaf),
            (SplitSize::Fill, Layout::Leaf),
        ]);

        let err = applier.apply_with(&mut fake, &layout).unwrap_err();

        assert!(matches!(err, LayoutError::SplitFailed { .. }));
    }
}
