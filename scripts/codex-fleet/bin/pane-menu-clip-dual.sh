#!/usr/bin/env bash
# pane-menu-clip-dual.sh — read stdin once and write it to BOTH the system
# clipboard and the X11 primary selection. Wired into tmux as the target of
# `copy-pipe-and-cancel` so any drag / double-click / triple-click /
# Enter / `y` lands in both selections at once. Why both:
#   - Clipboard (Ctrl+Shift+V, right-click→Paste, paste-buffer)   ← explicit
#   - Primary   (middle-click, Shift+Insert in xterm/gnome-term)  ← implicit
# Matches the convention every "normal Linux terminal" follows.
#
# Implemented by buffering stdin into a variable so we don't have to juggle
# tee's process-substitution race conditions in /bin/sh; the worst case is
# a multi-MB capture-pane, which fits comfortably in shell-var memory.
set -eo pipefail

data="$(cat)"
if command -v wl-copy >/dev/null 2>&1; then
  printf '%s' "$data" | wl-copy
  printf '%s' "$data" | wl-copy --primary 2>/dev/null || true
elif command -v xclip >/dev/null 2>&1; then
  printf '%s' "$data" | xclip -selection clipboard -in
  printf '%s' "$data" | xclip -selection primary   -in
elif command -v xsel >/dev/null 2>&1; then
  printf '%s' "$data" | xsel --clipboard --input
  printf '%s' "$data" | xsel --primary   --input
fi
