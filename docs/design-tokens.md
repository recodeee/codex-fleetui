# codex-fleetui Design Tokens

This document records the reusable visual tokens from the `images/*.html`
design references. The HTML files are computed-style exports, so the source
column names the artboard file where the value is visible. Values are facts
from the references; implementation guidance is adapted for ratatui, where
alpha, blur, radius, shadows, and point sizes have to become terminal colors,
cell counts, borders, and glyph choices.

## Source Artboards

| Ref | Source file | Surface |
| --- | --- | --- |
| A | `images/A _ Context menu _ pinned to active pane.html` | Pinned context menu over an active pane |
| B | `images/B _ Spotlight _ search-first palette.html` | Spotlight/search-first command palette |
| C | `images/C _ Action sheet _ grouped _ Cancel.html` | Grouped action sheet with separate cancel action |
| D | `images/D _ Session switcher _ card stack.html` | Session/window switcher cards |
| E | `images/E _ Glass dock _ floating top nav.html` | Floating top navigation dock |
| F | `images/F _ Section menu _ _K jump grid.html` | Section-jump grid |
| G | `images/G _ Fleet _ worker list _ live status.html` | Fleet worker list with live status |
| H | `images/H _ Plan _ wave tree_ topo levels.html` | Plan wave tree and topological levels |
| I | `images/I _ Waves _ spawn timeline.html` | Wave spawn timeline |
| J | `images/J _ Review _ approval queue.html` | Review approval queue |

## Palette

The references mix two layers:

- A terminal transcript layer in A-F, using GitHub-dark-like terminal colors.
- An iOS glass dashboard layer in G-J, using dark UIKit label, fill, accent,
  and status colors.

Ratatui has no alpha compositing, blur, or shadows. Where the source is
`rgba(...)`, the fleet equivalent should either use the existing opaque
`fleet_ui::palette` token or pre-blend a new opaque token in a palette lane.

### Core Surfaces

| Token | Source value | Source artboards | fleet-ui equivalent |
| --- | --- | --- | --- |
| `canvas.github_dark` | `#0d1117` (`rgb(13, 17, 23)`) | A-F frame background | No direct token. Use only for terminal-capture backdrops; nearest existing is `IOS_BG_SOLID` `#1c1c1e` in `rust/fleet-ui/src/palette.rs`. |
| `canvas.ios_dark` | `#0b0d12` (`rgb(11, 13, 18)`) | G-J page background with blue/orange radial wash | No direct token. Palette lane should consider `IOS_CANVAS_BG`; nearest existing is `IOS_BG_SOLID` `#1c1c1e`. |
| `surface.glass` | `rgba(38, 38, 40, 0.78)` over dark | A, E, G-J glass cards/overlays | `IOS_BG_GLASS` `#262628` (`rust/fleet-ui/src/palette.rs`). Alpha is not represented. |
| `surface.solid` | `#1c1c1e` / `rgba(28, 28, 30, 0.92)` | B-C modal surfaces | `IOS_BG_SOLID` `#1c1c1e`. |
| `surface.card` | `rgba(255, 255, 255, 0.04)` to `rgba(255, 255, 255, 0.08)` | G-J row/card fills | Existing `IOS_CARD_BG` `#2c2c30` is the closest opaque token. Consider explicit `IOS_FILL_04`, `IOS_FILL_06`, `IOS_FILL_08` if fidelity requires buckets. |
| `surface.chip` | dark raised chip fills, typically `rgba(255, 255, 255, 0.06)` or local dark fills | A-J shortcut/status chips | Existing `IOS_CHIP_BG` `#36363a`. |
| `surface.icon_chip` | dark square/rounded icon tile, near `#46464c`/`#484f58` | B, D, F, J icon cells | `IOS_ICON_CHIP` `#46464c`. |
| `border.hairline` | `rgba(255, 255, 255, 0.14)` and `rgba(120, 120, 128, 0.2)` | A-J subtle card/menu borders | Existing `IOS_HAIRLINE` `#3c3c41`. |
| `border.hairline_strong` | `rgba(255, 255, 255, 0.22)` to `rgba(255, 255, 255, 0.25)` | E-J active glass/card separators | Existing `IOS_HAIRLINE_STRONG` `#55555a`. |
| `shadow.dark` | `rgba(0, 0, 0, 0.3)` to `rgba(0, 0, 0, 0.7)` | A-F overlays, E dock, G-J cards | No ratatui equivalent. Use one-cell offset shadow rows sparingly, or omit when layout density matters. |

### Text

| Token | Source value | Source artboards | fleet-ui equivalent |
| --- | --- | --- | --- |
| `text.primary` | `#f2f2f7` (`rgb(242, 242, 247)`) | A-J labels, titles, active text | `IOS_FG` `#f2f2f7`. |
| `text.secondary_alpha` | `rgba(235, 235, 245, 0.60)` | B-J subtitles, metadata | Existing `IOS_FG_MUTED` `#a0a0aa`; alpha source should remain semantic as `secondaryLabel`. |
| `text.tertiary_alpha` | `rgba(235, 235, 245, 0.35)` | A-J placeholders, quiet metadata | Existing `IOS_FG_FAINT` `#6e6e78`. |
| `text.strong_secondary` | `rgba(235, 235, 245, 0.78)` | D selected-card/body text | No exact token. Use `IOS_FG` for selected text or add `IOS_FG_SECONDARY_STRONG` if many selected rows need it. |
| `text.terminal_primary` | `#c9d1d9` (`rgb(201, 209, 217)`) | A-F pane transcripts | No fleet-ui token. Use only when rendering terminal transcript examples. |
| `text.terminal_muted` | `#7d8590` (`rgb(125, 133, 144)`) | A-F transcript comments/paths | No fleet-ui token. Nearest existing is `IOS_FG_MUTED`. |
| `text.terminal_faint` | `#484f58` (`rgb(72, 79, 88)`) | A-F dim terminal glyphs | No fleet-ui token. Nearest existing is `IOS_FG_FAINT`. |

### Accents And Status

| Token | Source value | Source artboards | fleet-ui equivalent |
| --- | --- | --- | --- |
| `accent.primary` | `#0a84ff` (`rgb(10, 132, 255)`) | B, C, E-J focused controls, links, active tabs | `IOS_TINT` `#0a84ff`. |
| `accent.primary_soft` | `rgba(10, 132, 255, 0.08)` to `rgba(10, 132, 255, 0.50)` | G-J background wash, selected bars, focus rings | Existing `IOS_TINT`/`IOS_TINT_DARK`; add opaque soft-blue buckets only when a view needs background tint. |
| `accent.primary_terminal` | `#58a6ff` (`rgb(88, 166, 255)`) and `#7cb8ff` | A-F transcript links, G-J secondary blue text | No direct fleet-ui token. Use `IOS_TINT` for UI, reserve terminal blues for transcript or code-like details. |
| `status.success` | `#30d158` (`rgb(48, 209, 88)`) | A-J live, done, pass, merged, progress fill | `IOS_GREEN` `#30d158`. |
| `status.success_terminal` | `#56d364` and `#7adf95` | A-F terminal output, G/J secondary green text | No direct token. Use `IOS_GREEN` for UI state. |
| `status.warning` | `#ff9f0a` (`rgb(255, 159, 10)`) | C-J pending, warning, action accents | `IOS_ORANGE` `#ff9f0a`. |
| `status.warning_soft` | `#ffc068`, `#ffe070`, `rgba(255, 159, 10, 0.30)` | G-J secondary warning bars/chips | Existing `IOS_ORANGE`/`IOS_YELLOW`; add soft warning tokens if review/timeline need lower contrast. |
| `status.attention` | `#ffd60a` (`rgb(255, 214, 10)`) | H-J attention markers, approval states | `IOS_YELLOW` `#ffd60a`. |
| `status.danger` | `#ff453a` (`rgb(255, 69, 58)`) | A-D destructive menu/session states, J high priority | `IOS_DESTRUCTIVE` `#ff453a`. |
| `status.danger_soft` | `#ff8a82`, `#f85149`, `rgba(255, 69, 58, 0.18/0.40)` | A-D transcript/error, J priority and side rails | Existing `IOS_DESTRUCTIVE`; add soft-danger token only for large review-row fields. |
| `status.purple` | `#bf5af2` (`rgb(191, 90, 242)`) | B/F/J special/action glyphs | `IOS_PURPLE` `#bf5af2`. |

## Typography

The references use Apple system UI fonts for dashboard chrome and a compact
monospace stack for pane transcripts and shortcut chips.

| Token | Source value | Source artboards | Terminal-realistic equivalent |
| --- | --- | --- | --- |
| `font.ui` | `-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Segoe UI", system-ui, sans-serif` | A-J dashboard wrappers and labels | Use the terminal's monospace font. Express hierarchy with `Modifier::BOLD`, color, and visible cell width. |
| `font.terminal` | `"SF Mono", "JetBrains Mono", "Fira Code", ui-monospace, "Cascadia Mono", Menlo, monospace` | A-F transcripts, J labels, shortcut chips | Native fit for ratatui. Measure by `unicode-width` and reserve columns before rendering. |
| `font.body` | `16px`, weight `400`, normal line-height | A-J default body | One terminal row, normal weight, `IOS_FG` or `IOS_FG_MUTED`. |
| `font.transcript` | `11.5px`, line-height `16.6833px`, weight `400` | A-F pane transcript | One terminal row per transcript row. Approximate one source line-height as one terminal row. |
| `font.metadata` | `10px` to `12px`, weight `400/500/600` | E-J counters, status labels, tiny chips | One row, short labels, muted color; use bold only for selected/status-critical metadata. |
| `font.label` | `12.5px` to `14px`, weight `500/600` | B-J menu labels, row names | One row, semibold maps to `Modifier::BOLD`; reserve 8-24 visible cells depending on row type. |
| `font.title` | `15px` to `18px`, weight `600/700` | B-J overlay titles and group headers | One row, bold, primary text; use surrounding whitespace and separator rows instead of larger point size. |
| `font.hero` | `22px`, `26px`, `30px`, weight `600/700` | G-J large dashboard counts/headlines | Bold one-row headline when compact; two-row block only for dashboard hero counts where vertical space exists. |

### Visible Width Guidelines

| UI text role | Recommended terminal width | Notes |
| --- | --- | --- |
| Status chip label | 8-12 cells | Current `status_chip` is 12 cells wide including caps. Keep fixed width for row alignment. |
| Shortcut chip | 3-7 cells | Render as compact right-side metadata, not as a full button. |
| Menu/action label | 18-32 cells | Reserve right column for shortcuts or destructive/action badges first. |
| Worker/session title | 22-44 cells | Clip or elide before counters/chips so rows stay stable. |
| Dashboard count | 4-10 cells | Use bold and semantic color; do not enlarge with extra glyph rows unless a hero card owns the space. |
| Help/footer hint | Remaining width | Muted, one row, truncated at cell boundary. |

## Spacing Scale

The source spacing is pixel-based. For terminal rendering, treat the transcript
baseline (`11.5px` text with `16.6833px` line-height) as roughly one row, and
use about 8px as one column. This is an approximation for intent, not a fixed
layout engine.

| Token | Source values | Source artboards | ratatui mapping |
| --- | --- | --- | --- |
| `space.0` | `0px` | A-J reset/default computed styles | No shrink/inset. |
| `space.hairline` | `1px`, `1.6px`, `2px`, `2.91667px` | A-J borders, left rails, separators | One border glyph or one-cell side rail. Do not consume more than one terminal column. |
| `space.xs` | `3px`, `4px`, `5px` | G-J progress height, tiny gaps | Usually zero rows; one optional column when separating dense spans. |
| `space.sm` | `6px`, `7px`, `8px`, `9px` | E-J pill gaps, chip padding, grid gaps | One column, zero or one row. Good default inside compact pills. |
| `space.md` | `10px`, `11px`, `12px` | B-J row padding and compact cards | One terminal row for vertical rhythm or 1-2 columns horizontal inset. |
| `space.lg` | `14px`, `15px`, `16px`, `18px` | A-J overlay/card padding, menu rows | Standard overlay inner padding: `Rect` shrink by 2 columns and optionally one row. |
| `space.xl` | `20px`, `22px`, `24px`, `26px`, `28px`, `30px` | B-J grouped panels, card stacks | 3-4 columns or 1-2 rows. Use for modal/card gutters. |
| `space.2xl` | `36px`, `44px`, `50px`, `60px` | G-J dashboard sections and hero gaps | 4-8 columns or 2-4 rows. Use only on spacious dashboard panes. |
| `space.panel` | `80px`, `86px`, `90px`, `120px` | G-J columns, progress tracks, card widths | Fixed `Rect` widths for side panels, rails, or grouped metrics. |
| `radius.sm` | `6px`, `8px` | A-J small chips and inner badges | Use rounded-looking caps/glyphs; no real terminal radius. |
| `radius.md` | `12px`, `14px`, `16px` | G-J row cards, overlays, section blocks | Use rounded `Block` borders (`Rounded`) and one-cell insets. |
| `radius.lg` | `18px`, `20px`, `22px` | D/E dock, cards, large glass surfaces | Use rounded block plus airy padding; avoid nested cards. |
| `radius.pill` | `999px` | E live chip/nav pills, G/J progress bars | Use cap glyphs or balanced left/right padding. |

Practical layout rules:

- Reserve the right-side chip/shortcut/counter width before truncating labels.
- Treat all rows as one terminal row unless the design uses a card body or
  subtitle; then use a second row, not fractional line-height.
- Use one blank row to represent large vertical breathing room; use color and
  border hierarchy for smaller pixel gaps.
- Use left rails as one column even when the source rail is 2-3px wide.

## Component Intents

### `button`

Use for explicit commands that change state: spawn, approve, cancel, open,
merge, or destructive actions. Primary buttons use `accent.primary`, secondary
buttons use glass/chip fills, and destructive buttons use `status.danger`.
In ratatui, buttons should remain one row high with stable width; focus changes
color/bold/border, not geometry.

### `chip`

Use for compact state, counters, shortcuts, and badges. The reference chips are
pill-shaped, high-contrast, and often right aligned. Current `status_chip` maps
well: working uses tint, done/live uses green, approval uses yellow, polling
uses orange, blocked/capped uses red. Keep the visible width predictable.

### `progress_bar`

Use for quota, completion, worker usage, and wave progress where the precise
number is secondary to direction and bucket. Source progress bars are thin
pills with soft tracks and bright fills. In ratatui, `progress_rail` should use
one row with caps, filled cells, and semantic bucket colors.

### `text_input`

Use for search-first overlays such as Spotlight, filters, and command prompts.
The design favors a prominent single input band, faint placeholder, and active
accent focus. In ratatui, render one row for input and a separate row for
validation/help so cursor movement never resizes the field.

### `tool_tip`

Use for short shortcut hints, disabled reasons, and hover/focus context.
Tooltips should be small glass overlays, not instructional panels. In ratatui,
prefer one or two rows near the anchor, with `IOS_BG_GLASS`, strong hairline,
primary title, and muted detail.

### `list`

Use for worker rows, command results, menu actions, section jumps, sessions,
and review items. Source rows rely on stable rhythm, right-side metadata, and
colored rails/chips. In ratatui, keep each row's rectangle stable; selection or
priority should change border/color/fill, not row height.

### `notification`

Use for short status banners and review/agent events. Source notification-like
rows are compact, status-colored, and title-first. Keep visible content to a
title plus one short detail, using semantic color for severity and muted text
for origin/time.

### `context_menu`

Design A pins a glass menu to the active pane, with command rows, right-side
shortcut chips, one destructive row, and a subtle shadow. Use it for local pane
actions where the target is already known. Avoid explanatory copy; the command
label and shortcut must carry the interaction.

### `spotlight`

Design B is search-first: input at top, selected top hit, grouped results, and
footer shortcuts. Use it for cross-surface navigation and commands. The input
state should dominate the overlay, while lists stay dense and keyboard-first.

### `action_sheet`

Design C groups related actions and separates cancel. Use it for contextual
choices that need confirmation or hierarchy. Destructive actions should be red,
cancel should be visually separate, and group dividers should be hairline-thin.

### `glass_dock`

Design E uses a floating top-nav strip: active tab as a filled accent pill,
inactive tabs as glass pills, and a right-side LIVE chip with a pulse. Use it
as persistent chrome; it should be visually light and never crowd dashboard
content.

## Cross-Reference Table

| Token/component | codex-fleet surface |
| --- | --- |
| `accent.primary` | Active nav pill in `fleet-ui::tab_strip`, selected Spotlight row, `status_chip(ChipKind::Working)`, focused text input cursor. |
| `status.success` | LIVE chip, merged/done chips, passing review rows, low-risk progress rails. |
| `status.warning` | Polling/pending chips, wave waiting states, review warnings. |
| `status.attention` | Approval-required chips and review attention markers. |
| `status.danger` | Blocked/capped chips, destructive context menu rows, high-priority review rails. |
| `surface.glass` | Overlay shells in `fleet-ui::overlay`, Spotlight, tooltips, top dock background, fleet-state dashboard background. |
| `surface.card` | Worker-row cards in `fleet-state`, plan-tree wave rows, waves timeline bars, review queue cards. |
| `surface.chip` | Shortcut chips, inactive status chips, section-jump keys, secondary action buttons. |
| `border.hairline` | Overlay/card borders, timeline grid lines, row dividers, pane separators. |
| `text.primary` | Worker names, task titles, active tab labels, selected command labels. |
| `text.secondary_alpha` | Metadata, inactive tabs, timestamps, branch names, helper footer labels. |
| `text.terminal_primary` | Pane transcript snapshots and any code/log preview content; do not use for dashboard chrome. |
| `button` | Review approve/merge controls, spawn/respawn actions, action-sheet rows that execute immediately. |
| `chip` | `fleet-ui::chip::status_chip`, tab counters, worker statuses, plan/wave rollup statuses. |
| `progress_bar` | `fleet-ui::rail::progress_rail`, worker quota rails, wave completion bars, review readiness meters. |
| `text_input` | Planned Spotlight query, worker/filter search, section-jump typed filter. |
| `tool_tip` | Shortcut hints, disabled action reasons, hover/focus detail for dense dashboard chrome. |
| `list` | Worker-row list in `fleet-state`, plan-tree task list, section-jump grid, context menu rows, review queue. |
| `notification` | Status banner/event feed for task completion, blocker, auto-review, or merge state changes. |
| `context_menu` | `fleet-ui::overlay::ContextMenu` and `scripts/codex-fleet/bin/pane-context-menu.sh` pinned-pane command surface. |
| `spotlight` | `fleet-ui::spotlight_overlay` and watcher/plan-tree search overlays. |
| `action_sheet` | Destructive/cancel grouped overlays for pane, worker, and review actions. |
| `glass_dock` | `fleet-ui::tab_strip` and `rust/fleet-tab-strip/src/main.rs` top-nav chrome. |

## Palette Gaps For Follow-Up Lanes

These values appear repeatedly in the design references but do not have exact
opaque `fleet_ui::palette` constants yet:

| Proposed token | Source value | Why it matters |
| --- | --- | --- |
| `IOS_CANVAS_BG` | `#0b0d12` | Shared G-J dashboard canvas. Existing `IOS_BG_SOLID` is lighter and more UIKit-modal than dashboard-canvas. |
| `IOS_TERMINAL_FG` | `#c9d1d9` | A-F terminal transcript text should not reuse dashboard label color. |
| `IOS_TERMINAL_MUTED` | `#7d8590` | A-F transcript comments/paths. |
| `IOS_TERMINAL_BLUE` | `#58a6ff` | A-F transcript links and code-like accent. |
| `IOS_FILL_04` / `IOS_FILL_06` / `IOS_FILL_08` | white overlays at 4/6/8 percent | G-J card layering uses multiple glass depths; opaque ratatui approximations would make fidelity durable. |
| `IOS_HAIRLINE_ALPHA` | white overlays at 14/22/25 percent | Current hairlines work, but these alpha buckets are the source artboard language. |
| `IOS_SUCCESS_SOFT` | `#7adf95` or green alpha fills | Progress/timeline secondary green reads softer than `IOS_GREEN`. |
| `IOS_WARNING_SOFT` | `#ffc068` / `#ffe070` | Review/timeline warnings use softer golds for large areas. |
| `IOS_DANGER_SOFT` | `#ff8a82` | Review priority text and large danger surfaces need less glare than `IOS_DESTRUCTIVE`. |
