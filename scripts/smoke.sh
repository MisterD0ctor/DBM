#!/bin/sh
# The harness exercises that check what they see, each in a compositor of its
# own (see `headless.sh`) against the first episode `smoke-media.sh` makes,
# with an app-data folder of its own. Fails if a check failed or an exercise
# didn't reach its end.
#
#   scripts/smoke.sh <player> <media folder>
#
# The player has to be built with the harness:
# `cargo build --release -p dbm-player --features harness`.
set -u

if [ $# -ne 2 ]; then
    echo "usage: $0 <player> <media folder>" >&2
    exit 2
fi
player=$1
film="$2/Smoke Test S01E01 Episode 1.mkv"
here=$(dirname "$0")
failed=0

# How long past its own quit a run is given before it is taken to have hung.
# Then its log says how far it got, and its threads where it waits, where gdb
# and passwordless sudo are there to ask (as on CI); it is stopped, and the
# rest still run, rather than the job waiting out its own limit for hours.
grace=120

# The player started for this run, by the app-data folder only it was given.
run_pids() {
    for pid in $(pgrep -f dbm-player); do
        if tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null | grep -qx "APPDATA=$data"; then
            echo "$pid"
        fi
    done
}

hung() {
    echo "== $exercise did not end within $(( limit / 1000 + grace ))s"
    for pid in $(run_pids); do
        if command -v gdb >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
            sudo -n gdb -p "$pid" -batch -ex "thread apply all bt" 2>&1 \
                | grep -E '^(Thread|#)' | head -400
        fi
        kill -KILL "$pid" 2>/dev/null || true
    done
}

# Each exercise with how long it is given, and so how long its run lasts,
# well past how long it takes with no GPU: the input one is twenty-eight steps
# a second and a half apart, and the scrub one 120 steps that each wait for a
# frame, which software GL draws a few of a second. One marked `:empty` starts
# on an empty window and is handed the film to open itself.
for run in DBM_INPUT_TEST:90000 DBM_SCRUB_TEST:90000 DBM_SUBS_TEST:30000 \
    DBM_SKIP_TEST:90000 DBM_ALWAYS_TEST:90000 DBM_STAY_TEST:60000 \
    DBM_WAY_IN_TEST:60000:empty DBM_ALLOC_TEST:60000; do
    exercise=${run%%:*}
    rest=${run#*:}
    limit=${rest%%:*}
    if [ "$rest" = "$limit" ]; then
        value=1 file=$film
    else
        value=$film file=
    fi
    data=$(mktemp -d)
    log=$(mktemp)
    echo "== $exercise"
    if APPDATA="$data" timeout -k 10 "$(( limit / 1000 + grace ))" \
        "$here/headless.sh" env "$exercise=$value" \
        DBM_QUIT_AFTER="$limit" "$player" ${file:+"$file"} >"$log" 2>&1; then
        grep -E '^dbm: (pass|FAIL|---)' "$log"
    else
        status=$?
        if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
            hung
        fi
        cat "$log"
        failed=1
    fi
    rm -rf -- "$data" "$log"
done
exit "$failed"
