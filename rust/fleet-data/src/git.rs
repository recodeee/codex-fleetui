//! Cross-lane PR awareness via `gh pr list` + `git merge-base`.
//!
//! The fleet's `task_ready_for_agent` queue treats every candidate task as
//! independent. In practice, lane A may produce a file that lane B's task
//! depends on — and when A's PR isn't yet merged into B's base, B will pick
//! up the work and immediately rediscover the dependency at PR time. This
//! module is the data-layer half of the fix: it answers "which open PRs
//! touch which files, and is any given branch already past PR N's head."
//!
//! The colony-side consumer (`readyScopeOverlapWarnings` in
//! `colony/ready-queue.ts`) emits a `merge_pending` warning in `next_action`
//! when a candidate's file scope intersects an open PR's fileset that the
//! candidate's base has not yet absorbed. Per `task_claim_file`'s docstring,
//! Colony's coordination is "soft — never blocks writes" — so this surfaces
//! as a warning that the autopilot deprioritizes, not a hard block.
//!
//! ## Failure posture
//!
//! Mirrors [`crate::accounts`]: a missing `gh` binary, an unauthenticated
//! CLI, or a network failure all return `Ok(vec![])` from
//! [`open_prs_with_files`]. A dashboard without `gh` access should render
//! no merge-pending warnings, not crash. For [`branch_contains_pr`], a
//! lookup failure conservatively returns `Ok(false)` — "I cannot prove
//! this PR is merged" is the safe default that produces a warning.

use serde::{Deserialize, Serialize};

/// One open PR's metadata + fileset, as far as the ready-queue cares.
///
/// The fileset is *paths only* — `additions` / `deletions` counts from
/// `gh pr list --json files` are dropped on parse, because cross-lane
/// prereq detection only needs the set of paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrFileset {
    pub number: u64,
    pub head_ref: String,
    pub base_ref: String,
    pub files: Vec<String>,
}

impl PrFileset {
    /// `true` if any of this PR's files appears in the candidate scope.
    /// Both sides are matched exact-path; directory globbing is the
    /// caller's job (the colony side already normalizes to file paths).
    pub fn overlaps(&self, scope: &[String]) -> bool {
        self.files.iter().any(|f| scope.iter().any(|s| s == f))
    }
}

/// Raw `gh pr list --json` shape — `files` is an array of `{path, additions, …}`.
#[derive(Deserialize)]
struct RawPr {
    number: u64,
    #[serde(rename = "headRefName")]
    head_ref: String,
    #[serde(rename = "baseRefName")]
    base_ref: String,
    files: Vec<RawFile>,
}

#[derive(Deserialize)]
struct RawFile {
    path: String,
}

/// Parse `gh pr list --json number,headRefName,baseRefName,files` stdout
/// into typed [`PrFileset`] rows. Pure — feed it captured JSON in tests.
///
/// A malformed payload returns an empty Vec rather than an error; same
/// posture as [`crate::accounts::parse`] — the dashboard's "no information"
/// path is an empty fleet, not a panic.
pub fn parse(stdout: &str) -> Vec<PrFileset> {
    let raws: Vec<RawPr> = serde_json::from_str(stdout).unwrap_or_default();
    raws.into_iter()
        .map(|r| PrFileset {
            number: r.number,
            head_ref: r.head_ref,
            base_ref: r.base_ref,
            files: r.files.into_iter().map(|f| f.path).collect(),
        })
        .collect()
}

/// `true` when `branch` is at or past `pr_head` — i.e. the PR's head commit
/// is already an ancestor of `branch`. Wraps
/// `git merge-base --is-ancestor <pr_head> <branch>` (exit 0 = ancestor).
///
/// Returns `Ok(false)` on any failure path (unknown ref, no git on PATH) —
/// for cross-lane warnings, "I cannot prove this PR is merged" is the safe
/// default that produces a warning rather than silently suppressing it.
pub fn branch_contains_pr(branch: &str, pr_head: &str) -> std::io::Result<bool> {
    let status = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", pr_head, branch])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Shell out to `gh pr list` and parse the result.
///
/// Returns `Ok(vec![])` when `gh` is missing, unauthenticated, or returns a
/// non-zero exit — a dashboard without `gh` access renders no warnings, not
/// a crash. `--limit 100` is enough for any plausible fleet; a 100-PR
/// backlog is itself a signal worth surfacing elsewhere.
pub fn open_prs_with_files() -> std::io::Result<Vec<PrFileset>> {
    let out = match std::process::Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--json",
            "number,headRefName,baseRefName,files",
            "--limit",
            "100",
        ])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Ok(Vec::new()),
    };
    if !out.status.success() {
        return Ok(Vec::new());
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

fn cache() -> &'static crate::cache::TtlCache<Vec<PrFileset>> {
    static CACHE: std::sync::OnceLock<crate::cache::TtlCache<Vec<PrFileset>>> =
        std::sync::OnceLock::new();
    // 45s TTL: `gh pr list` is rate-limited (5000/hr authenticated).
    // Four dashboard binaries each refreshing once per TTL window peak at
    // ~320 calls/hr — two orders of magnitude under the cap, while still
    // surfacing a freshly-opened PR within ~one tick of a minute.
    CACHE.get_or_init(|| crate::cache::TtlCache::new(std::time::Duration::from_secs(45)))
}

/// Cached variant of [`open_prs_with_files`]. The fleet's dashboard tick
/// is 250 ms; calling the raw fn every tick would burn ~14k `gh`
/// invocations per hour per binary and trip GitHub's rate limit. The 45 s
/// TTL is tuned so a freshly-opened PR shows up within ~one minute and
/// readers stay cheap.
pub fn load_live_cached() -> std::io::Result<Vec<PrFileset>> {
    cache().get_or_refresh(open_prs_with_files)
}

/// Drop the cached PR list so the next [`load_live_cached`] re-shells.
/// Useful after observing a `gh pr merge` / `gh pr close` locally, so the
/// dashboard's warnings clear immediately rather than after the TTL.
pub fn invalidate_cache() {
    cache().invalidate();
}

/// Find every open PR whose fileset intersects `scope` and that has not yet
/// been merged into `branch`'s history. This is the question
/// `readyScopeOverlapWarnings` actually asks: "if lane B claims these
/// files, what un-merged PRs already touch them and would conflict on
/// rebase?"
///
/// Returns `Ok(vec![])` when nothing matches, including the trivial "no
/// open PRs" / "no `gh` on PATH" cases. The caller (colony) folds each
/// returned PR into a `merge_pending` warning on the task's `next_action`.
pub fn merge_pending_overlap(
    branch: &str,
    scope: &[String],
) -> std::io::Result<Vec<PrFileset>> {
    let prs = load_live_cached()?;
    let mut out = Vec::new();
    for pr in prs {
        if !pr.overlaps(scope) {
            continue;
        }
        // Skip PRs whose head is already an ancestor of `branch` — they're
        // either merged-and-pulled or this lane was cut from a tip that
        // already contained them. Either way, no merge-pending conflict.
        if branch_contains_pr(branch, &pr.head_ref).unwrap_or(false) {
            continue;
        }
        out.push(pr);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
  {
    "number": 24,
    "headRefName": "agent/claude/fleet-state-live-data-2026-05-14-20-19",
    "baseRefName": "main",
    "files": [
      {"path": "rust/fleet-state/src/main.rs", "additions": 247, "deletions": 40},
      {"path": "rust/fleet-data/src/fleet.rs", "additions": 419, "deletions": 0}
    ]
  },
  {
    "number": 25,
    "headRefName": "agent/codex/overlays-phase5-sub-1-spotlight",
    "baseRefName": "main",
    "files": [
      {"path": "rust/fleet-ui/src/overlay/spotlight.rs", "additions": 312, "deletions": 0}
    ]
  }
]"#;

    #[test]
    fn parses_pr_list_json() {
        let prs = parse(FIXTURE);
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].number, 24);
        assert_eq!(
            prs[0].head_ref,
            "agent/claude/fleet-state-live-data-2026-05-14-20-19"
        );
        assert_eq!(prs[0].base_ref, "main");
        assert_eq!(
            prs[0].files,
            vec![
                "rust/fleet-state/src/main.rs".to_string(),
                "rust/fleet-data/src/fleet.rs".to_string(),
            ]
        );
        assert_eq!(prs[1].number, 25);
        assert_eq!(prs[1].files.len(), 1);
    }

    #[test]
    fn malformed_json_yields_empty_vec() {
        assert!(parse("not json at all").is_empty());
        assert!(parse("").is_empty());
        assert!(parse("{}").is_empty(), "object, not array");
    }

    #[test]
    fn missing_files_field_drops_the_row() {
        // `files` is required — a row without it fails the whole array parse,
        // which unwrap_or_default turns into an empty Vec. That's intentional:
        // a partial gh response shouldn't half-populate the warnings list.
        let prs =
            parse(r#"[{"number": 1, "headRefName": "x", "baseRefName": "main"}]"#);
        assert!(prs.is_empty());
    }

    fn pr(number: u64, files: &[&str]) -> PrFileset {
        PrFileset {
            number,
            head_ref: format!("agent/x/{number}"),
            base_ref: "main".into(),
            files: files.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn overlap_matches_exact_path() {
        let p = pr(
            1,
            &["rust/fleet-data/src/fleet.rs", "rust/fleet-data/src/tmux.rs"],
        );
        assert!(p.overlaps(&["rust/fleet-data/src/fleet.rs".to_string()]));
        assert!(p.overlaps(&[
            "unrelated.txt".to_string(),
            "rust/fleet-data/src/tmux.rs".to_string()
        ]));
        assert!(!p.overlaps(&["rust/fleet-state/src/main.rs".to_string()]));
        assert!(!p.overlaps(&[]));
    }

    #[test]
    fn empty_pr_files_never_overlaps() {
        let p = pr(2, &[]);
        assert!(!p.overlaps(&["any.rs".to_string()]));
    }

    #[test]
    fn overlap_is_directional() {
        // Path strings must match exactly — `fleet-data/src/fleet.rs` doesn't
        // overlap `rust/fleet-data/src/fleet.rs`. Directory-prefix matching
        // is the caller's job; the colony side normalizes both before calling.
        let p = pr(3, &["rust/fleet-data/src/fleet.rs"]);
        assert!(!p.overlaps(&["fleet-data/src/fleet.rs".to_string()]));
    }
}
