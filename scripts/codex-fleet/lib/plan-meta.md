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

### `metadata.subtasks[<n>].difficulty: "trivial"|"standard"|"hard"`

Per-subtask hint consumed by the worker prompt's tier router. Schema
declared here; routing logic lives in `worker-prompt.md` (Agent B owns).
Default when absent: `"standard"`.

## Example

```json
{
  "plan_slug": "demo-2026-05-14",
  "metadata": {
    "writable_roots": [
      "/home/deadpool/Documents/codex-fleet",
      "/home/deadpool/Documents/recodee"
    ],
    "subtasks": {
      "0": { "difficulty": "trivial" },
      "1": { "difficulty": "hard" }
    }
  },
  "tasks": [ /* ... */ ]
}
```
