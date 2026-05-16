//! Bounded-wait wrapper around `std::process::Command::output()`.
//!
//! The dashboards in this crate poll on a ~250ms tick and shell out to external
//! tools (`tmux`, `agent-auth`, `git`, `gh`). When one of those children hangs
//! — a tmux server in the middle of restarting, an `agent-auth` prompting for
//! credentials, a `gh` call stuck on the network — the dashboard's tick blocks
//! indefinitely waiting for the child to exit.
//!
//! This module provides [`output_with_deadline`], a sync helper that spawns
//! the child, polls [`std::process::Child::try_wait`] on a short sleep loop,
//! and kills + reaps the child if it doesn't finish before the deadline.
//! On timeout it returns `io::Error::new(io::ErrorKind::TimedOut, ...)`, which
//! every existing call site already treats the same as a non-zero exit:
//! collapse to the empty / best-effort fallback.
//!
//! Sync on purpose. The rest of the crate is sync and pulling in tokio for a
//! handful of read-only subprocess calls would dwarf the fix. The poll
//! interval ([`POLL_INTERVAL`]) is tuned so a fast-finishing child still
//! returns within a single poll, and a slow one is reaped within
//! `deadline + POLL_INTERVAL` worst case.
//!
//! ## Choosing a deadline
//!
//! Two named constants are exported so call sites stay consistent:
//!
//! - [`TMUX_READ_DEADLINE`] (`500 ms`) — tmux read-only commands
//!   (`list-panes`, `capture-pane`, `display-message`, `select-window`,
//!   `set-option`). These talk to a local socket; under healthy conditions
//!   they return in single-digit milliseconds. 500ms is generous enough to
//!   ride out a momentary tmux-server stall (e.g. another client holding the
//!   command lock) without freezing the dashboard's 250ms tick across more
//!   than one or two frames.
//! - [`HEAVY_CMD_DEADLINE`] (`2 s`) — `agent-auth list`, `git`, `gh`. These
//!   can touch disk, the network, or remote APIs. 2s is short enough that a
//!   broken auth flow or stalled GitHub call drops out fast, but long enough
//!   for a real `gh pr list --json files` over a slow link to complete.
//!
//! If a future call site truly needs a longer wait, pass an explicit
//! [`std::time::Duration`] rather than redefining the constants.

use std::io;
use std::process::{Child, Command, Output};
use std::thread;
use std::time::{Duration, Instant};

/// Deadline for tmux read-only commands (`list-panes`, `capture-pane`,
/// `display-message`, `select-window`, `set-option`).
///
/// tmux talks over a local Unix socket and these calls normally return in
/// single-digit milliseconds. 500 ms keeps the dashboard's 250 ms tick
/// recoverable: a stalled tmux server costs at most two frames, not the
/// whole session.
pub const TMUX_READ_DEADLINE: Duration = Duration::from_millis(500);

/// Deadline for heavier subprocess calls — `agent-auth list`, `git`, `gh`.
///
/// These can touch disk, the network, or a remote API. 2 s is short enough
/// that a broken auth flow or stalled GitHub call drops out fast, but long
/// enough for a real `gh pr list --json files` to finish on a slow link.
pub const HEAVY_CMD_DEADLINE: Duration = Duration::from_secs(2);

/// How often [`output_with_deadline`] polls [`Child::try_wait`].
///
/// Small enough that a fast child returns within one poll, large enough that
/// a slow child doesn't burn CPU while we wait. The worst-case overshoot of
/// the deadline is one `POLL_INTERVAL`.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Spawn `cmd` and wait up to `deadline` for it to finish.
///
/// Reads the full stdout/stderr just like `Command::output()`. On timeout the
/// child is killed and reaped (`wait()` is called so it never zombies) and
/// the function returns `io::Error::new(io::ErrorKind::TimedOut, ...)`. Every
/// caller in this crate already treats a non-zero exit as "fall back to an
/// empty result", so the timeout collapses into the same path.
///
/// The poll loop uses a short [`thread::sleep`]; this is sync on purpose —
/// the rest of the crate is sync and the dashboards are not async runtimes.
pub fn output_with_deadline(mut cmd: Command, deadline: Duration) -> io::Result<Output> {
    // Make sure we own the stdout/stderr handles so `wait_with_output`
    // can drain them. If the caller already set `stdout`/`stderr`, this
    // is a no-op override — but every call site in this crate either
    // wants the bytes (`.output()` path) or doesn't care, and `Stdio::null()`
    // is set explicitly elsewhere via `.status()`.
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let start = Instant::now();

    loop {
        match child.try_wait()? {
            Some(_status) => {
                // Child exited; collect its output. wait_with_output also
                // reaps the process, so no zombie.
                return child.wait_with_output();
            }
            None => {
                if start.elapsed() >= deadline {
                    kill_and_reap(&mut child);
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "subprocess exceeded deadline of {:?}",
                            deadline
                        ),
                    ));
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Kill the child and wait for it to exit so it does not zombie.
///
/// All errors are swallowed: the child may have exited between our last
/// `try_wait` and the `kill`, which is fine — we still call `wait()` to
/// reap it.
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn output_returns_quickly_when_command_finishes_fast() {
        let cmd = Command::new("true");
        let start = Instant::now();
        let out = output_with_deadline(cmd, Duration::from_secs(2))
            .expect("`true` should succeed within the deadline");
        let elapsed = start.elapsed();
        assert!(out.status.success(), "`true` exits 0");
        // Should be well under the deadline — generous bound to keep the
        // test stable on a loaded CI runner.
        assert!(
            elapsed < Duration::from_secs(1),
            "fast command took {elapsed:?}, expected << 2s deadline"
        );
    }

    #[test]
    fn output_returns_timeout_error_for_sleep_longer_than_deadline() {
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let start = Instant::now();
        let err = output_with_deadline(cmd, Duration::from_millis(100))
            .expect_err("sleep 5 must exceed a 100ms deadline");
        let elapsed = start.elapsed();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut, "{err}");
        // Should give up shortly after the deadline; allow generous slack
        // for poll-interval overshoot and scheduler jitter.
        assert!(
            elapsed < Duration::from_secs(2),
            "timeout took {elapsed:?}, expected ~100ms + slack"
        );
    }

    #[test]
    fn child_is_reaped_on_timeout() {
        // Spawn `sh -c "sleep 10"` with a tight deadline. After the
        // function returns, the child must have been killed and reaped:
        // try_wait on a separately-spawned twin should not see ours, and
        // the function's own wait_with_output / kill_and_reap path should
        // leave nothing pending. We verify by spawning, timing out, then
        // checking that the returned error is TimedOut (which can only
        // happen on the kill+reap path).
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 10"]);
        let err = output_with_deadline(cmd, Duration::from_millis(50))
            .expect_err("sleep 10 must time out");
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);

        // Sanity: spawn the same shape manually, kill it, and confirm
        // wait() returns — proving the reaping primitive itself works on
        // this platform. (If this regressed, the in-function reap above
        // would also be suspect.)
        let mut sanity = Command::new("sh")
            .args(["-c", "sleep 10"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sanity child");
        sanity.kill().expect("kill sanity child");
        let status = sanity.wait().expect("reap sanity child");
        // On Unix, killed-by-signal has no exit code; .success() is false.
        assert!(!status.success(), "killed child should not report success");
    }
}
