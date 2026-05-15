# Design — fleet-tui ratatui port

## Boundary

The fleet has two layers and only one moves.

| Layer          | Examples                                         | Language after |
|----------------|--------------------------------------------------|----------------|
| Orchestration  | `full-bringup.sh`, `cap-swap-daemon.sh`, `force-claim.sh`, `cap-probe.sh`, `claim-release-supervisor.sh`, `style-tabs.sh` | **stays bash** |
| Rendering      | `watcher-board.sh`, `fleet-state-anim.sh`, `plan-tree-anim.sh`, `waves-anim*.sh`, `fleet-tick.sh` (rendering half) | **becomes Rust (ratatui)** |
| Data plumbing  | `plan.json` parsers, `agent-auth list` parsers, tmux pane introspection | **becomes Rust (typed)** |

`style-tabs.sh` *renders* (it sets tmux options + bindings) but it operates
on the tmux server, not on a frame buffer. It stays bash.

## Binary shape: four small binaries, not one

Two options were considered:

- **(A) One binary, internal tabs.** `fleet-tui` runs in a single tmux
  pane, tabs replace tmux windows.
- **(B) Four small binaries**, drop-in replacements for the four
  `*-anim.sh` scripts that `full-bringup.sh` launches into separate tmux
  windows.

**Chosen: B.** Reasons:

1. `full-bringup.sh` already creates windows `0 watcher / 1 overview /
   2 fleet / 3 plan / 4 waves` and `style-tabs.sh` decorates them as
   iOS tabs that mouse-click between windows. Migrating one binary at a
   time keeps that intact.
2. Each port is independently mergeable. A single binary makes Phase 3
   a 4-way port that has to land atomically — much higher PR risk.
3. The user's existing skill doc already calls each tab by its
   per-script name; preserving the 1:1 mapping reduces churn in
   `skills/codex-fleet/SKILL.md`.

Option A can revisit after all four binaries are proven equivalents.

## Crate layout

```
rust/
├── fleet-ui/        library crate — design system widgets
│   ├── palette.rs   iOS system colours as Color::Rgb consts
│   ├── chip.rs      status_chip(kind) -> Vec<Span>; pill ◖ ● working ◗
│   ├── rail.rs      progress_rail(pct, axis, width); segmented gauge
│   ├── card.rs      thin wrapper over Block with BorderType::Rounded + padding
│   └── overlay.rs   centered_overlay(area, w, h); Clear + bordered popup
├── fleet-data/      library crate — typed data layer
│   ├── plan.rs      serde structs for openspec/plans/*/plan.json
│   ├── accounts.rs  agent-auth list parser → Account { email, 5h%, weekly%, … }
│   └── panes.rs     tmux introspection, PaneState enum
├── fleet-watcher/   bin replacing watcher-board.sh
├── fleet-state/     bin replacing fleet-state-anim.sh + fleet-tick.sh (render half)
├── fleet-plan-tree/ bin replacing plan-tree-anim.sh
└── fleet-waves/     bin replacing waves-anim*.sh
```

Two library crates because `fleet-ui` (design system) and `fleet-data`
(typed data plumbing) are independently useful and the dependency graph
is `bin → ui + data`, `ui ⫮ data` (no cross-dep between the libs).

## Phases

### Phase 0 — POC (this PR)

Single file `rust/fleet-tui-poc/src/main.rs`. ~150 lines. Renders one
iOS chip + one rounded card. Logs mouse-click coords. Three things to
verify when running in a tmux pane:

1. `Color::Rgb(0, 122, 255)` renders as the same systemBlue as the
   surrounding tmux chrome (visual diff against a `style-tabs.sh`
   active tab).
2. `BorderType::Rounded` does not double-frame against tmux's
   `pane-border-status top` + `pane-border-format ' #[…] ▭ #{@panel} '`.
3. crossterm mouse-click events reach the binary through tmux's
   `mouse on` pass-through (click the chip → coord appears in the log).

POC is **throwaway**. Once Phase 1 ships, `rust/fleet-tui-poc/` deletes.

### Phase 1 — `fleet-ui` design-system crate

Implement `palette.rs`, `chip.rs`, `rail.rs`, `card.rs`, `overlay.rs`.
Each module ships with `insta` snapshot tests so the existing
`scripts/codex-fleet/test-*.sh` regression suite has a Rust analogue.

**Cannot start** until POC validates and the freeze gate
(`agent/* worktree count = 0`) passes. See proposal.md.

### Phase 2 — `fleet-data` typed data layer

`plan.rs` (`plan.json` deser), `accounts.rs` (`agent-auth list`
parser), `panes.rs` (tmux introspection + `PaneState` enum). First
port: keep scraping `tmux capture-pane` for pane classification. A
follow-up plan replaces scraping with a real signal (Colony presence
file). Don't conflate the two.

### Phase 3 — `fleet-watcher` (port one view end-to-end)

`watcher-board.sh` chosen first because it is the **hardest** —
Python-in-bash heredoc, the most complex layout, the most state
classification. Proves the framework on the difficult case so Phase 4
is mechanical.

Migration evidence required before Phase 4 starts: side-by-side
screenshots of `watcher-board.sh` vs `fleet-watcher` showing visual
parity, plus a 10-minute soak test in a real fleet pane confirming
identical pane-state classification across all 8 workers.

### Phase 4 — `fleet-state`, `fleet-plan-tree`, `fleet-waves`

Independently mergeable now that `fleet-ui` + `fleet-data` exist.
Order: state → plan-tree → waves (decreasing complexity).

### Phase 5 — Overlays

The Image 2 surfaces (context menu, spotlight palette). Triggered by
keypress in any view binary, rendered via `overlay.rs`. Uses
`tui-input` for the spotlight text field.

### Phase 6 — Retire bash

Update `full-bringup.sh` to launch the Rust binaries. Keep the bash
scripts one release as a `FLEET_DASHBOARD_RENDERER=bash` fallback.
Update `skills/codex-fleet/SKILL.md` "Canonical visual design" table.
After one quiet week, delete the bash dashboards.

## Dependencies

- `ratatui` 0.28 — TUI framework
- `crossterm` 0.28 — backend + events
- `serde` + `serde_json` — `plan.json` deser
- `insta` — snapshot tests (dev-dep)
- `tui-input` — spotlight text field (Phase 5 only)

Deliberately **not** using `tokio`. A synchronous tick loop on a 1s
interval is simpler than async and matches the bash scripts' polling
shape. Revisit only if a real dashboard needs sub-100 ms reactivity,
which none currently do.

## Risks (and what mitigates them)

| Risk                                                         | Mitigation                                                 |
|--------------------------------------------------------------|------------------------------------------------------------|
| `BorderType::Rounded` clashes with tmux pane borders         | POC validates before Phase 1 starts                        |
| Truecolor banding inside tmux on some terminals              | POC tested in operator's actual kitty + tmux config        |
| Mouse events don't pass through tmux                         | POC logs coord events; if no events arrive, design changes |
| Port loses behaviour from a battle-scar comment in bash      | Phase 3 requires side-by-side soak before Phase 4          |
| 4 binaries × ~1MB each = release-binary bloat                | Acceptable; can collapse to single binary in v2            |
| Compile step adds friction the bash scripts didn't have      | `cargo install --path …` once per host; bins go to PATH    |

## Rejected alternatives

- **Helix (Zellij UI library).** Same niche as ratatui, smaller
  ecosystem. ratatui is the safer bet.
- **A TUI written in Bun/Deno.** The parent project has Rust toolchain
  already (`codex-lb-runtime`, `codex-gpu-embedder`). Adding a Node
  TUI is a third runtime to maintain.
- **Keep bash, replace screen-scraping with Colony presence files.**
  Improves the data layer without the UI port. Worth doing
  *eventually* — but the rendering bugs (flicker, SGR math, wrap
  fragments) live in the UI layer, so the UI port is the win.
