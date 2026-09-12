# Third-party components

Death by MPV is licensed under the GNU General Public License, version 3 or
later — see [LICENSE](LICENSE). This file records what else is in the box,
under what terms, and where its source is, which is what the GPL requires of
anyone passing the binaries on.

It is a record of what the shipped artifacts actually are, established by
reading them, not a legal opinion.

## Shipped binaries

These two are redistributed verbatim in the installer and the portable zip.
Both carry GPLv3 obligations, and those obligations pass to anyone who
redistributes this player.

### libmpv (`libmpv-2.dll`)

| | |
|---|---|
| Version | mpv `v0.41.0-dev-g79dc1a2eb` |
| Built by | [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake) — a stock Windows build, not a fork of ours |
| Terms | mpv is GPLv2-or-later. **This build is GPLv3-or-later**: it statically links FFmpeg configured `--enable-gpl --enable-version3` and OpenSSL 3 (Apache-2.0), neither of which can be conveyed under GPLv2. |
| Source | mpv at commit [`79dc1a2eb`](https://github.com/mpv-player/mpv/commit/79dc1a2eb); the build recipe and the exact versions of everything it links are in the winbuild repository above. |

The player loads this library and calls its API, so the player and libmpv are
one work when distributed together. That is the fact that decides this
project's licence.

### FFmpeg (`ffmpeg-x86_64-pc-windows-msvc.exe`)

| | |
|---|---|
| Version | `8.1-essentials_build` from [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) |
| Terms | **GPLv3-or-later** — configured `--enable-gpl --enable-version3`, which pulls in components (libx264, libx265, libvidstab, librubberband, OpenCORE AMR) that require those terms. |
| Source | [ffmpeg.org](https://ffmpeg.org/download.html) for release 8.1, plus gyan's published build scripts for the configuration above. |

The player spawns this as a separate process to build seek-preview
thumbnails; it is aggregated with the player rather than linked into it.
Shipping it still carries the licence and source obligations above.

## Getting the corresponding source

The links above are the directions the GPL asks for. If any of them has gone
stale by the time you read this, open an issue and the matching source will be
provided — it is kept for this purpose.

## Libraries the player links

### Slint

[Slint](https://slint.dev) is offered under `GPL-3.0-only`, a royalty-free
desktop licence, or a paid licence. **This project takes the GPLv3 option.**

Note the asymmetry: Slint's grant is GPL-3.0-*only*, while this project's own
code, mpv and FFmpeg are all "or later". The assembled binary is therefore
GPLv3, without the option of a later version, for as long as Slint's terms say
so. Our own "or later" grant costs nothing and is left as it is.

### Everything else from crates.io

Six hundred-odd crates in the lockfile, effectively all of them permissive — MIT,
Apache-2.0, Zlib, ISC, Unicode-3.0, and combinations of those. All are
GPLv3-compatible. To regenerate the full list with versions:

```sh
cargo install cargo-about && cargo about generate about.hbs
```

## Icons

The 54 SVGs in `public/icons/` are the author's own work, drawn at 24×24 and
covered by this project's licence like everything else in the repository. Some
never had an outside source at all — no icon pack contains a control for a
refraction bevel, so `angle`, `curve`, `edge-blur`, `glass`, `panscan-*` and
`ambience-*` could only ever have been drawn here.

Five were once adapted from Uicons by Flaticon. Four were redrawn from scratch
and the fifth, which nothing referenced, was dropped; no Flaticon-derived file
remains in the working tree, and the attribution that was owed for them no
longer is.

Commits and releases made **before** that redraw still contain the adapted
versions, and the credit stands for those artifacts where they are still
published: *icons based on [Uicons by Flaticon](https://www.flaticon.com/uicons)*.
It does not apply to the current set.

`public/death-by-mpv.svg` is the project's own mark and is covered by this
project's licence.
