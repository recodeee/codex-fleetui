# Tasks

| # | Status | Title | Files | Depends on | Capability | Spec row | Owner |
| - | - | - | - | - | - | - | - |
0|available|Supervisor classifier prompt + Opus/Sonnet tiering + 3-strike loop guard documentation|`scripts/codex-fleet/supervisor.sh`|-|infra_work|-|-
1|available|Classifier replay fixtures + harness (new test dir)|`scripts/codex-fleet/test/classifier-fixtures/`<br>`scripts/codex-fleet/test/run-classifier-replay.sh`|-|infra_work|-|-
2|available|Cap-swap hand-off contract + worktree/claim preservation|`scripts/codex-fleet/cap-swap-daemon.sh`|-|infra_work|-|-
3|available|Claude fallback worker spawn polish — idempotent, claim-aware|`scripts/codex-fleet/claude-spawn.sh`|-|infra_work|-|-
4|available|Plan flat-parallelism validator (new lib script)|`scripts/codex-fleet/lib/plan-validator.sh`|-|infra_work|-|-
5|available|Wire plan-validator into plan-watcher.sh on every tick|`scripts/codex-fleet/plan-watcher.sh`|-|infra_work|-|-
6|available|Auto-reviewer daemon scaffold (file is currently MISSING)|`scripts/codex-fleet/auto-reviewer.sh`|-|infra_work|-|-
7|available|Review rubric + diff prepass (two new lib files)|`scripts/codex-fleet/lib/review-rubric.md`<br>`scripts/codex-fleet/lib/review-prepass.sh`|-|infra_work|-|-
8|available|Pane health crate (new rust workspace crate)|`rust/fleet-pane-health/Cargo.toml`<br>`rust/fleet-pane-health/src/main.rs`|-|ui_work|-|-
9|available|Metrics TSV viewer crate (new rust workspace crate)|`rust/fleet-metrics-viewer/Cargo.toml`<br>`rust/fleet-metrics-viewer/src/main.rs`|-|ui_work|-|-
10|available|Renderer polish crate (shared chrome primitives library)|`rust/fleet-renderer-polish/Cargo.toml`<br>`rust/fleet-renderer-polish/src/lib.rs`|-|ui_work|-|-
11|available|Attention inbox dedup helper (new lib script)|`scripts/codex-fleet/lib/inbox-dedup.sh`|-|infra_work|-|-
