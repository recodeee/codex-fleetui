# Tasks · codex-fleet-deps-taxonomy-2026-05-14

Checklist of the work authorized by `CHANGE.md` in this directory.

- [x] Extend `scripts/codex-fleet/lib/plan-meta.md` with the three new
  optional fields (`depends_on_artifacts`, `depends_on_files`,
  `depends_on_review_order`), their semantics, and the legacy
  `depends_on` → `depends_on_files` migration rule. Worked example
  showing a single subtask populating all three fields included.
- [x] Author `CHANGE.md` for this change (this directory): motivation,
  scope, schema delta, migration, acceptance, out-of-scope.
- [ ] At least one plan in `openspec/plans/` populates the new
  taxonomy on at least one subtask. The new overlay-modulesplit plan
  authored alongside this change (sibling agent) is the intended first
  adopter; coordination is via field names only, no cross-file edits.
- [ ] Open follow-up openspec change for the Colony-side enforcement
  (tentative slug `colony-typed-deps-enforcement-2026-05-XX`) covering:
  - `task_ready_for_agent` gates on `depends_on_artifacts` only.
  - Legacy `depends_on` is interpreted as `depends_on_files` for
    backward compatibility.
  - `depends_on_review_order` is ignored for dispatch.
  - Cockpit surfacing for `_files` hints and `_review_order` badges
    (may split into a sibling cockpit change).

  Tracked as a TODO; not in scope for this PR.
