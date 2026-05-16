//! Typed wrapper around the `tmux` subcommands the fleet dashboards use.
//!
//! Every `fleet-*` binary currently open-codes the same `Command::new("tmux")`
//! invocations: `fleet-state` / `fleet-watcher` / `fleet-waves` / `fleet-plan-tree`
//! each have their own `select_window`, and `panes::list_panes` hand-rolls the
//! `list-panes` + `capture-pane` pair. The bash side is worse — `fleet-tick.sh`,
//! `watcher-board.sh`, `force-claim.sh`, `cap-swap-daemon.sh` and friends each
//! re-spell `tmux list-panes -F …` with slightly different format strings.
//!
//! This module is the single typed surface for those calls. It is deliberately
//! thin: every fn shells out to the real `tmux` binary and returns
//! `io::Result`. There is no daemon, no persistent connection — tmux's control
//! protocol would be overkill for dashboards that poll on a 250ms tick.
//!
//! ## Failure posture
//!
//! Read calls ([`list_panes`], [`capture_pane`], [`display_message`]) return
//! `Ok(empty)` when tmux exits non-zero — a dashboard running outside tmux, or
//! pointed at a dead session, should render an empty frame, not crash. Write
//! calls ([`select_window`], [`set_pane_option`]) return the `bool` success so
//! the caller can decide; the existing binaries all treat a failed
//! `select-window` as best-effort (`let _ = …`), and that stays valid.

use std::process::Command;

use crate::subprocess::{output_with_deadline, TMUX_READ_DEADLINE};

/// A tmux target: `session:window` or `session:window.pane`. Construct via the
/// builders so callers don't hand-format the `:` / `.` separators (the class
/// of typo that produced `codex-fleet:1` vs `codex-fleet:overview` bugs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target(String);

impl Target {
    /// `session` — every window in the session.
    pub fn session(session: &str) -> Self {
        Target(format!("{session}:"))
    }

    /// `session:window` — a window by name or index.
    pub fn window(session: &str, window: &str) -> Self {
        Target(format!("{session}:{window}"))
    }

    /// `session:window.pane` — a specific pane by index.
    pub fn pane(session: &str, window: &str, pane_index: u32) -> Self {
        Target(format!("{session}:{window}.{pane_index}"))
    }

    /// A raw pane id (`%47`). tmux accepts these anywhere a target is expected.
    pub fn pane_id(pane_id: &str) -> Self {
        Target(pane_id.to_string())
    }

    /// The underlying `-t` string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `true` if a tmux session with this exact name exists.
///
/// Wraps `tmux has-session -t <session>`; the exit status is the answer, so
/// this never returns an error — an absent tmux binary just reports `false`.
pub fn has_session(session: &str) -> bool {
    let mut cmd = Command::new("tmux");
    cmd.args(["has-session", "-t", session]);
    output_with_deadline(cmd, TMUX_READ_DEADLINE)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// One pane's metadata from `tmux list-panes`.
///
/// This is the *raw* pane row — `panes::PaneInfo` is the richer type that
/// pairs this with captured scrollback. Kept separate so callers that only
/// need the index/command (e.g. the `select-window` click routers) don't pay
/// for a `capture-pane` per pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRow {
    /// `#{pane_id}` — e.g. `%47`.
    pub pane_id: String,
    /// `#{pane_index}` — 0-based position within the window.
    pub pane_index: u32,
    /// `#{pane_current_command}` — e.g. `node`, `bash`.
    pub current_command: String,
    /// `#{@panel}` — the fleet's per-pane label option, `[codex-<aid>]`.
    /// `None` when the option is unset.
    pub panel_label: Option<String>,
}

/// The `-F` format string [`list_panes`] requests. Field order must stay in
/// lock-step with [`parse_pane_row`].
const PANE_FORMAT: &str = "#{pane_id}\t#{pane_index}\t#{pane_current_command}\t#{@panel}";

fn parse_pane_row(line: &str) -> Option<PaneRow> {
    let mut parts = line.splitn(4, '\t');
    let pane_id = parts.next()?.to_string();
    let pane_index = parts.next()?.parse().ok()?;
    let current_command = parts.next()?.to_string();
    let panel_raw = parts.next().unwrap_or("").trim();
    let panel_label = if panel_raw.is_empty() {
        None
    } else {
        Some(panel_raw.to_string())
    };
    Some(PaneRow {
        pane_id,
        pane_index,
        current_command,
        panel_label,
    })
}

/// List the panes under `target`. Returns `Ok(vec![])` when tmux fails — a
/// dashboard outside tmux gets an empty fleet, not an `Err`.
///
/// This is the metadata-only call; `panes::list_panes` layers scrollback
/// capture on top of it.
pub fn list_panes(target: &Target) -> std::io::Result<Vec<PaneRow>> {
    let mut cmd = Command::new("tmux");
    cmd.args(["list-panes", "-t", target.as_str(), "-F", PANE_FORMAT]);
    let out = match output_with_deadline(cmd, TMUX_READ_DEADLINE) {
        Ok(o) => o,
        // Timeout or spawn failure collapses to the empty-fleet fallback.
        Err(_) => return Ok(Vec::new()),
    };
    if !out.status.success() {
        return Ok(Vec::new());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.lines().filter_map(parse_pane_row).collect())
}

/// Capture the last `lines` rows of a pane's scrollback (`capture-pane -p -S -N`).
///
/// `-p` prints to stdout; `-S -<lines>` starts the capture `lines` rows back
/// from the bottom. Returns `Ok(String::new())` on tmux failure.
pub fn capture_pane(pane_id: &str, lines: u32) -> std::io::Result<String> {
    let start = format!("-{lines}");
    let mut cmd = Command::new("tmux");
    cmd.args(["capture-pane", "-p", "-t", pane_id, "-S", &start]);
    let out = match output_with_deadline(cmd, TMUX_READ_DEADLINE) {
        Ok(o) => o,
        // Timeout / spawn failure → empty string, same as a non-zero exit.
        Err(_) => return Ok(String::new()),
    };
    if !out.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `tmux display-message -p -t <target> <format>` — resolve a single format
/// string against a target. The workhorse behind "what's this pane's tty",
/// "what session am I in", etc. Returns `Ok(String::new())` on failure.
pub fn display_message(target: &Target, format: &str) -> std::io::Result<String> {
    let mut cmd = Command::new("tmux");
    cmd.args(["display-message", "-p", "-t", target.as_str(), format]);
    let out = match output_with_deadline(cmd, TMUX_READ_DEADLINE) {
        Ok(o) => o,
        // Timeout / spawn failure → empty string fallback.
        Err(_) => return Ok(String::new()),
    };
    if !out.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Select (focus) a window. Best-effort: returns `false` when tmux fails
/// — e.g. the binary is running outside tmux, which is the exact case the
/// dashboards' click handlers already tolerate with `let _ = …`.
///
/// This replaces the four near-identical `select_window` fns in
/// `fleet-state` / `fleet-watcher` / `fleet-waves` / `fleet-plan-tree`.
pub fn select_window(target: &Target) -> bool {
    let mut cmd = Command::new("tmux");
    cmd.args(["select-window", "-t", target.as_str()]);
    output_with_deadline(cmd, TMUX_READ_DEADLINE)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Convenience for the dashboards' tab-click routers: focus
/// `<session>:<window_index>`. The binaries currently format
/// `"codex-fleet:{idx}"` inline; this keeps the `:` join in one place.
pub fn select_window_index(session: &str, window_index: usize) -> bool {
    select_window(&Target(format!("{session}:{window_index}")))
}

/// Set a pane-scoped option (`set-option -p -t <pane_id> <name> <value>`).
/// Used to stamp the `@panel` label — `set_pane_option(pid, "@panel", "[codex-foo]")`.
/// Best-effort `bool`, same posture as [`select_window`].
pub fn set_pane_option(pane_id: &str, name: &str, value: &str) -> bool {
    let mut cmd = Command::new("tmux");
    cmd.args(["set-option", "-p", "-t", pane_id, name, value]);
    output_with_deadline(cmd, TMUX_READ_DEADLINE)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests cover the pure parsing / formatting — the parts that don't
    // need a live tmux server. The subprocess fns (`list_panes`, `select_window`)
    // are integration-tested by the dashboards themselves; unit-testing them
    // would mean mocking `tmux` on PATH, which buys little over the parse tests.

    #[test]
    fn target_builders_format_separators() {
        assert_eq!(Target::session("codex-fleet").as_str(), "codex-fleet:");
        assert_eq!(
            Target::window("codex-fleet", "overview").as_str(),
            "codex-fleet:overview"
        );
        assert_eq!(
            Target::pane("codex-fleet", "overview", 3).as_str(),
            "codex-fleet:overview.3"
        );
        assert_eq!(Target::pane_id("%47").as_str(), "%47");
    }

    #[test]
    fn parses_a_full_pane_row() {
        let row = parse_pane_row("%47\t2\tnode\t[codex-admin-magnolia]").unwrap();
        assert_eq!(row.pane_id, "%47");
        assert_eq!(row.pane_index, 2);
        assert_eq!(row.current_command, "node");
        assert_eq!(row.panel_label.as_deref(), Some("[codex-admin-magnolia]"));
    }

    #[test]
    fn unset_panel_option_is_none() {
        // tmux prints an empty field when @panel is unset.
        let row = parse_pane_row("%1\t0\tbash\t").unwrap();
        assert!(row.panel_label.is_none());
    }

    #[test]
    fn panel_label_with_spaces_survives() {
        // fleet-tick.sh rewrites @panel with a status chip + branch suffix;
        // splitn(4) means the 4th field keeps its internal tabs/spaces — but
        // tmux's -F output uses a literal tab only between fields, so a
        // space-laden label comes through whole.
        let row =
            parse_pane_row("%9\t1\tnode\t◖ ● working ◗ [codex-admin-mite] → sub-3").unwrap();
        assert_eq!(
            row.panel_label.as_deref(),
            Some("◖ ● working ◗ [codex-admin-mite] → sub-3")
        );
    }

    #[test]
    fn malformed_rows_are_dropped() {
        // Too few fields → None, filtered out by list_panes.
        assert!(parse_pane_row("%1\t0").is_none());
        // Non-numeric pane_index → None.
        assert!(parse_pane_row("%1\tNOTANUM\tnode\t[x]").is_none());
        // Empty line → None.
        assert!(parse_pane_row("").is_none());
    }

    #[test]
    fn pane_format_field_count_matches_parser() {
        // Guard: if PANE_FORMAT gains/loses a field, this catches the parser drift.
        assert_eq!(PANE_FORMAT.matches('\t').count(), 3, "4 fields = 3 tabs");
    }
}
