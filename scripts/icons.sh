#!/bin/sh
# Render the player's icons from the logo.
#
#   scripts/icons.sh
#
# Every icon the player ships is the one drawing in public/death-by-mpv.svg:
# the window's (128x128@2x.png, which Slint hands the title bar and task
# bar), the Windows executable's and its installer's (icon.ico, compiled in
# by build.rs), and the Flatpak's at each size it installs beside the SVG
# itself. Run this after changing the logo, so that none of them goes on
# showing the old one.
#
# Each size is drawn from the vector at that size, not shrunk from a large
# one: what is crisp at 16px is the curve laid down on 16px, not a 512px
# picture averaged down to it.
#
# Needs ImageMagick with its librsvg delegate (`magick -list format` shows
# SVG as RSVG).
set -eu

cd "$(dirname "$0")/.."
src=public/death-by-mpv.svg
out=crates/player/icons

if ! magick -list format | grep -q 'RSVG'; then
    echo "icons.sh: needs ImageMagick with librsvg" >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The drawing's own width at 96 dpi, which is what `-density` scales from.
width=$(magick -background none "$src" -format '%w' info:)

render() { # size, destination
    density=$(awk -v s="$1" -v w="$width" 'BEGIN { printf "%.6f", 96 * s / w }')
    magick -background none -density "$density" "$src" \
        -resize "${1}x${1}" -gravity center -extent "${1}x${1}" \
        "PNG32:$2"
}

for size in 16 24 32 48 64 128 256 512; do
    render "$size" "$tmp/$size.png"
done

cp "$tmp/32.png" "$out/32x32.png"
cp "$tmp/64.png" "$out/64x64.png"
cp "$tmp/128.png" "$out/128x128.png"
cp "$tmp/256.png" "$out/128x128@2x.png"
cp "$tmp/512.png" "$out/icon.png"
# The sizes Windows asks an executable for, smallest first, each one the PNG
# rendered above. ImageMagick would write them as uncompressed bitmaps, ten
# times the size for the same pixels; Windows has read PNG inside an icon
# since Vista, and the container is only a directory in front of the files.
python3 -I - "$out/icon.ico" "$tmp"/16.png "$tmp"/24.png "$tmp"/32.png \
    "$tmp"/48.png "$tmp"/64.png "$tmp"/256.png <<'EOF'
import struct, sys
dest, pngs = sys.argv[1], [open(p, "rb").read() for p in sys.argv[2:]]
offset = 6 + 16 * len(pngs)
head = struct.pack("<HHH", 0, 1, len(pngs))
for png in pngs:
    width, height = struct.unpack(">II", png[16:24])
    # 0 stands for 256, which a byte cannot hold.
    head += struct.pack("<BBBBHHII", width % 256, height % 256, 0, 0, 1, 32, len(png), offset)
    offset += len(png)
open(dest, "wb").write(head + b"".join(pngs))
EOF

echo "icons.sh: rendered $out from $src"
