//! Per-worker row model for the Fleet dashboard ("G · Fleet" artboard).
//!
//! This is the join layer the `fleet-state` binary renders and `fleet-watcher`
//! can reuse: it stitches together the three independent data sources that
//! already live in this crate —
//!
//!   * [`crate::accounts`] — `agent-auth list` percents per account email.
//!   * [`crate::panes`]    — tmux pane scrollback + [`PaneState`] classifier.
//!   * the `@panel` label  — `[codex-<aid>]`, the join key between the two.
//!
//! Replaces the bespoke per-account / per-pane correlation scattered across
//! `fleet-tick.sh`'s `fleet_state_row` and `watcher-board.sh`'s pane loop.
//!
//! The split mirrors [`crate::accounts`]: [`join`] is pure (takes pre-loaded
//! inputs, fully unit-testable), [`load_live`] is the thin runner that shells
//! out and feeds `join`.

use crate::accounts::Account;
use crate::panes::{PaneInfo, PaneState};
use crate::scores::ScoresFile;
use crate::scrape::scrape_activity;
use serde::{Deserialize, Serialize};

/// One row of the Fleet table — an account, the pane it's running in, and
/// what that pane is doing right now. Everything the "G · Fleet" artboard
/// needs for a single line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRow {
    /// Account email — the `ACCOUNT` column's primary line.
    pub email: String,
    /// Agent id derived from the email (`admin@magnoliavilag.hu` → `admin-magnolia`).
    /// This is the `@panel` join key; also handy for display.
    pub agent_id: String,
    /// Dim model label under the email, e.g. `gpt-5.5 xhigh`. `None` when the
    /// pane scrollback didn't surface a model line.
    pub model_label: Option<String>,
    /// `weekly=` percent from `agent-auth list` — the `WEEKLY · 5H` rail.
    pub weekly_pct: u8,
    /// `5h=` percent from `agent-auth list` — the `WORKER · 5H` rail.
    pub five_h_pct: u8,
    /// Classified pane state — drives the `STATUS` chip. `None` when the
    /// account has no live pane (reserve account; renders as a blank/idle row
    /// or is filtered out by the caller).
    pub state: Option<PaneState>,
    /// `WORKING ON` headline — the task line scraped from scrollback, e.g.
    /// `scaffold rust/fleet-ui`. Empty when nothing could be scraped.
    pub working_on: String,
    /// Dim subtext under `working_on`, e.g. `pane %47 · 10m 28s`. Empty when
    /// the pane has no id / no runtime to show.
    pub pane_subtext: String,
    /// tmux pane id (`%47`) — the `PANE` column's `#N >` affordance. `None`
    /// for reserve accounts.
    pub pane_id: Option<String>,
    /// `true` when this account is the one `agent-auth` marks current (`*`).
    /// The artboard stars it; also useful for sort stability.
    pub is_current: bool,
    /// Advisory quality score for this agent's most recently merged PR,
    /// 0–100, where 100 means every plan acceptance criterion is met.
    /// `None` when the scorer has never run for this agent or the most
    /// recent PR had no associated plan (the scorer emits `null` in that
    /// case). Drives the third rail in the Fleet table — see
    /// [`crate::scores`] for the writer side.
    pub quality: Option<u8>,
}

impl WorkerRow {
    /// `true` when the row has a live pane in any non-terminal state — i.e. it
    /// counts toward the header's "N live" tally. `Dead` panes and reserve
    /// accounts (no pane) are not live.
    pub fn is_live(&self) -> bool {
        !matches!(self.state, None | Some(PaneState::Dead))
    }

    /// `true` when the pane is awaiting human approval — the header's
    /// "N in review" tally.
    pub fn is_in_review(&self) -> bool {
        matches!(self.state, Some(PaneState::Approval))
    }
}

/// Header summary line: "8 workers · 6 live · 1 in review".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetSummary {
    pub workers: usize,
    pub live: usize,
    pub in_review: usize,
}

impl FleetSummary {
    pub fn of(rows: &[WorkerRow]) -> Self {
        Self {
            workers: rows.len(),
            live: rows.iter().filter(|r| r.is_live()).count(),
            in_review: rows.iter().filter(|r| r.is_in_review()).count(),
        }
    }
}

/// Canonical `email` → `agent-id` derivation. Mirrors
/// `cap-swap-daemon.sh::email_to_id` and `fleet-tick.sh::derive_aid` exactly,
/// including the domain-stem aliases — so the id we compute here matches the
/// `@panel` label tmux carries (`[codex-<aid>]`).
pub fn derive_agent_id(email: &str) -> String {
    let (local, domain) = match email.split_once('@') {
        Some((l, d)) => (l, d),
        None => return email.to_string(),
    };
    let stem = domain.split('.').next().unwrap_or(domain);
    let stem = match stem {
        "magnoliavilag" => "magnolia",
        "gitguardex" => "gg",
        "pipacsclub" => "pipacs",
        other => other,
    };
    format!("{local}-{stem}")
}

/// Extract the `agent-id` out of a tmux `@panel` value like `[codex-admin-magnolia]`
/// (the panel may also carry a status-chip prefix + branch suffix once
/// `fleet-tick.sh` has rewritten it, so we match the `codex-<id>` token rather
/// than assuming the whole string is the label).
pub fn agent_id_from_panel(panel: &str) -> Option<String> {
    let start = panel.find("codex-")? + "codex-".len();
    let rest = &panel[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .unwrap_or(rest.len());
    let id = &rest[..end];
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Pure join: stitch `accounts` + `panes` + quality `scores` into the
/// Fleet table's rows.
///
/// `accounts` is the authoritative row set — one [`WorkerRow`] per account,
/// in the order `agent-auth list` returned them. Each account is matched to a
/// pane by `derive_agent_id(account.email) == agent_id_from_panel(pane.@panel)`.
/// An account with no matching pane is a reserve account: `state`, `pane_id`,
/// `working_on` stay empty. A pane with no matching account is dropped (it's
/// not a fleet worker we track).
///
/// `scores` is the per-agent quality file (see [`crate::scores`]). The
/// matching agent-id's most-recent score is folded into
/// [`WorkerRow::quality`]; missing or never-scored agents get `None`, and
/// the renderer hides the rail for them.
///
/// `panels` carries each pane's `@panel` value alongside its [`PaneInfo`],
/// since `PaneInfo::panel_label` is whatever `tmux list-panes -F #{@panel}`
/// returned — exactly the string [`agent_id_from_panel`] expects.
pub fn join(
    accounts: &[Account],
    panes: &[PaneInfo],
    scores: &ScoresFile,
) -> Vec<WorkerRow> {
    accounts
        .iter()
        .map(|acct| {
            let agent_id = derive_agent_id(&acct.email);

            // Find this account's pane by matching the @panel-derived id.
            let pane = panes.iter().find(|p| {
                p.panel_label
                    .as_deref()
                    .and_then(agent_id_from_panel)
                    .as_deref()
                    == Some(agent_id.as_str())
            });

            let (state, working_on, pane_subtext, pane_id, model_label) = match pane {
                Some(p) => {
                    let st = crate::panes::classify(p);
                    let act = scrape_activity(&p.scrollback_tail);
                    let subtext = match &act.runtime {
                        Some(rt) => format!("pane {} · {}", p.pane_id, rt),
                        None => format!("pane {}", p.pane_id),
                    };
                    (
                        Some(st),
                        act.working_on,
                        subtext,
                        Some(p.pane_id.clone()),
                        act.model_label,
                    )
                }
                None => (None, String::new(), String::new(), None, None),
            };

            let quality = scores.for_agent(&agent_id).and_then(|s| s.score);

            WorkerRow {
                email: acct.email.clone(),
                agent_id,
                model_label,
                weekly_pct: acct.weekly_pct,
                five_h_pct: acct.five_h_pct,
                state,
                working_on,
                pane_subtext,
                pane_id,
                is_current: acct.is_current,
                quality,
            }
        })
        .collect()
}

/// Live runner: shell out to `agent-auth list` + `tmux` and join the results.
///
/// `session` / `window` are the tmux target for [`crate::panes::list_panes`]
/// (`"codex-fleet"`, `Some("overview")` for the standard fleet). Mirrors
/// [`crate::accounts::load_live`] — dashboards on a tick should prefer the
/// cached account path, so this calls [`crate::accounts::load_live_cached`].
pub fn load_live(session: &str, window: Option<&str>) -> std::io::Result<Vec<WorkerRow>> {
    let accounts = crate::accounts::load_live_cached()?;
    let panes = crate::panes::list_panes(session, window)?;
    let scores = crate::scores::load_live_cached().unwrap_or_default();
    Ok(join(&accounts, &panes, &scores))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(email: &str, five_h: u8, weekly: u8, current: bool) -> Account {
        Account {
            email: email.to_string(),
            five_h_pct: five_h,
            weekly_pct: weekly,
            is_current: current,
        }
    }

    fn pane(panel: &str, pane_id: &str, cmd: &str, tail: &str) -> PaneInfo {
        PaneInfo {
            pane_id: pane_id.to_string(),
            panel_label: Some(panel.to_string()),
            current_command: cmd.to_string(),
            scrollback_tail: tail.to_string(),
        }
    }

    #[test]
    fn derive_agent_id_matches_bash_aliases() {
        assert_eq!(derive_agent_id("admin@magnoliavilag.hu"), "admin-magnolia");
        assert_eq!(derive_agent_id("admin@gitguardex.com"), "admin-gg");
        assert_eq!(derive_agent_id("admin@pipacsclub.hu"), "admin-pipacs");
        // Unaliased domain keeps its stem.
        assert_eq!(derive_agent_id("admin@mite.hu"), "admin-mite");
    }

    #[test]
    fn agent_id_survives_panel_decoration() {
        // Bare label.
        assert_eq!(
            agent_id_from_panel("[codex-admin-magnolia]").as_deref(),
            Some("admin-magnolia")
        );
        // fleet-tick.sh rewrites @panel with a status chip prefix + branch suffix.
        assert_eq!(
            agent_id_from_panel("◖ ● working ◗ [codex-admin-mite] → sub-3 · agent/codex/foo")
                .as_deref(),
            Some("admin-mite")
        );
        assert_eq!(agent_id_from_panel("[viz] plan-design"), None);
    }

    #[test]
    fn join_matches_account_to_pane() {
        let accounts = vec![
            acct("admin@kollarrobert.sk", 95, 100, true),
            acct("admin@mite.hu", 70, 72, false),
        ];
        let panes = vec![pane(
            "[codex-admin-kollarrobert]",
            "%1",
            "node",
            "scaffold rust/fleet-ui\n● Working (10m 28s · esc to interrupt)",
        )];

        let rows = join(&accounts, &panes, &ScoresFile::default());
        assert_eq!(rows.len(), 2, "one row per account, reserve included");

        let kollar = &rows[0];
        assert_eq!(kollar.agent_id, "admin-kollarrobert");
        assert_eq!(kollar.state, Some(PaneState::Working));
        assert_eq!(kollar.working_on, "scaffold rust/fleet-ui");
        assert_eq!(kollar.pane_subtext, "pane %1 · 10m 28s");
        assert_eq!(kollar.pane_id.as_deref(), Some("%1"));
        assert!(kollar.is_live());
        assert!(kollar.is_current);

        // mite.hu has no pane → reserve row, empty pane fields.
        let mite = &rows[1];
        assert_eq!(mite.state, None);
        assert!(mite.pane_id.is_none());
        assert!(mite.working_on.is_empty());
        assert!(!mite.is_live());
    }

    #[test]
    fn scrape_picks_runtime_and_model() {
        let tail = "port plan.rs\n\
                    ● Working (12m 04s)\n\
                    gpt-5.5 xhigh · 37% left   49% context";
        let act = scrape_activity(tail);
        assert_eq!(act.working_on, "port plan.rs");
        assert_eq!(act.runtime.as_deref(), Some("12m 04s"));
        assert_eq!(act.model_label.as_deref(), Some("gpt-5.5 xhigh"));
    }

    #[test]
    fn summary_counts_live_and_review() {
        let accounts = vec![
            acct("a@x.com", 0, 0, false),
            acct("b@x.com", 0, 0, false),
            acct("c@x.com", 0, 0, false),
        ];
        let panes = vec![
            pane("[codex-a-x]", "%1", "node", "doing a thing\n● Working (1m)"),
            pane(
                "[codex-b-x]",
                "%2",
                "node",
                "Auto-reviewer approved this command",
            ),
            // c@x.com → reserve, no pane.
        ];
        let rows = join(&accounts, &panes, &ScoresFile::default());
        let s = FleetSummary::of(&rows);
        assert_eq!(s.workers, 3);
        assert_eq!(s.live, 2, "two panes, both non-dead");
        assert_eq!(s.in_review, 1, "the Approval pane");
    }

    #[test]
    fn dead_pane_is_not_live() {
        let accounts = vec![acct("a@x.com", 0, 0, false)];
        let panes = vec![pane("[codex-a-x]", "%1", "bash", "$ ls\n")];
        let rows = join(&accounts, &panes, &ScoresFile::default());
        assert_eq!(rows[0].state, Some(PaneState::Dead));
        assert!(!rows[0].is_live());
    }

    #[test]
    fn quality_is_looked_up_by_agent_id() {
        let accounts = vec![
            acct("admin@magnoliavilag.hu", 0, 0, false), // → admin-magnolia
            acct("admin@mite.hu", 0, 0, false),          // → admin-mite (no score)
        ];
        let panes: Vec<PaneInfo> = vec![];
        let scores = crate::scores::parse(
            r#"{
              "generated_at": "2026-05-14T22:05:00Z",
              "scores": {
                "admin-magnolia": {
                  "score": 92, "agent_id": "admin-magnolia", "pr_number": 1,
                  "pr_title": "x", "branch": "y", "plan_slug": null,
                  "criteria_met": [], "criteria_missed": [], "reasoning": "",
                  "scored_at": "2026-05-14T22:00:00Z"
                }
              }
            }"#,
        );
        let rows = join(&accounts, &panes, &scores);
        assert_eq!(rows[0].quality, Some(92), "magnolia gets its score");
        assert_eq!(rows[1].quality, None, "mite has no score yet → None");
    }

    #[test]
    fn quality_null_is_none() {
        // A scored agent with `score: null` (PR had no plan) still maps to None
        // — the renderer treats no-score and null-score the same: hide the rail.
        let accounts = vec![acct("admin@mite.hu", 0, 0, false)];
        let scores = crate::scores::parse(
            r#"{
              "scores": {
                "admin-mite": {
                  "score": null, "agent_id": "admin-mite", "pr_number": 1,
                  "pr_title": "x", "branch": "y", "plan_slug": null,
                  "criteria_met": [], "criteria_missed": [],
                  "reasoning": "no plan",
                  "scored_at": "2026-05-14T22:00:00Z"
                }
              }
            }"#,
        );
        let rows = join(&accounts, &[], &scores);
        assert_eq!(rows[0].quality, None);
    }
}
