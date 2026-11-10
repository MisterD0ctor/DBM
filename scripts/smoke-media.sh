#!/bin/sh
# Two short episodes for the harness to play: ffmpeg's test picture, English
# and Swedish audio, and English subtitles, named the way a show's files are,
# so the player reads them as a season.
#
#   scripts/smoke-media.sh <folder>
#
# What `smoke.sh` checks is written against these: two audio tracks to pick
# between, a subtitle track to turn on, a minute to seek about in. Each
# episode sounds its own tones, so the two share nothing for the player to
# hear as titles: shared, the whole first half was a title sequence, which
# raced the chapters the exercises give, and lost them their skips.
set -eu

if [ $# -ne 1 ]; then
    echo "usage: $0 <folder>" >&2
    exit 2
fi
dir=$1
mkdir -p "$dir"

subs="$dir/smoke.en.srt"
i=0
while [ "$i" -lt 14 ]; do
    start=$((i * 4 + 1))
    printf '%d\n00:00:%02d,000 --> 00:00:%02d,500\nLine %d of the smoke test.\n\n' \
        $((i + 1)) "$start" $((start + 2)) $((i + 1))
    i=$((i + 1))
done >"$subs"

for n in 1 2; do
    ffmpeg -v error -y \
        -f lavfi -i "testsrc2=size=1280x720:rate=24:duration=60" \
        -f lavfi -i "sine=frequency=$((330 + 110 * n)):duration=60" \
        -f lavfi -i "sine=frequency=$((550 + 110 * n)):duration=60" \
        -i "$subs" \
        -map 0 -map 1 -map 2 -map 3 \
        -c:v libx264 -pix_fmt yuv420p -c:a aac -ac 1 -c:s srt \
        -metadata:s:a:0 language=eng -metadata:s:a:1 language=swe \
        -metadata:s:s:0 language=eng \
        "$dir/Smoke Test S01E0$n Episode $n.mkv"
done
rm -f "$subs"
