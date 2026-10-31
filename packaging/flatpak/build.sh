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

# Cargo's output is kept here between builds (see the manifest's player
# module), so a build compiles what changed rather than every crate.
mkdir -p "$out/state/ccache/cargo"

# Sources are fetched only when a commit the manifest pins isn't in
# flatpak-builder's mirrors yet. Asking each repository whether its tag has
# moved is the slowest step after Rust, and a new pin is fetched as before.
mirrored=yes
for commit in $(sed -n 's/^ *commit: *//p' "$here/$id.yml"); do
    found=
    for mirror in "$out"/state/git/*/; do
        if git --git-dir="$mirror" cat-file -e "$commit" 2>/dev/null; then
            found=yes
            break
        fi
    done
    [ -n "$found" ] || mirrored=
done

$builder --user --install --force-clean \
    ${mirrored:+--disable-updates} \
    --delete-build-dirs \
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
