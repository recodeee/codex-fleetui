//! Typed loader for `openspec/plans/*/plan.json`.
//!
//! Mirrors the schema documented at runtime by Colony (see existing plans
//! under `recodeee/recodee:openspec/plans/`). Replaces the hand-rolled `jq`
//! and Python parsing scattered across `plan-tree-anim.sh`,
//! `waves-anim-generic.sh`, `watcher-board.sh`, and `force-claim.sh`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub plan_slug: String,
    pub title: String,
    pub problem: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    pub tasks: Vec<Subtask>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub published: Option<Published>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub subtask_index: u32,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub file_scope: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<u32>,
    #[serde(default)]
    pub capability_hint: Option<String>,
    #[serde(default)]
    pub spec_row_id: Option<u64>,
    /// One of: "available", "claimed", "completed", "blocked".
    pub status: String,
    #[serde(default)]
    pub claimed_by_session_id: Option<String>,
    #[serde(default)]
    pub claimed_by_agent: Option<String>,
    #[serde(default)]
    pub completed_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Published {
    #[serde(default)]
    pub spec_task_id: Option<u64>,
    #[serde(default)]
    pub spec_change_path: Option<String>,
    #[serde(default)]
    pub auto_archive: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("io error reading {0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("invalid json in {0}: {1}")]
    Json(PathBuf, #[source] serde_json::Error),
}

/// Read + deserialize a `plan.json`. Returns the canonical-typed [`Plan`].
pub fn load(path: &Path) -> Result<Plan, PlanError> {
    let bytes = fs::read(path).map_err(|e| PlanError::Io(path.to_path_buf(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| PlanError::Json(path.to_path_buf(), e))
}

/// Find the newest plan workspace under `repo_root/openspec/plans/`.
///
/// Ordering matches `plan-tree-anim.sh::_latest_plan`: sort by trailing
/// `YYYY-MM-DD` slug suffix descending, with mtime as the tie-breaker for
/// slugs that lack a date. Returns the path to the **plan.json file**, or
/// `None` if no plans exist.
pub fn newest_plan(repo_root: &Path) -> Result<Option<PathBuf>, PlanError> {
    let plans_dir = repo_root.join("openspec").join("plans");
    let mut candidates: Vec<(PathBuf, (u16, u8, u8), u64)> = Vec::new();
    let read = match fs::read_dir(&plans_dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(PlanError::Io(plans_dir, e)),
    };
    for entry in read.flatten() {
        let path = entry.path();
        let json = path.join("plan.json");
        if !json.is_file() {
            continue;
        }
        let slug = entry.file_name().to_string_lossy().into_owned();
        let date = parse_trailing_date(&slug).unwrap_or((0, 0, 0));
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        candidates.push((json, date, mtime));
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
    Ok(candidates.into_iter().next().map(|t| t.0))
}

/// Parse a `…-YYYY-MM-DD` suffix into `(y, m, d)`. Returns `None` if the
/// trailing 10 characters don't form a valid date string.
pub fn parse_trailing_date(slug: &str) -> Option<(u16, u8, u8)> {
    if slug.len() < 10 {
        return None;
    }
    let tail = &slug[slug.len() - 10..];
    let bytes = tail.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: u16 = tail[0..4].parse().ok()?;
    let m: u8 = tail[5..7].parse().ok()?;
    let d: u8 = tail[8..10].parse().ok()?;
    Some((y, m, d))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "schema_version": 1,
        "plan_slug": "fleet-tui-ratatui-port-2026-05-14",
        "title": "fleet-tui port",
        "problem": "test",
        "tasks": [
            {"subtask_index": 0, "title": "Foundation", "description": "scaffold crates", "status": "completed", "depends_on": []},
            {"subtask_index": 1, "title": "Palette",    "description": "port palette",     "status": "available", "depends_on": [0]}
        ]
    }"#;

    #[test]
    fn deserialises_minimum_schema() {
        let p: Plan = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(p.plan_slug, "fleet-tui-ratatui-port-2026-05-14");
        assert_eq!(p.tasks.len(), 2);
        assert_eq!(p.tasks[0].status, "completed");
        assert_eq!(p.tasks[1].depends_on, vec![0]);
    }

    #[test]
    fn parses_trailing_date() {
        assert_eq!(parse_trailing_date("fleet-tui-2026-05-14"), Some((2026, 5, 14)));
        assert_eq!(parse_trailing_date("no-date-here"), None);
        assert_eq!(parse_trailing_date("2026-13-99"), Some((2026, 13, 99))); // doesn't validate ranges
    }

    #[test]
    fn missing_plans_dir_returns_none() {
        let tmp = std::env::temp_dir().join("fleet-data-test-empty");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let r = newest_plan(&tmp).unwrap();
        assert!(r.is_none(), "no plans dir → None, got {:?}", r);
    }
}
