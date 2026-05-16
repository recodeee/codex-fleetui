# codex-fleet

Multi-account Codex worker pool. Spawns N parallel `codex` panes in a tmux
session, each on its own `~/.codex/accounts/<email>.json` auth, all pulling
work from a shared [Colony](https://github.com/colonyq) task queue.

Ships with:

- **`full-bringup.sh`** — single command. Publishes the priority plan,
  cap-probes the account pool, stages N isolated `CODEX_HOME`s, creates
  the tmux session (overview / fleet / plan / waves / review / watcher /
  conductor windows) and the sibling `fleet-ticker` daemon session (ticker /
  cap-swap / state-pump / force-claim / claim-release / stall-watcher).
- **`force-claim.sh`** — polls Colony every 15 s, dispatches ready
  sub-tasks to idle codex panes via `tmux send-keys`.
- **`claim-release-supervisor.sh`** — watches for panes that go idle
  while still holding a Colony claim; releases the claim so force-claim
  can re-route the work.
- **`cap-swap-daemon.sh`** — replaces capped panes with healthy accounts
  using a live `codex exec` probe (not the agent-auth meter — those are
  different things).
- **iOS-style tmux chrome** (`style-tabs.sh`, `watcher-board.sh`, etc.) —
  rounded pill tabs, clickable status row, six animated dashboards.
- **`conductor.sh`** — interactive Claude operator that lives in the
  `conductor` tmux window. Briefed via `conductor-system-prompt.md` to
  supervise the autonomous daemons (force-claim, claude-supervisor,
  plan-watcher, …), read fleet state, and broadcast intent through Colony
  `task_messages` — the same shared context bus the workers use. Attach
  with `tmux attach -t codex-fleet` and switch to the `conductor` window
  to chat. Opt-out with `CODEX_FLEET_CONDUCTOR=0`.

## Install

```bash
git clone git@github.com:recodeee/codex-fleet.git ~/codex-fleet
cd ~/codex-fleet
bash install.sh
```

`install.sh` symlinks `~/.claude/skills/codex-fleet` → `<clone>/skills/codex-fleet`,
seeds `scripts/codex-fleet/accounts.yml` from the example, and prints the
PATH / env additions you should add to your shell rc.

## Dependencies

Standard:

- `bash` ≥ 4, `tmux` ≥ 3.4, `kitty`, `python3` ≥ 3.10, `git`, `inotifywait`,
  `jq`.

Account / task layer:

- `codex` CLI + `agent-auth` (Anthropic Codex)
- `colony` CLI ([colonyq](https://github.com/colonyq)) for the task queue,
  plan publishing, and stranded-claim rescue.
- One auth file per worker at `~/.codex/accounts/<email>.json`. Generate
  them with `agent-auth login` per account.

## Usage

```bash
# bring up an 8-pane fleet against the newest openspec/plans/* plan
bash scripts/codex-fleet/full-bringup.sh --n 8

# pin to a specific plan
bash scripts/codex-fleet/full-bringup.sh \
  --plan-slug my-feature-2026-05-14 --n 4

# attach
tmux attach -t codex-fleet

# tear down
bash scripts/codex-fleet/down.sh
```

## Plan-source layout

`full-bringup.sh` reads OpenSpec-style plans from
`$REPO/openspec/plans/<slug>/plan.json`. Out of the box, `$REPO` is the
codex-fleet clone itself — so seed your plans there OR set
`CODEX_FLEET_REPO_ROOT` to point at your project repo:

```bash
CODEX_FLEET_REPO_ROOT=$HOME/Documents/my-project \
  bash ~/codex-fleet/scripts/codex-fleet/full-bringup.sh --n 8
```

The fleet runs entirely against the plan tree referenced by that env var;
the clone itself contributes the scripts, not the plans.

## Configuration

`scripts/codex-fleet/accounts.yml` (gitignored, copied from
`accounts.example.yml` by `install.sh`):

```yaml
accounts:
  - id: research
    email: research@example.com         # ~/.codex/accounts/research@example.com.json
    skills: [research, planning]
    rate_limit_tier: high
  - id: coding
    email: coding@example.com
    skills: [implementation, testing]
    rate_limit_tier: standard
```

The `skills` list is informational — Colony's `task_ready_for_agent`
reads it to bias routing toward the right pane, but does not gate
assignment.

## Self-healing

- **force-claim** dispatches available tasks to idle panes (~15 s loop)
- **claim-release-supervisor** releases stale claims from agents whose
  panes went idle without completing (~60 s loop)
- **cap-swap-daemon** replaces capped accounts (~30 s loop)
- **stall-watcher** runs `colony rescue stranded --apply` for claims
  held > 30 min without progress

Together: a pane that dies, gets stuck on a usage-limit, or just stops
polling Colony is recovered automatically without operator intervention.

## Optional supervisors

By default, `full-bringup.sh` skips the kitty-spawning takeover
supervisor (legacy autonomous rescue that opens one external kitty
window per replacement) because most fleets prefer the
in-tmux `claim-release` daemon. Re-enable explicitly:

```bash
CODEX_FLEET_SUPERVISOR=1 bash scripts/codex-fleet/full-bringup.sh ...
```

## Skill

`skills/codex-fleet/SKILL.md` is the Claude Code orchestrator skill.
After `install.sh`, Claude Code in any repo recognizes "codex fleet"
trigger phrases and routes through this skill (lifecycle, monitoring,
deduping handoffs, dispatching tasks).

## License

MIT. See `LICENSE`.

## Origin

Extracted from [`recodeee/recodee/scripts/codex-fleet/`](https://github.com/recodeee/recodee)
in May 2026. Full commit history preserved via `git subtree split`.
