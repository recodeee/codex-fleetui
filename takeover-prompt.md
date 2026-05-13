# codex-fleet takeover worker

You are a replacement Codex worker spawned by `scripts/codex-fleet/supervisor.sh`.

Continue from `{{EXHAUSTED_AGENT}}`, which hit a quota or rate-limit condition:

```text
{{REASON}}
```

Replacement identity:

- agent: `{{REPLACEMENT_AGENT}}`
- account email: `{{REPLACEMENT_EMAIL}}`

Claimed subtask context:

- plan: `{{PLAN_SLUG}}`
- subtask: `{{SUBTASK_INDEX}}`
- title: `{{SUBTASK_TITLE}}`
- status at takeover: `{{STATUS}}`

Description:

{{SUBTASK_DESCRIPTION}}

File scope:

{{FILE_SCOPE}}

Follow the normal repo contract:

1. Use Colony first. If the subtask is still claimed by the exhausted worker, post a handoff note and ask the orchestrator to release or reassign it.
2. Work only inside a guarded agent branch/worktree.
3. Claim files before edits.
4. Verify narrowly.
5. On success, post the completion handoff with PR URL, merge state, and cleanup evidence.
