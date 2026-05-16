//! Spawn codex / claude / gemini / claw sessions in kitty windows.
//!
//! Ported (and trimmed hard) from `~/Documents/hcom/src/terminal.rs`. hcom's
//! 2300-line terminal layer covers a dozen terminals (kitty, wezterm, tmux,
//! iterm, gnome-terminal, ...), per-CLI dispatch tables, IPC sockets, PTY
//! wrappers, transcript capture, etc. We need exactly **one** capability:
//! "open a fresh kitty window, run `codex` (or `claude`, `gemini`, `claw`)
//! under the right `*_HOME` env, detach, return the PID". Everything else
//! is out of scope.
//!
//! ## Why a bash script wrapper?
//!
//! hcom learned the hard way that passing a long composite shell line as
//! `kitty bash -c "<...>"` arg-quotes inconsistently across kitty versions
//! (`terminal.rs:611-731` builds a script file precisely for this reason).
//! Writing a self-deleting `/tmp/fleet-launcher-<pid>-<rand>.sh` and running
//! `kitty --title <name> bash <script>` sidesteps every quoting edge case.
//!
//! ## Detach
//!
//! `setsid()` in `pre_exec` so the kitty process survives the parent — same
//! shape as hcom's `terminal.rs:1311-1320`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub mod heartbeat;
pub use heartbeat::{
    is_alive, list_sessions, prune_stale, record_session, touch_heartbeat, SessionRecord,
};

/// Which CLI to spawn inside the kitty window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliKind {
    /// `codex` — uses `CODEX_HOME` for per-account auth isolation.
    Codex,
    /// `claude` — uses `CLAUDE_CONFIG_DIR`. Spawned with
    /// `--dangerously-skip-permissions` because the fleet panes are
    /// autonomous and the codex-fleet contract pre-authorizes that flag
    /// (see `scripts/codex-fleet/claude-worker.sh`).
    Claude,
    /// `gemini` — uses `GEMINI_CLI_HOME`. Optional `GEMINI_SYSTEM_MD` env
    /// is honoured if the caller passes it through `extra_env`.
    Gemini,
    /// `claw` (ultraworkers/claw-code) — uses `CLAW_CONFIG_HOME` for
    /// per-account auth and config isolation. Spawned with
    /// `--dangerously-skip-permissions` for the same reason as Claude:
    /// fleet panes are autonomous workers.
    Claw,
}

impl CliKind {
    /// Binary name on PATH (`codex`, `claude`, `gemini`, `claw`).
    pub fn binary(self) -> &'static str {
        match self {
            CliKind::Codex => "codex",
            CliKind::Claude => "claude",
            CliKind::Gemini => "gemini",
            CliKind::Claw => "claw",
        }
    }

    /// Env var name that points at the CLI's per-account config / auth
    /// directory. Each CLI uses a different convention; matching hcom's
    /// `launcher.rs:279-281`.
    pub fn home_env(self) -> &'static str {
        match self {
            CliKind::Codex => "CODEX_HOME",
            CliKind::Claude => "CLAUDE_CONFIG_DIR",
            CliKind::Gemini => "GEMINI_CLI_HOME",
            CliKind::Claw => "CLAW_CONFIG_HOME",
        }
    }

    /// Parse from a CLI string (`"codex"`, `"claude"`, `"gemini"`). Used by
    /// the `fleet-spawn` binary's argv parser. Case-insensitive.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "codex" => Some(CliKind::Codex),
            "claude" | "claude-code" => Some(CliKind::Claude),
            "gemini" => Some(CliKind::Gemini),
            "claw" | "claw-code" => Some(CliKind::Claw),
            _ => None,
        }
    }
}

/// All inputs needed to launch one CLI inside a fresh kitty window.
///
/// Field meanings:
///
/// - `title` — kitty window title (`--title`). Surface this so an operator
///   can `wmctrl` / scrobble for the window later.
/// - `home_dir` — the directory the CLI's `*_HOME` env should point at.
///   Caller is responsible for creating it (e.g. `/tmp/codex-fleet/<aid>`)
///   and copying `auth.json` / `config.toml` in.
/// - `cwd` — working directory inside the kitty window. Defaults to the
///   parent's CWD when `None`.
/// - `prompt` — optional first user message. For `codex` it becomes a
///   positional arg (`codex "<prompt>"`); for `claude` it's appended after
///   `--dangerously-skip-permissions`; for `gemini` it's piped via stdin.
/// - `extra_args` — appended verbatim after the prompt.
/// - `extra_env` — KEY=VALUE pairs exported into the spawned shell before
///   the CLI runs. Use this for `GEMINI_SYSTEM_MD`, `CODEX_GUARD_BYPASS=1`,
///   account labels for telemetry, etc.
#[derive(Clone, Debug)]
pub struct LaunchSpec {
    pub cli: CliKind,
    pub title: String,
    pub home_dir: PathBuf,
    pub cwd: Option<PathBuf>,
    pub prompt: Option<String>,
    pub extra_args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
}

/// Spawn the given CLI inside a new kitty window. Returns the kitty
/// process PID.
///
/// The flow mirrors hcom's `terminal.rs::launch_terminal` background
/// branch:
///
/// 1. Build a self-deleting bash script with the env exports + cd + the
///    final CLI command line.
/// 2. `Command::new("kitty").args(["--title", title, "bash", script])`.
/// 3. `pre_exec(setsid)` so kitty outlives the caller process.
/// 4. Spawn, capture PID, return.
///
/// On `kitty` not being on PATH we fail fast with `NotFound` so the caller
/// can fall back to `tmux split-window` (the existing codex-fleet path).
pub fn spawn_in_kitty(spec: &LaunchSpec) -> io::Result<u32> {
    if which("kitty").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "kitty binary not on PATH — install kitty or fall back to tmux split-window",
        ));
    }

    let script_path = write_launch_script(spec)?;

    let mut cmd = Command::new("kitty");
    cmd.arg("--title").arg(&spec.title);
    cmd.arg("bash").arg(&script_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Detach so kitty survives the parent process. Same approach as
    // hcom/terminal.rs:1311-1320.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn()?;
    let pid = child.id();

    // Drop a session file so the supervisor / `fleet-sessions` tooling
    // can answer liveness + heartbeat queries later. A failure here
    // shouldn't block the spawn — the kitty window is already running
    // and the caller still gets the PID; just surface the error via
    // stderr so the operator notices.
    if let Err(e) = heartbeat::record_session(spec, pid) {
        eprintln!(
            "fleet-launcher: spawned pid {pid} but failed to record session file: {e}"
        );
    }

    Ok(pid)
}

/// Build the bash script the kitty window will run.
///
/// Layout (mirroring hcom/terminal.rs:611-731 in spirit, much shorter):
///
/// ```bash
/// #!/bin/bash
/// printf "\033]0;fleet: starting <cli>...\007"
/// export CODEX_HOME='/tmp/codex-fleet/<id>'
/// export ...extra_env...
/// cd '<cwd>'
/// exec codex '<prompt>' <extra_args>
/// ```
///
/// Script self-deletes on exec via the `rm -f` line below the `exec`
/// (which never runs because exec replaces the process — but a panic
/// before exec leaves the file behind, so we add a trap).
pub fn write_launch_script(spec: &LaunchSpec) -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("fleet-launcher");
    std::fs::create_dir_all(&dir)?;
    let script_path = dir.join(format!(
        "spawn-{}-{}.sh",
        std::process::id(),
        // Cheap nonce — enough to avoid collisions when one parent spawns
        // multiple kitty windows in the same millisecond.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let mut envs: BTreeMap<String, String> = BTreeMap::new();
    envs.insert(
        spec.cli.home_env().to_string(),
        spec.home_dir.to_string_lossy().to_string(),
    );
    for (k, v) in &spec.extra_env {
        envs.insert(k.clone(), v.clone());
    }

    let mut f = std::fs::File::create(&script_path)?;
    writeln!(f, "#!/bin/bash")?;
    writeln!(f, "set -u")?;
    // Title escape so the operator can spot the window in their WM.
    writeln!(
        f,
        "printf '\\033]0;fleet: {} ({})\\007'",
        shell_quote(spec.cli.binary()),
        shell_quote(&spec.title)
    )?;
    // Self-cleanup on any exit path. Kept before the `exec` so even an
    // early bash error removes the temp file.
    writeln!(
        f,
        "trap 'rm -f {}' EXIT",
        shell_quote(&script_path.to_string_lossy())
    )?;

    for (k, v) in &envs {
        writeln!(f, "export {}={}", k, shell_quote(v))?;
    }

    if let Some(cwd) = &spec.cwd {
        writeln!(f, "cd {}", shell_quote(&cwd.to_string_lossy()))?;
    }

    let cmd_line = build_command_line(spec);
    // Use exec so the bash wrapper PID is replaced by the CLI PID — kitty
    // close-window detection then matches the CLI exit, not the wrapper's.
    writeln!(f, "exec {}", cmd_line)?;

    f.sync_all()?;
    drop(f);

    let mut perms = std::fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms)?;
    Ok(script_path)
}

/// Render the CLI command line (binary + per-CLI prompt convention +
/// extra args).
///
/// Per-CLI conventions (mirrors hcom's `launcher.rs::build_*_command`):
///
/// - **Codex**: `codex "<prompt>" <extra...>` — prompt is a positional arg
///   (matches `scripts/codex-fleet/up.sh::pane_cmd=`).
/// - **Claude**: `claude --dangerously-skip-permissions "<prompt>" <extra...>`
///   — bypass flag matches `scripts/codex-fleet/claude-worker.sh`.
/// - **Gemini**: `printf '%s' "<prompt>" | gemini <extra...>` — gemini's
///   CLI takes the first prompt over stdin, not as a positional.
/// - **Claw**: `claw "<prompt>" <extra...>` — same shape as Codex; claw's
///   argv parser routes a bare positional through the `prompt` subcommand
///   (see `claw-code/rust/crates/rusty-claude-cli/src/main.rs::parse_args`).
///   The permission-bypass flag is intentionally NOT injected here —
///   callers wanting autonomous workers should pass it via `extra_args`,
///   so the choice is explicit at the supervisor layer.
fn build_command_line(spec: &LaunchSpec) -> String {
    let bin = spec.cli.binary();
    let extras: Vec<String> = spec.extra_args.iter().map(|a| shell_quote(a)).collect();
    let extras_joined = if extras.is_empty() {
        String::new()
    } else {
        format!(" {}", extras.join(" "))
    };

    match (spec.cli, spec.prompt.as_deref()) {
        (CliKind::Codex, Some(p)) => format!("{} {}{}", bin, shell_quote(p), extras_joined),
        (CliKind::Codex, None) => format!("{}{}", bin, extras_joined),
        (CliKind::Claude, Some(p)) => format!(
            "{} --dangerously-skip-permissions {}{}",
            bin,
            shell_quote(p),
            extras_joined
        ),
        (CliKind::Claude, None) => {
            format!("{} --dangerously-skip-permissions{}", bin, extras_joined)
        }
        (CliKind::Gemini, Some(p)) => format!(
            "printf '%s' {} | {}{}",
            shell_quote(p),
            bin,
            extras_joined
        ),
        (CliKind::Gemini, None) => format!("{}{}", bin, extras_joined),
        (CliKind::Claw, Some(p)) => format!("{} {}{}", bin, shell_quote(p), extras_joined),
        (CliKind::Claw, None) => format!("{}{}", bin, extras_joined),
    }
}

/// Bash-safe single-quoting. Identical contract to hcom's
/// `terminal.rs::shell_quote` so the env exports look identical to a
/// reader who already knows hcom.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/_-.=:,@".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Find a binary on PATH — minimal `which` so we don't pull in a crate.
fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable<P: AsRef<Path>>(p: P) -> bool {
    let p = p.as_ref();
    let Ok(meta) = std::fs::metadata(p) else {
        return false;
    };
    meta.is_file() && (meta.permissions().mode() & 0o111 != 0) && p.file_name() != Some(OsStr::new(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_safe_chars_passthrough() {
        assert_eq!(shell_quote("codex"), "codex");
        assert_eq!(shell_quote("/tmp/codex-fleet/x"), "/tmp/codex-fleet/x");
        assert_eq!(shell_quote("admin@example.com"), "admin@example.com");
    }

    #[test]
    fn shell_quote_escapes_singles_and_spaces() {
        assert_eq!(shell_quote("hello world"), "'hello world'");
        assert_eq!(shell_quote("it's a trap"), "'it'\\''s a trap'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn cli_home_envs_match_each_cli() {
        assert_eq!(CliKind::Codex.home_env(), "CODEX_HOME");
        assert_eq!(CliKind::Claude.home_env(), "CLAUDE_CONFIG_DIR");
        assert_eq!(CliKind::Gemini.home_env(), "GEMINI_CLI_HOME");
        assert_eq!(CliKind::Claw.home_env(), "CLAW_CONFIG_HOME");
    }

    #[test]
    fn cli_parse_accepts_aliases() {
        assert_eq!(CliKind::parse("codex"), Some(CliKind::Codex));
        assert_eq!(CliKind::parse("CLAUDE"), Some(CliKind::Claude));
        assert_eq!(CliKind::parse("claude-code"), Some(CliKind::Claude));
        assert_eq!(CliKind::parse("Gemini"), Some(CliKind::Gemini));
        assert_eq!(CliKind::parse("claw"), Some(CliKind::Claw));
        assert_eq!(CliKind::parse("Claw-Code"), Some(CliKind::Claw));
        assert_eq!(CliKind::parse("opencode"), None);
    }

    #[test]
    fn build_command_line_per_cli_conventions() {
        // Codex — positional prompt. `--flag` is all safe chars so
        // shell_quote leaves it bare; an arg with a space gets quoted.
        let s = LaunchSpec {
            cli: CliKind::Codex,
            title: "t".into(),
            home_dir: "/tmp/x".into(),
            cwd: None,
            prompt: Some("hello".into()),
            extra_args: vec!["--flag".into(), "with space".into()],
            extra_env: vec![],
        };
        assert_eq!(build_command_line(&s), "codex hello --flag 'with space'");

        // Claude — bypass flag, then prompt
        let s = LaunchSpec {
            cli: CliKind::Claude,
            prompt: Some("review this".into()),
            extra_args: vec![],
            ..s.clone()
        };
        assert_eq!(
            build_command_line(&s),
            "claude --dangerously-skip-permissions 'review this'"
        );

        // Gemini — piped over stdin
        let s = LaunchSpec {
            cli: CliKind::Gemini,
            prompt: Some("hi".into()),
            extra_args: vec![],
            ..s.clone()
        };
        assert_eq!(build_command_line(&s), "printf '%s' hi | gemini");

        // Claw — same plain positional-prompt shape as Codex; no
        // permission-bypass flag is auto-injected (callers add it via
        // extra_args if they want autonomous workers).
        let s = LaunchSpec {
            cli: CliKind::Claw,
            prompt: Some("ship it".into()),
            extra_args: vec![],
            ..s.clone()
        };
        assert_eq!(build_command_line(&s), "claw 'ship it'");

        // Claw — no prompt, no auto-injected flags
        let s = LaunchSpec {
            cli: CliKind::Claw,
            prompt: None,
            extra_args: vec![],
            ..s.clone()
        };
        assert_eq!(build_command_line(&s), "claw");

        // No prompt, with extras
        let s = LaunchSpec {
            cli: CliKind::Codex,
            prompt: None,
            extra_args: vec!["--continue".into()],
            ..s.clone()
        };
        assert_eq!(build_command_line(&s), "codex --continue");
    }

    #[test]
    fn write_launch_script_emits_exports_cd_and_exec() {
        let dir = std::env::temp_dir().join(format!("fleet-launcher-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = LaunchSpec {
            cli: CliKind::Codex,
            title: "alpha@x.com".into(),
            home_dir: dir.clone(),
            cwd: Some(std::env::temp_dir()),
            prompt: Some("first prompt".into()),
            extra_args: vec![],
            extra_env: vec![("CODEX_GUARD_BYPASS".into(), "1".into())],
        };

        let path = write_launch_script(&spec).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("export CODEX_HOME="), "missing CODEX_HOME export:\n{body}");
        assert!(body.contains("export CODEX_GUARD_BYPASS=1"), "missing extra env:\n{body}");
        assert!(body.contains("cd "), "missing cd line:\n{body}");
        assert!(body.contains("exec codex 'first prompt'"), "missing exec line:\n{body}");
        std::fs::remove_file(&path).ok();
    }
}
