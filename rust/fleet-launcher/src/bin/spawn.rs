//! `fleet-spawn` — open a kitty window running codex / claude / gemini.
//!
//! Thin argv parser around `fleet_launcher::spawn_in_kitty`. Designed so a
//! supervisor (Rust binary, bash script, or another codex/claude pane) can
//! simply call:
//!
//! ```bash
//! fleet-spawn --cli codex \
//!   --account alpha@example.com \
//!   --home   /tmp/codex-fleet/alpha-example \
//!   --cwd    /home/user/repo \
//!   --env    CODEX_GUARD_BYPASS=1 \
//!   --env    CODEX_FLEET_AGENT_NAME=codex-alpha \
//!   --prompt "claim a ready task"
//! ```
//!
//! and get back a kitty window, detached, running the right CLI under the
//! right per-account home dir. Exit code is 0 + the kitty PID printed on
//! stdout on success; non-zero with a stderr message otherwise.

use std::path::PathBuf;
use std::process::ExitCode;

use fleet_launcher::{spawn_in_kitty, CliKind, LaunchSpec};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let spec = match parse_args(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("fleet-spawn: {e}");
            eprintln!("run with --help for usage.");
            return ExitCode::from(2);
        }
    };

    match spawn_in_kitty(&spec) {
        Ok(pid) => {
            println!("{pid}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fleet-spawn: failed to spawn kitty window: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Result<LaunchSpec, String> {
    let mut cli: Option<CliKind> = None;
    let mut title: Option<String> = None;
    let mut home: Option<PathBuf> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut prompt: Option<String> = None;
    let mut extra_args: Vec<String> = Vec::new();
    let mut extra_env: Vec<(String, String)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--cli" => {
                let v = take(args, &mut i, "--cli")?;
                cli = Some(
                    CliKind::parse(&v)
                        .ok_or_else(|| format!("unknown --cli value: {v} (codex|claude|gemini)"))?,
                );
            }
            "--account" | "--title" => {
                title = Some(take(args, &mut i, a)?);
            }
            "--home" => {
                home = Some(PathBuf::from(take(args, &mut i, "--home")?));
            }
            "--cwd" => {
                cwd = Some(PathBuf::from(take(args, &mut i, "--cwd")?));
            }
            "--prompt" => {
                prompt = Some(take(args, &mut i, "--prompt")?);
            }
            "--env" => {
                let v = take(args, &mut i, "--env")?;
                let (k, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--env expects KEY=VALUE, got: {v}"))?;
                extra_env.push((k.to_string(), val.to_string()));
            }
            "--arg" => {
                extra_args.push(take(args, &mut i, "--arg")?);
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {other}"));
            }
            other => {
                return Err(format!("unexpected positional arg: {other}"));
            }
        }
        i += 1;
    }

    let cli = cli.ok_or_else(|| "--cli is required".to_string())?;
    let home = home.ok_or_else(|| {
        format!("--home is required (target dir for {})", cli.home_env())
    })?;
    let title = title.unwrap_or_else(|| format!("fleet-{}", cli.binary()));

    Ok(LaunchSpec {
        cli,
        title,
        home_dir: home,
        cwd,
        prompt,
        extra_args,
        extra_env,
    })
}

fn take(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    let me = std::env::args().next().unwrap_or_else(|| "fleet-spawn".to_string());
    println!(
        "{me} — spawn a kitty window running codex / claude / gemini.\n\
\n\
USAGE\n  \
{me} --cli <codex|claude|gemini> --home <DIR> [options]\n\
\n\
REQUIRED\n  \
--cli codex|claude|gemini   which CLI to launch\n  \
--home DIR                  per-account home directory (becomes\n                              CODEX_HOME / CLAUDE_CONFIG_DIR /\n                              GEMINI_CLI_HOME depending on --cli)\n\
\n\
OPTIONAL\n  \
--account NAME              kitty window title (default: fleet-<cli>)\n  \
--title NAME                alias for --account\n  \
--cwd DIR                   working directory inside the window\n  \
--prompt TEXT               initial prompt (codex: positional arg;\n                              claude: positional after\n                              --dangerously-skip-permissions;\n                              gemini: piped over stdin)\n  \
--env KEY=VALUE             extra env to export (repeatable)\n  \
--arg STRING                extra CLI arg (repeatable, appended verbatim)\n  \
--help, -h                  show this help\n\
\n\
OUTPUT\n  \
On success: prints the kitty process PID to stdout, exits 0.\n  \
On failure: prints reason to stderr, exits non-zero.\n"
    );
}
