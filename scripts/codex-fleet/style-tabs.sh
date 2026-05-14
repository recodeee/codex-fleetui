#!/usr/bin/env bash
# style-tabs — apply iOS-style tab chrome to a codex-fleet tmux session.
#
# Palette and pill caps mirror scripts/codex-fleet/fleet-tick.sh so the tabs
# read as one continuous surface with the dashboards: systemBlue active pill,
# systemOrange session badge, systemGreen live chip, ◖◗ half-circle caps,
# secondaryLabel grays on a true-black backdrop.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh                     # default: single-row, clicks WORK
#   STYLE_TABS_HEIGHT=3 bash scripts/codex-fleet/style-tabs.sh # taller padding, clicks BROKEN
#   STYLE_TABS_HEIGHT=5 bash scripts/codex-fleet/style-tabs.sh # very tall (5 rows), clicks BROKEN
#
# Visual model (HEIGHT=1, default — tabs on a single row, mouse-clickable):
#   ┌──────────────────────────────────────────────────────────────────────┐
#   │  ◖ ◆ codex-fleet ◗  ◖ 0  overview ◗  ◖ 2  plan ◗  …  ◖ ● live ◗      │  row 0  (tabs)
#   └──────────────────────────────────────────────────────────────────────┘
#
# Multi-row was the previous default (PR f92229c81) but tmux 3.6 silently
# refuses to fire MouseDown1Status on custom status-format[N] rows, so tab
# clicks never reached `select-window -t =` and the operator had no way to
# switch tabs by mouse. Single-row uses tmux's built-in template, which is the
# only code path where the rendered range=window|N markers actually route.
#
# How taller-than-default works without blanking the strip:
#   tmux's `status N` for N>1 makes status-format an array. Each row index
#   that is NOT explicitly set renders BLANK — tmux does not fall back to a
#   default template per-row. So we have to:
#     1. Set status-format[N-1] to tmux's full default template (which
#        honours status-left + window-status-format + status-right). This
#        is the row that draws the actual tab strip.
#     2. Set status-format[0..N-2] to a uniform dark BG fill so the extra
#        rows act as visual padding without overdrawing anything.
#
# Earlier drafts (#1826/#1827) tried to mirror the active-tab BG into the
# accent rows using #{W:F1,F2} with fixed-width literals — but tab labels
# vary in width, so the fills landed at wrong x positions and overdrew the
# label row. Hence: uniform dark BG for padding rows, no per-tab geometry.
set -eo pipefail

SESSION="${CODEX_FLEET_SESSION:-codex-fleet}"
# Default to single-row status. tmux 3.6 does NOT fire MouseDown1Status when
# the click lands on a custom status-format[N] row (verified by binding the
# event to display-message and seeing zero pop-ups on tab clicks). With
# HEIGHT=1 we let tmux render the default built-in template, which is the only
# code path where range=window|N markers actually route clicks to
# `select-window -t =`. Multi-row stays opt-in for users who don't need clicks.
HEIGHT="${STYLE_TABS_HEIGHT:-1}"
case "$HEIGHT" in 1|2|3|4|5) ;; *) HEIGHT=1 ;; esac

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# tmux helper — set a global option. NB: never use `-t SESSION` with options
# that accept a numeric value like `status` because some tmux builds parse
# the value as a session index and bail with "unknown value: 1".
tx_set() { tmux set-option -g "$@" >/dev/null; }
tx_unset() { tmux set-option -gu "$@" >/dev/null 2>&1 || true; }
# Session-local unset — clears `set-option -t SESSION` overrides that would
# otherwise shadow our globals. tmux precedence is session > global, so any
# leftover per-session value (manual `set-option -t`, an older bringup script,
# or codex-fleet-2.sh's pre-iOS chrome block) silently wins over `tx_set`.
ts_unset() { tmux set-option -t "$SESSION" -u "$@" >/dev/null 2>&1 || true; }

# Wipe any stale status-format[N] entries from prior runs before configuring.
for idx in 0 1 2 3 4; do tx_unset "status-format[$idx]"; done

# Wipe session-local chrome overrides on the target session before applying
# globals. Without this, a session like `codex-fleet` that was created with
# `tmux set-option -t codex-fleet status-style ...` (deep-blue colour24 default
# theme, status-position bottom, plain `#I:#W` formats) ignores every tx_set
# below — the global iOS palette is set but never visible. Symptom: tabs render
# as `0:overview` not `◖ 0  overview ◗`, and a solid colour24 bar appears.
for opt in \
  status status-position status-style status-interval status-justify \
  status-left status-left-length status-left-style \
  status-right status-right-length status-right-style \
  window-status-format window-status-current-format \
  window-status-style  window-status-current-style \
  window-status-separator window-status-activity-style window-status-bell-style \
  pane-border-status pane-border-format pane-border-style pane-active-border-style \
  message-style message-command-style mode-style \
  menu-style menu-selected-style menu-border-style menu-border-lines \
  monitor-activity monitor-bell visual-activity visual-bell
do ts_unset "$opt"; done
for idx in 0 1 2 3 4; do ts_unset "status-format[$idx]"; done

# Activity tracking — drives the orange "hover" highlight on background tabs.
tx_set monitor-activity on
tx_set monitor-bell on
tx_set visual-activity off
tx_set visual-bell off

tx_set status on
tx_set status-position top
tx_set status-interval 1
tx_set status-justify left

# ── iOS system palette ──────────────────────────────────────────────────────
# Canonical values match scripts/codex-fleet/fleet-tick.sh so the tab strip
# reads as one continuous surface with the dashboards. Names mirror Apple's
# UIKit system colors (UIColor.systemBlue, systemOrange, etc.).
#   IOS_BG       = systemBackground (dark)         #000000
#   IOS_BG2      = secondarySystemBackground       #1C1C1E
#   IOS_BG3      = tertiarySystemBackground        #2C2C2E
#   IOS_BLUE     = systemBlue                      #007AFF  (active accent)
#   IOS_ORANGE   = systemOrange                    #FF9500  (session badge)
#   IOS_GREEN    = systemGreen                     #34C759  (live chip)
#   IOS_RED      = systemRed                       #FF3B30  (bell)
#   IOS_YELLOW   = systemYellow                    #FFCC00  (mode/copy)
#   IOS_LABEL    = label                           #FFFFFF
#   IOS_LABEL2   = secondaryLabel                  #AEAEB2
#   IOS_LABEL3   = tertiaryLabel                   #8E8E93
# Pill caps use ◖ / ◗ — same half-circle glyphs as fleet-tick.sh's
# IOS_CHIP_LEFT/RIGHT, so caps render identically across surfaces.
tx_set status-style "bg=#000000,fg=#8E8E93"

# ── status-left: session badge — iOS-orange pill ─────────────────────────────
tx_set status-left-length 40
tx_set status-left "#[fg=#FF9500,bg=#000000]◖#[fg=#000000,bg=#FF9500,bold] ◆ #S #[fg=#FF9500,bg=#000000]◗ "

# ── status-right: live indicator + clock — iOS-green pill ────────────────────
tx_set status-right-length 64
tx_set status-right \
  "#[fg=#34C759,bg=#000000]◖#[fg=#000000,bg=#34C759,bold] ● live #[fg=#34C759,bg=#000000]◗ #[fg=#AEAEB2,bg=#000000] #(date +%H:%M:%S)  "

# ── tab separator — empty so range=window|N markers butt up against each ────
# Why empty: tmux emits `#{window-status-separator}` AFTER each tab's
# `range=window|N ... norange` pair closes — meaning the separator cell falls
# in a no-range region. `MouseDown1Status` only fires when the click lands
# inside a `range=window|N` zone, so a 1-cell space gap was the difference
# between "click the pill" (works) and "click the gap between pills" (silently
# does nothing because `MouseDown1StatusDefault` was deliberately unbound in
# the STATUSDEFAULT-toast fix to suppress debug overlays).
#
# With separator = "", consecutive pills' `◗` and `◖` half-caps touch
# directly and every horizontal cell of the tab strip is inside some window's
# range, eliminating dead pixels for click handling. Visual: the pills look
# slightly tighter together; with the iOS palette + contrasting `◗◖` cap
# colours, the boundary stays legible without a literal space gap.
tx_set window-status-separator ""

# ── inactive tab — recessed pill, secondary label ────────────────────────────
# iOS ghost-pill: faint secondarySystemBackground card, tertiaryLabel text.
tx_set window-status-format \
  "#[fg=#1C1C1E,bg=#000000]◖#[fg=#8E8E93,bg=#1C1C1E]  #I  #[fg=#AEAEB2]#W  #[fg=#1C1C1E,bg=#000000]◗"
tx_set window-status-style "fg=#8E8E93,bg=#1C1C1E"

# ── active tab — iOS-blue pill, white label, bold ────────────────────────────
# Earlier revisions tried to render a heavy ✖ close chip on the active tab
# for non-core windows only, gated by `#{?#{m/r:^(core|...)$,#W},,#[…] ✖ }`.
# That format had two failure modes the operator kept seeing:
#   1. The close-chip `#[…]` block had to contain commas (the canonical
#      tmux style separator) but those commas collided with the
#      `#{?cond,true,false}` separator, truncating the false branch at the
#      first inner comma and leaking the tail ("bg=#ffd07a,bold] ✖ }") as
#      literal text on every non-core active tab.
#   2. Switching to space-separated style attrs to dodge (1) does not work
#      either — tmux's `#[…]` style parser stops at the first space, then
#      emits the rest (e.g. `bold]`) as plain text.
# tmux doesn't fire click handlers on the ✖ glyph either way (the chip was
# decorative), so just drop the conditional. Active tab keeps the bright
# iOS-blue pill, bold label, and ◖◗ half-circle caps; no per-tab close.
tx_set window-status-current-format \
  "#[fg=#007AFF,bg=#000000]◖#[fg=#FFFFFF,bg=#007AFF,bold]  #I  #W  #[fg=#007AFF,bg=#000000]◗"
tx_set window-status-current-style "fg=#FFFFFF,bg=#007AFF,bold"

# ── activity / bell highlights — iOS-orange / iOS-red ────────────────────────
tx_set window-status-activity-style "fg=#FF9500,bg=#1C1C1E,bold"
tx_set window-status-bell-style     "fg=#FF3B30,bg=#1C1C1E,bold"

# ── pane border styling — iOS-blue active, secondary gray idle ───────────────
tx_set pane-border-status top
tx_set pane-border-format " #[fg=#FF9500,bold]▭#[default] #[fg=#AEAEB2]#{@panel}#[default] "
tx_set pane-border-style        "fg=#2C2C2E"
tx_set pane-active-border-style "fg=#007AFF"

# ── message / mode styling — iOS-blue alerts, iOS-yellow copy mode ───────────
tx_set message-style         "fg=#FFFFFF,bg=#007AFF,bold"
tx_set message-command-style "fg=#FFFFFF,bg=#1C1C1E"
tx_set mode-style            "fg=#000000,bg=#FFCC00,bold"

# ── multi-row status: dark padding rows + one tabs row ───────────────────────
# tmux's default template for status-format[0] — replicated verbatim so the
# tab strip row honours status-left / window-status-format / status-right.
# Source: `tmux show-options -gv status-format[0]` from a default tmux build.
DEFAULT_TABS_FORMAT='#[align=left range=left #{E:status-left-style}]#[push-default]#{T;=/#{status-left-length}:status-left}#[pop-default]#[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{W:#[range=window|#{window_index} #{E:window-status-style}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-format}#[pop-default]#[norange default]#{?loop_last_flag,,#{window-status-separator}},#[range=window|#{window_index} list=focus #{?#{!=:#{E:window-status-current-style},default},#{E:window-status-current-style},#{E:window-status-style}}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-current-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{window-status-separator}}}#[nolist align=right range=right #{E:status-right-style}]#[push-default]#{T;=/#{status-right-length}:status-right}#[pop-default]#[norange default]'

# Uniform dark padding row — iOS systemBackground (true black) so the chrome
# reads as one continuous surface with the tabs row below.
PADDING_ROW="#[fg=#000000,bg=#000000]$(printf '%*s' 400 '')"

# Clear any session-local `status` override first. tmux's new-session default
# can leave a per-session `status on` (boolean) that shadows the global numeric
# `status N`, clamping back to 1 row and silently hiding the tab strip on
# row N-1 (the dark padding row then appears as a thin bar with no tabs).
tmux set-option -t "$SESSION" -u status >/dev/null 2>&1 || true
# Set status height. tmux 3.6 rejects `set -g status 1` with "unknown value: 1"
# because the boolean alias `on` already covers the 1-row case — pass `on` for
# HEIGHT=1 and the numeric only for 2-5. NOT via `-t SESSION` — that misparses
# the int.
if [[ "$HEIGHT" == "1" ]]; then
  tmux set-option -g status on >/dev/null
else
  tmux set-option -g status "$HEIGHT" >/dev/null
fi

if (( HEIGHT == 1 )); then
  # Single-row mode: leave status-format[N] unset so tmux uses its built-in
  # default template. That template is the only code path where MouseDown1Status
  # fires for tab clicks in tmux 3.6 — a custom status-format[0] silently breaks
  # the mouse routing even when the rendered range=window|N markers are present.
  for idx in 0 1 2 3 4; do tx_unset "status-format[$idx]"; done
else
  # Multi-row mode (opt-in via STYLE_TABS_HEIGHT>=2): tabs on the LAST row,
  # uniform dark padding above. Tab clicks DO NOT WORK in this mode (tmux 3.6
  # MouseDown1Status quirk on custom multi-row status-format) — keyboard nav
  # only (prefix+0..5).
  last_idx=$(( HEIGHT - 1 ))
  tx_set "status-format[$last_idx]" "$DEFAULT_TABS_FORMAT"
  for ((i=0; i<last_idx; i++)); do
    tx_set "status-format[$i]" "$PADDING_ROW"
  done
fi

# ── iOS-style menu styling ───────────────────────────────────────────────────
# Sheet-style card on tertiarySystemBackground, rounded border, iOS-blue
# selection row — mirrors a UIKit context menu on dark mode.
tx_set menu-style          "fg=#FFFFFF,bg=#1C1C1E"
tx_set menu-selected-style "fg=#FFFFFF,bg=#007AFF,bold"
tx_set menu-border-style   "fg=#2C2C2E"
tx_set menu-border-lines   "rounded"

# ── sticky right-click menu ──────────────────────────────────────────────────
# tmux's default MouseDown3Pane binding calls `display-menu` WITHOUT `-O`, so
# the menu only stays open while the right mouse button is held down. When
# you release the button (or move the mouse to click an item) the menu can
# disappear before you reach it.
#
# Rebind with `-O` (sticky): menu stays open until an item is clicked, Escape
# is pressed, or a click lands outside the menu. This mirrors browser/native
# right-click menu behaviour.
#
# Use `tmux source-file` with a here-doc so tmux parses the brace blocks the
# same way it does .tmux.conf — passing the binding via shell argv breaks
# because bash splits the {} groups by whitespace.
sticky_menu_conf=$(mktemp -t codex-fleet-menu.XXXXXX.tmux.conf)
trap 'rm -f "$sticky_menu_conf"' EXIT
# iOS-style right-click context menu.
#
# tmux's built-in `display-menu` can't render the rounded card chrome, the
# pill-shaped accent shortcut chips on the right, or the live-status header
# that the operator-approved design calls for — it only exposes menu-style /
# menu-selected-style / menu-border-style plus inline #[…] markup, no
# per-row two-tone layout or right-aligned chip columns.
#
# Switch to `display-popup -E -B`: a full pty inside the popup that runs
# scripts/codex-fleet/bin/pane-context-menu.sh, which draws the design
# directly with ANSI escapes (lib/ios-menu.sh palette + helpers), reads one
# keystroke, and dispatches the same tmux commands the old display-menu did.
#
# CODEX_FLEET_MENU_LINE carries #{mouse_line} into the popup pty so the
# script can implement "Copy this line"; #{q:…} quoting survives embedded
# quotes/spaces in the line content.
#
# CODEX_FLEET_REPO_ROOT must be in the tmux global environment because the
# bind uses tmux's ${VAR} substitution to find pane-context-menu.sh, and
# tmux ${VAR} does NOT support shell-style ${VAR:-default} (it parses the
# whole `VAR:-default` as one variable name and rejects it with "invalid
# environment variable"). Resolve the fallback in bash first, then push it
# into tmux's env so the binding sees a plain ${CODEX_FLEET_REPO_ROOT}.
_repo_root="${CODEX_FLEET_REPO_ROOT:-$HOME/Documents/recodee}"
tmux set-environment -g CODEX_FLEET_REPO_ROOT "$_repo_root" 2>/dev/null || true
cat >"$sticky_menu_conf" <<'TMUX_CONF'
unbind-key -T root MouseDown3Pane
# Right-click context menu — runs the ratatui-rendered iOS menu when its
# binary exists, otherwise falls back to the bash renderer.
# The chooser shell script keeps the conditional out of tmux's substitution
# layer (tmux eats `$FB`-style shell vars before the popup spawns).
bind-key   -T root MouseDown3Pane if-shell -F -t = "#{||:#{mouse_any_flag},#{&&:#{pane_in_mode},#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}}}" { select-pane -t = ; send-keys -M } { set-environment -g CODEX_FLEET_MENU_LINE "#{q:mouse_line}" ; display-popup -E -B -w 60 -h 28 -x M -y M -t = "bash ${CODEX_FLEET_REPO_ROOT}/scripts/codex-fleet/bin/pane-context-menu-chooser.sh '#{pane_id}'" }

# Mouse-wheel scroll into copy-mode even when the pane is in alt-screen
# (plan-tree-anim / fleet-state-anim use \033[?1049h; the default tmux
# WheelUpPane binding forwards wheel events to the app whenever
# #{alternate_on} is set, which makes the viz panes unscrollable).
# Drop the alternate_on check so the wheel always enters copy-mode unless
# the app explicitly opts in via mouse_any_flag.
unbind-key -T root WheelUpPane
unbind-key -T root WheelDownPane
bind-key   -T root WheelUpPane   if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" "send-keys -M" "copy-mode -e"
bind-key   -T root WheelDownPane if-shell -F "#{||:#{pane_in_mode},#{mouse_any_flag}}" "send-keys -M" "copy-mode -e"

# Close ONLY the focused pane on `prefix + w` (operator's habit).
#
# SAFETY: tmux's kill-pane on the LAST pane in a window cascades — the
# window dies, and if it was the last window the session dies, and if
# that was the last session the tmux server exits. When tmux exits, kitty
# pops its "Are you sure you want to close this window?" prompt and the
# operator loses the whole fleet (PR #1897 fix).
#
# Gate the kill on window_panes > 1; for a single-pane window show a hint
# pointing at `prefix + &` (kill-window with confirmation) instead, which
# makes the cascade intentional rather than accidental.
unbind-key -T prefix w
bind-key   -T prefix w if-shell -F '#{>:#{window_panes},1}' \
  'kill-pane' \
  'display-message -d 2500 " single pane in window — use prefix+& to kill window "'

# Status-bar tab click: tmux's inherited default binds `MouseDown1Status` to
# `switch-client -t =`, which does not reliably select the window under the
# mouse when the active pane is a TUI app holding mouse_any_flag (codex CLI).
# Rebind to the canonical `select-window -t=` so clicking a tab always jumps
# to that window in the current session.
unbind-key -T root MouseDown1Status
bind-key   -T root MouseDown1Status select-window -t=
TMUX_CONF
tmux source-file "$sticky_menu_conf" >/dev/null 2>&1 || echo "[style-tabs] WARN: sticky menu rebind failed (see $sticky_menu_conf)"

# Immediate redraw.
tmux refresh-client -S >/dev/null 2>&1 || true

if (( HEIGHT == 1 )); then
  echo "[style-tabs] applied iOS-palette tabs (height=1, tmux-default template, mouse clicks WORK) to session=$SESSION"
else
  echo "[style-tabs] applied iOS-palette tabs (height=$HEIGHT, tabs on row $((HEIGHT-1)), mouse clicks BROKEN — tmux 3.6 multi-row quirk) to session=$SESSION"
fi
