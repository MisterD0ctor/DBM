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

# Each exercise with how long it is given, and so how long its run lasts,
# well past how long it takes with no GPU: the input one is twenty-eight steps
# a second and a half apart, and the scrub one 120 steps that each wait for a
# frame, which software GL draws a few of a second.
for run in DBM_INPUT_TEST:90000 DBM_SCRUB_TEST:90000 DBM_SUBS_TEST:30000 \
    DBM_SKIP_TEST:90000 DBM_ALWAYS_TEST:90000 DBM_STAY_TEST:60000 \
    DBM_ALLOC_TEST:60000; do
    exercise=${run%%:*}
    data=$(mktemp -d)
    log=$(mktemp)
    echo "== $exercise"
    if APPDATA="$data" "$here/headless.sh" env "$exercise=1" \
        DBM_QUIT_AFTER="${run#*:}" "$player" "$film" >"$log" 2>&1; then
        grep -E '^dbm: (pass|FAIL|---)' "$log"
    else
        cat "$log"
        failed=1
    fi
    rm -rf -- "$data" "$log"
done
exit "$failed"
