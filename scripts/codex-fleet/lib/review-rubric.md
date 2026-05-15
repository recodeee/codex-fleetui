# codex-fleet auto-review rubric

Answer each bullet for the supplied diff with one of: `yes`, `no`, `n/a`.
Cite a file path + hunk when answering `yes` to a risk bullet.

## REGRESSION RISK

- Does the diff change observable behavior beyond what the plan's `acceptance_criteria` promised?
- Does it touch a code path covered by an existing test/fixture without updating that test?
- Does it change a public function signature, env-var contract, CLI flag, or output format that another lane or downstream script consumes?
- Does it silently swallow errors (e.g. `|| true`, `try/except: pass`) where the prior code surfaced them?
- Does it remove logging, metrics, or assertions that another component depends on?

## SCOPE CREEP

- Does any change touch a file outside the sub-task's declared `file_scope` in `plan.json`?
- Does it edit a file that another lane in the same plan owns (claim collision)?
- Does it introduce new top-level files, crates, or scripts not enumerated in the plan?
- Does it bundle an unrelated drive-by fix that should be its own PR?
- Does the commit message describe work outside this sub-task's title?

## ANTI-PATTERN FLAGS (CLAUDE.md violations)

- Backwards-compat shim, deprecation alias, or "old + new path" branch with no migration plan?
- Dead comments, commented-out code, `TODO`/`FIXME` without an owner or ticket?
- Premature abstraction: a trait/interface/wrapper used in exactly one place?
- Error handling for genuinely impossible states (e.g. `unreachable!` guarded by an existing invariant) that adds noise?
- New dependency added for a one-liner that the stdlib already covers?

## BLAST RADIUS

- Touches CI config, build scripts, Dockerfiles, or release pipelines?
- Touches DB migrations, schema, or seed data?
- Edits a shared helper, base script, or workspace `Cargo.toml`/`pnpm-workspace.yaml`?
- Changes `.env.example`, secrets handling, or auth flow?
- Modifies a path used by an active long-running daemon (plan-watcher, cap-swap, supervisor)?
