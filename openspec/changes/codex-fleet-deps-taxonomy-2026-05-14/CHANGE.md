---
base_root_hash: f43dddb0
slug: codex-fleet-deps-taxonomy-2026-05-14
---

# CHANGE · codex-fleet-deps-taxonomy-2026-05-14

## §P  proposal
# Typed depends_on taxonomy: artifacts / files / review-order

## Motivation

Today every `plan.json` subtask carries a single `depends_on: [N]` list,
and Colony's `task_ready_for_agent` treats it as a hard block: sub-1 with
`depends_on: [0]` is never `ready` until sub-0 reaches `done`.

In practice this conflates three very different dependency kinds:

1. **Artifact dep** — sub-1 imports a symbol or file that sub-0 creates.
   Sub-1 truly cannot start before sub-0 lands. Rare.
2. **File dep** — sub-1 and sub-0 edit the same file. They cannot
   parallel-write, but they CAN start in parallel; `task_claim_file` at
   edit time already prevents concurrent writes.
3. **Review-order dep** — the human author wants sub-0 reviewed first
   for sanity / merge readability. There is no technical block.

Because the fleet treats all three identically, plans like
`codex-fleet-overlays-phase5-2026-05-14` end up with a fully serialized
chain (sub-0 → sub-1 → sub-2 → sub-3 → sub-4 → sub-5) even though
sub-1 and sub-2 only share a file with sub-0 (file dep) and sub-5 is
purely review-order. The observable symptom on the live fleet is **7 of
8 panes idling** behind a single working pane while the chain drains.

The deps taxonomy below lets plan authors mark each edge with its true
kind, so a future Colony query can keep hard blocks (`_artifacts`) and
release soft ones (`_files`, `_review_order`) to parallel dispatch.

## Scope

In scope (this change):

- Document three new optional per-subtask fields in
  `scripts/codex-fleet/lib/plan-meta.md`.
- Define the legacy → typed migration rule.
- Provide one worked example.
- Register the Colony-side enforcement as future work.

Out of scope (separate, future change — see §Out of scope):

- Modifying Colony's `task_ready_for_agent` query.
- Modifying the cockpit / Rust dashboards to surface the new fields.
- Mass-migrating existing plans. Plans opt in individually.

## Schema delta

Three new optional fields **inline on each `tasks[<n>]` entry** (same
shape as the existing `depends_on`, `file_scope`, `subtask_index`). All
are `number[]` of sibling subtask indexes. Any combination may be set;
all default to `[]`.

```json
{
  "tasks": [
    {
      "subtask_index": 2,
      "depends_on_artifacts": [0],
      "depends_on_files": [1],
      "depends_on_review_order": [0, 1]
    }
  ]
}
```

Semantics:

- `depends_on_artifacts: number[]` — sub-N produces a file/symbol/API
  that THIS sub-task imports. Hard block. The fleet WILL gate claim on
  every listed sub-task reaching `done`. (Enforcement deferred — see
  Out of scope.)
- `depends_on_files: number[]` — sub-N touches files THIS sub-task also
  touches. Soft hint. The fleet MAY dispatch in parallel;
  `task_claim_file` at the moment of edit serializes the actual writes.
  Cockpit SHOULD surface the hint so a human can manually re-order if
  desired.
- `depends_on_review_order: number[]` — sub-N is expected to be reviewed
  or merged before this one for readability. The fleet ignores it for
  dispatch; the cockpit MAY surface it as a soft ordering badge.

## Migration

When a subtask carries the legacy `depends_on: [N]` field and **none**
of the three new fields, fleet/cockpit code MUST treat it as
`depends_on_files: [N]`. This is the conservative default: it preserves
the current behavior under the legacy "block until upstream is done"
reading while allowing plans to opt in to the looser semantics
incrementally.

When any of the three new fields IS present on a subtask, the legacy
`depends_on` field is ignored for that subtask. Authors should pick one
form per subtask and not mix.

## Acceptance criteria

- `scripts/codex-fleet/lib/plan-meta.md` documents the three fields,
  the migration rule, and a worked example.
- At least one plan under `openspec/plans/` populates the new
  taxonomy on at least one subtask. (The new overlay-modulesplit plan
  authored alongside this change is the intended first adopter.)
- A follow-up openspec change is registered for the Colony-side
  enforcement, blocked on this schema landing.

## Out of scope

The actual behavioral change — teaching Colony's `task_ready_for_agent`
to gate only on `depends_on_artifacts` (plus the `_files`-as-legacy
fallback) and to ignore `depends_on_review_order` — is **deferred**.
That work will be tracked as a separate openspec change, tentatively
`colony-typed-deps-enforcement-2026-05-XX`. Until that change ships,
the new fields are documentation-only: plans may declare them, but the
runtime still reads only the legacy `depends_on` list.

Cockpit surfacing (file-dep hints, review-order badges) is also
deferred and will be scoped under the same follow-up change or a
sibling cockpit change.

## §S  delta
op|target|row
-|-|-
add|scripts/codex-fleet/lib/plan-meta.md|depends_on_artifacts|number[]|hard artifact dep
add|scripts/codex-fleet/lib/plan-meta.md|depends_on_files|number[]|soft file-overlap hint
add|scripts/codex-fleet/lib/plan-meta.md|depends_on_review_order|number[]|cockpit-only ordering

## §T  tasks
id|status|task|cites
-|-|-|-

## §B  bugs
id|status|task|cites
-|-|-|-
