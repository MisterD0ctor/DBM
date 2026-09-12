# Vendored binaries

The two dependencies the player ships with, so nothing is asked of the user's
machine. Both are tracked in Git LFS — see `.gitattributes` at the repo root.

| File | Why |
|---|---|
| `libmpv-2.dll` | Playback. Loaded at runtime through `libloading`, not linked, so the exact build can be swapped without recompiling. |
| `ffmpeg-x86_64-pc-windows-msvc.exe` | Seek-preview thumbnails only. Nothing else in the player shells out. |

## How they are found

`paths::vendored` checks, in order:

1. **Beside the executable** — where a packaged build puts them.
2. **This directory** — where a `cargo run` from a checkout finds them, since
   cargo never copies anything next to the binary.

Each lookup has an environment override that wins outright — `DBM_LIBMPV` and
`DBM_FFMPEG` — so a stock libmpv or a different ffmpeg can be dropped in
without touching the tree.

The shipped copy is deliberately preferred over `PATH`: a machine with its own
ffmpeg would otherwise quietly use that one, which is a different build with
possibly a different decoder set, for no reason anybody chose.

## The filename suffix

`ffmpeg-x86_64-pc-windows-msvc.exe` keeps its target-triple suffix because
Tauri's `externalBin` mechanism requires it, and the Tauri build still
references this directory while it remains as a porting reference. The lookup
accepts both that name and a plain `ffmpeg.exe`, which is what a packaged build
will place beside the executable.

## Licensing

Both are **GPLv3-or-later** as built — libmpv from shinchiro's Windows build
with FFmpeg at `--enable-gpl --enable-version3`, and ffmpeg from gyan.dev with
the same flags. That is why this project is GPLv3-or-later: the player links
libmpv, so the two are one work when shipped together.

Versions, build origins and where to get the corresponding source are recorded
in [`THIRD-PARTY.md`](../../../THIRD-PARTY.md) at the repository root. Replacing
either binary means checking those facts again — `ffmpeg -version` prints its
configuration, and libmpv carries FFmpeg's inside it.
