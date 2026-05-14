# codex-fleet orchestration v2 — design

Three items deferred from the 2026-05-14 improvement set because they need
cross-system work. Each section: problem → proposal → minimal viable cut →
acceptance → risks. All three are independent in spec but partially ordered
in shipping (see each section's *Scheduling order* subsection).

Sibling artifacts in the same plan slug touch overlays, dispatch UX, and
ticker hygiene — do not duplicate that scope here. This doc is the single
source of truth for the **pull / orchestrator / dispatch** layer only.

---

## 1. Event-driven worker pulls — kill poll. workers wake on real events.

### Problem

Every codex pane runs the loop from `scripts/codex-fleet/worker-prompt.md`:

```
ready = mcp__colony__task_ready_for_agent({ agent, limit: 1 })
if empty: sleep 60; goto 2
```

`worker-prompt.md` also pins step 1 to `mcp__colony__hivemind_context` on
boot. In practice the hivemind probe leaks into the steady-state loop on
some panes (the model re-fetches context when its working memory drifts),
so each idle pane is ~1 `task_ready_for_agent` call per 60s, occasionally
plus a `hivemind_context` re-fetch.

Rough magnitude on the default 8-pane fleet, idle:

- 8 panes × 1 ready-call / 60s        = 8 calls/min   = 480/hour
- + drift-rate `hivemind_context`     ≈ 1 call/pane/5min = 96/hour
- Total floor when nothing is queued  ≈ **~575 Colony reads/hour, doing nothing**

Each call is cheap on Colony, but every read is a fresh context inflation
on the pane side (the model re-reads the tool result, the surrounding
prompt, and any working-memory delta). On `model_reasoning_effort=xhigh`
that idle floor is the dominant fleet cost when no plans are publishing.

### Proposal

Replace the 60s poll with a per-agent event source that the worker blocks
on until Colony has something for that agent. Two flavours:

1. **MCP streaming.** New tool `mcp__colony__task_ready_stream({ agent })`
   that holds the call open and emits a single `TaskReady` event when a
   claim is available. Cleaner: rides the existing MCP transport, gets
   auth + cancellation for free. Costs a Colony server change (new
   long-lived handler, plus a per-agent subscriber map).
2. **File-tail.** Colony appends one JSON line per ready-event to
   `$COLONY_HOME/queue/<agent>.jsonl`. Worker runs `tail -F` over MCP
   shell or via a thin filesystem-watching MCP tool. No new long-lived
   Colony connection, just a small append-on-claim hook.

Both shapes preserve today's worker loop *semantics* — the worker still
calls `task_ready_for_agent` after waking to claim authoritatively. The
event only carries `{ task_id, plan_slug, sub_idx, ready_at }` as a wake
signal, not a claim grant. Colony stays the source of truth.

### Minimal viable cut

**Pick file-tail for v0.** Reasoning:

- Smaller diff to Colony: one append-on-`task_make_available` hook inside
  the existing claim-state transition, plus a startup truncate. No new
  MCP handler, no streaming/cancellation correctness window.
- Per-agent file gives natural multi-reader fan-out — a future cockpit
  process can tail the same file for telemetry without competing with the
  worker.
- `tail -F` survives Colony restart automatically (file-renamed-on-rotate
  is the common rotate path; `-F` re-opens).
- If file-tail proves too brittle (FS event reliability across overlayfs
  or NFS), we promote to MCP streaming with the same event shape; only
  the transport changes, the worker prompt grammar does not.

Concretely v0 is three things:

1. Colony writes `$COLONY_HOME/queue/<agent>.jsonl` lines on every
   `available` transition that names the agent (direct claim, broadcast
   plan publish, hand-off-to-agent).
2. `worker-prompt.md` boot adds: `exec tail -F "$COLONY_HOME/queue/$AGENT.jsonl" | <loop>`
   where the loop reads lines and only then runs `task_ready_for_agent`.
3. A poll backstop stays at **300s** (not 60s) for safety against missed
   FS events. Five-minute cap is acceptable — the wakeups handle the hot
   path.

### Acceptance

- With 8 idle panes and no plan publishes, total Colony reads over 10
  minutes drop from ~80 (today's 1/min/pane floor) to **≤ 10** (the
  300s backstop firing roughly twice per pane).
- A `task_post(kind: 'queue', content: 'plan=X/sub-N agent=Y')`
  published into Colony triggers the matching pane to claim within
  **≤ 2s** wallclock, measured by `(claim_ts - publish_ts)` in Colony
  observation rows.
- Worker prompt grammar is unchanged for any branch *after* wake —
  i.e. the only behavioural change is *when* the worker calls
  `task_ready_for_agent`, not *what* it does after.

### Risks

- **Stale watchers after Colony restart.** If Colony deletes
  `$COLONY_HOME/queue/` on cold boot, every worker's `tail -F` keeps the
  old inode and silently never wakes. Mitigation: Colony truncates
  (`> $file`) rather than `rm`, and the 300s backstop catches the gap.
- **Per-agent file paths leaking plan slugs.** `queue/<agent>.jsonl`
  becomes a side-channel for whoever can read `$COLONY_HOME`. The
  worker pane already has read access; the risk is third-party panes
  (e.g. a logs window) accidentally tailing it. Mitigation: mode `0600`
  on the file, owner == colony service user.
- **`tail -F` reliability across filesystem boundaries.** If
  `$COLONY_HOME` ends up on a bind-mount or overlayfs (e.g. inside a
  future container migration), `inotify` may miss events. Mitigation:
  document the supported FS classes in the worker prompt and keep the
  poll backstop alive forever.
- **Wake storms on broadcast publish.** A plan with N agents listed all
  fire at once; first to call `task_claim` wins, the rest no-op. This is
  fine but worth measuring — if the no-op claims show up as cost, switch
  the broadcast publish to emit one line per agent on a small jittered
  fan-out (50–250ms).

### Scheduling order

- Ships **first**. §3 (send-keys-free dispatch) depends directly on the
  per-agent queue file existing. §2 (Rust orchestrator) is independent
  but is much smaller once §1 lands, because the orchestrator can read
  the same queue file instead of re-implementing a Colony event probe.

---

## 2. Consolidated Rust orchestrator — one binary owns the ticker zoo.

### Problem

`scripts/codex-fleet/full-bringup.sh` brings up a sibling tmux session
`fleet-ticker` with one window per daemon. Today's roster (`lines 365-403`
in `full-bringup.sh` and surrounding):

- `fleet-tick`                  — 15s clock + heartbeat row
- `cap-swap`                    — rotates per-pane account caps on quota
- `state-pump`                  — flattens Colony into `/tmp/claude-viz/*.json`
- `force-claim` (+ `claim-trigger`) — dispatch idle panes via send-keys
- `claim-release-supervisor`    — calls `colony rescue stranded --apply` every 60s
- `stall-watcher`               — flags claims with no progress in N minutes
- `supervisor` (optional)       — re-bringup on daemon death

Six-plus separate bash daemons, each scanning every `openspec/plans/*/plan.json`
on its own 15–60s interval. Concrete pain:

- Duplicated work. `state-pump`, `force-claim`, `stall-watcher`, and
  `claim-release-supervisor` all walk the same plan tree.
- Hard to reason about ordering. If `claim-release-supervisor` reaps a
  stranded claim 100ms before `force-claim` reads it, the dispatch fires
  on a task that's already back in the queue and races itself. Today
  this is silently absorbed by Colony's idempotent claim API, but
  observability is bad — you see the race only in Colony observation
  spam.
- Crash blast radius. A bash `set -eo pipefail` daemon that hits a
  transient `python3` JSON parse error exits and stays dead until
  `supervisor` restarts it (if `supervisor` is even on). Meanwhile its
  invariant (e.g. "no claim older than 30min stays stranded") silently
  lapses.

### Proposal

A new bin `rust/fleet-orchestrator` that subsumes the daemons into a
single event loop. One process, one log, one PID to babysit. The
orchestrator owns:

- claim-release (port of `claim-release-supervisor.sh`)
- stall detection (port of `stall-watcher.sh`)
- dispatch (port of `force-claim.sh` + `claim-trigger.sh`)
- state pump (port of `state-pump.sh`)
- plan-complete detection

It does *not* own:

- tmux pane geometry / window layout       → `style-tabs.sh` keeps that
- cap rotation                              → `cap-swap` stays a sidecar; ties to
                                              codex CLI state we don't model
- bringup orchestration                     → `full-bringup.sh` stays the entrypoint

Single 1Hz tick + event triggers. Structured logging to one file (JSONL
to `/tmp/claude-viz/orchestrator.jsonl`) so the cockpit can tail-render it.

### Event loop (ASCII)

```
                ┌──────────────────────────────────────────────┐
                │            fleet-orchestrator                │
                │                                              │
   1Hz tick ───►│  tick()                                      │
   colony evt ──►│   ├─ scan_plans()    (cached, mtime-keyed)  │
   queue line ──►│   ├─ reap_stranded() (≥30min claimed → rel) │
                │   ├─ detect_stall()   (no progress ≥N min)   │
                │   ├─ dispatch_ready() (write queue.jsonl)    │
                │   ├─ pump_state()     (flatten → viz json)   │
                │   └─ emit_log()       (one JSONL per action) │
                │                                              │
                │  state cache:                                │
                │   - plans:   slug → (mtime, parsed_json)     │
                │   - claims:  task_id → (agent, claimed_at)   │
                │   - panes:   idx → (state, last_seen)        │
                └──────────────────────────────────────────────┘
                            │            │
                            ▼            ▼
                  $COLONY_HOME/      /tmp/claude-viz/
                  queue/*.jsonl      *.json (state pump)
                                     orchestrator.jsonl (log)
```

The Colony-event input lands as a thin `inotify` reader on `$COLONY_HOME`
(reusing the §1 queue-file path) plus a periodic full reconcile every
60s. Pane state input is `tmux list-panes` + `tmux capture-pane` calls
identical to today's bash — the bash regex moves into Rust, the tmux
shell-out stays.

### Minimal viable cut

Four phases. Each phase deletes a bash daemon end-to-end before the
next phase starts. No "ports half-done; both daemons running in
parallel forever" state.

**Phase 1 — claim-release.** Simplest: 60s scan, call
`colony rescue stranded --apply`, log. No dispatch, no tmux. Single
file `src/main.rs` + `claim_release.rs`. Replaces `claim-release-supervisor.sh`.

**Phase 2 — stall-watcher.** Adds the per-claim age check. Same input
(Colony plan scan), different output (Colony `task_post` with kind=stall).
Replaces `stall-watcher.sh`.

**Phase 3 — dispatch.** Depends on §1 file-tail being live. Replaces
`tmux send-keys` in `force-claim.sh` with `append to queue/<agent>.jsonl`
(see §3). The orchestrator becomes the sole writer for the dispatch lane.
This is the phase where the tmux coupling actually goes away.

**Phase 4 — state pump + plan-complete detector.** Last because it's
the loudest output (a JSON file written every 5s) and the most
likely to surface latent dashboard assumptions about the file shape.
Cockpit (the ratatui Rust UI) gets validated against the new writer
before this phase closes.

### Acceptance

- `full-bringup.sh` ticker session shrinks from 7+ windows to **1**
  (`orchestrator`) by end of phase 4. `cap-swap` may remain as a
  sidecar (explicitly out of scope above), but every plan-walking
  daemon is gone.
- Colony query rate (counted at the Colony server) drops by **≥ 50%**
  after phase 4, measured by `task_*` call count over a 10-minute
  idle window vs. the today-baseline.
- Orchestrator restart preserves the **no-stale-claim invariant**:
  killing the process for 30s and restarting must not produce a
  stranded claim older than the existing 30-minute reap threshold.
  Verified by a restart-during-claim integration test.
- One log file. `grep -c '"event":' /tmp/claude-viz/orchestrator.jsonl`
  grows monotonically with every action.

### Risks

- **Porting bash regex to Rust types.** The dispatcher's
  `idle_panes()` is a sed/grep stack matching against a pane tail. The
  Rust port must keep parity with the exact ANSI-stripped regexes
  (`Working \([0-9]+[ms]`, `Reviewing approval request`,
  `^› (Find and fix|…)`). Mitigation: lift those into a single
  `pane_state.rs` module with a golden-file test corpus of captured
  pane tails. Do this before phase 3, even though parity only matters
  in phase 3.
- **`FORCE_CLAIM_WINDOW=overview` tmux coupling.** Phase 1 and 2 do
  not touch tmux, but phase 3 does — the orchestrator has to know
  *which window* contains the codex panes to read their state. That
  knob can't be hard-coded; it has to remain an env override matching
  today's bash defaults. Mitigation: a small `tmux_target.rs`
  ingesting `FORCE_CLAIM_SESSION`, `FORCE_CLAIM_WINDOW`,
  `CODEX_FLEET_REPO_ROOT` exactly as today.
- **Single-process blast radius.** Today, a `state-pump` bug doesn't
  kill claim-release. After consolidation it might. Mitigation: each
  subsystem runs in its own task with `tokio::select!` + per-task
  panic catch; a panic logs and continues the loop instead of
  aborting the process.
- **The `supervisor` daemon's job becomes ours.** Today there's a
  bash `supervisor` window that restarts dead daemons. After phase 4
  there's nothing to restart — only the orchestrator itself. A
  systemd-style restart-on-exit wrapper at the `tmux new-window`
  layer covers this without re-introducing the bash zoo.

### Scheduling order

- Phase 1 and Phase 2 are **independent** of §1 and §3 — ship them
  first to capture the consolidation win even if §1 stalls.
- Phase 3 **blocks on §1** (file-tail queue must exist) and **co-ships
  with §3** (deleting `force-claim.sh` is the §3 acceptance gate).
- Phase 4 is last and independent.

---

## 3. Send-keys-free dispatch — control plane stops typing.

### Problem

`scripts/codex-fleet/force-claim.sh` lines 165–179 dispatch tasks by:

```bash
tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" -l "$prompt"
tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" Enter
```

This types a literal claim prompt into the pane's stdin. Concretely it
fails or misroutes when:

- the pane is in tmux **copy-mode** or **scrollback** — keystrokes go
  into the selection cursor, not the codex CLI
- the pane is showing an **approval request** — keystrokes accept or
  reject the prompt rather than dispatching the next task
- the pane is mid-`Working (…)` — `idle_panes()` excludes these, but
  there's a race window between capture and send
- the status-line redraws mid-paste — `send-keys -l` is literal mode,
  so the prompt survives, but the trailing `Enter` can hit a stale
  buffer position on slow paints

The deeper problem is architectural: the control plane is coupled to
the presentation surface. Anything that changes the pane's UI state
(future ratatui rewrite of the worker pane, a kitty graphics overlay,
a tmux popup) becomes a dispatch correctness concern.

### Proposal

Once §1 ships, the worker tails `$COLONY_HOME/queue/<agent>.jsonl` and
processes lines as wake events. The orchestrator (§2, Rust) appends to
that queue file instead of typing into the pane. No more `send-keys`.

Wire shape:

```
orchestrator                        worker pane
─────────────                       ────────────
detect ready task        ─────►     tail -F queue/<agent>.jsonl
                                       │
append JSONL line                      ▼
{                                   read line
  "task_id": "...",                    │
  "plan_slug": "...",                  ▼
  "sub_idx": 3,                     mcp__colony__task_ready_for_agent
  "ready_at": "..."                    │
}                                      ▼
                                    claim, work, finish
```

The line itself is **a wake signal, not a claim grant** — same shape as
§1's event. Colony stays the source of truth on who-owns-what; the
queue file just tells the worker "stop sleeping, go ask Colony now."

### Minimal viable cut

Strictly dependent on §1 file-tail being live. Then it's a one-line
change in the Rust orchestrator's dispatch path (§2 phase 3):

- **Before:** `tmux send-keys -t "$SESSION:$WINDOW.$pane_idx" -l "$prompt"`
- **After:**  `append_jsonl("$COLONY_HOME/queue/<agent>.jsonl", event)`

Followed by:

1. Delete `scripts/codex-fleet/force-claim.sh`.
2. Delete `scripts/codex-fleet/claim-trigger.sh` (its job is now the
   orchestrator's `inotify` reader).
3. Update `full-bringup.sh` to stop opening the `force-claim` window.
4. Update `worker-prompt.md` to reflect the new wake source (one-line
   edit at step 2 of the Loop section).

### Acceptance

- `scripts/codex-fleet/force-claim.sh` no longer exists in the tree.
- `grep -rn 'send-keys' scripts/codex-fleet/` returns **only**
  cosmetic / presentation uses — `style-tabs.sh` and similar — and
  zero control-plane uses.
- A dispatch fired while the target pane is in copy-mode still
  triggers a claim within the worker's next loop iteration (≤ 2s),
  proving control plane is decoupled from pane UI state.
- The `force-claim` tmux window is gone from `full-bringup.sh`'s
  ticker layout.

### Risks

- **Queue file growth.** Today's dispatch is fire-and-forget; if it
  becomes append-only file lines, the file grows. Mitigation: the
  worker truncates the file after reading each line, OR the
  orchestrator rotates the file when it exceeds a small threshold
  (e.g. 1 MiB). Pick truncate-on-read for v0 — simpler, only one
  writer concern.
- **Missing acks.** With `send-keys`, success was visually obvious:
  the prompt appeared in the pane. With JSONL append, the dispatch
  side can't tell whether the worker actually woke. Mitigation: the
  worker emits a Colony `task_post(kind: 'wake', evidence: ready_at)`
  on consume, and the orchestrator measures the dispatch→wake
  latency. If a wake doesn't arrive within 5s, retry-append (same
  line, same `ready_at` — idempotent on the worker side).
- **Concurrent writers.** Colony itself writes to `queue/<agent>.jsonl`
  for native ready events (§1); the orchestrator also writes for
  dispatch decisions. Two writers on one append-only file is safe
  on POSIX **only** for writes ≤ `PIPE_BUF` (typically 4096 bytes)
  with `O_APPEND`. Mitigation: cap each JSONL line at 1 KiB and
  always open with `O_APPEND`. Lines that would exceed 1 KiB get
  rejected at write-time and logged — the wake event payload is
  small enough that this is a hard ceiling, not a soft one.
- **Direct-typing escape hatch.** Operators sometimes want to paste
  a one-off prompt into a pane to override the orchestrator (the
  pre-existing approved flow documented in
  `feedback_gx_fleet_dispatch_authorized`). Removing `send-keys`
  from the control plane must not remove the *operator's* ability
  to do this manually. Mitigation: keep `tmux send-keys` working
  at the operator's shell — the deletion is of `force-claim.sh`
  and its in-process automation, not of tmux's ability to receive
  keystrokes.

### Scheduling order

- Strictly **after §1** (queue file must exist).
- Co-ships with **§2 phase 3** (the Rust dispatch port is where the
  `send-keys` line gets replaced).
- Independent of §2 phases 1, 2, and 4.

---

## Cross-cutting notes

### Shipping order, end-to-end

```
§1 (file-tail queue)
  └─► §2 phase 3 (dispatch port)
        └─► §3 (delete force-claim.sh)

§2 phase 1 (claim-release)   ── independent, ship first or in parallel
§2 phase 2 (stall-watcher)   ── after phase 1
§2 phase 4 (state pump)      ── last; cockpit gate
```

### Out of scope

- Worker pane visual rewrite (ratatui-based codex pane). Tracked
  separately under `codex-fleet-overlays-phase5-2026-05-14`.
- Cap rotation (`cap-swap`). Stays bash for now; binds to codex CLI
  internals we don't model in Colony.
- Colony server's `task_ready_for_agent` semantics. Unchanged. This
  doc is purely about *when* workers call it and *who* triggers
  that call.

### Verification gates per item

- §1: a 10-minute idle observation window with Colony read counts
  before/after, plus a one-shot end-to-end "publish → claim ≤ 2s"
  measurement.
- §2: phase-by-phase, the deleted bash daemon's invariants
  restated as a Rust integration test (stranded reap, stall
  detection, dispatch fan-out, state-pump output diff against
  the bash baseline).
- §3: `grep` acceptance above, plus a copy-mode-during-dispatch
  manual reproduction confirming the worker still wakes.
