#!/bin/sh
# Builds the Flatpak from this checkout and installs it for the current user.
#
#   packaging/flatpak/build.sh            build and install
#   packaging/flatpak/build.sh --bundle   also write target/flatpak/death-by-mpv.flatpak
#
# The bundle is one file someone can install without Flathub: it names
# Flathub as the place to fetch the runtime from, so only the app is inside it.
set -eu

here=$(dirname "$(realpath "$0")")
id=io.github.MisterD0ctor.DBM
out="$here/../../target/flatpak"

if command -v flatpak-builder >/dev/null 2>&1; then
    builder=flatpak-builder
else
    builder="flatpak run --filesystem=host org.flatpak.Builder"
fi

$builder --user --install --force-clean \
    --install-deps-from=flathub \
    --state-dir="$out/state" \
    --repo="$out/repo" \
    "$out/build" "$here/$id.yml"

if [ "${1:-}" = "--bundle" ]; then
    flatpak build-bundle \
        --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
        "$out/repo" "$out/death-by-mpv.flatpak" "$id"
    echo "bundle: $(realpath "$out/death-by-mpv.flatpak")"
fi
