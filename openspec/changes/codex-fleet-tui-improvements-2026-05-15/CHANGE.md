---
base_root_hash: f43dddb0
slug: codex-fleet-tui-improvements-2026-05-15
---

# CHANGE · codex-fleet-tui-improvements-2026-05-15

## §P  proposal
# codex-fleet TUI improvements: supervisor classifier, cap-swap hand-off, plan validation, auto-reviewer, pane health, metrics viewer, renderer polish, inbox dedup

## Problem

The codex-fleet TUI is functional but has eight measurable gaps that compound throughput loss. (1) The supervisor's asking/blocked/working classifier in scripts/codex-fleet/supervisor.sh has no replayable fixtures and no documented Opus 4.7 + Sonnet tiering thresholds, so regressions in classification land silently; the 3-strike loop guard is also opaque. (2) cap-swap-daemon.sh hands a 429'd pane off to a fallback worker without an explicit contract for Colony claim transfer, worktree preservation, or env carry-over, so swapped panes sometimes orphan their claim. (3) Plan publication is unguarded — there is no validator that enforces the parallel-first rule (flat depends_on, disjoint file_scope), so bad plans deadlock workers on task_claim_file. (4) scripts/codex-fleet/auto-reviewer.sh is referenced by the dispatch protocol and the codex-fleet-dispatch skill but does not actually exist on disk — end-of-plan review is therefore manual. (5) There is no per-pane health surface — operators tail individual logs to diagnose a stuck pane; pane-health (last activity, claim state, cap-status) should be a first-class crate. (6) The supervisor metrics TSV (when written) has no live viewer, only post-hoc grep. (7) The dashboard chrome (iOS page headers in fleet-waves, fleet-state, etc) is rendered ad-hoc per crate; a shared renderer-polish layer would give consistent corner radii / hairlines / iOS-blue accent across every dashboard binary. (8) Colony attention_inbox returns duplicate-near handoffs (same task, slightly different timestamps or rationales); a dedup helper would let the orchestrator show the operator a clean list. All 12 lanes are flat-parallel — every lane edits exactly one disjoint file path (or one new disjoint set), depends_on=[] on all of them, so a fleet of 12 workers can claim and ship in parallel without task_claim_file contention. New rust crates use the workspace's fleet-* glob, so no shared rust/Cargo.toml edit is required. Soft references (Lane 5 calls Lane 4's validator at a known path; Lane 6's auto-reviewer reads Lane 7's rubric at a known path) tolerate either landing order — the consumer no-ops or warns until the producer ships.

## Acceptance criteria

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

## Sub-tasks

### Sub-task 0: Supervisor classifier prompt + Opus/Sonnet tiering + 3-strike loop guard documentation

Audit and rewrite scripts/codex-fleet/supervisor.sh's classifier section. Required: (1) Extract the classifier prompt (the one sent to Claude that decides working/asking/blocked/done) into a clearly delimited heredoc at the top of the script with explicit labeled categories and one example per category. (2) Add prompt-cache markers (the static system prompt portion should be inside a cache_control: ephemeral block when invoking claude --print, document the cache hit expectation in a comment). (3) Document Opus 4.7 escalation: by default route to Sonnet 4.6; escalate to Opus only when (a) the classifier itself returns 'uncertain' or (b) a pane has been flagged 'blocked' for ≥ 3 consecutive ticks. Add an env var CODEX_FLEET_FORCE_OPUS=1 override for debugging. (4) 3-strike loop guard: document the existing or add a new counter — after 3 consecutive identical classifier outputs on the same pane, the supervisor must escalate to a different action (poke pane, post Colony note, or page operator) rather than re-running the classifier. Add a top-of-file comment block describing the loop guard with the exact metric (pane id + classification + timestamp) that trips it. (5) shellcheck must remain clean. (6) No behavior change to existing daemons that source this script; only the classifier section is rewritten. file_scope: exactly scripts/codex-fleet/supervisor.sh — do NOT add fixtures (Lane 1 owns them) and do NOT touch plan-watcher.sh (Lane 5 owns it).

File scope: scripts/codex-fleet/supervisor.sh

### Sub-task 1: Classifier replay fixtures + harness (new test dir)

Create scripts/codex-fleet/test/classifier-fixtures/ with at least 20 real captured pane snapshots covering each classifier category. Required: (1) Run tmux capture-pane against a variety of pane states during normal fleet operation (or use the existing pane logs under /tmp/claude-viz/*-worker-*.log) and save each snapshot as a .txt file named after the expected category (e.g. working-001.txt, asking-001.txt, blocked-001.txt, done-001.txt). (2) For each .txt, write a sibling .label file containing the single-line expected classification. (3) Cover edge cases: long log tails, ANSI-heavy output, codex CLI prompts mid-stream, plan-mode 'paste your answer' prompts, 429 rate-limit messages. (4) Write scripts/codex-fleet/test/run-classifier-replay.sh which iterates the fixtures, invokes the classifier from scripts/codex-fleet/supervisor.sh (source it and call the classifier function, or shell out to a minimal wrapper), collects the predicted vs expected label, and prints a confusion matrix + accuracy. Exit non-zero if accuracy < 0.85. (5) Make the harness deterministic — set CODEX_FLEET_FORCE_MODEL=sonnet-4-6 or similar so replays don't burn Opus. (6) shellcheck clean. file_scope: exactly the two new paths below — do NOT modify supervisor.sh (Lane 0 owns it).

File scope: scripts/codex-fleet/test/classifier-fixtures/, scripts/codex-fleet/test/run-classifier-replay.sh

### Sub-task 2: Cap-swap hand-off contract + worktree/claim preservation

Harden scripts/codex-fleet/cap-swap-daemon.sh so a 429'd codex pane hands off cleanly to a fallback worker (Kiro or Claude). Required: (1) Add a top-of-file CONTRACT comment block enumerating every field that must transfer: Colony task_id, agent/* branch, worktree path, CODEX_HOME path, account email, last claim timestamp, accumulated lane context (last Colony task_post note). (2) Before triggering the swap, the daemon MUST: (a) call task_note_working or task_post with a 'swapping due to 429' marker, (b) NOT release the Colony claim — the new worker re-claims by inheriting CODEX_FLEET_TASK_ID, (c) preserve the agent/* worktree (the new worker `cd`'s into it). (3) Add a smoke-test stanza near the top of the script (commented) that simulates a 429: against a fixture pane, set CODEX_FLEET_SIMULATE_429=1, run one tick, assert the claim is preserved and the new worker starts in the same worktree. (4) Defensive: if any required env var is missing on the fallback side, the daemon must log + skip the swap rather than fire a broken worker. (5) shellcheck clean. (6) No edits to claude-spawn.sh (Lane 3 owns it). file_scope: exactly scripts/codex-fleet/cap-swap-daemon.sh.

File scope: scripts/codex-fleet/cap-swap-daemon.sh

### Sub-task 3: Claude fallback worker spawn polish — idempotent, claim-aware

Polish scripts/codex-fleet/claude-spawn.sh so it cleanly handles the cap-swap inheritance case. Required: (1) Idempotent — re-running on a pane that already has a live claude worker is a 0-exit no-op (detect via tmux capture-pane checking for the claude prompt or a process check). (2) Accept CODEX_FLEET_TASK_ID env: when set, the spawned claude worker is told to immediately resume that Colony task (insert it into the prompt or the initial message). (3) Refuse to spawn into a pane whose @panel does not match an expected worker pattern (e.g. starts with [codex- or [kiro- or [claude-). Log + exit 1 cleanly on mismatch. (4) Inherit CODEX_HOME, ACCOUNT_EMAIL from caller env when present; otherwise log + fall back to defaults. (5) Add a top-of-file comment documenting the cap-swap inheritance contract (mirror of Lane 2's contract section). (6) shellcheck clean. (7) No edits to cap-swap-daemon.sh (Lane 2 owns it) or claude-worker.sh (out of scope). file_scope: exactly scripts/codex-fleet/claude-spawn.sh.

File scope: scripts/codex-fleet/claude-spawn.sh

### Sub-task 4: Plan flat-parallelism validator (new lib script)

Create scripts/codex-fleet/lib/plan-validator.sh — a standalone validator for plan.json files. Required: (1) Usage: plan-validator.sh <path-to-plan.json> [--allow-waves]. (2) Validate three rules: (a) every sub-task's depends_on is empty UNLESS --allow-waves is passed, (b) no two sub-tasks share any file path in file_scope (note: a sub-task scope that lists a directory like 'foo/bar/' counts as overlapping with any file path under that directory), (c) acceptance_criteria is a non-empty array of strings each ≥ 40 chars. (3) Output: human-readable findings on stderr; one JSON summary on stdout with shape {ok, warnings: [...], errors: [...]}. (4) Exit codes: 0 ok, 2 warnings only, 3 hard errors. (5) Use jq for JSON parsing (jq is already a project dependency). (6) Add a self-test stanza near the top: a small inline plan.json fixture validates ok, another fails on overlapping file_scope. (7) shellcheck clean, executable bit set. (8) file_scope: exactly scripts/codex-fleet/lib/plan-validator.sh — do NOT wire it into plan-watcher.sh (Lane 5 owns that).

File scope: scripts/codex-fleet/lib/plan-validator.sh

### Sub-task 5: Wire plan-validator into plan-watcher.sh on every tick

Edit scripts/codex-fleet/plan-watcher.sh so each tick (when looping with --loop) invokes the plan validator against the currently-pinned plan. Required: (1) Read .codex-fleet/active-plan to get the slug; resolve plan.json at openspec/plans/<slug>/plan.json. (2) Invoke scripts/codex-fleet/lib/plan-validator.sh on that path. If the validator script is missing (Lane 4 not yet shipped), log 'PLAN-VALIDATE: skipped (validator missing)' and continue — never crash. (3) On exit code 0: log 'PLAN-VALIDATE: ok' once per tick at INFO. (4) On exit code 2 (warnings): log 'PLAN-VALIDATE: WARN <count>' with the JSON summary on the next line; continue dispatching. (5) On exit code 3 (hard errors): log 'PLAN-VALIDATE: ERROR <count>' with the JSON summary, SKIP dispatch for that tick, continue the loop. (6) All log lines go to /tmp/plan-watcher.log with the stable 'PLAN-VALIDATE:' prefix so a grep can extract them. (7) shellcheck clean. (8) Smoke test: run one tick locally against the current openspec/plans/codex-fleet-tui-improvements-2026-05-15/plan.json — should be ok. (9) file_scope: exactly scripts/codex-fleet/plan-watcher.sh.

File scope: scripts/codex-fleet/plan-watcher.sh

### Sub-task 6: Auto-reviewer daemon scaffold (file is currently MISSING)

Create scripts/codex-fleet/auto-reviewer.sh — the end-of-plan auto-reviewer that the codex-fleet-dispatch protocol references but does not currently exist on disk. Required: (1) On invocation, read .codex-fleet/active-plan to get the slug. (2) Use gh pr list --search 'head:agent/* <slug>' or similar to find every PR linked to this plan (or read openspec/plans/<slug>/plan.json and harvest PR URLs from completed_summary fields). (3) For each PR, fetch the diff (gh pr diff <num>). (4) Build a review prompt using the rubric at scripts/codex-fleet/lib/review-rubric.md (Lane 7) and the prepass output from scripts/codex-fleet/lib/review-prepass.sh (Lane 7) — if either is missing, fall back to a built-in minimal rubric and inline diff dump, and log a warning. (5) Invoke claude --print --permission-mode bypassPermissions with the assembled prompt; capture stdout. (6) Write the review to /tmp/claude-viz/plan-review-<slug>.md (mkdir -p the parent). (7) Support --dry-run (do everything except the claude invocation; print the prompt that would be sent). (8) Support --plan-slug <slug> override for ad-hoc runs. (9) shellcheck clean, executable bit set. (10) file_scope: exactly scripts/codex-fleet/auto-reviewer.sh — do NOT touch the rubric or prepass (Lane 7 owns them).

File scope: scripts/codex-fleet/auto-reviewer.sh

### Sub-task 7: Review rubric + diff prepass (two new lib files)

Create the two artifacts the auto-reviewer (Lane 6) consumes. Required: (1) scripts/codex-fleet/lib/review-rubric.md — a markdown checklist with four sections: REGRESSION RISK (does the diff change behavior beyond what the plan promised?), SCOPE CREEP (does any sub-task touch files outside its declared file_scope?), ANTI-PATTERN FLAGS (CLAUDE.md violations: backwards-compat shims, dead comments, premature abstractions, error-handling for impossible states), BLAST RADIUS (config/CI/migration/db touches; shared helper edits). Each section gets 3-5 bullets the reviewer model can answer yes/no/n-a against. Keep total length under 2k chars so it caches well. (2) scripts/codex-fleet/lib/review-prepass.sh — usage: review-prepass.sh <plan-slug>. Output: a single markdown blob on stdout with sections: ## Plan acceptance criteria (verbatim from plan.json), ## Sub-task scope claims (per-lane file_scope), ## PR diffs (one fenced block per PR, fetched via gh pr diff). Limit each diff to 200 lines (head -200) to keep the prompt bounded. shellcheck clean. (3) Both files executable bit set where applicable (.md no, .sh yes). (4) file_scope: exactly the two new lib paths below — do NOT touch auto-reviewer.sh (Lane 6 owns it).

File scope: scripts/codex-fleet/lib/review-rubric.md, scripts/codex-fleet/lib/review-prepass.sh

### Sub-task 8: Pane health crate (new rust workspace crate)

Create rust/fleet-pane-health/ as a new workspace crate (auto-picked-up by the fleet-* glob in rust/Cargo.toml). Required: (1) Cargo.toml: name 'fleet-pane-health', version 0.0.1, edition 2021, ratatui = '0.30', crossterm = '0.29', fleet-ui = { path = '../fleet-ui' } for shared chrome, fleet-data = { path = '../fleet-data' } if needed, [[bin]] name 'fleet-pane-health' path 'src/main.rs'. (2) src/main.rs: a ratatui app that polls every 1s and renders a vertical list of per-pane health rows. Each row: pane id (e.g. '%337'), pane @panel label, last-activity age (now - mtime of /tmp/claude-viz/{kiro,claude,codex}-worker-<id>.log), current Colony claim state (parsed from a JSON dump at /tmp/claude-viz/colony-claims.json if present, else 'unknown'), cap-probe state (read /tmp/claude-viz/cap-probe-cache/<email>.json mtime + ok/429). (3) Use the existing fleet-ui chrome (page header, hairline dividers, iOS-blue accent). (4) Quit on q or Esc. (5) cargo check -p fleet-pane-health succeeds from rust/. (6) Read-only — no writes to /tmp. (7) file_scope: rust/fleet-pane-health/Cargo.toml and rust/fleet-pane-health/src/main.rs — do NOT edit any existing crate or rust/Cargo.toml (the glob picks it up).

File scope: rust/fleet-pane-health/Cargo.toml, rust/fleet-pane-health/src/main.rs

### Sub-task 9: Metrics TSV viewer crate (new rust workspace crate)

Create rust/fleet-metrics-viewer/ as a new workspace crate. Required: (1) Cargo.toml: name 'fleet-metrics-viewer', version 0.0.1, edition 2021, ratatui = '0.30', crossterm = '0.29', fleet-ui = { path = '../fleet-ui' }, [[bin]] name 'fleet-metrics-viewer' path 'src/main.rs'. (2) src/main.rs: a ratatui live tail of a TSV file. CLI: --path <tsv> (default /tmp/claude-viz/supervisor-metrics.tsv if present, else exit 0 with an 'idle (no metrics file yet)' message). (3) Render the last 30 rows in a scrollable table with iOS-style chrome. Auto-refresh every 500ms via crossterm event poll + file mtime check. (4) Column headers parsed from the TSV's first line. (5) Use the existing fleet-ui chrome helpers for headers and dividers. (6) Quit on q or Esc; PgUp/PgDn to scroll. (7) cargo check -p fleet-metrics-viewer succeeds. (8) file_scope: rust/fleet-metrics-viewer/Cargo.toml and rust/fleet-metrics-viewer/src/main.rs — do NOT edit any existing crate.

File scope: rust/fleet-metrics-viewer/Cargo.toml, rust/fleet-metrics-viewer/src/main.rs

### Sub-task 10: Renderer polish crate (shared chrome primitives library)

Create rust/fleet-renderer-polish/ as a new workspace crate exposing a small library of ratatui chrome primitives factored from patterns already present in fleet-waves/fleet-state. Required: (1) Cargo.toml: name 'fleet-renderer-polish', version 0.0.1, edition 2021, ratatui = '0.30', [lib] path 'src/lib.rs'. (2) src/lib.rs: pub fn rounded_corner_block(title: &str) -> Block — returns a ratatui Block with rounded corners and a one-cell padding; pub fn hairline_divider(width: u16) -> Line — a single line of '─' in fg=#3A3A3C; pub fn ios_status_chip(label: &str, color: IosColor) -> Span — a small chip like ' LIVE ' in fg-only ANSI matching the project's iOS palette (enum IosColor { Green, Blue, Yellow, Red, Gray }); pub fn page_header_with_accent(title: &str, accent: IosColor) -> Vec<Line> — a two-line iOS page header (bold title + accent underline). (3) Each fn has a doc comment with one usage example. (4) cargo check -p fleet-renderer-polish succeeds. (5) Library only — no main.rs, no binary. (6) Do NOT touch any existing crate; adoption is a separate follow-up. (7) file_scope: rust/fleet-renderer-polish/Cargo.toml and rust/fleet-renderer-polish/src/lib.rs.

File scope: rust/fleet-renderer-polish/Cargo.toml, rust/fleet-renderer-polish/src/lib.rs

### Sub-task 11: Attention inbox dedup helper (new lib script)

Create scripts/codex-fleet/lib/inbox-dedup.sh — a stdin-to-stdout filter that dedupes Colony attention_inbox output. Required: (1) Reads JSONL on stdin (one JSON object per line; Colony's attention_inbox export shape: task_id, kind, content, timestamp, agent). (2) Groups near-duplicates by a composite key: (task_id, kind, content-hash-with-whitespace-normalized). The content hash normalizes by lowercasing, collapsing runs of whitespace to single spaces, and stripping leading/trailing whitespace before hashing (sha1 of normalized content). (3) For each group, emit only the latest (largest timestamp) entry on stdout. (4) Idempotent: running it twice yields the same output. (5) Pure bash + jq + sha1sum — no python. (6) Add a self-test stanza near the top: pipe an inline JSONL fixture with three duplicates through the script, assert the output count is correct. (7) shellcheck clean, executable bit set. (8) file_scope: exactly scripts/codex-fleet/lib/inbox-dedup.sh.

File scope: scripts/codex-fleet/lib/inbox-dedup.sh


## §S  delta
op|target|row
-|-|-

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
