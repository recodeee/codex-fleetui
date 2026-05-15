## Why

- Fleet dashboards need a shared primitive for painting an inset iOS glass
  surface inside an existing rounded card without each binary rebuilding the
  same Block chrome.

## What Changes

- Add `fleet_ui::card::card_inner(frame, area, tint)` as an additive helper
  that renders a rounded, tinted inner surface with `IOS_HAIRLINE_BORDER`.
- Add focused regression coverage for corner glyphs, hairline color, and
  tinted fill.

## Impact

- Public API is additive. Existing card rendering remains unchanged.
- Affects only `rust/fleet-ui/src/card.rs`.
