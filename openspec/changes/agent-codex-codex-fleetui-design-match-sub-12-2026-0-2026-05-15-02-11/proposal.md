## Why

- The design references include a distinct `live` chip state alongside the
  existing status pills, and the current cap background should blend with the
  base dark surface rather than a literal black fill.

## What Changes

- Add a `Live` chip kind mapped to the success-green surface.
- Render chip caps against `IOS_BG_SOLID` so the pill reads as a single
  surface on dark backgrounds.
- Keep the fixed-width three-span pill shape and seven-cell label budget.

## Impact

- Affects every caller of `fleet_ui::chip::status_chip`.
- Visual-only change; regression coverage stays in `fleet-ui` tests.
