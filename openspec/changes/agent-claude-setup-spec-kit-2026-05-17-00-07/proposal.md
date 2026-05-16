## Why

- Bring Spec-Driven Development (SDD) slash skills into codex-fleetui so Claude sessions can use `/speckit-specify`, `/speckit-plan`, `/speckit-tasks`, `/speckit-implement` and friends alongside the existing gx workflow.

## What Changes

- Add tracked `.specify/` (workflows, templates, scripts, integration manifests, constitution skeleton, bundled git extension).
- Add 14 `.claude/skills/speckit-*` skill prompt files.
- Append a 3-line `<!-- SPECKIT START -->` marker to `AGENTS.md`.
- No source, build, or runtime changes.

## Impact

- No runtime behavior change.
- New slash skills become invocable in Claude sessions started at the repo root.
