use std::env;
use std::process::{Command, ExitCode};

use fleet_layout::Applier;

#[path = "../preset.rs"]
mod preset;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    target: String,
    preset: String,
    header_rows: u16,
    workers: u16,
    header_cmd: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("fleet-apply-layout: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args = match parse_args(env::args().skip(1))? {
        ParseResult::Help => {
            print_help();
            return Ok(());
        }
        ParseResult::Args(args) => args,
    };
    if args.preset != preset::OVERVIEW_HEADER_TILE {
        return Err(format!("unknown preset '{}'", args.preset));
    }
    if args.workers == 0 {
        return Err("--workers must be greater than 0".to_string());
    }

    let (session, window) = parse_target(&args.target)?;
    let layout = preset::overview_header_tile(args.header_rows, args.workers);
    let pane_ids = Applier::new(session, window)
        .apply(&layout)
        .map_err(|err| err.to_string())?;

    if args.header_rows > 0 {
        if let (Some(header_pane), Some(header_cmd)) = (pane_ids.first(), args.header_cmd.as_ref())
        {
            run_tmux(["send-keys", "-t", header_pane, header_cmd, "C-m"])?;
            run_tmux([
                "set-option",
                "-t",
                header_pane,
                "-p",
                "@panel",
                "[codex-fleet-tab-strip]",
            ])?;
        }
    }

    for pane_id in pane_ids {
        println!("{pane_id}");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseResult {
    Help,
    Args(Args),
}

fn parse_args<I>(args: I) -> Result<ParseResult, String>
where
    I: IntoIterator<Item = String>,
{
    let mut target = None;
    let mut preset = None;
    let mut header_rows = 1;
    let mut workers = 8;
    let mut header_cmd = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseResult::Help),
            "--target" => target = Some(take_value("--target", &mut args)?),
            "--preset" => preset = Some(take_value("--preset", &mut args)?),
            "--header-rows" => {
                header_rows = parse_u16("--header-rows", &take_value("--header-rows", &mut args)?)?
            }
            "--workers" => workers = parse_u16("--workers", &take_value("--workers", &mut args)?)?,
            "--header-cmd" => header_cmd = Some(take_value("--header-cmd", &mut args)?),
            _ => return Err(format!("unknown flag '{arg}'")),
        }
    }

    Ok(ParseResult::Args(Args {
        target: target.ok_or_else(|| "--target is required".to_string())?,
        preset: preset.ok_or_else(|| "--preset is required".to_string())?,
        header_rows,
        workers,
        header_cmd,
    }))
}

fn take_value(flag: &str, args: &mut impl Iterator<Item = String>) -> Result<String, String> {
    args.next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_u16(flag: &str, value: &str) -> Result<u16, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} must be an integer, got '{value}'"))
}

fn parse_target(target: &str) -> Result<(&str, &str), String> {
    let (session, window) = target
        .split_once(':')
        .ok_or_else(|| "--target must use <session:window>".to_string())?;
    if session.is_empty() || window.is_empty() {
        return Err("--target must use <session:window>".to_string());
    }
    Ok((session, window))
}

fn run_tmux<const N: usize>(args: [&str; N]) -> Result<(), String> {
    let status = Command::new("tmux")
        .args(args)
        .status()
        .map_err(|err| format!("failed to run tmux: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("tmux exited with {status}"))
    }
}

fn print_help() {
    println!(
        "\
fleet-apply-layout

USAGE:
    fleet-apply-layout --target <session:window> --preset overview-header-tile [OPTIONS]

OPTIONS:
    --target <session:window>      tmux target, e.g. codex-fleet:overview
    --preset overview-header-tile  layout preset to apply
    --header-rows <N>              header rows; default 1, 0 skips header
    --workers <N>                  worker panes; default 8
    --header-cmd <shell-cmd>       command sent to the header pane after layout
    -h, --help                     show this help
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_flags_and_defaults() {
        assert_eq!(
            parse_args([
                "--target".to_string(),
                "codex-fleet:overview".to_string(),
                "--preset".to_string(),
                "overview-header-tile".to_string(),
            ])
            .unwrap(),
            ParseResult::Args(Args {
                target: "codex-fleet:overview".to_string(),
                preset: "overview-header-tile".to_string(),
                header_rows: 1,
                workers: 8,
                header_cmd: None,
            })
        );
    }

    #[test]
    fn parses_optional_values() {
        assert_eq!(
            parse_args([
                "--target".to_string(),
                "codex-fleet:overview".to_string(),
                "--preset".to_string(),
                "overview-header-tile".to_string(),
                "--header-rows".to_string(),
                "0".to_string(),
                "--workers".to_string(),
                "4".to_string(),
                "--header-cmd".to_string(),
                "fleet-tab-strip".to_string(),
            ])
            .unwrap(),
            ParseResult::Args(Args {
                target: "codex-fleet:overview".to_string(),
                preset: "overview-header-tile".to_string(),
                header_rows: 0,
                workers: 4,
                header_cmd: Some("fleet-tab-strip".to_string()),
            })
        );
    }

    #[test]
    fn rejects_bad_target_shape() {
        assert!(parse_target("codex-fleet").is_err());
        assert!(parse_target(":overview").is_err());
        assert!(parse_target("codex-fleet:").is_err());
    }
}
