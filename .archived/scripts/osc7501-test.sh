#!/bin/sh
# Manual check for OSC 7501 (Program Status Protocol) in a OneTerm tab.
# Watch the tab title icon while it runs. Works locally (Git Bash) and over SSH.
#
#   sh scripts/osc7501-test.sh            # demo: every state, then the multi-id priority,
#                                         # repeated until Ctrl+C (which clears the icon)
#   sh scripts/osc7501-test.sh set STATE [ID]
#                                         # STATE: idle working done blocked error clear
#   sh scripts/osc7501-test.sh probe      # the support probe; OneTerm echoes it back
#
# Note: a new shell prompt (OSC 133 A) drops working / blocked / idle records,
# so after `set` exits only done / error stay on the tab.

osc() { printf '\033]7501;%s\007' "$1"; }
b64() { printf '%s' "$1" | base64 | tr -d '\n'; }
step() { echo "$1"; osc "$2"; sleep "${3:-3}"; }

case "${1:-demo}" in
set)
    [ -n "$2" ] || { echo "usage: $0 set STATE [ID]" >&2; exit 2; }
    osc "state=$2${3:+:id=$3}"
    ;;
probe)
    old=$(stty -g) || exit 1
    stty raw -echo min 0 time 20
    printf '\033]7501;?\007'
    reply=$(dd bs=64 count=1 2>/dev/null | od -An -c)
    stty "$old"
    if [ -n "$reply" ]; then echo "reply:$reply"; else echo "no reply (not supported)"; fi
    ;;
demo)
    trap 'osc "state=clear"; echo; echo stopped; exit 0' INT TERM
    round=0
    while :; do
        round=$((round + 1)); echo "--- round $round (Ctrl+C to stop) ---"
        step "working"                 "state=working"
        step "blocked (waiting on you)" "state=blocked"
        step "error"                   "state=error"
        step "done"                    "state=done"
        step "idle"                    "state=idle"
        step "clear: icon gone"        "state=clear"
        # title / msg are validated and never displayed: the tab must not show this text.
        step "working + title/msg (text must NOT appear)" \
            "state=working:title=$(b64 'SHOULD NOT SHOW'):msg=$(b64 'hidden message')"
        step "invalid state: ignored, still working" "state=bogus"
        osc "state=clear"
        echo "priority: agent/a working + agent/b blocked -> tab shows blocked"
        osc "state=working:id=agent/a"; step "" "state=blocked:id=agent/b"
        step "agent/b done -> tab shows working" "state=done:id=agent/b"
        step "clear agent (and its subtree) -> icon gone" "state=clear:id=agent"
        step "done" "state=done"
    done
    ;;
*)
    sed -n '2,12p' "$0"; exit 2
    ;;
esac
