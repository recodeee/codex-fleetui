# SPEC

## §G  goal
Provide the root Colony coordination contract for codex-fleet so the multi-account tmux worker fleet can publish, claim, and audit plan subtasks before any live or rendered design is treated as complete.

## §C  constraints
- OpenSpec plans under `openspec/plans/*/plan.json` remain the source of truth for what work is claimable.
- A plan workspace on disk is not claimable until it is registered in Colony's task graph (`registry_status: published`).
- Static prompt packs, screenshots, and design mockups are not ready tasks; only plan subtasks routed through Colony are claimable work.
- Evidence that advances a plan subtask must come from merged PR state with the `PR #<n>` badge in `completed_summary`, never from raw worktree edits.
- Keep this file compact; detailed migration state belongs in OpenSpec plan artifacts and task threads.

## §I  interfaces
- `task_plan_publish` registers a plan workspace into Colony so its subtasks become claimable.
- `task_ready_for_agent` surfaces the next claimable subtask for an agent, gated on this SPEC.md being present.
- `task_plan_claim_subtask` plus `task_claim_file` record ownership before any edit.
- `task_plan_complete_subtask` closes a subtask using merged PR evidence.
- OpenSpec validation (`openspec validate --specs`) remains the proof gate for OpenSpec artifacts.

## §V  invariants
id|rule|cites
-|-|-
V1.always|Worker panes must not edit files outside an `agent/*` worktree owned by their CODEX_FLEET_AGENT_NAME.|-
V2.always|A plan subtask can only be marked complete when `completed_summary` carries the `PR #<n>` merged-state badge.|-
V3.always|`task_ready_for_agent` must refuse to surface work until SPEC.md is present at repo root — protects against silent claim drift across repos.|-
V4.always|The codex-fleet bringup must point `FORCE_CLAIM_REPO` at this repo so force-claim does not dispatch tasks from a sibling repo's plans.|-

## §T  tasks
id|status|task|cites
-|-|-|-
T1|todo|Keep SPEC.md present at codex-fleet repo root so Colony continues to surface ready subtasks to fleet workers.|V3.always
T2|todo|Pin fleet daemons (force-claim, claim-release-supervisor) to this repo so multi-repo agents do not dispatch into the wrong plan registry.|V4.always

## §B  bugs
id|bug|cites
-|-|-
