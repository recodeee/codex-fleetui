# Architecture Decision Records

This directory holds ADRs (Architecture Decision Records) for the
codex-fleet future protocol. ADRs capture *why* a decision was made —
the rationale, alternatives, and consequences — so future contributors
do not re-litigate settled debates.

## When to write an ADR

- A `docs/future/PROTOCOL.md` improvement moves to REJECTED.
- A non-obvious architectural choice gets accepted.
- A previously SHIPPED improvement is rolled back.
- A subsystem captain rotates.

## Numbering

- ADRs are numbered monotonically: `001-<slug>.md`, `002-<slug>.md`, …
- The slug after the number is short, lowercase, hyphenated.
- Numbers are never reused. If an ADR is superseded, the new ADR
  takes the next free number and the old ADR's status changes to
  `SUPERSEDED-BY-ADR-NNN`.

## Format

Copy `_template.md` and fill in. Keep ADRs to a single page.

## Index

| # | Title | Status | Date |
|---|-------|--------|------|
| _ | _template.md | TEMPLATE | — |

(Replace this seed row as ADRs land.)
