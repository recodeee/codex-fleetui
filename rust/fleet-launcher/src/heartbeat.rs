//! Presence + heartbeat tracking for spawned kitty sessions.
//!
//! When `spawn_in_kitty` returns a kitty PID, we drop a tiny session file
//! at `/tmp/fleet-launcher/sessions/<pid>.session`. That file lets a
//! supervisor (or the next `fleet-spawn` invocation) answer:
//!
//! - "is this PID still alive?"  — via [`is_alive`] (cross-platform
//!   process-table check from [`sysinfo`]).
//! - "when was this session last seen?" — via [`SessionRecord::last_heartbeat`]
//!   (a [`jiff::Timestamp`] the spawned process can update by calling
//!   [`touch_heartbeat`] periodically).
//! - "which PIDs have died but their session file is still around?"  —
//!   via [`prune_stale`].
//!
//! ## Why a flat file per session?
//!
//! Three reasons:
//!
//! 1. No DB / no IPC. The fleet's existing coordination layer is Colony
//!    + tmux; this module stays out of it.
//! 2. Atomic-by-rename writes. Each `record_session` writes to a `.tmp`
//!    sibling and `rename(2)`s into place — no half-written records even
//!    if multiple supervisors race.
//! 3. `ls /tmp/fleet-launcher/sessions/` already shows the live set; an
//!    operator can `cat` one without learning a new tool.
//!
//! ## File format
//!
//! Plain `KEY=VALUE` lines, one per attribute. Values are *not* shell-
//! quoted (they're raw text); newlines and `=` in values are escaped as
//! `\n` / `\=`. We don't pull `serde` for one struct.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::{Span, Timestamp};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{CliKind, LaunchSpec};

/// Where session files live on disk. Overridable via env so the cargo
/// tests can sandbox themselves to a per-test directory.
fn sessions_dir() -> PathBuf {
    if let Ok(v) = std::env::var("FLEET_LAUNCHER_SESSIONS_DIR") {
        return PathBuf::from(v);
    }
    std::env::temp_dir()
        .join("fleet-launcher")
        .join("sessions")
}

/// One tracked spawn. Persisted at `<sessions_dir>/<pid>.session`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    /// The kitty process PID (returned by [`crate::spawn_in_kitty`]).
    pub pid: u32,
    /// Which CLI runs inside the kitty window.
    pub cli: CliKind,
    /// Human label — usually the account email or worker name.
    pub title: String,
    /// `*_HOME` directory the spawned CLI was bound to.
    pub home_dir: PathBuf,
    /// Wall-clock time of the spawn call.
    pub spawned_at: Timestamp,
    /// Wall-clock time of the most recent heartbeat. Equals
    /// [`Self::spawned_at`] until the spawned process calls
    /// [`touch_heartbeat`].
    pub last_heartbeat: Timestamp,
}

impl SessionRecord {
    fn path(&self) -> PathBuf {
        sessions_dir().join(format!("{}.session", self.pid))
    }
}

/// Drop a session file recording `(spec, pid)` with `spawned_at` =
/// `last_heartbeat` = now.
///
/// Atomic-by-rename: writes `<pid>.session.tmp` first, then
/// `rename(2)`s into place. Concurrent supervisors can call this for the
/// same PID without producing a torn file.
pub fn record_session(spec: &LaunchSpec, pid: u32) -> io::Result<SessionRecord> {
    let now = Timestamp::now();
    let rec = SessionRecord {
        pid,
        cli: spec.cli,
        title: spec.title.clone(),
        home_dir: spec.home_dir.clone(),
        spawned_at: now,
        last_heartbeat: now,
    };
    write_record(&rec)?;
    Ok(rec)
}

/// Update the `last_heartbeat` timestamp for `pid`. Used by long-running
/// spawned processes that want to advertise "I'm still healthy" without
/// round-tripping through Colony.
///
/// Returns `Ok(false)` if no session file exists for that PID (caller can
/// decide whether to record a fresh one).
pub fn touch_heartbeat(pid: u32) -> io::Result<bool> {
    let path = sessions_dir().join(format!("{pid}.session"));
    let mut rec = match read_record(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    rec.last_heartbeat = Timestamp::now();
    write_record(&rec)?;
    Ok(true)
}

/// Cross-platform "is this PID alive right now" check, backed by
/// [`sysinfo`]. Unlike `kill(pid, 0)` this works on Windows too, and
/// distinguishes "PID exists" from "PID exists but is a zombie owned by a
/// different uid" cleanly.
pub fn is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    let target = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        // We only need the process-table presence check, not cmdline /
        // cpu / mem fields. `new()` opts out of every per-process probe.
        ProcessRefreshKind::new(),
    );
    sys.process(target).is_some()
}

/// Read every session file under [`sessions_dir`].
///
/// Files that fail to parse are silently skipped — a corrupt record
/// shouldn't blind a supervisor to its healthy siblings. Caller can
/// `prune_stale` to clean up the corrupt ones.
pub fn list_sessions() -> io::Result<Vec<SessionRecord>> {
    let dir = sessions_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut out: Vec<SessionRecord> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "session") {
            if let Ok(rec) = read_record(&p) {
                out.push(rec);
            }
        }
    }
    out.sort_by_key(|r| r.pid);
    Ok(out)
}

/// Sweep stale records and return the PIDs we deleted.
///
/// A record is stale when **either**:
///
/// - The PID is no longer alive (per [`is_alive`]) — kitty exited, the
///   user closed the window, the process crashed.
/// - The PID is alive but `last_heartbeat` is older than `max_idle` —
///   meaningful when the spawned CLI is supposed to call
///   [`touch_heartbeat`] on a known cadence and has gone silent.
///
/// Pass `Span::default()` (zero) for `max_idle` to disable the idle path
/// and only prune dead PIDs.
pub fn prune_stale(max_idle: Span) -> io::Result<Vec<u32>> {
    let now = Timestamp::now();
    let sessions = list_sessions()?;
    let mut pruned: Vec<u32> = Vec::new();

    for rec in sessions {
        let alive = is_alive(rec.pid);
        let idle_too_long = if max_idle.is_zero() {
            false
        } else {
            // jiff Span comparison: build the threshold timestamp and
            // check whether last_heartbeat predates it.
            match now.checked_sub(max_idle) {
                Ok(threshold) => rec.last_heartbeat < threshold,
                Err(_) => false,
            }
        };

        if !alive || idle_too_long {
            let path = rec.path();
            if fs::remove_file(&path).is_ok() {
                pruned.push(rec.pid);
            }
        }
    }
    Ok(pruned)
}

// ---- file I/O internals ---------------------------------------------------

fn write_record(rec: &SessionRecord) -> io::Result<()> {
    let dir = sessions_dir();
    fs::create_dir_all(&dir)?;
    let final_path = dir.join(format!("{}.session", rec.pid));
    let tmp_path = dir.join(format!("{}.session.tmp", rec.pid));

    let body = format!(
        "pid={}\ncli={}\ntitle={}\nhome_dir={}\nspawned_at={}\nlast_heartbeat={}\n",
        rec.pid,
        cli_to_str(rec.cli),
        escape(&rec.title),
        escape(&rec.home_dir.to_string_lossy()),
        rec.spawned_at,
        rec.last_heartbeat,
    );

    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

fn read_record(path: &Path) -> io::Result<SessionRecord> {
    let body = fs::read_to_string(path)?;
    let mut kv: HashMap<&str, String> = HashMap::new();
    for line in body.lines() {
        if let Some((k, v)) = line.split_once('=') {
            kv.insert(k, unescape(v));
        }
    }

    let get = |k: &str| -> io::Result<String> {
        kv.get(k).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("session file {} missing key `{}`", path.display(), k),
            )
        })
    };

    let pid: u32 = get("pid")?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad pid: {e}")))?;
    let cli = cli_from_str(&get("cli")?).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "unknown cli value")
    })?;
    let title = get("title")?;
    let home_dir = PathBuf::from(get("home_dir")?);
    let spawned_at: Timestamp = get("spawned_at")?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad spawned_at: {e}")))?;
    let last_heartbeat: Timestamp = get("last_heartbeat")?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad last_heartbeat: {e}")))?;

    Ok(SessionRecord {
        pid,
        cli,
        title,
        home_dir,
        spawned_at,
        last_heartbeat,
    })
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('=', "\\=")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('=') => out.push('='),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn cli_to_str(cli: CliKind) -> &'static str {
    match cli {
        CliKind::Codex => "codex",
        CliKind::Claude => "claude",
        CliKind::Gemini => "gemini",
        CliKind::Claw => "claw",
    }
}

fn cli_from_str(s: &str) -> Option<CliKind> {
    CliKind::parse(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Process-wide lock so the `FLEET_LAUNCHER_SESSIONS_DIR` env var is
    /// only mutated by one test at a time. Cargo runs tests in parallel
    /// by default; without this, two tests both calling `isolate()` race
    /// on the env var and one of them ends up reading the other's files.
    fn test_lock() -> &'static Mutex<()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
    }

    /// RAII guard returned by [`isolate`]. Holds the process-wide test
    /// lock until it drops, so the env var stays valid for the entire
    /// test body. Recover from panicked siblings by treating a poisoned
    /// mutex as a still-usable lock (the env var will be re-set anyway).
    struct Isolated {
        _guard: MutexGuard<'static, ()>,
    }

    fn isolate(name: &str) -> Isolated {
        let guard = test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "fleet-launcher-test-{}-{}",
            std::process::id(),
            name
        ));
        // SAFETY: the mutex above serializes env-var mutation across all
        // tests in this binary, which is the only correctness rule
        // `set_var` requires.
        unsafe {
            std::env::set_var("FLEET_LAUNCHER_SESSIONS_DIR", &dir);
        }
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Isolated { _guard: guard }
    }

    #[test]
    fn record_then_read_round_trip() {
        let _dir = isolate("round_trip");
        let spec = LaunchSpec {
            cli: CliKind::Codex,
            title: "alpha@example.com".into(),
            home_dir: PathBuf::from("/tmp/codex-fleet/alpha"),
            cwd: None,
            prompt: None,
            extra_args: vec![],
            extra_env: vec![],
        };
        let rec = record_session(&spec, 12345).unwrap();
        let listed = list_sessions().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], rec);
    }

    #[test]
    fn touch_heartbeat_updates_timestamp_only() {
        let _dir = isolate("touch");
        let spec = LaunchSpec {
            cli: CliKind::Claude,
            title: "t".into(),
            home_dir: PathBuf::from("/tmp/x"),
            cwd: None,
            prompt: None,
            extra_args: vec![],
            extra_env: vec![],
        };
        let before = record_session(&spec, 99).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(touch_heartbeat(99).unwrap());
        let after = list_sessions().unwrap().pop().unwrap();
        assert_eq!(after.spawned_at, before.spawned_at);
        assert!(after.last_heartbeat > before.last_heartbeat);
    }

    #[test]
    fn touch_heartbeat_returns_false_when_missing() {
        let _dir = isolate("touch_missing");
        assert!(!touch_heartbeat(7777).unwrap());
    }

    #[test]
    fn is_alive_true_for_self_false_for_dead_pid() {
        // Our own PID is always alive.
        assert!(is_alive(std::process::id()));
        // 0 is "any process in our group" on Unix and is never returned
        // as a process by sysinfo's process table — safe stand-in for
        // "definitely not running".
        assert!(!is_alive(0));
    }

    #[test]
    fn prune_stale_removes_dead_pid_records() {
        let _dir = isolate("prune");
        let spec = LaunchSpec {
            cli: CliKind::Codex,
            title: "ghost".into(),
            home_dir: PathBuf::from("/tmp/x"),
            cwd: None,
            prompt: None,
            extra_args: vec![],
            extra_env: vec![],
        };
        // Record a "session" for PID 0 (definitely dead) and our own PID
        // (definitely alive).
        record_session(&spec, 0).unwrap();
        record_session(&spec, std::process::id()).unwrap();

        let pruned = prune_stale(Span::new()).unwrap();
        assert_eq!(pruned, vec![0]);

        let remaining = list_sessions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].pid, std::process::id());
    }

    #[test]
    fn escape_round_trips_specials() {
        for s in &["plain", "with=equal", "with\nnewline", "back\\slash"] {
            let e = escape(s);
            assert_eq!(unescape(&e), *s);
        }
    }
}
