# plan.json metadata schema

Optional `metadata` object on the plan root. Consumers: `full-bringup.sh`
(writable_roots) and the worker prompt's tier router (difficulty).

## Fields

### `metadata.writable_roots: string[]`

Absolute paths passed as `--add-dir <path>` to `codex` at worker spawn.
Required when the plan touches files outside the codex-fleet repo
(workspace-write sandbox blocks writes outside listed roots → workers spin
on `outside writable roots` / `Read-only file system`).

Fallback when absent or empty: `["/home/deadpool/Documents/recodee",
"/home/deadpool/Documents/codex-fleet"]`.

`full-bringup.sh` preflights each path: `test -d` + `test -w`. Missing or
read-only → `die`. Fix with `chmod` / `chown` / remount.

### `tasks[<n>].difficulty: "trivial"|"standard"|"hard"`

Per-subtask hint consumed by the worker prompt's tier router. Lives inline
on each `tasks[]` entry (same shape as `depends_on`, `file_scope`, etc.).
Default when absent: `"standard"`.

### Typed `depends_on` taxonomy (schema only — enforcement deferred)

The legacy per-task `depends_on: [N]` field conflates three distinct
dependency kinds and over-serializes the fleet. Plans MAY split it into
three typed fields, **inline on each `tasks[]` entry** (same shape as
the existing `depends_on`). All three are optional `number[]` lists of
sibling subtask indexes. Colony enforcement of the new fields is **not
yet shipped**; see change `codex-fleet-deps-taxonomy-2026-05-14`.

- `depends_on_artifacts: number[]` — sub-N produces a file, symbol, or
  API that THIS sub-task imports. Fleet MUST wait for sub-N to be
  `done` before claiming. Hard block. Rare.
- `depends_on_files: number[]` — sub-N edits files THIS sub-task also
  edits. Fleet MAY dispatch in parallel; `task_claim_file` at edit time
  prevents concurrent writes. Surfaced in the cockpit as a hint.
- `depends_on_review_order: number[]` — sub-N should be reviewed/merged
  before this one for human readability. No technical block. Fleet
  ignores; cockpit surfaces it as a soft ordering.

Migration: when only the legacy `depends_on: [N]` is present (none of
the three taxonomy fields set), it is treated as
`depends_on_files: [N]` — the conservative default that preserves
today's serialized behavior until plans opt in.

## Example

```json
{
  "plan_slug": "demo-2026-05-14",
  "metadata": {
    "writable_roots": [
      "/home/deadpool/Documents/codex-fleet",
      "/home/deadpool/Documents/recodee"
    ]
  },
  "tasks": [
    { "subtask_index": 0, "difficulty": "trivial", "depends_on": [] },
    { "subtask_index": 1, "difficulty": "hard", "depends_on": [] },
    {
      "subtask_index": 2,
      "difficulty": "standard",
      "depends_on_artifacts": [0],
      "depends_on_files": [1],
      "depends_on_review_order": [0, 1]
    }
  ]
}
```
