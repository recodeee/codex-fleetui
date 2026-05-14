# Microsandbox Integration

Microsandbox is an optional isolation layer for codex-fleet verification and
short-lived experiments. It is not part of the main worker loop. Workers still
run in tmux panes on Guardex worktrees so they can see the host repository,
Colony claims, git state, GitHub credentials, cargo caches, and operator-visible
pane output.

## Why microsandbox in codex-fleet

The fleet can create high test pressure: many agents may finish near the same
time and run `cargo test`, `pnpm test`, or prototype commands against one host.
Those commands share target directories, temp paths, package-manager caches, and
process state unless we isolate them.

Microsandbox fits two narrower surfaces:

- Sandboxed verification, where a plan step runs tests inside a disposable
  runtime instead of racing other agents on the host.
- Ephemeral experiments, where an agent can try an untrusted tool, dependency,
  script, or generated command without giving it direct access to the host
  working tree.

The goal is damage containment and reproducibility. Any speed improvement is a
bonus, not the contract.

## What we use it for

Use microsandbox through the fleet wrapper scripts, not by scattering raw `msb`
calls through task prompts:

- `scripts/codex-fleet/lib/sandbox-run.sh` for arbitrary plan verification
  commands.
- `scripts/codex-fleet/lib/sandboxed-cargo-test.sh` for Rust crate tests.
- `scripts/codex-fleet/lib/sandboxed-pnpm-test.sh` for package tests.
- `scripts/codex-fleet/bin/microsandbox-mcp-install.sh` when an operator wants
  the microsandbox MCP server registered for local AI CLIs.

The wrappers keep fallback behavior consistent. When `msb` is missing, or when
`MICROSANDBOX_DISABLE=1` is set, verification runs on the host and prints a
clear fallback message instead of failing a normal development lane.

## What we deliberately do not use it for

Do not replace the main worker loop with microsandbox. Fleet workers need host
filesystem access for git, `gh`, cargo, pnpm, staged `CODEX_HOME` directories,
and Colony file-claim coordination.

Do not replace tmux plus Guardex worktrees. The supervisor depends on pane-level
visibility, tmux status, worktree paths, branch ownership, and Colony
observations. A microVM is a good test boundary, not an operator surface.

Do not position microsandbox as a startup accelerator. Cold images, mounts, and
runtime setup can add cost. The integration is opt-in for verification and
experiments where isolation matters more than the fastest possible launch.

## Caveats

Microsandbox is still a beta dependency for codex-fleet purposes. Keep failures
non-fatal where possible and prefer host fallback for routine verification when
the runtime is unavailable.

The host must support the backend. Linux machines need KVM; Apple laptops need
Apple Silicon. Other environments should expect fallback or an operator setup
step before sandboxed runs work.

Each sandboxed run may consume disk for images, writable layers, caches, and
mounted working copies. Long-lived cache growth should be treated as operator
maintenance, not as a worker problem.

Microsandbox is Apache-2.0 licensed. That license is compatible with the
codex-fleet MIT license for this optional integration, but preserve upstream
license notices in vendored examples or copied snippets.

## Worked example

A plan's verification step can request isolated Rust tests like this:

```bash
scripts/codex-fleet/lib/sandbox-run.sh \
  --image rust \
  --cwd "$PWD" \
  -- cargo test -p fleet-ui
```

For a Colony task, record the exact command in the task evidence:

```text
verification=scripts/codex-fleet/lib/sandbox-run.sh --image rust --cwd "$PWD" -- cargo test -p fleet-ui PASS
```

If `msb` is installed, the wrapper runs the command in a disposable runtime. If
`msb` is unavailable or `MICROSANDBOX_DISABLE=1` is set, the same command runs
on the host and the wrapper emits a fallback note to stderr. The plan still gets
one verification command and one result line.

## Bootstrapping

Install the CLI with:

```bash
bash scripts/codex-fleet/install-microsandbox.sh
```

Then, if MCP access is desired for local AI CLIs, run:

```bash
bash scripts/codex-fleet/bin/microsandbox-mcp-install.sh
```

Both steps are operator setup, not worker-loop requirements. A worker lane should
use the wrappers and let them decide whether to run inside microsandbox or fall
back to the host.
