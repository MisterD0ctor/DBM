<img src="public/death-by-mpv.svg" alt="Death by MPV" width="128" height="128">

# Death By MPV

A video player built on mpv, whose interface is the reason it exists.

mpv's output is rendered **into the application's own OpenGL context** as a
texture rather than into a window of its own. That one decision is the product:
the liquid glass genuinely refracts the frame behind it, and the ambient border
is the picture's own edges spread into the letterbox — both ordinary shader
passes over a texture the interface can actually sample.

Native Rust throughout: [Slint](https://slint.dev) for the interface, libmpv's
render API for the video, OpenGL for everything drawn between them.

## Installing

Grab the installer from [Releases](https://github.com/MisterD0ctor/DBM/releases)
and run it. It installs for the current user only — into
`%LOCALAPPDATA%\Programs\Death by MPV`, with no administrator prompt — and
offers itself in the *Open with* menu for video files without taking any file
type over; that choice stays yours in Settings > Default apps. There is also a
portable zip if you would rather unpack it somewhere and run it.

It is not code-signed, so Windows SmartScreen will warn the first time.

## Building from source

You need the **Rust stable toolchain** ([rustup](https://rustup.rs/)) and
nothing else. There is no Node, no bundler and no WebView.

```sh
cargo build --release -p dbm-player
cargo run -p dbm-player -- path/to/video.mkv
```

The two binaries the player ships with — `libmpv-2.dll` and ffmpeg, the latter
only for seek-preview thumbnails — are checked in under
[`crates/player/vendor/`](crates/player/vendor/) and tracked in Git LFS, so
`git lfs install` before cloning or they arrive as pointer files. Nothing is
asked of the machine the player runs on; see that directory's README for how
they are found and for the licensing that shipping them implies.

## Platforms

Windows is what ships. Linux is planned and the code is kept honest for it —
the Windows-only pieces (the modal resize-loop hook, the media keys, the
drop target) are isolated behind `cfg` — but it is not verified per change.
macOS is out of scope.

## Packaging

```powershell
.\packaging\build-installer.ps1
```

Builds the release binary, stages it with libmpv and ffmpeg beside it — the
layout the player's own lookup expects — and wraps the lot with NSIS into
`target/installer/`. Needs NSIS on the machine (`winget install NSIS.NSIS`);
the script also finds the copy Tauri leaves in `%LOCALAPPDATA%` if that is
what you have. `packaging/death-by-mpv.nsi` is the whole installer and is
meant to be read.

## Licence

**GPL-3.0-or-later.** Not a preference — the two things this player is built
on require it. Slint is offered under `GPL-3.0-only` or a paid licence, and the
bundled libmpv and ffmpeg are both GPLv3-or-later as built, because the FFmpeg
inside them is configured `--enable-gpl --enable-version3`. The player links
libmpv, so a release is one work with it.

[LICENSE](LICENSE) is the full text. [THIRD-PARTY.md](THIRD-PARTY.md) records
what ships, under what terms, and where to get its source — which is what the
licence asks of anyone passing the binaries on, including you if you fork this.


## Layout

| path | what it is |
|---|---|
| `crates/player/src/` | the application; the module table at the top of `main.rs` says what each file owns |
| `crates/player/ui/` | the interface, in Slint |
| `crates/player/shaders/` | the border, blur and glass passes |
| `crates/player/vendor/` | libmpv and ffmpeg, shipped |
| `packaging/` | the Windows installer |
| `public/icons/` | the icon set, drawn for 24×24 |

Earlier versions of this player were a Tauri 2 + Leptos application, with mpv
in a sibling window the WebView could never see. That tree was removed once the
native one had everything it did; it is in the history if it is ever wanted.
