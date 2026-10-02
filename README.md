<img src="public/death-by-mpv.svg" alt="Death by MPV" width="128" height="128">

# Death by MPV

A video player built on mpv, whose interface is the reason it exists.

mpv's output is rendered **into the application's own OpenGL context** as a
texture rather than into a window of its own. That one decision is the product:
the liquid glass genuinely refracts the frame behind it, and the ambient border
is the picture's own edges spread into the letterbox — both ordinary shader
passes over a texture the interface can actually sample.

Native Rust throughout: [Slint](https://slint.dev) for the interface, libmpv's
render API for the video, OpenGL for everything drawn between them.

## Installing

No release has been cut yet. Releases stays empty until the first `v*` tag is
pushed, so for now the way in is [building from source](#building-from-source);
what follows is what a release carries.

Grab the installer from [Releases](https://github.com/MisterD0ctor/DBM/releases)
and run it. It installs for the current user only — into
`%LOCALAPPDATA%\Programs\Death by MPV`, with no administrator prompt — and
offers itself in the *Open with* menu for video files without taking any file
type over; that choice stays yours in Settings > Default apps. There is also a
portable zip if you would rather unpack it somewhere and run it.

It is not code-signed, so Windows SmartScreen will warn the first time.

On Linux it is a Flatpak, distributed only as a file on Releases — the player
itself is not on Flathub and cannot be found in a software store. Download
`death-by-mpv.flatpak` and open it, or run
`flatpak install --user death-by-mpv.flatpak`. It installs for the current
user and appears in *Open with* for video files without becoming the default.

The file holds only the player. The shared runtime it runs on, which supplies
hardware decoding and the NVIDIA driver, is downloaded from the Flathub
remote on install, so that remote has to be set up; most desktops that ship
Flatpak already have it.

## Building from source

You need the **Rust stable toolchain** ([rustup](https://rustup.rs/)) and
nothing else. There is no Node, no bundler and no WebView.

```sh
cargo build --release -p dbm-player
cargo run -p dbm-player -- path/to/video.mkv
```

The two binaries the Windows build ships with — `libmpv-2.dll` and ffmpeg, the
latter only for seek-preview thumbnails — are checked in under
[`crates/player/vendor/`](crates/player/vendor/) and tracked in Git LFS, so
`git lfs install` before cloning or they arrive as pointer files. Nothing is
asked of the machine the player runs on; see that directory's README for how
they are found and for the licensing that shipping them implies.

On Linux, `cargo build` also needs the development packages for fontconfig,
freetype, Wayland, xkbcommon and OpenGL. Running the result needs a
`libmpv.so.2` whose FFmpeg can decode HEVC — Fedora's stock FFmpeg cannot, and
the video plays black with sound — and seek previews want an `ffmpeg` on the
`PATH`, without which they are simply absent. The Flatpak build below takes
care of all of it.

## Platforms

Windows and Linux both ship. The media keys and the desktop's own media
controls reach the player on both, through different systems: Windows' SMTC
and, on Linux, MPRIS over D-Bus — see `smtc` and `mpris`, and
`DBM_MPRIS_TEST=1` to watch it work. The pieces that are still Windows-only
(the modal resize-loop hook, the drop target, keeping the display awake)
compile away on Linux and have no counterpart yet. macOS is out of scope.

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

CI does the same on every push to `main`. A `v*` tag, which has to match the
version in `crates/player/Cargo.toml`, also opens a draft release with the
installer and the portable zip attached.

```sh
packaging/flatpak/build.sh --bundle
```

Builds the Flatpak from the working tree, installs it for the current user,
and writes `target/flatpak/death-by-mpv.flatpak` for Releases — attached by
hand, since CI builds the Windows side only. Needs
`flatpak-builder` (or the `org.flatpak.Builder` Flatpak) and the Flathub
remote; the SDK and Rust extension are fetched on first run. The manifest,
`packaging/flatpak/io.github.MisterD0ctor.DBM.yml`, builds libass, libplacebo
and libmpv, and takes FFmpeg from the runtime.

The build is offline, so every crate is listed in
`packaging/flatpak/cargo-sources.json`. Regenerate it whenever a crate in
`Cargo.lock` is added, removed or changes version, with
`flatpak-cargo-generator.py` from
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools):

```sh
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
```

The app is not on Flathub. Submitting it would mean replacing the manifest's
`type: dir` source with a git tag and commit, adding screenshots to the
metainfo, and asking for an exception for `--filesystem=host:ro`, which
Flathub's linter rejects by default and the player needs: opening one episode
reads the rest of its folder.

## Licence

**GPL-3.0-or-later.** Not a preference: the libmpv and ffmpeg the Windows build
ships are GPLv3-or-later as built, because the FFmpeg inside them is configured
`--enable-gpl --enable-version3`, and the player links libmpv, so a release is
one work with it. Slint is offered under `GPL-3.0-only`, a royalty-free licence
or a paid one; this project takes the GPL, to match.

[LICENSE](LICENSE) is the full text. [THIRD-PARTY.md](THIRD-PARTY.md) records
what ships, under what terms, and where to get its source — which is what the
licence asks of anyone passing the binaries on, including you if you fork this.


## Layout

| path | what it is |
|---|---|
| `crates/player/src/` | the application; the module table at the top of `main.rs` says what each file owns |
| `crates/player/ui/` | the interface, in Slint |
| `crates/player/shaders/` | the border, blur and glass passes, and the backdrop drawn while nothing is loaded |
| `crates/player/vendor/` | libmpv and ffmpeg, shipped |
| `packaging/` | the Windows installer, and the Flatpak under `flatpak/` |
| `public/icons/` | the icon set, drawn for 24×24 |

Earlier versions of this player were a Tauri 2 + Leptos application, with mpv
in a sibling window the WebView could never see. That tree was removed once the
native one had everything it did; it is in the history if it is ever wanted.
