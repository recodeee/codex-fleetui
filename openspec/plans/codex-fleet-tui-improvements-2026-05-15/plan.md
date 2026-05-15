# codex-fleet TUI improvements: supervisor classifier, cap-swap hand-off, plan validation, auto-reviewer, pane health, metrics viewer, renderer polish, inbox dedup

Plan slug: `codex-fleet-tui-improvements-2026-05-15`

## Problem

The codex-fleet TUI is functional but has eight measurable gaps that compound throughput loss. (1) The supervisor's asking/blocked/working classifier in scripts/codex-fleet/supervisor.sh has no replayable fixtures and no documented Opus 4.7 + Sonnet tiering thresholds, so regressions in classification land silently; the 3-strike loop guard is also opaque. (2) cap-swap-daemon.sh hands a 429'd pane off to a fallback worker without an explicit contract for Colony claim transfer, worktree preservation, or env carry-over, so swapped panes sometimes orphan their claim. (3) Plan publication is unguarded — there is no validator that enforces the parallel-first rule (flat depends_on, disjoint file_scope), so bad plans deadlock workers on task_claim_file. (4) scripts/codex-fleet/auto-reviewer.sh is referenced by the dispatch protocol and the codex-fleet-dispatch skill but does not actually exist on disk — end-of-plan review is therefore manual. (5) There is no per-pane health surface — operators tail individual logs to diagnose a stuck pane; pane-health (last activity, claim state, cap-status) should be a first-class crate. (6) The supervisor metrics TSV (when written) has no live viewer, only post-hoc grep. (7) The dashboard chrome (iOS page headers in fleet-waves, fleet-state, etc) is rendered ad-hoc per crate; a shared renderer-polish layer would give consistent corner radii / hairlines / iOS-blue accent across every dashboard binary. (8) Colony attention_inbox returns duplicate-near handoffs (same task, slightly different timestamps or rationales); a dedup helper would let the orchestrator show the operator a clean list. All 12 lanes are flat-parallel — every lane edits exactly one disjoint file path (or one new disjoint set), depends_on=[] on all of them, so a fleet of 12 workers can claim and ship in parallel without task_claim_file contention. New rust crates use the workspace's fleet-* glob, so no shared rust/Cargo.toml edit is required. Soft references (Lane 5 calls Lane 4's validator at a known path; Lane 6's auto-reviewer reads Lane 7's rubric at a known path) tolerate either landing order — the consumer no-ops or warns until the producer ships.

## Acceptance Criteria

- All 12 sub-tasks land independently as 12 separate PRs against main with depends_on=[] honored; no two PRs touch the same file path.
- After Lane 0 ships, scripts/codex-fleet/supervisor.sh contains: an explicit classifier prompt with labeled categories (working / asking / blocked / done), a documented Opus 4.7 escalation threshold, prompt-cache markers around the static system prompt, and a comment block describing the 3-strike loop guard with the exact metric that trips it. shellcheck exits 0.
- After Lane 1 ships, scripts/codex-fleet/test/classifier-fixtures/ contains at least 20 captured pane snapshots (.txt files) with sibling .label files marking the expected category. scripts/codex-fleet/test/run-classifier-replay.sh iterates the fixtures, invokes the classifier, and prints a confusion matrix; exits non-zero if accuracy < 0.85 on the captured set.
- After Lane 2 ships, cap-swap-daemon.sh has a top-of-file CONTRACT section enumerating every field that must transfer on hand-off (Colony task_id, branch, worktree path, CODEX_HOME, account email, last claim timestamp) and a smoke-test stanza that simulates a 429 against a fixture pane and asserts the claim is released cleanly. No orphaned worktrees after the simulated swap.
- After Lane 3 ships, scripts/codex-fleet/claude-spawn.sh is idempotent (re-running on a live pane is a no-op + 0 exit), accepts an inherited CODEX_FLEET_TASK_ID env var so a cap-swapped pane resumes the same Colony claim, and refuses to spawn into a pane whose @panel does not match the expected worker pattern (defensive guard).
- After Lane 4 ships, scripts/codex-fleet/lib/plan-validator.sh exists, is executable, and validates a plan.json file at a given path against three rules: (a) every sub-task's depends_on is empty unless --allow-waves is passed, (b) no two sub-tasks share a file path in file_scope, (c) acceptance_criteria is a non-empty array of strings each ≥ 40 chars. Exit codes: 0 ok, 2 warnings, 3 hard errors.
- After Lane 5 ships, plan-watcher.sh invokes plan-validator.sh on every tick against the currently-active plan and surfaces warnings/errors in /tmp/plan-watcher.log with a stable prefix ('PLAN-VALIDATE:'). Hard errors do NOT crash the watcher but DO skip dispatch for that tick. shellcheck exits 0.
- After Lane 6 ships, scripts/codex-fleet/auto-reviewer.sh exists, is executable, and on invocation reads .codex-fleet/active-plan, gathers all PRs merged for that plan slug via gh pr list, builds a review prompt using the rubric (Lane 7) and the prepass diff (Lane 7), invokes the local claude CLI in --print mode, and writes the output to /tmp/claude-viz/plan-review-<slug>.md. Supports --dry-run for offline test.
- After Lane 7 ships, scripts/codex-fleet/lib/review-rubric.md exists and documents the explicit review checklist (regression risk, scope creep against plan acceptance_criteria, anti-pattern flags, blast radius); scripts/codex-fleet/lib/review-prepass.sh exists and emits a single markdown blob with the plan's stated scope + the actual diff for every PR, ready to be piped to a model.
- After Lane 8 ships, rust/fleet-pane-health/ is a new workspace crate that builds (cargo check -p fleet-pane-health) and renders a per-pane health row (pane id, last activity age, current Colony claim if any, cap-probe cache state) using ratatui + the shared fleet-ui chrome helpers. It reads from /tmp/claude-viz/cap-probe-cache/*.json and tmux capture-pane output. No edits to existing crates.
- After Lane 9 ships, rust/fleet-metrics-viewer/ is a new workspace crate that tails a TSV path passed via --path (default /tmp/claude-viz/supervisor-metrics.tsv when present, else --no-op) and renders the last N rows as a live dashboard with iOS-style chrome. cargo check -p fleet-metrics-viewer succeeds. No edits to existing crates.
- After Lane 10 ships, rust/fleet-renderer-polish/ is a new workspace crate exposing a small library of ratatui chrome primitives (rounded_corner_block, hairline_divider, ios_status_chip, page_header_with_accent) factored from the patterns already in fleet-waves/fleet-state. cargo check -p fleet-renderer-polish succeeds; it does NOT yet replace any existing crate's chrome — adoption is a follow-up. No edits to existing crates.
- After Lane 11 ships, scripts/codex-fleet/lib/inbox-dedup.sh exists and reads JSONL from stdin (Colony attention_inbox export), groups near-duplicates by (task_id, kind, content-hash with whitespace normalized), and prints a deduped JSONL to stdout. Exit 0 always; idempotent on already-deduped input.
- No regression in existing dashboards: cargo check --workspace from rust/ still passes after all 12 lanes land.
- Every PR's final note records: branch, files changed, command + output evidence (shellcheck, cargo check, fixture replay where applicable), PR URL, MERGED state, and sandbox cleanup proof per the Guardex completion contract.

## Roles

- [planner](./planner.md)
- [architect](./architect.md)
- [critic](./critic.md)
- [executor](./executor.md)
- [writer](./writer.md)
- [verifier](./verifier.md)

## Operator Flow

1. Refine this workspace until scope, risks, and tasks are explicit.
2. Publish the plan with `colony plan publish codex-fleet-tui-improvements-2026-05-15` or the `task_plan_publish` MCP tool.
3. Claim subtasks through Colony plan tools before editing files.
4. Close only when all subtasks are complete and `checkpoints.md` records final evidence.
