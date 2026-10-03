#!/bin/sh
# Run a command against a compositor of its own, with no screen.
#
#   scripts/headless.sh <command...>
#   scripts/headless.sh env DBM_SHOWCASE=bar DBM_CAPTURE=bar.bmp \
#       ./target/release/dbm-player path/to/video.mkv
#
# For the harness: a window it opens on the desktop takes focus as it maps,
# and sits wherever a hand was about to click. Asking it not to is not
# possible on Wayland — focus is the compositor's decision, and winit's
# `with_active(false)` is unsupported there — so the window goes somewhere
# else instead: a headless Mutter with one virtual monitor, on a Wayland
# socket of its own, which nothing on the desktop can see. The player renders
# there as it would on screen, GPU and all, and `DBM_CAPTURE` reads the frame
# back the same way.
#
# DISPLAY is taken away from the command, so a player that cannot reach the
# headless socket fails rather than falling back to the desktop's Xwayland.
# The compositor gets its own D-Bus session, so it cannot collide with the
# running shell, and it is stopped when the command ends.
#
# DBM_HEADLESS_SIZE sets the virtual monitor, 1920x1080 by default: larger
# than the 1280x720 the window asks for, so the window gets that size — its
# title bar is drawn by the client and would otherwise come out of it.
#
# Commands run through distrobox see the same socket: `distrobox enter`
# forwards WAYLAND_DISPLAY, and the socket is in the shared runtime directory.
set -eu

if [ $# -eq 0 ]; then
    echo "usage: $0 <command...>" >&2
    exit 2
fi
if ! command -v mutter >/dev/null 2>&1; then
    echo "headless.sh: needs mutter on the host (GNOME's compositor)" >&2
    exit 1
fi

name="dbm-headless-$$"
size="${DBM_HEADLESS_SIZE:-1920x1080}"
log="${XDG_RUNTIME_DIR:?}/$name.log"

# In a session of its own, so the whole group — dbus-run-session, its bus and
# mutter — can be stopped together.
setsid dbus-run-session -- mutter --headless --wayland --no-x11 \
    --wayland-display="$name" --virtual-monitor "$size" >"$log" 2>&1 &
compositor=$!

# Ask the group to stop and wait until all of it has: mutter takes a moment to
# shut down after its parent has gone. Killed outright after five seconds.
stop() {
    kill -TERM -- "-$compositor" 2>/dev/null || true
    tries=0
    while kill -0 -- "-$compositor" 2>/dev/null; do
        tries=$((tries + 1))
        if [ "$tries" -gt 50 ]; then
            kill -KILL -- "-$compositor" 2>/dev/null || true
            break
        fi
        sleep 0.1
    done
    rm -f "$log"
}
trap stop EXIT
trap 'exit 130' INT TERM

tries=0
until [ -S "$XDG_RUNTIME_DIR/$name" ]; do
    tries=$((tries + 1))
    if [ "$tries" -gt 100 ] || ! kill -0 "$compositor" 2>/dev/null; then
        echo "headless.sh: mutter did not start:" >&2
        cat "$log" >&2
        exit 1
    fi
    sleep 0.1
done

status=0
env -u DISPLAY WAYLAND_DISPLAY="$name" "$@" || status=$?
exit "$status"
