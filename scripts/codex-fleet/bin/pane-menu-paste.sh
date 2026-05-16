#!/usr/bin/env bash
# pane-menu-paste.sh — robust clipboard paste helper for fleet pane menus.
# Reads system clipboard via wl-paste, picks best MIME (image > file URI >
# text), validates payload (size + magic bytes for images), saves images
# under /tmp/codex-fleet-clipboard/, and pastes resulting text/path into the
# target tmux pane via a bracketed paste-buffer. Every invocation appends
# one structured line to /tmp/codex-fleet-paste.log for debuggability.
# Usage: pane-menu-paste.sh <pane_id>
set -eo pipefail

# shellcheck source=/dev/null
source "$(dirname "${BASH_SOURCE[0]}")/../lib/_tmux.sh"

PANE_ID="${1:-}"
LOG_FILE="/tmp/codex-fleet-paste.log"
CLIP_DIR="/tmp/codex-fleet-clipboard"
BUF="_menu_paste"

[[ -z "$PANE_ID" ]] && { echo "pane-menu-paste.sh: missing pane_id arg" >&2; exit 2; }
mkdir -p "$CLIP_DIR" 2>/dev/null && chmod 700 "$CLIP_DIR" 2>/dev/null || true  # SC2174

# ── logging ────────────────────────────────────────────────────────────────
log_line() {
  local outcome="$1" mime="$2" size="$3" path="$4" err="$5"
  printf '[%s] pane=%s outcome=%s mime=%s size=%s path=%s err=%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$PANE_ID" \
    "$outcome" "${mime:--}" "${size:--}" "${path:--}" "${err:--}" \
    >> "$LOG_FILE" 2>/dev/null || true
}

# shellcheck disable=SC2329  # called via trap, not directly
cleanup() {
  find "$CLIP_DIR" -type f -mmin +60 -delete 2>/dev/null || true
  [[ -f "$LOG_FILE" && $(wc -c < "$LOG_FILE") -gt 102400 ]] \
    && tail -c 51200 "$LOG_FILE" > /tmp/.paste.log.new 2>/dev/null \
    && mv /tmp/.paste.log.new "$LOG_FILE" 2>/dev/null || true
}
trap cleanup EXIT

# ── helpers ────────────────────────────────────────────────────────────────
mime_to_ext() {
  case "$1" in
    image/jpeg)    printf 'jpg' ;;
    image/svg+xml) printf 'svg' ;;
    image/x-icon)  printf 'ico' ;;
    image/*)       printf '%s' "${1#image/}" ;;
    *)             printf 'bin' ;;
  esac
}

# First 16 bytes as hex. Used for magic-byte checks + failure diagnostics.
hex_head() { xxd -p -l 16 "$1" 2>/dev/null | tr -d '\n'; }

# Validate magic bytes against claimed MIME. Returns 0 on match.
validate_magic() {
  local mime="$1" file="$2" hex
  hex="$(hex_head "$file")"
  [[ -z "$hex" ]] && return 1
  case "$mime" in
    image/png)     [[ "$hex" == 89504e47* ]] ;;
    image/jpeg)    [[ "$hex" == ffd8ff* ]] ;;
    image/gif)     [[ "${hex:0:8}" == "47494638" ]] ;;
    image/webp)    [[ "${hex:0:8}" == "52494646" && "${hex:16:8}" == "57454250" ]] ;;
    image/bmp)     [[ "${hex:0:4}" == "424d" ]] ;;
    image/tiff)    [[ "${hex:0:8}" == "49492a00" || "${hex:0:8}" == "4d4d002a" ]] ;;
    image/x-icon)  [[ "${hex:0:8}" == "00000100" ]] ;;
    image/avif|image/heic|image/heif)
      # ISO-BMFF box: 4-byte size then 'ftyp' at byte 4.
      [[ "${hex:8:8}" == "66747970" ]] ;;
    image/svg+xml)
      head -c 256 "$file" 2>/dev/null | grep -qE '<\?xml|<svg' ;;
    *) return 0 ;;
  esac
}

paste_text_buf() {
  local payload="$1"
  printf '%s' "$payload" | tmux load-buffer -b "$BUF" - 2>/dev/null || return 1
  tmux paste-buffer -b "$BUF" -t "$PANE_ID" -p 2>/dev/null || return 1
  tmux delete-buffer -b "$BUF" 2>/dev/null || true
  return 0
}

# urldecode for file:// URIs (RFC 3986).
url_decode() { local s="${1//+/ }"; printf '%b' "${s//%/\\x}"; }

# ── wl-paste missing → fall back to tmux's own paste buffer ────────────────
if ! command -v wl-paste >/dev/null 2>&1; then
  if tmux paste-buffer -t "$PANE_ID" -p 2>/dev/null; then
    tmux display-message -d 1500 '⤓  pasted from tmux buffer (no wl-paste)' || true
    log_line "text" "tmux-buffer" "-" "-" "wl-paste-missing"
    exit 0
  fi
  tmux display-message -d 2000 '⚠  no clipboard helper and tmux buffer empty' || true
  log_line "fail-fallback" "-" "-" "-" "wl-paste-missing-and-buffer-empty"
  exit 1
fi

# ── enumerate available MIME types once ────────────────────────────────────
mimes="$(wl-paste --list-types 2>/dev/null || true)"

# ── (1) image preference order ─────────────────────────────────────────────
IMG_PREF=(
  image/png image/webp image/jpeg image/avif image/heic image/heif
  image/gif image/bmp image/tiff image/svg+xml image/x-icon
)

for mime in "${IMG_PREF[@]}"; do
  printf '%s\n' "$mimes" | grep -Fxq "$mime" || continue
  ext="$(mime_to_ext "$mime")"
  out="$CLIP_DIR/clipboard-paste-$(date +%Y%m%d-%H%M%S).$ext"

  if ! wl-paste --type "$mime" > "$out" 2>/dev/null; then
    log_line "fail-fallback" "$mime" "0" "$out" "wl-paste-exit-nonzero"
    rm -f "$out" 2>/dev/null || true
    continue
  fi

  size="$(wc -c < "$out" 2>/dev/null || echo 0)"
  if (( size <= 100 )); then
    log_line "fail-fallback" "$mime" "$size" "$out" "too-small hex=$(hex_head "$out")"
    rm -f "$out" 2>/dev/null || true
    continue
  fi

  if ! validate_magic "$mime" "$out"; then
    log_line "fail-fallback" "$mime" "$size" "$out" "magic-mismatch hex=$(hex_head "$out")"
    rm -f "$out" 2>/dev/null || true
    continue
  fi

  if paste_text_buf "$out"; then
    tmux display-message -d 1800 "⤓  image pasted: $(basename "$out")" || true
    log_line "image" "$mime" "$size" "$out" "-"
    exit 0
  fi
  log_line "fail-fallback" "$mime" "$size" "$out" "tmux-paste-failed"
  # Keep the saved file so the operator can recover it manually.
  break
done

# ── (2) text/uri-list — paste file path(s) ────────────────────────────────
if printf '%s\n' "$mimes" | grep -Fxq "text/uri-list"; then
  uris="$(wl-paste --type text/uri-list 2>/dev/null || true)"
  paths=""
  while IFS= read -r line; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" == \#* ]] && continue
    case "$line" in
      file://*)
        decoded="$(url_decode "${line#file://}")"
        if [[ -f "$decoded" && -r "$decoded" ]]; then
          paths+="${paths:+$'\n'}$decoded"
        fi
        ;;
    esac
  done <<<"$uris"

  if [[ -n "$paths" ]]; then
    if paste_text_buf "$paths"; then
      first="${paths%%$'\n'*}"
      tmux display-message -d 1800 "⤓  path pasted: $(basename "$first")" || true
      log_line "uri-list" "text/uri-list" "${#paths}" "$first" "-"
      exit 0
    fi
    log_line "fail-fallback" "text/uri-list" "${#paths}" "$first" "tmux-paste-failed"
  fi
fi

# ── (3) text fallback (text/plain or any text/*) ──────────────────────────
text_mime=""
if printf '%s\n' "$mimes" | grep -Fxq "text/plain;charset=utf-8"; then
  text_mime="text/plain;charset=utf-8"
elif printf '%s\n' "$mimes" | grep -Fxq "text/plain"; then
  text_mime="text/plain"
else
  text_mime="$(printf '%s\n' "$mimes" | grep -m1 -E '^text/' || true)"
fi

if [[ -n "$text_mime" ]]; then
  text="$(wl-paste --type "$text_mime" --no-newline 2>/dev/null || true)"
  if [[ -n "$text" ]]; then
    if paste_text_buf "$text"; then
      tmux display-message -d 1500 "⤓  text pasted (${#text} chars)" || true
      log_line "text" "$text_mime" "${#text}" "-" "-"
      exit 0
    fi
    log_line "fail-fallback" "$text_mime" "${#text}" "-" "tmux-paste-failed"
  else
    log_line "fail-fallback" "$text_mime" "0" "-" "wl-paste-empty"
  fi
fi

# ── nothing usable on the clipboard ───────────────────────────────────────
tmux display-message -d 2000 '⚠  clipboard empty or unsupported' || true
log_line "fail-fallback" "-" "-" "-" "no-usable-mime mimes=$(printf '%s' "$mimes" | tr '\n' ',' | head -c 200)"
exit 1
