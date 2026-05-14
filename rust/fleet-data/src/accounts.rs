//! Typed parser for `codex-auth list` output.
//!
//! Replaces the awk/sed regex scattered across `cap-probe.sh`,
//! `cap-swap-daemon.sh`, and `fleet-tick.sh`'s account-discovery block.
//! Sample input line (whitespace-flexible):
//!
//! ```text
//!   admin@magnoliavilag.hu  type=ChatGPT seat (Business)  5h=12%  weekly=62%
//! * admin@kollarrobert.sk   type=ChatGPT seat (Business)  5h=6%   weekly=54%
//! ```
//!
//! Leading `*` (or `>`) marks the current account.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub email: String,
    pub five_h_pct: u8,
    pub weekly_pct: u8,
    pub is_current: bool,
}

/// Parse the entire stdout of `codex-auth list` into a vector of [`Account`].
///
/// Lines that don't match the expected shape are silently skipped (matches
/// the bash `awk` lenience). Each returned account has both `5h=` and
/// `weekly=` values; if either is missing the line is dropped.
pub fn parse(stdout: &str) -> Vec<Account> {
    stdout.lines().filter_map(parse_line).collect()
}

fn parse_line(raw: &str) -> Option<Account> {
    let trimmed = raw.trim_start();
    let (current, rest) = match trimmed.chars().next() {
        Some('*') | Some('>') => (true, trimmed[1..].trim_start()),
        _ => (false, trimmed),
    };
    let email = extract_email(rest)?;
    let five_h_pct = extract_percent(rest, "5h=")?;
    let weekly_pct = extract_percent(rest, "weekly=")?;
    Some(Account {
        email,
        five_h_pct,
        weekly_pct,
        is_current: current,
    })
}

/// Very small extractor: first `<local>@<host>.<tld>` token in the line.
fn extract_email(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        if c == b'@' {
            let local_start = bytes[..i]
                .iter()
                .rposition(|&b| b.is_ascii_whitespace() || b == b'>' || b == b'*')
                .map(|p| p + 1)
                .unwrap_or(0);
            let mut end = i + 1;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            let candidate = &s[local_start..end];
            // require at least one dot in the host portion
            if candidate[i - local_start..].contains('.') {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

/// Find `<key>NN%` (e.g. `5h=12%`) and return the integer percent.
fn extract_percent(s: &str, key: &str) -> Option<u8> {
    let idx = s.find(key)?;
    let after = &s[idx + key.len()..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &after[digits.len()..];
    if !rest.starts_with('%') {
        return None;
    }
    digits.parse().ok()
}

/// Convenience runner: shells out to `codex-auth list`, parses stdout. The
/// dashboards keep calling this on a tick; the parse cost is negligible
/// next to the subprocess spawn.
pub fn load_live() -> std::io::Result<Vec<Account>> {
    let output = std::process::Command::new("codex-auth").arg("list").output()?;
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

fn cache() -> &'static crate::cache::TtlCache<Vec<Account>> {
    static CACHE: std::sync::OnceLock<crate::cache::TtlCache<Vec<Account>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| crate::cache::TtlCache::new(std::time::Duration::from_secs(5)))
}

/// Cached variant of [`load_live`]. Dashboards on a 250 ms tick should call
/// this instead — the `codex-auth list` subprocess is the most expensive
/// part of a tick (account list shifts on the order of seconds, not frames),
/// so a 5 s TTL keeps the UI responsive without burning a fork per frame.
///
/// Cache is process-local; the four dashboard binaries each maintain their
/// own. Call [`invalidate_cache`] after an operation that mutates account
/// state (login/logout/swap) to force a refetch on the next read.
pub fn load_live_cached() -> std::io::Result<Vec<Account>> {
    cache().get_or_refresh(load_live)
}

/// Drop the cached account list so the next [`load_live_cached`] re-shells.
pub fn invalidate_cache() {
    cache().invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
*  admin@kollarrobert.sk  type=ChatGPT seat (Business)  5h=12%  weekly=62%
   admin@magnoliavilag.hu  type=ChatGPT seat (Business)  5h=6%  weekly=54%
   admin@mite.hu  type=ChatGPT seat (Business)  5h=0%  weekly=47%
   admin@pipacsclub.hu  type=ChatGPT seat (Business)  5h=100%  weekly=69%
not-a-line-at-all
   admin@zazrifka.sk  type=ChatGPT seat (Business)  5h=28%  weekly=50%
";

    #[test]
    fn parses_real_world_block() {
        let accounts = parse(FIXTURE);
        assert_eq!(accounts.len(), 5, "{:#?}", accounts);
        assert_eq!(accounts[0].email, "admin@kollarrobert.sk");
        assert!(accounts[0].is_current);
        assert_eq!(accounts[0].five_h_pct, 12);
        assert_eq!(accounts[0].weekly_pct, 62);
        assert!(!accounts[1].is_current);
        assert_eq!(accounts[3].five_h_pct, 100);
    }

    #[test]
    fn drops_lines_without_email() {
        let accounts = parse("garbage line\n  another  ");
        assert!(accounts.is_empty());
    }

    #[test]
    fn drops_lines_missing_a_percent() {
        let accounts = parse("  foo@bar.com  5h=10%  weekly=??\n");
        assert!(accounts.is_empty(), "missing weekly% must drop the line");
    }
}
