//! Shared helpers for the codex-fleet tuirealm binaries.
//!
//! Every dashboard binary (fleet-state, fleet-plan-tree, fleet-waves,
//! fleet-watcher, fleet-tab-strip, fleet-tui-poc) reaches for the same
//! two helpers right after `Application::init`:
//!
//!   1. spin up a `CrosstermTerminalAdapter` with raw mode +
//!      alt-screen + optional mouse capture, mapping the
//!      `TerminalError` into `io::Error::other` so the binary's
//!      `io::Result<()>` `main()` works without a custom error type.
//!   2. dispatch `tmux select-window -t <session>:<idx>` when a
//!      click hits a tab pill.
//!
//! Until now both were copy-pasted across the six tuirealm-ported
//! binaries. They live here so the next tuirealm-shaped binary picks
//! up matching semantics for free.
//!
//! What does **not** live here yet:
//!   - the `Model<T: TerminalAdapter>` struct each binary defines.
//!     The body is ~25 lines and parameterised over the binary-local
//!     `Id` / `Msg` types; the duplication is mostly in the type
//!     signature. A generic `run<C: Component + AppComponent<Msg,
//!     NoUserEvent>>` runner is a natural follow-up but needs
//!     Hash/Eq bounds on the local Id enum that aren't satisfied by
//!     tuirealm's stock derive.
//!   - tuirealm event re-exports. Each binary imports those from
//!     `tuirealm::event` directly; the helper crate would just add
//!     an indirection.

use std::io;

use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

/// Initialise a Crossterm-backed `TerminalAdapter` with raw mode +
/// the alternate screen + optional mouse capture. Used by every
/// codex-fleet dashboard binary's `Model::init_adapter` call.
///
/// `enable_mouse` toggles `EnableMouseCapture`. Pass `true` when the
/// binary has clickable surfaces (fleet-tab-strip pills,
/// fleet-tui-poc session-switcher cards) and `false` for the
/// read-only dashboards (fleet-state, fleet-watcher,
/// fleet-plan-tree, fleet-waves) so a stray mouse event in those
/// panes doesn't get intercepted away from tmux's own copy-mode /
/// status-bar bindings.
pub fn init_crossterm_adapter(enable_mouse: bool) -> io::Result<CrosstermTerminalAdapter> {
    let mut adapter = CrosstermTerminalAdapter::new()
        .map_err(|e| io::Error::other(format!("crossterm adapter init: {e:?}")))?;
    adapter
        .enable_raw_mode()
        .map_err(|e| io::Error::other(format!("enable raw mode: {e:?}")))?;
    adapter
        .enter_alternate_screen()
        .map_err(|e| io::Error::other(format!("enter alternate screen: {e:?}")))?;
    if enable_mouse {
        adapter
            .enable_mouse_capture()
            .map_err(|e| io::Error::other(format!("enable mouse capture: {e:?}")))?;
    }
    Ok(adapter)
}

/// Tear down the adapter the way every binary's exit path does:
/// disable mouse capture (best-effort), then raw mode, then leave
/// the alternate screen. Failures are swallowed because the process
/// is on its way out — surfacing them would mask the original error.
pub fn shutdown_adapter<T: TerminalAdapter>(adapter: &mut T) {
    let _ = adapter.disable_mouse_capture();
    let _ = adapter.disable_raw_mode();
    let _ = adapter.leave_alternate_screen();
}

/// Dispatch `tmux select-window -t <session>:<idx>`. Session
/// defaults to the `CODEX_FLEET_SESSION` env var, falling back to
/// the literal `"codex-fleet"`. Used by the fleet-tab-strip click
/// handler and any future binary that wants to forward a
/// window-switch from inside a ratatui surface.
///
/// Best-effort — a missing tmux binary or wrong session name is
/// silently ignored. The dashboards stay alive even when tmux isn't
/// reachable so the operator can keep reading the rendered state.
pub fn select_tmux_window(idx: usize) {
    let session =
        std::env::var("CODEX_FLEET_SESSION").unwrap_or_else(|_| "codex-fleet".to_string());
    let _ = std::process::Command::new("tmux")
        .args(["select-window", "-t", &format!("{}:{}", session, idx)])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_tmux_window_does_not_panic_on_missing_session() {
        // Use a session that does not exist; the helper must swallow
        // the tmux non-zero exit without panicking.
        unsafe {
            std::env::set_var(
                "CODEX_FLEET_SESSION",
                "fleet-components-test-nonexistent-session",
            );
        }
        select_tmux_window(42);
    }
}
