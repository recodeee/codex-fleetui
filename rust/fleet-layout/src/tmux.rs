use std::process::{Command, Stdio};

use crate::SplitSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitAxis {
    Horizontal,
    Vertical,
}

pub(crate) trait Tmux {
    fn list_panes(&mut self, target: &str) -> std::io::Result<Vec<String>>;
    fn split_window(&mut self, target: &str, axis: SplitAxis, size: SplitSize) -> bool;
}

#[derive(Debug, Default)]
pub(crate) struct CommandTmux;

impl Tmux for CommandTmux {
    fn list_panes(&mut self, target: &str) -> std::io::Result<Vec<String>> {
        let out = Command::new("tmux")
            .args(["list-panes", "-t", target, "-F", "#{pane_id}"])
            .output()?;
        if !out.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(stdout
            .lines()
            .map(str::trim)
            .filter(|pane_id| !pane_id.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }

    fn split_window(&mut self, target: &str, axis: SplitAxis, size: SplitSize) -> bool {
        Command::new("tmux")
            .args(split_window_args(target, axis, size))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

pub(crate) fn split_window_args(target: &str, axis: SplitAxis, size: SplitSize) -> Vec<String> {
    let mut args = vec![
        "split-window".to_string(),
        match axis {
            SplitAxis::Horizontal => "-h".to_string(),
            SplitAxis::Vertical => "-v".to_string(),
        },
        "-t".to_string(),
        target.to_string(),
    ];

    match size {
        SplitSize::Lines(lines) => {
            args.push("-l".to_string());
            args.push(lines.to_string());
        }
        SplitSize::Percent(percent) => {
            args.push("-p".to_string());
            args.push(percent.to_string());
        }
        SplitSize::Fill => {}
    }

    args
}
