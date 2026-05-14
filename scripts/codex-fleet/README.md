# codex-fleet

Spin up a tmux session of parallel **codex workers**, each logged in
under a different `~/.codex/accounts/<email>.json` account, all pulling
tasks from a shared **Colony queue**. The host Claude session is the
orchestrator; the panes are the worker bees.

This is the answer to "I have N codex accounts — can I have N parallel
codex agents working on one feature without burning a single account's
quota?". Yes, you can.

## Architecture

```
host Claude session (orchestrator)
   │
   │ task_propose × N  (Colony MCP)
   ▼
Colony task queue (~/.colony/data.db)
   ▲
   │ task_ready_for_agent  (each pane polls)
   │
┌──┴────── tmux session: codex-fleet ─────────────────────────┐
│ pane 1   CODEX_HOME=/tmp/codex-fleet/research/  codex (...) │
│ pane 2   CODEX_HOME=/tmp/codex-fleet/coding/    codex (...) │
│ pane 3   CODEX_HOME=/tmp/codex-fleet/review/    codex (...) │
│ pane 4   CODEX_HOME=/tmp/codex-fleet/docs/      codex (...) │
└─────────────────────────────────────────────────────────────┘
```

Each pane has its **own isolated `CODEX_HOME`**, populated by `up.sh`
from a chosen `~/.codex/accounts/<email>.json` auth file. That isolation
matters because `codex` reads `$CODEX_HOME/auth.json` at startup and any
shared file would race between panes after spawn.

## Quick start

```bash
# 1. Copy the example config and edit it to match your real accounts.
cp scripts/codex-fleet/accounts.example.yml scripts/codex-fleet/accounts.yml
$EDITOR scripts/codex-fleet/accounts.yml

# 2. Verify each `email:` field has a matching auth file.
for f in $(awk -F': ' '/email:/{print $2}' scripts/codex-fleet/accounts.yml); do
  test -f "$HOME/.codex/accounts/$f.json" \
    && echo "✓ $f" \
    || echo "✗ MISSING: $HOME/.codex/accounts/$f.json"
done

# 3. Dry-run to inspect the plan.
bash scripts/codex-fleet/up.sh --dry-run

# 4. Bring the fleet up.
bash scripts/codex-fleet/up.sh

# 5. From the orchestrator session, propose tasks.
#    (Use the Colony MCP tools from your host Claude session.)

# 6. Tear down when done.
bash scripts/codex-fleet/down.sh           # preserves /tmp/codex-fleet/
bash scripts/codex-fleet/down.sh --purge   # also wipes the staged auth files
```

## How it routes tasks

The orchestrator does **not** push tasks directly to specific panes. It
proposes tasks into the Colony queue tagged with skills / hints, and
panes pull via `task_ready_for_agent` on their own.

This is intentional:

- **Fault tolerance** — a dead pane (network, rate-limit, panic) does
  not block work. Other panes pick up the orphaned claim.
- **Skill match** — the orchestrator hints which `skills` a task wants
  (e.g. `skills: [code-review]`); Colony's ready queue surfaces it to a
  pane whose agent profile includes that skill first.
- **Rate-limit absorption** — when one account hits 429, that pane
  releases the claim and sleeps; another pane with a different account
  picks it up within the next ready-poll cycle.

The worker prompt in `worker-prompt.md` codifies these rules in the
codex pane's behavior.

## Claim dispatch mode

`force-claim.sh --loop` starts `claim-trigger.sh` in the background and
keeps a slower 30s polling pass as a backstop. The trigger watches
`openspec/plans/*/plan.json` plus Colony WAL/event files under
`~/.colony`, then wakes the first idle `codex-fleet:overview` pane.

Set `CODEX_FLEET_CLAIM_MODE` to choose the path:

- `both` (default): event trigger plus 30s poll backstop
- `event`: event trigger only
- `poll`: polling only, no trigger

## Requirements

- `tmux` on PATH
- `codex` on PATH (the actual CLI, not just `codex login` — must run a
  worker-loop session)
- `inotifywait` on PATH for event-driven claim dispatch
- `python3` on PATH (used by the YAML parser in `up.sh` to avoid a
  hard `yq` dep)
- Auth files staged at `~/.codex/accounts/<email>.json` for every
  account referenced in `accounts.yml`
- Colony MCP server reachable from `codex` (configure once via
  `codex mcp add colony ...`)

## File layout

```
scripts/codex-fleet/
├── README.md              this file
├── up.sh                  spawn tmux session, stage CODEX_HOME per account
├── down.sh                tear down session, optionally purge staged dirs
├── accounts.example.yml   commit-tracked template
├── accounts.yml           your real config (gitignored)
└── worker-prompt.md       prompt loaded into every codex pane at boot
```

Run `bash scripts/codex-fleet/up.sh --help` for full flag reference.

## Limitations

- **Single-host only.** All panes run on the same machine. Distributing
  panes across hosts would need an explicit message-bus / process
  supervisor (k8s, nomad). Out of scope.
- **No automatic account discovery.** You have to list which accounts
  to use in `accounts.yml`. The script does not enumerate
  `~/.codex/accounts/` for you, because most users have stale or
  disabled accounts mixed in there.
- **No live UI.** The status line is plain tmux. The Colony viewer
  (`colony viewer` web UI) shows the task queue + observations in real
  time — that is the recommended dashboard while the fleet runs.
- **Smoke-test caveat.** This crate cannot self-test the multi-account
  flow on a single-account dev box. Per-account isolation is verified
  by the `CODEX_HOME=...` per-pane env line; the actual "different
  account" verification is a manual `nvidia-smi`-equivalent: spawn the
  fleet, attach to two panes, run a token-counting MCP call in each,
  confirm two distinct usage counters move.

## Related

- recodee#1702 — `codex-gpu-embedder` perf pack (lets Colony's
  semantic features run at GPU latency)
- colony#515 — `codex-gpu` embedding provider
- colony#517 — `semantic_search` MCP tool
- colony#518 — `cluster_observations` MCP tool (handoff dedupe)

Together: the fleet propose-and-supervise UX gets sub-50ms semantic
recall + dedupe from the same hardware that runs the workers.
