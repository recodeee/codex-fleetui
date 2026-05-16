//! tmux pane introspection + state classifier.
//!
//! Replaces the regex sprinkles in `watcher-board.sh` / `force-claim.sh` /
//! `cap-swap-daemon.sh` / `stall-watcher.sh` — each script independently
//! re-parsed `tmux capture-pane` for substrings like "Working", "hit your
//! usage limit", "[Process completed]". This module consolidates the
//! classification into one [`PaneState`] enum + [`classify`] function so
//! every consumer agrees on what a pane's status is.
//!
//! The raw tmux plumbing — the `list-panes -F …` format string, the
//! `capture-pane` invocation — lives in [`crate::tmux`]. This module layers
//! the *concurrent* scrollback capture and the classifier on top of that
//! typed surface, so the `-F` field list has exactly one definition.

use crate::tmux::{self, Target};
use serde::{Deserialize, Serialize};

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

/// Number of scrollback rows [`list_panes`] captures per pane. `watcher-board.sh`
/// and the bash scrapers settled on a ~30-200 line tail; 200 is generous enough
/// that a cap banner wrapped mid-phrase still lands inside the window.
const SCROLLBACK_LINES: u32 = 200;

/// List the panes in `session:window` (or all panes if `window` is None),
/// each with its scrollback tail already captured.
///
/// Two-phase, same shape as before — but the metadata phase now goes through
/// [`crate::tmux::list_panes`] instead of an inline `Command`, so the `-F`
/// format string has a single home. The capture phase is unchanged: every
/// pane's `tmux capture-pane` is spawned **concurrently** (non-blocking
/// `Command::spawn`) before any output is drained, so ~20 panes cost roughly
/// one slow fork instead of the sum of 20 sequential ones.
pub fn list_panes(session: &str, window: Option<&str>) -> std::io::Result<Vec<PaneInfo>> {
    let target = match window {
        Some(w) => Target::window(session, w),
        None => Target::session(session),
    };

    // Phase 1 — metadata. tmux::list_panes returns Ok(vec![]) when tmux fails
    // (no session, running outside tmux), so an empty fleet falls straight
    // through to an empty Vec rather than an error.
    let rows = tmux::list_panes(&target)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 2 — spawn every capture-pane up front; the OS runs them in
    // parallel. We keep the raw child handles so we can drain them in
    // submission order and line each output up with its metadata row.
    //
    // Each child gets its own deadline (`TMUX_READ_DEADLINE`, 500 ms) so
    // a single wedged tmux capture-pane can't stall the whole batch. The
    // deadline budgets per-child, not across the join: a slow child can
    // run to completion as long as it finishes within its own window.
    // Stragglers are killed and reaped; the corresponding pane ends up
    // with an empty `scrollback_tail`, which `classify` will treat as
    // `PaneState::Idle` — the same fallback as a non-zero capture-pane
    // exit elsewhere.
    let deadline = crate::subprocess::TMUX_READ_DEADLINE;
    let mut children = Vec::with_capacity(rows.len());
    for row in &rows {
        let child = std::process::Command::new("tmux")
            .args([
                "capture-pane",
                "-p",
                "-t",
                &row.pane_id,
                "-S",
                &format!("-{SCROLLBACK_LINES}"),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // Record spawn time so each child's deadline starts from when it
        // was spawned — drain order is sequential, so without per-child
        // start tracking the later children would inherit the earlier
        // ones' wait time and trip the deadline spuriously.
        children.push((std::time::Instant::now(), child));
    }

    // Drain in submission order so each PaneInfo lines up with its row.
    let mut out = Vec::with_capacity(rows.len());
    for (row, (started, mut child)) in rows.into_iter().zip(children) {
        let scrollback_tail = match wait_child_with_deadline(&mut child, started, deadline) {
            Some(stdout) => String::from_utf8_lossy(&stdout).into_owned(),
            // Timeout or wait error → empty tail. classify() defaults to Idle.
            None => String::new(),
        };
        out.push(PaneInfo {
            pane_id: row.pane_id,
            panel_label: row.panel_label,
            current_command: row.current_command,
            scrollback_tail,
        });
    }
    Ok(out)
}

/// Drain `child`'s stdout, waiting up to `deadline` from `started`.
///
/// Returns `Some(stdout)` on a normal exit (regardless of status — the
/// caller only cares about the captured bytes), or `None` if the child
/// did not finish within the deadline (in which case it is killed and
/// reaped first). Errors from `try_wait` / `wait_with_output` collapse
/// to `None` so a transient failure on one pane never aborts the whole
/// batch.
fn wait_child_with_deadline(
    child: &mut std::process::Child,
    started: std::time::Instant,
    deadline: std::time::Duration,
) -> Option<Vec<u8>> {
    const POLL: std::time::Duration = std::time::Duration::from_millis(10);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                // Exited; drain stdout. wait_with_output would re-call
                // wait(), which is fine on an already-reaped child.
                // To do that we need to move the child, but we only have
                // &mut — take stdout manually instead.
                let mut buf = Vec::new();
                if let Some(mut stdout) = child.stdout.take() {
                    use std::io::Read;
                    let _ = stdout.read_to_end(&mut buf);
                }
                return Some(buf);
            }
            Ok(None) => {
                if started.elapsed() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(POLL);
            }
            Err(_) => return None,
        }
    }
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
