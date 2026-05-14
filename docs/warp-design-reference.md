# Warp Design Reference for fleet-ui

Source survey: `~/Documents/warp-reference/crates/warp_core/src/ui/theme/*`,
`~/Documents/warp-reference/crates/warp_core/src/ui/appearance.rs`,
`~/Documents/warp-reference/crates/warp_core/src/ui/builder.rs`,
`~/Documents/warp-reference/crates/warpui_core/src/ui_components/*`, and
`~/Documents/warp-reference/crates/ui_components/src/*`. This document maps
Warp's reusable UI ideas into the terminal-only `rust/fleet-ui` palette and
ratatui surfaces. It does not require Rust source changes.

## Palette

Warp treats a theme as a small set of primitives plus derived overlays:
background, foreground, accent, cursor, terminal ANSI colors, and detail
opacities. Reusable components should call theme methods instead of reaching
into internal color helpers.

| Warp token | Source value | Role | fleet-ui mapping |
| --- | --- | --- | --- |
| `background` | theme supplied | large terminal/app background | `IOS_BG_SOLID` (`#1c1c1e`) |
| `foreground` | theme supplied | primary text and icons | `IOS_FG` (`#f2f2f7`) |
| `accent` | theme supplied | primary action/focus color | `IOS_TINT` (`#0a84ff`) |
| `surface_1` | foreground over background at 5% | low-raised UI panels | `IOS_BG_GLASS` (`#262628`) |
| `surface_2` | foreground over background at 10% | grouped rows, inactive controls | `IOS_CARD_BG` (`#2c2c30`) |
| `surface_3` | foreground over background at 15% | raised controls and disabled fill | `IOS_CHIP_BG` (`#36363a`) |
| `outline` / `fg_overlay_2` | foreground at 10% opacity | subtle borders | `IOS_HAIRLINE` (`#3c3c41`) |
| `fg_overlay_3` | foreground at 15% opacity | stronger borders/split panes | `IOS_HAIRLINE_STRONG` (`#55555a`) |
| `main_text_color` | contrast-picked text at 90% opacity | default label text | `IOS_FG` (`#f2f2f7`) |
| `sub_text_color` | contrast-picked text at 60% opacity | secondary labels | `IOS_FG_MUTED` (`#a0a0aa`) |
| `disabled_text_color` | contrast-picked text at 40% opacity | disabled/faint labels | `IOS_FG_FAINT` (`#6e6e78`) |
| `accent_overlay_1` | accent at 10% opacity | quiet selected tint | `IOS_TINT_SUB` (`#d2e0ff`) when text, `IOS_TINT_DARK` (`#0764dc`) when fill |
| `accent_overlay_2` | accent at 25% opacity | block selection/find selection | `IOS_TINT_DARK` (`#0764dc`) |
| `accent_overlay_3` | accent at 40% opacity | pressed primary button | `IOS_TINT_DARK` (`#0764dc`) |
| `accent_overlay_4` | accent at 60% opacity | hovered primary button | `IOS_TINT` (`#0a84ff`) |
| `ui_warning_color` | `#c28000` | warning state | `IOS_ORANGE` (`#ff9f0a`) |
| `ui_error_color` | `#bc362a` | error state | `IOS_DESTRUCTIVE` (`#ff453a`) |
| `ui_yellow_color` | `#e5a01a` | caution/highlight state | `IOS_YELLOW` (`#ffd60a`) |
| `ui_green_color` | `#1ca05a` | success state | `IOS_GREEN` (`#30d158`) |
| `tooltip_background` | `neutral_6` | high-contrast tooltip fill | `IOS_FG` fill with `IOS_BG_SOLID` text, or `IOS_CHIP_BG` for in-terminal overlays |

Phenomenon theme constants are useful as a direct Warp snapshot: background
`#121212`, foreground `#faf9f6`, accent `#2e5d9e`, blue `#3780e9`, modal
background `#2a2a2a`, modal badge text `#bf409d`, modal feature title
`#e6e6e6`, and modal feature description `#9b9b9b`. For fleet-ui, keep the
existing iOS dark palette as the concrete ratatui source of truth and use the
Warp tokens as semantic names.

## Typography

Warp separates UI text from terminal text. `Appearance` stores
`monospace_font_family`, `monospace_font_size`, `monospace_font_weight`,
`ui_font_family`, AI/password font families, and `line_height_ratio`.
`DEFAULT_UI_FONT_SIZE` is `12.0`, `DEFAULT_COMMAND_PALETTE_FONT_SIZE` is
`14.0`, header text is `18.0`, overline text is `10.0`, and the mock/test line
height ratio is `1.4`.

Warp font weights are `Thin`, `ExtraLight`, `Light`, `Normal`, `Medium`,
`Semibold`, `Bold`, `ExtraBold`, and `Black`; style is `Normal` or `Italic`.
The terminal equivalent is intentionally coarser:

| Warp typography | Terminal-realistic equivalent |
| --- | --- |
| UI body `12.0 Normal` | plain `Style::default().fg(IOS_FG)` |
| UI secondary `12.0 Normal` | `IOS_FG_MUTED`; avoid extra decoration |
| Overline `10.0` | uppercase/short labels in `IOS_FG_FAINT` |
| Command palette `14.0` | one-cell command rows; active labels may use `Modifier::BOLD` |
| Button default `14.0 Semibold` | `Modifier::BOLD` for primary/action labels |
| Button small `12.0 Semibold` | compact bold labels, no extra row height |
| Header `18.0` | card or overlay title with `Modifier::BOLD` |
| Italic | avoid as a dependency; many terminals render it inconsistently |
| Line height `1.4` | reserve extra blank rows only in large overlays; most rows stay one cell high |

Use monospace assumptions for all ratatui rendering. Width-sensitive labels
should be measured with terminal cell width, then clipped or elided; do not
borrow pixel font metrics directly.

## Spacing scale

Warp's component spacing is pixel-based. ratatui spacing is cell-based, so the
mapping should preserve density and hit hierarchy rather than exact pixels.

| Fleet scale | Warp source examples | ratatui use |
| --- | --- | --- |
| `0` cells | no margin/padding | tight separators, joined spans |
| `1` cell | list item padding `2`, chip vertical padding `2`, small button inner gap `2`, tooltip vertical padding `3-4`, icon gaps `4-5` | row gutters, chip caps, shortcut padding, adjacent icon/text gap |
| `2` cells | button horizontal padding `8-12`, tooltip horizontal padding `7-8`, text input padding `10`, button inner spacing `4` | normal left/right inset for rows, overlays, buttons, and tooltip text |
| `3` cells | base button horizontal padding `15`, autosuggestion tooltip horizontal padding `14` | roomy overlay commands and action-sheet rows |
| `4+` cells | dialog/card gutters, command palette breathing room | large modal side gutters only |

Specific Warp sizes to preserve semantically:

| Warp measurement | Terminal adaptation |
| --- | --- |
| Border width `1.0` | one ratatui border cell or hairline-colored separator |
| Border radius `4.0` | `BorderType::Rounded` where a block exists; otherwise cap spans |
| Default button height `32`, small button height `24` | one terminal row; add a second row only for multiline labels |
| Default button icon `16`, small icon `14` | one glyph/icon cell, with one-cell text gap |
| Chip horizontal padding `4`, vertical padding `2` | one cell around label; use cap spans for rounded illusion |
| Chip max label width `240` px | clamp by available terminal cells with `visible_width` |
| Progress bar width `70`, height `2` | configurable cell width; one-row rail using filled/empty segments |
| Tooltip offsets `4-8` px | adjacent overlay row above/below the anchor |
| List item default padding `2` | one-cell indent; one row per item |

## Component Intents

**button**

Warp buttons are stateful hover/click containers with `Basic`, `Secondary`,
`Accent`, `Outlined`, `Warn`, `Error`, `Text`, and `Link` variants. Newer
button themes simplify that to primary, secondary, disabled, and naked styles.
For fleet-ui, keep one-row buttons: primary uses `IOS_TINT` + bold foreground,
secondary uses transparent fill + `IOS_HAIRLINE`, naked/link uses foreground or
accent text only. Pressed/hovered states should swap to darker/lighter tint
tokens, not change layout.

**chip**

Warp chips are compact label containers with optional leading icon and optional
close button. They use small padding, a border, and style-provided radius/color.
fleet-ui already models this with cap spans around status labels; keep chips
single-row, color-coded by semantic state, and cap/truncate labels before they
push neighboring columns.

**progress_bar**

Warp's progress bar divides a fixed width into foreground progress and
background remainder; the builder default is width `70` and height `2`. In
fleet-ui this should remain a segmented rail: compute filled cells from percent
and width, use the semantic axis color for filled cells, and use
`IOS_HAIRLINE`/surface tokens for the remainder.

**text_input**

Warp text input wraps an editor view in a clipped container with background,
border, optional dimensions, and `10` px padding. In a terminal, use an
outlined row or overlay input band: one-cell horizontal padding, muted border,
and no visual resize when the cursor, placeholder, or validation text changes.
Focused input should use accent border; disabled input should use faint text.

**tool_tip**

Warp tooltips are small high-contrast overlays with a one-pixel border, `4` px
radius, compact padding, and optional shortcut/sublabel content. Positioning is
anchored to the hovered element with a small offset. For fleet-ui, prefer a
single overlay row above/below the anchor, background contrast strong enough for
`IOS_BG_SOLID` text, and keep shortcut hints in a muted mini-chip.

**list**

Warp lists render numbered or bulleted text rows with default `2` px padding
and theme-provided font/color. In ratatui, lists should be stable one-row items:
prefix with index/bullet/status dot, keep labels left-aligned, and reserve the
right edge for shortcuts or badges. Selection should tint the row, not alter
row height.

**notification**

Warp framework notifications are platform-level payloads with title, body,
optional data, and sound flag. The documented practical limits are title `40`
characters and body `120` characters. fleet-ui should treat notifications as
short status banners or event rows: title first, body clipped to the available
cells, semantic color from success/warn/error, and hidden machine data kept out
of visible text.

## Cross-reference Table

| Token or intent | Warp source | fleet-ui surface |
| --- | --- | --- |
| `background` | `WarpTheme::background()` | dashboard background, full-pane clears, inactive overlay backdrop |
| `surface_1` | `neutral_1` / foreground 5% | tab-strip/logo/live chip fill (`IOS_BG_GLASS`) |
| `surface_2` | `neutral_2` / foreground 10% | cards, inactive segmented tabs, list row backgrounds |
| `surface_3` | `neutral_3` / foreground 15% | shortcut chips, raised menu rows, disabled controls |
| `foreground` | `WarpTheme::foreground()` | primary labels, active tab text, overlay titles |
| `text_sub` | contrast text at 60% | secondary metadata, timestamps, inactive tabs |
| `text_disabled` | contrast text at 40% | unavailable actions, faint helper labels, elided metadata |
| `accent` | `WarpTheme::accent()` | active tab, selected command, primary action, focused input |
| `accent_overlay_2` | accent 25% | selected list row or context-menu highlight |
| `outline` | `fg_overlay_2` | card borders, text input border, tooltip border |
| `split_pane_border_color` | `fg_overlay_3` | strong overlay/card border and pane separators |
| `ui_green_color` | `#1ca05a` | done/live/success chip; `IOS_GREEN` |
| `ui_warning_color` | `#c28000` | waiting/rate-limit/caution chip; `IOS_ORANGE` |
| `ui_error_color` | `#bc362a` | failed/blocked/destructive action; `IOS_DESTRUCTIVE` |
| `ui_yellow_color` | `#e5a01a` | needs-attention/highlight; `IOS_YELLOW` |
| `Button::Primary` | accent fill, contrast text | one-row primary command/action button |
| `Button::Secondary` | transparent fill, neutral border | secondary command, toolbar action |
| `Button::Naked` / link | text-only action | inline action or shortcut-like command |
| chip | label + optional icon/close, compact padding | status chips in watcher/plan tree/tab strip |
| progress bar | foreground/background width split | `rail::progress_rail` and quota/usage bars |
| text input | clipped editor container | command prompt row, filter/search overlay input |
| tooltip | anchored compact overlay | hover/help row, shortcut hint, disabled reason |
| list | numbered/bulleted padded rows | command palette rows, menu choices, session list |
| notification | title/body/data/sound payload | status banner, event feed row, blocker alert |
