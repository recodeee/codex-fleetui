//! Reader for the per-agent quality scores produced by the
//! `scripts/codex-fleet/score-merged-pr.sh` pipeline.
//!
//! The scorer (a Python script invoked post-merge or on a cron) compares
//! a merged PR's diff against its plan's acceptance criteria and writes
//! a per-agent score to `/tmp/claude-viz/fleet-quality-scores.json`.
//! This module is the read path the dashboards use to render that score
//! as the third rail next to WEEKLY · 5H.
//!
//! ## Schema
//!
//! ```json
//! {
//!   "generated_at": "2026-05-14T22:05:00Z",
//!   "scores": {
//!     "<agent-id>": {
//!       "score": 87,
//!       "agent_id": "claude",
//!       "pr_number": 30,
//!       "pr_title": "...",
//!       "branch": "agent/claude/...",
//!       "plan_slug": null,
//!       "criteria_met": ["..."],
//!       "criteria_missed": [],
//!       "reasoning": "...",
//!       "scored_at": "2026-05-14T19:35:00Z"
//!     }
//!   }
//! }
//! ```
//!
//! Quality scores are **advisory only** — they do not feed routing or
//! claim decisions. The score is what the LLM judges, not what the test
//! suite proves; treat low scores as a "look at this" signal, not a fail.
//!
//! ## Failure posture
//!
//! Mirrors [`crate::accounts`]: a missing file or malformed JSON returns
//! `Ok(empty)`. A dashboard that's never run the scorer should render no
//! quality rail (all rows show `quality: None`), not crash.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Per-agent quality score row, as the scorer writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityScore {
    /// 0–100 where 100 means every criterion is demonstrably met. `None`
    /// when the scorer found no acceptance criteria to grade against
    /// (e.g. a hotfix PR with no associated plan).
    pub score: Option<u8>,
    pub agent_id: String,
    pub pr_number: u64,
    pub pr_title: String,
    pub branch: String,
    /// Plan slug the criteria were lifted from; `None` when no plan was
    /// found for the PR's branch.
    pub plan_slug: Option<String>,
    pub criteria_met: Vec<String>,
    pub criteria_missed: Vec<String>,
    pub reasoning: String,
    /// RFC 3339 timestamp the score was generated. The renderer uses this
    /// only as a tiebreaker — when an agent has multiple historical PRs,
    /// the freshest score wins.
    pub scored_at: String,
}

/// Full scores file shape. Top-level `generated_at` is when the scorer
/// last ran; `scores` is keyed by agent-id (`claude`, `codex-magnolia`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScoresFile {
    pub generated_at: Option<String>,
    #[serde(default)]
    pub scores: HashMap<String, QualityScore>,
}

impl ScoresFile {
    /// Lookup the most recent score for `agent_id`. Returns `None` when
    /// the agent has never been scored.
    pub fn for_agent(&self, agent_id: &str) -> Option<&QualityScore> {
        self.scores.get(agent_id)
    }
}

/// Default JSON path. Mirrors the convention already established by
/// `tab_strip.rs` reading `/tmp/claude-viz/fleet-tab-counters.json` — a
/// runtime cache directory the dashboards agree on, kept out of git.
pub fn default_path() -> PathBuf {
    PathBuf::from("/tmp/claude-viz/fleet-quality-scores.json")
}

/// Parse a JSON string into a [`ScoresFile`]. Malformed input returns an
/// empty file (same "no information" posture as [`crate::accounts::parse`])
/// rather than an error — a dashboard should render a blank rail, not
/// crash, when the scorer hasn't run or wrote junk.
pub fn parse(stdout: &str) -> ScoresFile {
    serde_json::from_str(stdout).unwrap_or_default()
}

/// Read `path` and parse. Returns an empty [`ScoresFile`] when the file
/// doesn't exist; propagates `io::Error` for any other I/O failure
/// (permission denied, etc.) so the caller can surface it.
pub fn load_from(path: &Path) -> std::io::Result<ScoresFile> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(parse(&s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ScoresFile::default()),
        Err(e) => Err(e),
    }
}

fn cache() -> &'static crate::cache::TtlCache<ScoresFile> {
    static CACHE: std::sync::OnceLock<crate::cache::TtlCache<ScoresFile>> =
        std::sync::OnceLock::new();
    // 5s TTL — scores update at PR-merge cadence (seconds-to-minutes apart),
    // so a short TTL keeps the dashboard fresh without re-reading a file on
    // every 250 ms frame.
    CACHE.get_or_init(|| crate::cache::TtlCache::new(std::time::Duration::from_secs(5)))
}

/// Cached load from [`default_path`]. Dashboards on a 250 ms tick should
/// call this rather than [`load_from`] directly.
pub fn load_live_cached() -> std::io::Result<ScoresFile> {
    cache().get_or_refresh(|| load_from(&default_path()))
}

/// Drop the cached scores so the next [`load_live_cached`] re-reads disk.
/// Useful right after running the scorer locally so the dashboard updates
/// without waiting for the TTL.
pub fn invalidate_cache() {
    cache().invalidate();
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
  "generated_at": "2026-05-14T22:05:00Z",
  "scores": {
    "claude": {
      "score": 87,
      "agent_id": "claude",
      "pr_number": 30,
      "pr_title": "refactor: drop in-binary tab strips",
      "branch": "agent/claude/drop-in-binary-tab-strip-2026-05-14-21-24",
      "plan_slug": null,
      "criteria_met": ["render_tab_strip removed from all four binaries"],
      "criteria_missed": [],
      "reasoning": "Diff cleanly removes all four strips.",
      "scored_at": "2026-05-14T19:35:00Z"
    },
    "codex-magnolia": {
      "score": null,
      "agent_id": "codex-magnolia",
      "pr_number": 28,
      "pr_title": "Add fleet input pipeline",
      "branch": "agent/codex/fleet-input",
      "plan_slug": null,
      "criteria_met": [],
      "criteria_missed": [],
      "reasoning": "No plan.md found for this branch; advisory score unavailable.",
      "scored_at": "2026-05-14T21:24:00Z"
    }
  }
}"#;

    #[test]
    fn parses_fixture() {
        let f = parse(FIXTURE);
        assert_eq!(f.generated_at.as_deref(), Some("2026-05-14T22:05:00Z"));
        assert_eq!(f.scores.len(), 2);
        let claude = f.for_agent("claude").expect("claude present");
        assert_eq!(claude.score, Some(87));
        assert_eq!(claude.pr_number, 30);
        assert_eq!(claude.criteria_met.len(), 1);
        assert!(claude.criteria_missed.is_empty());
    }

    #[test]
    fn score_can_be_null() {
        let f = parse(FIXTURE);
        let cm = f.for_agent("codex-magnolia").expect("present");
        assert_eq!(cm.score, None, "null score for PRs without a plan");
        assert!(cm.reasoning.contains("advisory score unavailable"));
    }

    #[test]
    fn missing_agent_returns_none() {
        let f = parse(FIXTURE);
        assert!(f.for_agent("nobody").is_none());
    }

    #[test]
    fn malformed_json_yields_empty_file() {
        let f = parse("not json at all");
        assert!(f.scores.is_empty());
        assert!(f.generated_at.is_none());
    }

    #[test]
    fn empty_string_yields_empty_file() {
        assert!(parse("").scores.is_empty());
    }

    #[test]
    fn load_from_missing_path_is_empty_not_error() {
        let f = load_from(Path::new("/tmp/this-path-should-not-exist-xyz-12345.json"))
            .expect("missing file is ok");
        assert!(f.scores.is_empty());
    }
}
