#!/bin/sh
# Render the player's icons from the logo.
#
#   scripts/icons.sh
#
# Every icon the player ships is the one drawing in public/death-by-mpv.svg:
# the window's (128x128@2x.png, which Slint hands the task bar), the Windows
# executable's and its installer's (icon.ico, compiled in by build.rs), and
# the Flatpak's at each size it installs beside the SVG itself. The one
# exception is the symbolic icon (symbolic.svg), which is
# public/death-by-mpv-simple.svg, the version drawn for small sizes. Run this
# after changing either drawing, so that none of the icons goes on showing
# the old one.
#
# Each size is drawn from the vector at that size, not shrunk from a large
# one: what is crisp at 16px is the curve laid down on 16px, not a 512px
# picture averaged down to it.
#
# Needs ImageMagick with its librsvg delegate (`magick -list format` shows
# SVG as RSVG), and Inkscape, installed or as its Flatpak.
set -eu

cd "$(dirname "$0")/.."
src=public/death-by-mpv.svg
simple=public/death-by-mpv-simple.svg
out=crates/player/icons

if ! magick -list format | grep -q 'RSVG'; then
    echo "icons.sh: needs ImageMagick with librsvg" >&2
    exit 1
fi
if command -v inkscape >/dev/null; then
    inkscape=inkscape
elif flatpak info org.inkscape.Inkscape >/dev/null 2>&1; then
    inkscape="flatpak run org.inkscape.Inkscape"
else
    echo "icons.sh: needs Inkscape" >&2
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

# What GNOME shows in place of the full icon wherever it draws icons in the
# colour of the text around them, notifications among them; an app without
# one gets its full icon in grey. GNOME recolours by forcing a fill onto
# every path, which would fill in a stroke as well, so Inkscape first turns
# every shape into a path and every stroke into the outline of its line.
# What is left is those paths alone, in the near-black GNOME draws its own
# symbolic icons in, for anything that shows the file as it is.
$inkscape --pipe --actions="select-all:all;object-to-path;object-stroke-to-path" \
    --export-plain-svg --export-type=svg --export-filename=- \
    <"$simple" >"$tmp/outlined.svg" 2>/dev/null
python3 -I - "$tmp/outlined.svg" "$out/symbolic.svg" <<'EOF'
import re, sys, xml.etree.ElementTree as ET
SVG = "{http://www.w3.org/2000/svg}"
ET.register_namespace("", SVG[1:-1])
src, dest = sys.argv[1:]
root = ET.parse(src).getroot()

def style(element, name):
    found = re.search(rf"(?:^|;)\s*{name}\s*:\s*([^;]+)", element.get("style", ""))
    return found.group(1).strip() if found else element.get(name)

def strip(parent):
    for child in list(parent):
        tag = child.tag.removeprefix(SVG)
        # Inkscape's own elements, and anything never drawn, which a fill
        # forced onto it would make drawn.
        if (not child.tag.startswith(SVG) or tag in ("defs", "metadata", "title", "desc")
                or style(child, "display") == "none" or style(child, "fill") == "none"):
            parent.remove(child)
            continue
        if tag not in ("g", "path"):
            sys.exit(f"icons.sh: Inkscape left a <{tag}> in the simple logo as it was")
        rule = style(child, "fill-rule")
        child.attrib = {k: v for k, v in child.attrib.items() if k in ("d", "transform")}
        if rule == "evenodd":
            child.set("fill-rule", rule)
        if tag == "path":
            child.set("fill", "#241f31")
        strip(child)

root.attrib = {k: v for k, v in root.attrib.items() if k in ("width", "height", "viewBox")}
strip(root)
ET.indent(root)
ET.ElementTree(root).write(dest, encoding="unicode")
open(dest, "a").write("\n")
EOF

echo "icons.sh: rendered $out from $src and $simple"
