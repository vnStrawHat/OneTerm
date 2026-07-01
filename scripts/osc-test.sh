#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# osc-test.sh — Interactive OSC escape-sequence tester for OneTerm.
#
# Run this INSIDE a OneTerm terminal (local shell or SSH). Pick a number to fire
# exactly one OSC sequence so you can verify each feature in isolation. Query
# commands (OSC 10/11/12/4/52 with `?`) also read back and print the terminal's
# reply so you can confirm the response path works.
#
# Usage:
#   bash scripts/osc-test.sh            # interactive menu
#   bash scripts/osc-test.sh 11q        # run one test by id (see the menu)
#
# Portable to any POSIX bash (Linux/macOS/WSL/git-bash). For the Windows local
# shell (PowerShell), use scripts/osc-test.ps1 instead.
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

ESC=$'\033'
BEL=$'\007'
ST=$'\033\\'

# Emit a raw byte string. The ESC/BEL/ST vars already hold real bytes (ANSI-C
# quoting), so use %s — NOT %b, which would re-interpret the backslash in ST.
emit() { printf '%s' "$1"; }

# Send a query sequence, then read the terminal's reply and print it with
# escapes made visible. Puts the tty in non-canonical mode so the reply (which
# has no trailing newline) is delivered immediately instead of being line-buffered.
query() {
    local seq="$1" reply="" c old
    old=$(stty -g 2>/dev/null || true)
    stty -icanon -echo 2>/dev/null || true
    printf '%s' "$seq"
    # Wait up to 1s for the reply to start, then drain until a ~0.1s gap.
    if IFS= read -rsn1 -t 1 c 2>/dev/null; then
        reply+="$c"
        while IFS= read -rsn1 -t 0.1 c 2>/dev/null; do
            reply+="$c"
            [ "$c" = "$BEL" ] && break
        done
    fi
    [ -n "$old" ] && stty "$old" 2>/dev/null || true
    if [ -z "$reply" ]; then
        printf '  \033[31m(no reply within 1s)\033[0m\n'
    else
        printf '  reply: '
        printf '%s' "$reply" | cat -v
        printf '\n'
    fi
}

pause() { printf '\033[2m(enter to continue)\033[0m'; read -r _; }

# ── Individual tests ─────────────────────────────────────────────────────────
t_osc0()   { emit "${ESC}]0;OneTerm OSC0 title${BEL}";  echo "OSC 0  → title+icon set to 'OneTerm OSC0 title'"; }
t_osc2()   { emit "${ESC}]2;OneTerm OSC2 title${BEL}";  echo "OSC 2  → window title set to 'OneTerm OSC2 title'"; }

t_osc4()   { emit "${ESC}]4;1;rgb:00ff/0000/0000${BEL}"; echo "OSC 4  → palette index 1 (red) set to pure #ff0000. Type a red-colored char (e.g. \\e[31mX) to see it."; }
t_osc4b()  { emit "${ESC}]4;1;#00ff00${BEL}";            echo "OSC 4  → palette index 1 set to #00ff00 (hex form)."; }
t_osc4q()  { echo "OSC 4  query index 1:"; query "${ESC}]4;1;?${BEL}"; }
t_osc104() { emit "${ESC}]104;1${BEL}";                  echo "OSC 104 → palette index 1 reset to theme default."; }
t_osc104a(){ emit "${ESC}]104${BEL}";                    echo "OSC 104 → ALL palette colors reset to theme default."; }

t_osc7()   { emit "${ESC}]7;file://localhost/tmp${BEL}"; echo "OSC 7  → cwd set to /tmp (check breadcrumb)."; }
t_osc8()   { emit "${ESC}]8;;https://github.com${ST}OneTerm repo link${ESC}]8;;${ST}"; echo "  ← Ctrl+click the link above (OSC 8)."; }

t_osc9()   { emit "${ESC}]9;Hello from OneTerm (OSC 9)${BEL}"; echo "OSC 9  → desktop notification (expect a toast)."; }

t_osc9_4() {
    echo "OSC 9;4 → animating progress 0→100 (normal), then remove..."
    for p in 0 20 40 60 80 100; do emit "${ESC}]9;4;1;${p}${BEL}"; sleep 0.4; done
    sleep 0.5; emit "${ESC}]9;4;0${BEL}"; echo "  done (progress removed)."
}
t_osc9_4e(){ emit "${ESC}]9;4;2;66${BEL}";  echo "OSC 9;4 → error state @ 66% (expect danger-colored bar)."; }
t_osc9_4i(){ emit "${ESC}]9;4;3${BEL}";     echo "OSC 9;4 → indeterminate (expect full-width bar)."; }
t_osc9_4p(){ emit "${ESC}]9;4;4;50${BEL}";  echo "OSC 9;4 → paused @ 50% (expect warning-colored bar)."; }
t_osc9_4x(){ emit "${ESC}]9;4;0${BEL}";     echo "OSC 9;4 → remove progress."; }

t_osc10()  { emit "${ESC}]10;rgb:ffff/8000/0000${BEL}"; echo "OSC 10 → default foreground set to orange."; }
t_osc11()  { emit "${ESC}]11;rgb:0000/2000/4000${BEL}"; echo "OSC 11 → default background set to dark blue."; }
t_osc12()  { emit "${ESC}]12;rgb:00ff/ff00/0000${BEL}"; echo "OSC 12 → cursor color set to yellow."; }
t_osc10q() { echo "OSC 10 query (foreground):"; query "${ESC}]10;?${BEL}"; }
t_osc11q() { echo "OSC 11 query (background):"; query "${ESC}]11;?${BEL}"; }
t_osc12q() { echo "OSC 12 query (cursor):";     query "${ESC}]12;?${BEL}"; }
t_osc110() { emit "${ESC}]110${BEL}"; echo "OSC 110 → foreground reset to theme default."; }
t_osc111() { emit "${ESC}]111${BEL}"; echo "OSC 111 → background reset to theme default."; }
t_osc112() { emit "${ESC}]112${BEL}"; echo "OSC 112 → cursor color reset to theme default."; }

t_osc52()  {
    # base64 of "OneTerm OSC52 clipboard" (no trailing newline)
    local b64; b64=$(printf '%s' "OneTerm OSC52 clipboard" | base64 | tr -d '\n')
    emit "${ESC}]52;c;${b64}${BEL}"; echo "OSC 52 → wrote 'OneTerm OSC52 clipboard' to the system clipboard (paste to verify)."
}
t_osc52q() { echo "OSC 52 query (clipboard read):"; query "${ESC}]52;c;?${BEL}"; }

t_osc133() {
    local nl=$'\n'
    echo "OSC 133 → emitting a full prompt cycle (A/B/C/D;0)..."
    emit "${ESC}]133;A${BEL}"; emit "fake\$ "
    emit "${ESC}]133;B${BEL}"; emit "echo hi${nl}"
    emit "${ESC}]133;C${BEL}"; emit "hi${nl}"
    emit "${ESC}]133;D;0${BEL}"; echo "  done (prompt_count should increment)."
}

# ── Dispatch table: id → function + label ────────────────────────────────────
run_one() {
    case "$1" in
        0)     t_osc0 ;;
        2)     t_osc2 ;;
        4)     t_osc4 ;;
        4b)    t_osc4b ;;
        4q)    t_osc4q ;;
        104)   t_osc104 ;;
        104a)  t_osc104a ;;
        7)     t_osc7 ;;
        8)     t_osc8 ;;
        9)     t_osc9 ;;
        94)    t_osc9_4 ;;
        94e)   t_osc9_4e ;;
        94i)   t_osc9_4i ;;
        94p)   t_osc9_4p ;;
        94x)   t_osc9_4x ;;
        10)    t_osc10 ;;
        11)    t_osc11 ;;
        12)    t_osc12 ;;
        10q)   t_osc10q ;;
        11q)   t_osc11q ;;
        12q)   t_osc12q ;;
        110)   t_osc110 ;;
        111)   t_osc111 ;;
        112)   t_osc112 ;;
        52)    t_osc52 ;;
        52q)   t_osc52q ;;
        133)   t_osc133 ;;
        *) printf '\033[31mUnknown id: %s\033[0m\n' "$1"; return 1 ;;
    esac
}

menu() {
    cat <<'EOF'

  ── OneTerm OSC tester ──────────────────────────────────────────────
   Title      :  0  OSC 0 title      2   OSC 2 title
   Palette    :  4  set idx1 (rgb)   4b  set idx1 (#hex)   4q  query idx1
                104 reset idx1      104a reset ALL
   CWD/Link   :  7  OSC 7 cwd         8   OSC 8 hyperlink
   Notify     :  9  OSC 9 notification
   Progress   : 94  animate 0→100    94e error   94i indeterminate
                94p paused           94x remove
   FG/BG/Cur  : 10  set fg   11 set bg   12 set cursor
                10q query fg   11q query bg   12q query cursor
                110 reset fg   111 reset bg   112 reset cursor
   Clipboard  : 52  OSC 52 set        52q OSC 52 query
   Shell int. : 133 OSC 133 A/B/C/D
   ────────────────────────────────────────────────────────────────────
   q  quit
EOF
}

# ── One-shot mode: `osc-test.sh <id>` ────────────────────────────────────────
if [ "$#" -ge 1 ]; then
    run_one "$1"
    exit $?
fi

# ── Interactive loop ─────────────────────────────────────────────────────────
while true; do
    menu
    printf '  > '
    if ! read -r choice; then break; fi
    case "$choice" in
        q|Q|quit|exit) break ;;
        "") ;;
        *) run_one "$choice"; pause ;;
    esac
done
echo "bye."
