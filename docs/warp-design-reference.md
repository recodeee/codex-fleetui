# Warp Design Reference for fleet-ui

## Palette

Warp's reusable color model is semantic: a theme exposes background,
foreground, accent, surfaces, text roles, overlays, and status colors. The
fleet-ui equivalent is the iOS-dark ratatui palette in `fleet-ui/src/palette.rs`.

| Semantic token | Warp source | Warp fact | fleet-ui equivalent |
| --- | --- | --- | --- |
| `background` | `warp_core/src/ui/theme/color.rs:100-103`; Phenomenon sample `warp_core/src/ui/theme/phenomenon.rs:7` | Large terminal/app background; Phenomenon uses `#121212`. | `IOS_BG_SOLID` `#1c1c1e` (`fleet-ui/src/palette.rs:23`, `fleet-ui/src/palette.rs:49`) |
| `foreground` | `warp_core/src/ui/theme/color.rs:96-98`; Phenomenon sample `warp_core/src/ui/theme/phenomenon.rs:8` | Primary text/icon source; Phenomenon uses `#faf9f6`. | `IOS_FG` `#f2f2f7` (`fleet-ui/src/palette.rs:19`, `fleet-ui/src/palette.rs:43`) |
| `primary` / `accent` | `warp_core/src/ui/theme/color.rs:91-94`; Phenomenon sample `warp_core/src/ui/theme/phenomenon.rs:9` | Primary action/focus fill; Phenomenon uses `#2e5d9e`. | `IOS_TINT` `#0a84ff` (`fleet-ui/src/palette.rs:13`, `fleet-ui/src/palette.rs:35`) |
| `accent_hover` | `ui_components/src/button/themes.rs:35-41`; Phenomenon blue `warp_core/src/ui/theme/phenomenon.rs:10` | Hovered primary buttons move from accent toward a stronger accent overlay; Phenomenon blue is `#3780e9`. | `IOS_TINT` or `IOS_TINT_DARK` (`fleet-ui/src/palette.rs:35`, `fleet-ui/src/palette.rs:57`) |
| `surface_1` | `warp_core/src/ui/theme/color.rs:126-131`; neutral math `warp_core/src/ui/theme/color.rs:493-498` | Background mixed with foreground at 5%. | `IOS_BG_GLASS` `#262628` (`fleet-ui/src/palette.rs:22`, `fleet-ui/src/palette.rs:48`) |
| `surface_2` | `warp_core/src/ui/theme/color.rs:118-123`; neutral math `warp_core/src/ui/theme/color.rs:500-505` | Background mixed with foreground at 10%. | `IOS_CARD_BG` `#2c2c30` (`fleet-ui/src/palette.rs:27`, `fleet-ui/src/palette.rs:53`) |
| `surface_3` | `warp_core/src/ui/theme/color.rs:110-115`; neutral math `warp_core/src/ui/theme/color.rs:507-512` | Background mixed with foreground at 15%. | `IOS_CHIP_BG` `#36363a` (`fleet-ui/src/palette.rs:26`, `fleet-ui/src/palette.rs:52`) |
| `modal_surface` | `warp_core/src/ui/theme/phenomenon.rs:15` | Phenomenon modal background is `#2a2a2a`. | `IOS_CARD_BG` `#2c2c30` (`fleet-ui/src/palette.rs:53`) |
| `outline` | `warp_core/src/ui/theme/color.rs:154-156`; overlay math `warp_core/src/ui/theme/color.rs:533-538` | Foreground overlay at 10%. | `IOS_HAIRLINE` `#3c3c41` (`fleet-ui/src/palette.rs:24`, `fleet-ui/src/palette.rs:50`) |
| `strong_outline` | split-pane/restored overlay `warp_core/src/ui/theme/color.rs:340-347`; overlay math `warp_core/src/ui/theme/color.rs:540-543` | Foreground overlay at 15%. | `IOS_HAIRLINE_STRONG` `#55555a` (`fleet-ui/src/palette.rs:25`, `fleet-ui/src/palette.rs:51`) |
| `main_text` | `warp_core/src/ui/theme/color.rs:168-170`; opacity math `warp_core/src/ui/theme/color.rs:469-472` | Contrast-picked text at 90% opacity. | `IOS_FG` `#f2f2f7` (`fleet-ui/src/palette.rs:43`) |
| `secondary_text` | `warp_core/src/ui/theme/color.rs:172-174`; opacity math `warp_core/src/ui/theme/color.rs:474-477` | Contrast-picked text at 60% opacity. | `IOS_FG_MUTED` `#a0a0aa` (`fleet-ui/src/palette.rs:20`, `fleet-ui/src/palette.rs:44`) |
| `faint_text` | `warp_core/src/ui/theme/color.rs:176-183`; opacity math `warp_core/src/ui/theme/color.rs:479-481` | Hint/disabled text uses lower-opacity contrast text. | `IOS_FG_FAINT` `#6e6e78` (`fleet-ui/src/palette.rs:21`, `fleet-ui/src/palette.rs:45`) |
| `danger` | `warp_core/src/ui/theme/color.rs:142-144` | UI error color is `#bc362a`. | `IOS_DESTRUCTIVE` `#ff453a` (`fleet-ui/src/palette.rs:14`, `fleet-ui/src/palette.rs:36`) |
| `warning` | `warp_core/src/ui/theme/color.rs:138-140` | UI warning color is `#c28000`. | `IOS_ORANGE` `#ff9f0a` (`fleet-ui/src/palette.rs:16`, `fleet-ui/src/palette.rs:38`) |
| `attention` | `warp_core/src/ui/theme/color.rs:146-148` | UI yellow color is `#e5a01a`. | `IOS_YELLOW` `#ffd60a` (`fleet-ui/src/palette.rs:17`, `fleet-ui/src/palette.rs:39`) |
| `success` | `warp_core/src/ui/theme/color.rs:150-152` | UI green color is `#1ca05a`. | `IOS_GREEN` `#30d158` (`fleet-ui/src/palette.rs:15`, `fleet-ui/src/palette.rs:37`) |
| `purple_badge` | `warp_core/src/ui/theme/phenomenon.rs:16-17` | Phenomenon modal badge uses `#bf409d` text and a low-opacity fill. | `IOS_PURPLE` `#bf5af2` (`fleet-ui/src/palette.rs:18`, `fleet-ui/src/palette.rs:40`) |
| `tooltip_surface` | `warp_core/src/ui/theme/color.rs:357-360`; tooltip styles `warp_core/src/ui/builder.rs:148-164` | Tooltip background is a high-contrast neutral surface. | `IOS_CHIP_BG` for in-terminal tooltips, or inverted `IOS_FG`/`IOS_BG_SOLID` for high contrast (`fleet-ui/src/palette.rs:43`, `fleet-ui/src/palette.rs:52`) |
| `icon_tile` | Icon/text sizing in `warpui_core/src/ui_components/button.rs:302-323`; chip icon sizing in `warpui_core/src/ui_components/chip.rs:57-64` | Warp uses styled icon/text rows; fleet-ui has a dedicated tile token. | `IOS_ICON_CHIP` `#46464c` (`fleet-ui/src/palette.rs:28`, `fleet-ui/src/palette.rs:54`) |
| `active_shadow` | pressed/hovered accent behavior `warp_core/src/ui/theme/color.rs:43-52`; button pressed style `warp_core/src/ui/builder.rs:310-351` | Button states blend accent/background rather than resize controls. | `IOS_TINT_DARK` `#0764dc` (`fleet-ui/src/palette.rs:29`, `fleet-ui/src/palette.rs:57`) |
| `subtitle_on_accent` | Phenomenon body/label text `warp_core/src/ui/theme/phenomenon.rs:11-14` | Lighter accent-adjacent label color. | `IOS_TINT_SUB` `#d2e0ff` (`fleet-ui/src/palette.rs:30`, `fleet-ui/src/palette.rs:58`) |

No `*.json` or token-bearing `*.toml` files were found under the `warpui*`
crates; the matching TOML files are crate manifests, not design-token config.

## Typography

Warp stores font choices as family IDs and metrics rather than fixed family
names. `Appearance` carries monospace, UI, AI, and password font family IDs,
plus monospace size/weight and line-height ratio (`warp_core/src/ui/appearance.rs:19-32`,
`warp_core/src/ui/appearance.rs:71-99`). `FontInfo` is where display family
names live when enumerated from the platform (`warpui_core/src/fonts.rs:200-208`).

| Warp typography fact | Source | Terminal-realistic equivalent |
| --- | --- | --- |
| UI body size is `12.0`; command palette size is `14.0`; header size is `18.0`; overline size is `10.0`. | `warp_core/src/ui/appearance.rs:8-13`, `warp_core/src/ui/appearance.rs:305-326` | Point size does not apply in ratatui. Use one terminal cell row for body/command labels, bold for headers, and faint uppercase labels for overlines. |
| Warp keeps monospace font family, size, and weight separate from UI font family. | `warp_core/src/ui/appearance.rs:19-32`, `warp_core/src/ui/appearance.rs:281-302` | Treat all fleet-ui text as monospace. Size by visible width in cells, not pixels. |
| Font weights are `Thin`, `ExtraLight`, `Light`, `Normal`, `Medium`, `Semibold`, `Bold`, `ExtraBold`, `Black`. | `warpui_core/src/fonts.rs:27-48` | Terminals usually expose normal, bold, sometimes faint/italic. Map `Semibold` and above to `Modifier::BOLD`; map lower-priority weight to muted color. |
| Font style is `Normal` or `Italic`. | `warpui_core/src/fonts.rs:142-149` | Avoid italic as a required signal; some terminal themes ignore it. Use color and prefix glyphs instead. |
| Font metrics track units-per-em, ascent, descent, and line gap. | `warpui_core/src/fonts/metrics.rs:1-21` | Approximate vertical rhythm with row counts: one row for controls, one optional subtext row, blank rows only inside spacious overlays. |
| Warp has a line-height ratio field; test/mock setup uses `1.4`. | `warp_core/src/ui/appearance.rs:118-119`, `warp_core/src/ui/appearance.rs:325-326` | For fleet-ui, line height is one terminal row. A `1.4` feel maps to occasional spacer rows around modal sections, not fractional rows. |
| Font rasterization tracks subpixel alignment. | `warpui_core/src/fonts.rs:170-198` | Ignore subpixel behavior. Ratatui alignment should use `unicode-width`/visible cell counts and stable `Rect` columns. |

Typography mapping for fleet-ui:

| UI role | Warp source | fleet-ui terminal rule |
| --- | --- | --- |
| Body label | default UI font size `12.0` (`warp_core/src/ui/appearance.rs:12`) | `IOS_FG`, normal modifier, clip by visible width. |
| Secondary metadata | sub text opacity 60% (`warp_core/src/ui/theme/color.rs:474-477`) | `IOS_FG_MUTED`, no bold. |
| Faint placeholder | disabled/faint opacity 40% (`warp_core/src/ui/theme/color.rs:479-481`) | `IOS_FG_FAINT`, no extra punctuation unless it aids scanning. |
| Header/title | header size `18.0` (`warp_core/src/ui/appearance.rs:8-10`) | `IOS_FG` + `Modifier::BOLD`, one row. |
| Command label | command palette size `14.0` (`warp_core/src/ui/appearance.rs:12-13`) | one row; selected/focused row may add `Modifier::BOLD`. |
| Shortcut hint | keyboard shortcut font from command palette (`warp_core/src/ui/builder.rs:354-360`) | compact chip, visible width reserved before rendering. |

## Spacing scale

Warp spacing is pixel-based through `Coords` and component styles
(`warpui_core/src/ui_components/components.rs:17-33`,
`warpui_core/src/ui_components/components.rs:56-73`). `Lines` and `Pixels`
can convert by line height (`warpui_core/src/units.rs:31-42`,
`warpui_core/src/units.rs:138-145`), but fleet-ui should use terminal `Rect`
rows/columns directly.

| Fleet scale | Warp source examples | ratatui `Rect` math |
| --- | --- | --- |
| `0` cols/rows | no padding or margin in `UiComponentStyles` (`warpui_core/src/ui_components/components.rs:56-73`) | Adjacent spans and rails; no `Rect` shrink. |
| `1` col | small button inner spacing `2`, chip vertical padding `2`, list padding `2` (`ui_components/src/button/params.rs:142-155`, `warpui_core/src/ui_components/chip.rs:81-88`, `warpui_core/src/ui_components/list.rs:64-79`) | `Rect { x: inner.x + 1, width: inner.width - 2, .. }` for compact rows. |
| `2` cols | button horizontal padding `8-12`, tooltip horizontal padding `7-8`, text input padding `10` (`ui_components/src/button/params.rs:125-139`, `warp_core/src/ui/builder.rs:148-164`, `warp_core/src/ui/builder.rs:119-135`) | Standard row insets; overlay inner rects usually shrink by 2 per side. |
| `3` cols | base button left/right padding `15`, autosuggestion tooltip left/right `14` (`warp_core/src/ui/builder.rs:86-107`, `warp_core/src/ui/builder.rs:167-180`) | Roomy command rows and action sheets; shrink by 3 per side only when width allows. |
| `1` row control | default button height `32`, small button height `24`, progress height `2` (`ui_components/src/button/params.rs:125-155`, `warp_core/src/ui/builder.rs:138-145`) | Render as one terminal row; use bold/color for state, not extra height. |
| `2+` rows overlay | tooltip offsets `4-8` px and overlay positioning (`warpui_core/src/ui_components/button.rs:402-438`, `warp_core/src/ui/builder.rs:439-543`) | Place popups one row above/below anchors; use `Clear` + card block for modal content. |
| bounded label | chip max width `240` px (`warpui_core/src/ui_components/chip.rs:67-70`) | Convert to max visible cells based on available `Rect.width`; elide before rendering. |
| progress width | default width `70` px (`warp_core/src/ui/builder.rs:138-145`) | Caller supplies rail cell width; filled cells = `(pct * width) / 100`. |

Practical rule: subtract horizontal insets before measuring text, reserve the
right-side shortcut/badge width first, then render the label into the remaining
cells. This matches current fleet-ui overlay geometry (`fleet-ui/src/overlay.rs:109-114`,
`fleet-ui/src/overlay.rs:196-215`).

## Component Intents

**button**

Use for explicit actions, mode switches, and toolbar commands. Warp supports
content as label, icon, or icon+label, plus default/small/custom sizing
(`ui_components/src/button/params.rs:53-68`,
`ui_components/src/button/params.rs:114-155`). Visual variants should
map to primary, secondary, disabled, and naked/link themes
(`ui_components/src/button/themes.rs:30-122`). In fleet-ui, keep the
button one row high; state changes alter color/boldness, not geometry.

**chip**

Use for compact state labels, filters, badges, and low-cost metadata. Warp
chips accept a label, optional icon, optional close button, border, radius, and
compact padding (`warpui_core/src/ui_components/chip.rs:15-40`,
`warpui_core/src/ui_components/chip.rs:52-88`). In fleet-ui, chips are fixed
visible-width span groups so tables stay aligned (`fleet-ui/src/chip.rs:89-110`).
Choose the color from semantic status, not from the label string.

**progress_bar**

Use for quota, completion, and capacity signals where exact numbers are less
important than trend/bucket. Warp splits a fixed width into foreground and
background segments (`warpui_core/src/ui_components/progress_bar.rs:7-37`) and
builder defaults to accent foreground over background (`warp_core/src/ui/builder.rs:138-145`).
In fleet-ui, use `progress_rail`: caps plus filled/empty cells, with color
thresholds by axis (`fleet-ui/src/rail.rs:19-71`).

**text_input**

Use for command palettes, filters, search, and modal prompts. Warp wraps an
editor view in a clipped container with optional padding, border, background,
height, and width (`warpui_core/src/ui_components/text_input.rs:8-35`,
`warpui_core/src/ui_components/text_input.rs:38-83`). In fleet-ui, render a
single-row input band by default; focused state uses accent border, placeholder
uses faint text, and validation/status text goes to a separate row.

**tool_tip**

Use for short hover help, disabled reasons, and shortcut hints. Warp tooltips
are small text containers with font, color, padding, border, background, and an
optional sublabel at 40% opacity (`warpui_core/src/ui_components/tool_tip.rs:7-14`,
`warpui_core/src/ui_components/tool_tip.rs:80-114`). Anchoring is handled from
buttons/builders with small offsets (`warpui_core/src/ui_components/button.rs:394-440`).
In fleet-ui, tooltips should be one-row overlays near the anchor unless the
message needs a modal.

**list**

Use for menu options, command results, worker/session rows, and grouped choices.
Warp list supports numbered or bulleted rendering and per-item padding from
styles (`warpui_core/src/ui_components/list.rs:8-21`,
`warpui_core/src/ui_components/list.rs:51-80`). In fleet-ui, list rows should
remain one terminal row with a reserved right column for shortcuts or badges.
Selection changes row tint/accent, not row height.

**notification**

Use for short status banners and event-feed rows, not for long logs. Warp's
framework notification payload is title, body, optional data, and sound flag
(`warpui_core/src/notification.rs:8-18`), with practical limits of title `40`
and body `120` characters (`warpui_core/src/notification.rs:20-28`). In
fleet-ui, display title first, clip body to available cells, and keep machine
data out of the visible row.

## Cross-reference Table

| Token/component | codex-fleet surface |
| --- | --- |
| `accent` / `primary` | `status_chip(ChipKind::Working)` uses `IOS_TINT` (`fleet-ui/src/chip.rs:45-57`); planned Spotlight selection/focus uses tint. |
| `danger` | blocked/capped chips use `IOS_DESTRUCTIVE` (`fleet-ui/src/chip.rs:51-54`); destructive context-menu rows use `IOS_DESTRUCTIVE` (`fleet-ui/src/overlay.rs:178-195`). |
| `warning` | polling/cap-warning rails and chips use `IOS_ORANGE` (`fleet-ui/src/chip.rs:51`, `fleet-ui/src/rail.rs:44-48`). |
| `attention` | approval/review chip uses `IOS_YELLOW` (`fleet-ui/src/chip.rs:54`). |
| `success` | done/live chip and low-risk rail buckets use `IOS_GREEN` (`fleet-ui/src/chip.rs:52`, `fleet-ui/src/rail.rs:44-48`). |
| `surface_1` | tab-strip/logo/live chip glass fill; palette token is `IOS_BG_GLASS` (`fleet-ui/src/palette.rs:48`). |
| `surface_2` | dashboard cards and grouped panes; palette token is `IOS_CARD_BG` (`fleet-ui/src/palette.rs:53`). |
| `surface_3` | shortcut chips and raised menu rows; palette token is `IOS_CHIP_BG` (`fleet-ui/src/palette.rs:52`). |
| `outline` / `strong_outline` | card borders, overlay hairlines, and pane separators (`fleet-ui/src/overlay.rs:123-129`, `fleet-ui/src/palette.rs:50-51`). |
| `main_text` | worker row account/status text in `fleet-state` (`rust/fleet-state/src/main.rs:190-218`). |
| `secondary_text` | worker subtext and inactive metadata (`rust/fleet-state/src/main.rs:220-222`). |
| `faint_text` | empty/reserve worker text (`rust/fleet-state/src/main.rs:209-214`). |
| `button` | planned action buttons in overlays; state should follow primary/secondary/naked mapping, one row high. |
| `chip` | `status_chip` across watcher/state/plan-tree/waves dashboards (`fleet-ui/src/chip.rs:89-110`). |
| `progress_bar` | weekly and five-hour quota rails in `fleet-state` (`rust/fleet-state/src/main.rs:199-203`), implemented by `progress_rail` (`fleet-ui/src/rail.rs:51-71`). |
| `text_input` | planned Spotlight input/filter row; use one-row input band with accent focus. |
| `tool_tip` | planned shortcut hints and disabled-reason overlays; one-row anchored overlay where possible. |
| `list` | worker-row list in `fleet-state` (`rust/fleet-state/src/main.rs:260-280`) and command/context-menu rows (`fleet-ui/src/overlay.rs:126-135`). |
| `notification` | status banner/event-feed row; constrain visible text to title/body limits and semantic color. |
