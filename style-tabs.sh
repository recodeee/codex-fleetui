#!/usr/bin/env bash
# style-tabs — apply browser-like tab styling to a codex-fleet tmux session.
#
# Idempotent. Call after spawning panes (up.sh) or against an already-running
# session.
#
# Usage:
#   bash scripts/codex-fleet/style-tabs.sh                     # default height=3
#   STYLE_TABS_HEIGHT=1 bash scripts/codex-fleet/style-tabs.sh # compact single row
#   STYLE_TABS_HEIGHT=5 bash scripts/codex-fleet/style-tabs.sh # very tall (5 rows)
#
# Visual model (HEIGHT=3, default):
#   ┌──────────────────────────────────────────────────────────────────────┐
#   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  row 0  (dark padding)
#   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │  row 1  (dark padding)
#   │  ◆ codex-fleet ▌    0  overview    │▎   2  plan  ✖    ▎│ …    ● live │  row 2  (tabs)
#   └──────────────────────────────────────────────────────────────────────┘
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
HEIGHT="${STYLE_TABS_HEIGHT:-3}"
case "$HEIGHT" in 1|2|3|4|5) ;; *) HEIGHT=3 ;; esac

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[style-tabs] no tmux session '$SESSION' — run up.sh first" >&2
  exit 1
fi

# tmux helper — set a global option. NB: never use `-t SESSION` with options
# that accept a numeric value like `status` because some tmux builds parse
# the value as a session index and bail with "unknown value: 1".
tx_set() { tmux set-option -g "$@" >/dev/null; }
tx_unset() { tmux set-option -gu "$@" >/dev/null 2>&1 || true; }

# Wipe any stale status-format[N] entries from prior runs before configuring.
for idx in 0 1 2 3 4; do tx_unset "status-format[$idx]"; done

# Activity tracking — drives the orange "hover" highlight on background tabs.
tx_set monitor-activity on
tx_set monitor-bell on
tx_set visual-activity off
tx_set visual-bell off

tx_set status on
tx_set status-position top
tx_set status-interval 1
tx_set status-justify left

tx_set status-style "bg=#0a0a0a,fg=#7a7a7a"

# ── status-left: session badge ───────────────────────────────────────────────
tx_set status-left-length 40
tx_set status-left "#[fg=#0a0a0a,bg=#e67e22,bold]  ◆ #S  #[fg=#e67e22,bg=#0a0a0a]▌ "

# ── status-right: live indicator + clock chip ────────────────────────────────
tx_set status-right-length 64
tx_set status-right \
  "#[fg=#1a1a1a,bg=#0a0a0a]▐#[fg=#83c87e,bg=#1a1a1a,bold]  ● live  #[fg=#1a1a1a,bg=#0a0a0a]▌ #[fg=#bbbbbb,bg=#0a0a0a]  #(date +%H:%M:%S)  "

# ── tab separator ────────────────────────────────────────────────────────────
tx_set window-status-separator "#[fg=#2a2a2a,bg=#0a0a0a] "

# ── inactive tab — padded label, recessed BG ─────────────────────────────────
tx_set window-status-format \
  "#[fg=#1a1a1a,bg=#0a0a0a]#[fg=#7a7a7a,bg=#1a1a1a]   #I  #[fg=#aaaaaa]#W   #[fg=#1a1a1a,bg=#0a0a0a]"
tx_set window-status-style "fg=#7a7a7a,bg=#1a1a1a"

# ── active tab — bright fill, ▎ caps, bold label ─────────────────────────────
# Close chip (heavy ✖ on amber) renders ONLY for non-core tabs. Core tabs
# (overview / fleet / plan / waves / review / watcher) are permanent UI
# surfaces — the X shouldn't be there. Suppression is purely visual; tmux
# doesn't act on clicks on the ✖ either way, so removing it just stops
# misleading the user into thinking those tabs are closable.
#
# IMPORTANT: the close-chip `#[...]` block uses SPACE-separated attrs, not
# commas. Commas inside `#[fg=,bg=,bold]` collide with the conditional
# `#{?cond,true,false}` separator — tmux truncates the false branch at the
# first inner comma and the tail ("bg=#ffd07a,bold] ✖ }") leaks as literal
# text on every non-core active tab. Space-separated attrs render identically.
tx_set window-status-current-format \
  "#[fg=#3a7ebf,bg=#0a0a0a]▎#[fg=#ffffff,bg=#3a7ebf,bold]   #I  #W   #{?#{m/r:^(overview|fleet|plan|waves|review|watcher)$,#W},,#[fg=#0a0a0a bg=#ffd07a bold] ✖ }#[fg=#3a7ebf,bg=#0a0a0a]▎"
tx_set window-status-current-style "fg=#ffffff,bg=#3a7ebf,bold"

# ── activity / bell highlights ───────────────────────────────────────────────
tx_set window-status-activity-style "fg=#ffaa55,bg=#241a10,bold"
tx_set window-status-bell-style     "fg=#ff5555,bg=#2a0a0a,bold"

# ── pane border styling ──────────────────────────────────────────────────────
tx_set pane-border-status top
tx_set pane-border-format " #[fg=#ffd07a,bold]▭#[default] #[fg=#bbbbbb]#{@panel}#[default] "
tx_set pane-border-style        "fg=#2a2a2a"
tx_set pane-active-border-style "fg=#3a7ebf"

# ── message / mode styling ───────────────────────────────────────────────────
tx_set message-style         "fg=#ffffff,bg=#3a7ebf,bold"
tx_set message-command-style "fg=#ffffff,bg=#1a1a1a"
tx_set mode-style            "fg=#0a0a0a,bg=#ffd07a,bold"

# ── multi-row status: dark padding rows + one tabs row ───────────────────────
# tmux's default template for status-format[0] — replicated verbatim so the
# tab strip row honours status-left / window-status-format / status-right.
# Source: `tmux show-options -gv status-format[0]` from a default tmux build.
DEFAULT_TABS_FORMAT='#[align=left range=left #{E:status-left-style}]#[push-default]#{T;=/#{status-left-length}:status-left}#[pop-default]#[norange default]#[list=on align=#{status-justify}]#[list=left-marker]<#[list=right-marker]>#[list=on]#{W:#[range=window|#{window_index} #{E:window-status-style}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-format}#[pop-default]#[norange default]#{?loop_last_flag,,#{window-status-separator}},#[range=window|#{window_index} list=focus #{?#{!=:#{E:window-status-current-style},default},#{E:window-status-current-style},#{E:window-status-style}}#{?#{&&:#{window_last_flag},#{!=:#{E:window-status-last-style},default}}, #{E:window-status-last-style},}#{?#{&&:#{window_bell_flag},#{!=:#{E:window-status-bell-style},default}}, #{E:window-status-bell-style},#{?#{&&:#{||:#{window_activity_flag},#{window_silence_flag}},#{!=:#{E:window-status-activity-style},default}}, #{E:window-status-activity-style},}}]#[push-default]#{T:window-status-current-format}#[pop-default]#[norange list=on default]#{?loop_last_flag,,#{window-status-separator}}}#[nolist align=right range=right #{E:status-right-style}]#[push-default]#{T;=/#{status-right-length}:status-right}#[pop-default]#[norange default]'

# Uniform dark padding row — 400 spaces is wider than any reasonable terminal.
PADDING_ROW="#[fg=#0a0a0a,bg=#0a0a0a]$(printf '%*s' 400 '')"

# Clear any session-local `status` override first. tmux's new-session default
# can leave a per-session `status on` (boolean) that shadows the global numeric
# `status N`, clamping back to 1 row and silently hiding the tab strip on
# row N-1 (the dark padding row then appears as a thin bar with no tabs).
tmux set-option -t "$SESSION" -u status >/dev/null 2>&1 || true
# Set status height as a number (NOT via `-t SESSION` — that misparses the int).
tmux set-option -g status "$HEIGHT" >/dev/null

# Last row index = HEIGHT - 1. That's where we put the actual tab strip.
last_idx=$(( HEIGHT - 1 ))
tx_set "status-format[$last_idx]" "$DEFAULT_TABS_FORMAT"

# Fill rows 0..last_idx-1 with dark padding.
for ((i=0; i<last_idx; i++)); do
  tx_set "status-format[$i]" "$PADDING_ROW"
done

# ── iOS-style menu styling ───────────────────────────────────────────────────
# Card-like background, rounded border, bold accent on the highlighted row.
tx_set menu-style          "fg=#e6e6e6,bg=#16181d"
tx_set menu-selected-style "fg=#ffffff,bg=#3a7ebf,bold"
tx_set menu-border-style   "fg=#2a3038"
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
# iOS-style sectioned menu:
#   1. CAPTURE  — Copy whole session (full scrollback), Copy visible, Copy line
#   2. NAVIGATE — Search history, Scroll to top/bottom
#   3. PANE     — Horizontal/Vertical split, Zoom toggle
#   4. ARRANGE  — Swap up/down/with-marked, Mark
#   5. DANGER   — Respawn, Kill
# Each item prefixed with a glyph for icon-led readability; sections separated
# by tmux's '' separator. `-O` keeps the menu open until selection/Escape.
cat >"$sticky_menu_conf" <<'TMUX_CONF'
unbind-key -T root MouseDown3Pane
bind-key   -T root MouseDown3Pane if-shell -F -t = "#{||:#{mouse_any_flag},#{&&:#{pane_in_mode},#{?#{m/r:(copy|view)-mode,#{pane_mode}},0,1}}}" { select-pane -t = ; send-keys -M } { display-menu -O -T "#[align=centre,fg=#ffd07a,bold] ◆  pane #{pane_index} · #{pane_id} " -t = -x M -y M "  📋  Copy whole session" C "run-shell \"tmux capture-pane -t '#{pane_id}' -p -S - -E - | wl-copy && tmux display-message -d 1500 '📋  Pane history copied to clipboard'\"" "  📄  Copy visible" c "run-shell \"tmux capture-pane -t '#{pane_id}' -p | wl-copy && tmux display-message -d 1500 '📄  Visible area copied'\"" "  ✂   Copy this line" l "run-shell \"echo -n '#{q:mouse_line}' | wl-copy && tmux display-message -d 1500 '✂   Line copied'\"" '' "  🔎  Search history…" / { copy-mode -t= ; send-keys -X search-backward "" } "  ⬆   Scroll to top" '<' { copy-mode -t= ; send-keys -X history-top } "  ⬇   Scroll to bottom" '>' { copy-mode -t= ; send-keys -X history-bottom } '' "  ⬓   Horizontal split" h { split-window -h } "  ⬒   Vertical split" v { split-window -v } "#{?#{>:#{window_panes},1},,-}  ⛶   #{?window_zoomed_flag,Unzoom,Zoom}" z { resize-pane -Z } '' "#{?#{>:#{window_panes},1},,-}  ▲   Swap up" u { swap-pane -U } "#{?#{>:#{window_panes},1},,-}  ▼   Swap down" d { swap-pane -D } "#{?pane_marked_set,,-}  ⇄   Swap with marked" s { swap-pane } "  ◈   #{?pane_marked,Unmark,Mark} pane" m { select-pane -m } '' "  ↻   Respawn pane" R { respawn-pane -k } "  ✕   Kill pane" X { kill-pane } }
TMUX_CONF
tmux source-file "$sticky_menu_conf" >/dev/null 2>&1 || echo "[style-tabs] WARN: sticky menu rebind failed (see $sticky_menu_conf)"

# Immediate redraw.
tmux refresh-client -S >/dev/null 2>&1 || true

echo "[style-tabs] applied browser-style tabs (height=$HEIGHT, tabs on row $last_idx, sticky right-click menu) to session=$SESSION"
