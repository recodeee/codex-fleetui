//! tmux pane introspection + state classifier.
//!
//! Replaces the regex sprinkles in `watcher-board.sh` / `force-claim.sh` /
//! `cap-swap-daemon.sh` / `stall-watcher.sh` — each script independently
//! re-parsed `tmux capture-pane` for substrings like "Working", "hit your
//! usage limit", "[Process completed]". This module consolidates the
//! classification into one [`PaneState`] enum + [`classify`] function so
//! every consumer agrees on what a pane's status is.
//!
//! Shells out via `std::process::Command` for the tmux queries.

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneInfo {
    pub pane_id: String,
    pub panel_label: Option<String>,
    pub current_command: String,
    pub scrollback_tail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneState {
    /// Active codex task.
    Working,
    /// Codex idle prompt visible (`›` cursor).
    Idle,
    /// Polling Colony for next task.
    Polling,
    /// Hit cap-probe / "usage limit" / rate limit.
    Capped,
    /// Awaiting human approval for a tool call.
    Approval,
    /// Codex booting / handshaking.
    Boot,
    /// Bash shell (codex died and pane fell through to its login shell).
    Dead,
}

/// Classify a single pane from its scrollback + current command.
///
/// Matching order is significant: cap-state and approval markers win over
/// generic "Working" hints because the latter can appear in scrollback
/// from a previous task even after the pane is now stuck.
pub fn classify(info: &PaneInfo) -> PaneState {
    let cmd = info.current_command.as_str();
    let tail = info.scrollback_tail.as_str();

    // Bash pane = codex died.
    if matches!(cmd, "bash" | "sh" | "zsh") && !tail.contains("codex") {
        return PaneState::Dead;
    }

    // Cap markers — checked first so a recently-capped pane doesn't get
    // mislabelled as Working off stale scrollback.
    if tail.contains("hit your usage limit")
        || tail.contains("rate-limit")
        || tail.contains("429")
        || tail.contains("usage-limit")
    {
        return PaneState::Capped;
    }

    // Approval gates pop a "approval required" / "auto-reviewer" prompt.
    if tail.contains("approval required")
        || tail.contains("Auto-reviewer approved")
        || tail.contains("approve this command")
    {
        return PaneState::Approval;
    }

    // Boot — codex hasn't yet finished its handshake.
    if tail.contains("Loading codex")
        || tail.contains("Connecting to")
        || tail.contains("Boot complete")
    {
        return PaneState::Boot;
    }

    // Working — the active-task indicator.
    if tail.contains("• Working") || tail.contains("Working (") {
        return PaneState::Working;
    }

    // Polling Colony — `task_ready_for_agent` in flight.
    if tail.contains("task_ready_for_agent")
        || tail.contains("polling Colony")
        || tail.contains("no ready tasks")
    {
        return PaneState::Polling;
    }

    // Default: codex CLI shows its `›` cursor placeholder prompt.
    PaneState::Idle
}

/// List the panes in `session:window` (or all panes if `window` is None).
/// Returns `PaneInfo` with scrollback already captured (`-S -200`).
pub fn list_panes(session: &str, window: Option<&str>) -> std::io::Result<Vec<PaneInfo>> {
    let target = match window {
        Some(w) => format!("{session}:{w}"),
        None => format!("{session}:"),
    };
    let list = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            &target,
            "-F",
            "#{pane_id}\t#{@panel}\t#{pane_current_command}",
        ])
        .output()?;
    if !list.status.success() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for line in String::from_utf8_lossy(&list.stdout).lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 {
            continue;
        }
        let pane_id = parts[0].to_string();
        let panel = parts[1].trim();
        let panel_label = if panel.is_empty() { None } else { Some(panel.to_string()) };
        let current_command = parts[2].to_string();

        let cap = Command::new("tmux")
            .args(["capture-pane", "-p", "-t", &pane_id, "-S", "-200"])
            .output()?;
        let scrollback_tail = String::from_utf8_lossy(&cap.stdout).into_owned();

        out.push(PaneInfo { pane_id, panel_label, current_command, scrollback_tail });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(cmd: &str, tail: &str) -> PaneInfo {
        PaneInfo {
            pane_id: "%1".into(),
            panel_label: None,
            current_command: cmd.into(),
            scrollback_tail: tail.into(),
        }
    }

    #[test]
    fn working_pane_classified() {
        assert_eq!(classify(&info("node", "• Working (1m 30s)")), PaneState::Working);
    }

    #[test]
    fn capped_overrides_working() {
        let tail = "• Working (5s)\n... \nhit your usage limit reset in 3h";
        assert_eq!(classify(&info("node", tail)), PaneState::Capped);
    }

    #[test]
    fn bash_pane_is_dead() {
        assert_eq!(classify(&info("bash", "$ ls\n")), PaneState::Dead);
    }

    #[test]
    fn bash_pane_with_codex_isnt_dead() {
        assert_eq!(
            classify(&info("bash", "running codex worker loop\n• Working (5s)")),
            PaneState::Working
        );
    }

    #[test]
    fn approval_classified() {
        assert_eq!(
            classify(&info("node", "Auto-reviewer approved this command")),
            PaneState::Approval
        );
    }

    #[test]
    fn polling_classified() {
        assert_eq!(
            classify(&info("node", "polling Colony for next task")),
            PaneState::Polling
        );
    }

    #[test]
    fn idle_default() {
        assert_eq!(classify(&info("node", "› \n")), PaneState::Idle);
    }
}
