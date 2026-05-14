#!/usr/bin/env bash
# watcher-board — fleet watcher dashboard, iOS-styled (truecolor palette,
# rounded card containers, status pill chips, calm spacing).
set -eo pipefail
SESSION="${SESSION:-codex-fleet}"
STATUS_FILE="${STATUS_FILE:-/tmp/claude-viz/cap-swap-status.txt}"
LOG="${LOG:-/tmp/claude-viz/cap-swap.log}"
PROBE_CACHE="${PROBE_CACHE:-/tmp/claude-viz/cap-probe-cache}"
INTERVAL_MS="${WATCHER_INTERVAL_MS:-1000}"

trap 'printf "\033[?25h"; echo; exit' INT TERM EXIT
printf "\033[?25l"

INTERVAL_S=$(python3 -c "print(${INTERVAL_MS}/1000)")
f=0
while :; do
  python3 - "$SESSION" "$STATUS_FILE" "$LOG" "$PROBE_CACHE" "$f" <<'PY'
import sys, subprocess, re, os, json, time, glob
SESSION, STATUS_FILE, LOG, PROBE_CACHE, f = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5])

# ── iOS truecolor palette (kept in sync with fleet-tick.sh) ────────────
E = "\033"
R, B, D = E+"[0m", E+"[1m", E+"[2m"
IOS_BLUE     = E+"[38;2;0;122;255m"
IOS_GREEN    = E+"[38;2;52;199;89m"
IOS_RED      = E+"[38;2;255;59;48m"
IOS_ORANGE   = E+"[38;2;255;149;0m"
IOS_YELLOW   = E+"[38;2;255;204;0m"
IOS_GRAY     = E+"[38;2;142;142;147m"
IOS_GRAY2    = E+"[38;2;174;174;178m"
IOS_GRAY6    = E+"[38;2;242;242;247m"
IOS_WHITE    = E+"[38;2;255;255;255m"
# Chip backgrounds (bg color + fg white)
BG_GREEN     = E+"[48;2;52;199;89m"  + E+"[38;2;255;255;255m"
BG_RED       = E+"[48;2;255;59;48m"  + E+"[38;2;255;255;255m"
BG_ORANGE    = E+"[48;2;255;149;0m"  + E+"[38;2;255;255;255m"
BG_YELLOW    = E+"[48;2;255;204;0m"  + E+"[38;2;30;30;30m"
BG_BLUE      = E+"[48;2;0;122;255m"  + E+"[38;2;255;255;255m"
BG_GRAY      = E+"[48;2;142;142;147m"+ E+"[38;2;255;255;255m"
# Section accent gradient — used for rail headers, separators
TEAL = E+"[38;5;73m"
ICE  = E+"[38;5;117m"

CLR = "\033[K"
HOME = "\033[H"
CLR_EOS = "\033[J"

def sh(*a, default=""):
    try:
        return subprocess.check_output(list(a), text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return default

def trunc(s, n):
    return s if len(s) <= n else s[:n-1] + "…"

now = int(time.time())
spinner = ["◐", "◓", "◑", "◒"][(f//2) % 4]

# ── 1. Status file ────────────────────────────────────────────────────
panes_n = capped_n = swap_n = ranked_n = 0
last_sweep = "—"
interval = 30
cooldown = 180
if os.path.exists(STATUS_FILE):
    txt = open(STATUS_FILE).read()
    m = re.search(r"last sweep: (\S+)", txt);   last_sweep = m.group(1) if m else "—"
    m = re.search(r"interval=(\d+)", txt);      interval = int(m.group(1)) if m else 30
    m = re.search(r"cooldown=(\d+)", txt);      cooldown = int(m.group(1)) if m else 180
    m = re.search(r"overview: (\d+)", txt);     panes_n = int(m.group(1)) if m else 0
    m = re.search(r"capped this sweep: (\d+)", txt);  capped_n = int(m.group(1)) if m else 0
    m = re.search(r"swaps this sweep: +(\d+)", txt);  swap_n = int(m.group(1)) if m else 0
    m = re.search(r"ranked, not yet probed\): (\d+)", txt); ranked_n = int(m.group(1)) if m else 0

if last_sweep != "—":
    try:
        h, mm, ss = map(int, last_sweep.split(":"))
        today = time.localtime(now)
        sweep_epoch = int(time.mktime((today.tm_year, today.tm_mon, today.tm_mday, h, mm, ss, 0, 0, today.tm_isdst)))
        delta_since = now - sweep_epoch
        if delta_since < 0 or delta_since > 3600:
            next_in = "?"
        else:
            next_in = max(0, interval - delta_since)
    except Exception:
        next_in = "?"
else:
    next_in = "?"

# ── 2. id→email + state ───────────────────────────────────────────────
id_to_email = {}
for entry in os.listdir("/proc"):
    if not entry.isdigit(): continue
    p = f"/proc/{entry}/environ"
    try:
        with open(p, "rb") as fh:
            data = fh.read().decode("utf8", "replace")
    except Exception:
        continue
    if "CODEX_FLEET_AGENT_NAME" not in data: continue
    env = dict(line.split("=", 1) for line in data.split("\x00") if "=" in line)
    name = env.get("CODEX_FLEET_AGENT_NAME", "")
    email = env.get("CODEX_FLEET_ACCOUNT_EMAIL", "")
    if name and email:
        short = name.removeprefix("codex-")
        id_to_email[short] = email

# ── 3. Per-pane state ─────────────────────────────────────────────────
panes = []
for line in sh("tmux", "list-panes", "-t", f"{SESSION}:overview", "-F",
               "#{pane_id}\t#{pane_dead}\t#{pane_current_command}").splitlines():
    parts = line.split("\t")
    if len(parts) < 3: continue
    pid, dead, cmd = parts
    panel_raw = sh("tmux", "show-option", "-pqv", "-t", pid, "@panel")
    m = re.search(r"\[codex-([a-z0-9-]+)\]?", panel_raw)
    agent_id = m.group(1) if m else (panel_raw.strip("[]") or pid)
    scroll = sh("tmux", "capture-pane", "-p", "-t", pid, "-S", "-30")
    if dead == "1":
        state = "dead"
    elif re.search(r"usage limit|Rate limit reached|hit your usage", scroll, re.I):
        state = "capped"
    elif "branchStart command failed" in scroll or "agent-branch-start.sh failed" in scroll:
        state = "guard-fail"
    elif re.search(r"Working \(\d+", scroll):
        state = "working"
    elif "Reviewing approval" in scroll:
        state = "approval"
    elif "Starting MCP servers" in scroll:
        state = "boot"
    elif "polling" in panel_raw.lower():
        state = "polling"
    else:
        state = "idle"
    panes.append({"pid": pid, "agent": agent_id, "state": state})

# ── 4. Quotas ─────────────────────────────────────────────────────────
quotas = {}
for line in sh("codex-auth", "list").splitlines():
    em = re.search(r"([\w.+-]+@[\w.-]+\.[a-z]+)", line)
    if not em: continue
    h5 = re.search(r"5h=(\d+)%", line); wk = re.search(r"weekly=(\d+)%", line)
    if not h5 or not wk: continue
    quotas[em.group(1)] = (int(h5.group(1)), int(wk.group(1)))

# ── 5. Cap pool + healthy pool + per-email probe map ─────────────────
# probe_map: {email -> (verdict, eta_str_or_None)} — used by FLEET PANES card
# below as the live "5h" signal, since codex-auth list's 5h column reports the
# API meter (often 100% used while the rolling cap is still fine) and is
# misleading. cap-probe's verdict comes from an actual `codex exec` round-trip.
cap_pool = []
healthy_pool = []
probe_map = {}
for cf in glob.glob(f"{PROBE_CACHE}/*.json"):
    try:
        d = json.load(open(cf))
    except Exception:
        continue
    email = os.path.basename(cf).rsplit(".", 1)[0]
    v = d.get("verdict", "unknown")
    eta = None
    if v == "healthy":
        healthy_pool.append(email)
    elif v == "capped":
        until = d.get("until_epoch", 0) or 0
        if until <= now:
            v = "unknown"
        else:
            delta = until - now
            if delta > 24*3600: eta = f"{delta//(24*3600)}d"
            elif delta > 3600:  eta = f"{delta//3600}h"
            else:               eta = f"{delta//60}m"
            cap_pool.append((until, email, d.get("until_text", "—"), eta))
    probe_map[email] = (v, eta)
cap_pool.sort()

working_n  = sum(1 for p in panes if p["state"] in ("working","approval"))
pane_capped = sum(1 for p in panes if p["state"] == "capped")
pane_dead   = sum(1 for p in panes if p["state"] in ("dead","guard-fail"))
pane_idle   = sum(1 for p in panes if p["state"] in ("idle","polling","boot"))
health_pct  = int(100 * working_n / max(1, len(panes))) if panes else 0

# ── iOS card helpers ──────────────────────────────────────────────────
def strip_ansi(s):
    return re.sub(r"\x1B\[[0-9;]*[A-Za-z]", "", s)

def visible_len(s):
    return len(strip_ansi(s))

CARD_W_DEFAULT = 120
# `tput cols` reads a controlling tty; when run from a subprocess piped from
# python it falls back to 80. Prefer tmux's own pane width — that's always
# correct. Fall back to COLUMNS env, then tput, then 120.
cols = 0
try:
    pw = sh("tmux", "display-message", "-p", "#{pane_width}")
    if pw.isdigit():
        cols = int(pw)
except Exception:
    pass
if cols < 40:
    try:
        cols = int(os.environ.get("COLUMNS", "0")) or cols
    except Exception:
        pass
if cols < 40:
    try:
        cols = int(sh("tput", "cols", default="120") or "120")
    except Exception:
        cols = 120
CARD_W = max(70, min(cols - 4, 180))

def card_top(title, w=CARD_W):
    # ╭─ TITLE ───────────╮ — title in WHITE BOLD, border in GRAY2
    label = f" {B}{IOS_WHITE}{title}{R}{IOS_GRAY2} "
    label_vis = visible_len(label)
    fill = w - label_vis - 4
    if fill < 1: fill = 1
    return f"{IOS_GRAY2}╭─{label}{'─'*fill}─╮{R}"

def card_bottom(w=CARD_W):
    return f"{IOS_GRAY2}╰{'─'*(w-2)}╯{R}"

def card_row(content, w=CARD_W):
    # Total width math: │ + "  " + content + pad + "  " + │ = 6 + vis + pad
    # So pad = w - 6 - vis (not w - 4 — that's an off-by-2 bug).
    vis = visible_len(content)
    pad = w - 6 - vis
    if pad < 0: pad = 0
    return f"{IOS_GRAY2}│{R}  {content}{' '*pad}  {IOS_GRAY2}│{R}"

def card_blank(w=CARD_W):
    return card_row("", w)

# Rounded status pill — `◖ ● working ◗` style with colored bg
def chip(label, bg, fg_icon=""):
    return f"{IOS_GRAY2}◖{R}{bg} {fg_icon}{' ' if fg_icon else ''}{label} {R}{IOS_GRAY2}◗{R}"

# ── 7. RENDER ─────────────────────────────────────────────────────────
out = [HOME]

# ── Header banner card ────────────────────────────────────────────────
if pane_capped == 0 and pane_dead == 0:
    health_label, health_bg, health_icon = "ALL CLEAR", BG_GREEN, "✓"
elif working_n > 0:
    health_label, health_bg, health_icon = "DEGRADED", BG_YELLOW, "⚠"
else:
    health_label, health_bg, health_icon = "STALLED", BG_RED, "✕"

hdr_line = (f"{B}{TEAL}WATCHER{R}  {IOS_WHITE}{SESSION}{R}   "
            f"{IOS_GREEN}●{R} {D}live{R}  "
            f"{D}{time.strftime('%H:%M:%S')}{R}   "
            f"{D}last sweep{R} {ICE}{last_sweep}{R}   "
            f"{D}next in{R} {ICE}{next_in}s{R}   "
            f"{chip(health_label, health_bg, health_icon)}")
out.append(card_top("FLEET WATCHER · iOS"))
out.append(card_row(hdr_line))
out.append(card_bottom())
out.append(CLR)

# ── Stat cards row — 4 mini iOS cards, rounded ────────────────────────
MINI_W = 26
def mini_card(label, value, value_color, sub):
    # Same off-by-2 fix as card_row: chrome is 6 chars (│ + 2 margin + 2 margin + │).
    title_pad = MINI_W - 6 - len(label)
    if title_pad < 0: title_pad = 0
    value_vis = len(value)
    value_pad = MINI_W - 6 - value_vis
    if value_pad < 0: value_pad = 0
    sub_pad = MINI_W - 6 - len(sub)
    if sub_pad < 0: sub_pad = 0
    return [
      f"{IOS_GRAY2}╭{'─'*(MINI_W-2)}╮{R}",
      f"{IOS_GRAY2}│{R}  {D}{label}{R}{' '*title_pad}  {IOS_GRAY2}│{R}",
      f"{IOS_GRAY2}│{R}  {B}{value_color}{value}{R}{' '*value_pad}  {IOS_GRAY2}│{R}",
      f"{IOS_GRAY2}│{R}  {D}{sub}{R}{' '*sub_pad}  {IOS_GRAY2}│{R}",
      f"{IOS_GRAY2}╰{'─'*(MINI_W-2)}╯{R}",
    ]

cap_col   = IOS_GREEN if pane_capped == 0 else IOS_RED
swap_col  = IOS_YELLOW if swap_n > 0 else IOS_GRAY
rank_col  = ICE if ranked_n > 0 else IOS_GRAY

cards = [
    mini_card("PANES",   f"{panes_n} ({working_n} work)", IOS_WHITE, "in overview"),
    mini_card("CAPPED",  str(pane_capped),                cap_col,   "this sweep"),
    mini_card("SWAPPED", str(swap_n),                     swap_col,  "this sweep"),
    mini_card("RANKED",  str(ranked_n),                   rank_col,  "candidates"),
]
for row in range(5):
    out.append("  " + "  ".join(c[row] for c in cards) + CLR)
out.append(CLR)

# ── ACCOUNT POOL card ─────────────────────────────────────────────────
healthy_count = len(healthy_pool)
capped_count = len(cap_pool)
soonest = ""
if cap_pool:
    _, _, _, eta = cap_pool[0]
    soonest = f"   {D}·{R}   {D}soonest reset{R} {IOS_YELLOW}{eta}{R}"
pool_line = (f"{chip(f'✓ {healthy_count} healthy', BG_GREEN)}   "
             f"{chip(f'✕ {capped_count} capped',  BG_RED)}"
             f"{soonest}")
out.append(card_top("ACCOUNT POOL"))
out.append(card_row(pool_line))
out.append(card_bottom())
out.append(CLR)

# ── FLEET PANES card — status chips per pane ──────────────────────────
state_meta = {
    "capped":    (BG_RED,    "✕", "CAPPED"),
    "working":   (BG_GREEN,  spinner, "working"),
    "approval":  (BG_YELLOW, "⟳", "approval"),
    "boot":      (BG_BLUE,   "⋯", "booting"),
    "polling":   (BG_GRAY,   "◌", "polling"),
    "idle":      (BG_GRAY,   "◇", "idle"),
    "dead":      (BG_RED,    "☠", "DEAD"),
    "guard-fail":(BG_ORANGE, "⚠", "guard-fail"),
}
out.append(card_top("FLEET PANES"))
hdr = (f"{D}PANE  AGENT                          STATE          "
       f"5h-LIVE   WK-USED   ACCOUNT{R}")
out.append(card_row(hdr))
# 5h column shows the live cap-probe verdict (healthy/capped/unknown) instead
# of codex-auth's API-meter percentage. WK-USED matches the value `codex-auth
# list` prints for `weekly=`, so the watcher reads identically to the shell.
for p in panes:
    bg, icon, label = state_meta.get(p["state"], (BG_GRAY, "◇", p["state"]))
    email = id_to_email.get(p["agent"], "")
    _h5, wk = quotas.get(email, (None, None))
    pv, peta = probe_map.get(email, ("unknown", None))
    if pv == "healthy":
        five_chip = chip("✓ OK", BG_GREEN)
    elif pv == "capped":
        five_chip = chip(f"✕ {peta or 'cap'}", BG_RED)
    else:
        five_chip = chip("? ??", BG_GRAY)
    five_pad = max(0, 9 - visible_len(five_chip))
    if wk is None:
        wk_cell = f"{D}  —  {R}"
    else:
        wk_col = IOS_GREEN if wk <= 40 else (IOS_YELLOW if wk <= 75 else IOS_RED)
        wk_cell = f"{wk_col}{wk:>3}%{R} "
    q_avail = f"{five_chip}{' '*five_pad} {wk_cell}  "
    email_short = email.split("@")[0] if email else "—"
    pid_short = p["pid"].lstrip("%")[:4]
    agent_disp = p["agent"][:28].ljust(28)
    chip_str = chip(f"{icon} {label}", bg)
    # Pad the chip to a fixed visible width so the next column aligns.
    chip_vis = visible_len(chip_str)
    chip_pad = max(0, 18 - chip_vis)
    row = (f"{D}{pid_short:<5}{R} {IOS_WHITE}{agent_disp}{R} "
           f"{chip_str}{' '*chip_pad}  "
           f"{q_avail}  {D}{email_short[:18]}{R}")
    out.append(card_row(row))
out.append(card_bottom())
out.append(CLR)

# ── CAP POOL card ─────────────────────────────────────────────────────
out.append(card_top("CAP POOL · burned accounts · sorted by reset ETA"))
if not cap_pool:
    out.append(card_row(f"{D}(empty — probe to populate){R}"))
else:
    for until, email, text, eta in cap_pool[:8]:
        if "d" in eta:   ecol = IOS_RED
        elif "h" in eta: ecol = IOS_YELLOW
        else:            ecol = IOS_GREEN
        row = (f"{chip('✕', BG_RED)}  {IOS_WHITE}{email[:30].ljust(30)}{R}   "
               f"{D}resets in{R} {ecol}{eta:>4}{R}   {D}{text[:36]}{R}")
        out.append(card_row(row))
out.append(card_bottom())
out.append(CLR)

# ── RECENT ACTIVITY card ──────────────────────────────────────────────
out.append(card_top("RECENT ACTIVITY"))
try:
    log_lines = open(LOG).read().splitlines()[-30:]
except Exception:
    log_lines = []
filtered = []
seen_last = ""
for line in log_lines:
    line = line.rstrip()
    if not line: continue
    if not re.match(r"^\[\d\d:\d\d:\d\d\]|^\[cap-probe\]", line): continue
    if line.startswith("[cap-probe] only ") and "healthy accounts" in line: continue
    if line == seen_last: continue
    filtered.append(line); seen_last = line
filtered = filtered[-10:]
if not filtered:
    out.append(card_row(f"{D}(no recent activity){R}"))
else:
    for line in filtered:
        if "SWAPPED" in line:                            col, icon = IOS_YELLOW, "⇄"
        elif "DETECTED" in line:                         col, icon = IOS_RED,    "✕"
        elif "started" in line:                          col, icon = ICE,        "▶"
        elif "no healthy" in line:                       col, icon = IOS_GRAY,   "·"
        elif "cap-probe confirmed" in line:              col, icon = IOS_GREEN,  "✓"
        elif "cache HIT capped" in line:                 col, icon = IOS_RED,    "•"
        elif "cache HIT healthy" in line:                col, icon = IOS_GREEN,  "•"
        elif "probing" in line:                          col, icon = ICE,        "⟳"
        elif "probe " in line and "-> capped" in line:   col, icon = IOS_RED,    "↓"
        elif "probe " in line and "-> healthy" in line:  col, icon = IOS_GREEN,  "↑"
        else:                                            col, icon = IOS_GRAY,   " "
        clean = re.sub(r"^\[\d\d:\d\d:\d\d\]\s*", "", line)
        clean = re.sub(r"^\[cap-probe\]\s*", "", clean)
        ts_m = re.match(r"^\[(\d\d:\d\d:\d\d)\]", line)
        ts_str = ts_m.group(1) if ts_m else ""
        row = f"{D}{ts_str:<8}{R}  {col}{icon}{R}  {IOS_WHITE}{clean[:120]}{R}"
        out.append(card_row(row))
out.append(card_bottom())
out.append(CLR)

# Footer — provenance line, dim
out.append(f"  {D}iOS palette · #007AFF / #34C759 / #FF3B30 / #FF9500 · rounded cards{R}{CLR}")

out.append(CLR_EOS)
sys.stdout.write("\n".join(out))
sys.stdout.flush()
PY
  f=$((f+1))
  sleep "$INTERVAL_S"
done
