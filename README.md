<img src="public/death-by-mpv.svg" alt="Death by MPV" width="128" height="128">

# Death by MPV

A video player for Windows and Linux, made for watching films and shows you
have on disk. It plays anything mpv can play, and there's nothing to set up.

What makes it different is the way it looks. The controls sit on panels of
liquid glass that bend the picture behind them. When a film doesn't fill the
window, the black bars pick up the colours from the edges of the picture.

## What it does

- **Opens files and folders.** Open one episode and the rest of the folder
  becomes the playlist. Open a folder and it finds everything inside it.
- **Remembers where you were.** Every file keeps its position, the audio and
  subtitle tracks you picked, and any subtitle timing you changed.
- **Skips intros.** It finds a show's opening titles and closing credits, from
  the chapters if the file has them or by comparing two episodes. You get a
  button to skip the intro, and one to go to the next episode when the credits
  start.
- **Shows previews on the timeline.** Hover over the timeline to see a
  thumbnail of that moment.
- **Fixes out-of-sync subtitles.** *Sync to speech* listens to the audio and
  lines the subtitles up with it. You can also move them earlier or later by
  hand.
- **Finds subtitles online.** Search OpenSubtitles for what you're watching
  and choose one from the list. The first few downloads each day need no
  account; after that, the player asks you to sign in with your own
  OpenSubtitles account. It keeps the sign-in only until you quit, and never
  writes down your password. Searching, downloading and signing in are the
  only times the player goes online, and only when you ask. Subtitles
  provided by [OpenSubtitles.com](https://www.opensubtitles.com).
- **Works from the keyboard.** Everything has a shortcut, and the menu lists
  them all. Media keys and your desktop's media controls work too.
- **Speaks your language.** English, Swedish, German, Spanish, French,
  Italian, Dutch and Portuguese. It follows your system language, or you can
  pick one in Settings.
- **Lets you tune the look.** The glass and the border have their own sliders
  in Settings, and you can turn the border off.

## How it works

Most players that put controls over a video draw them in front of a separate
video window, and can't see the picture underneath. This one has mpv draw each
frame into the player's own OpenGL texture instead. Because the interface has
the frame, the glass can refract the real picture and the border can grow out
of the picture's real edges.

It's written in Rust, with [Slint](https://slint.dev) for the interface and
libmpv for playback.

## Installing

Download it from the [Releases](https://github.com/MisterD0ctor/DBM/releases)
page, or [build it yourself](#building-from-source).

**Windows:** there's an installer, plus a portable zip if you'd rather not
install anything. The installer only installs for your user, so it doesn't ask
for administrator rights. It adds the player to *Open with* for video files
but won't make itself the default; you can do that in Settings > Default apps.
It isn't code-signed, so SmartScreen will warn you the first time.

**Linux:** download `death-by-mpv.flatpak` and open it, or run
`flatpak install --user death-by-mpv.flatpak`. Like on Windows, it shows up in
*Open with* without becoming the default. The player isn't on Flathub, but the
runtime it needs comes from there, so the Flathub remote has to be set up.
Most desktops that come with Flatpak already have it.

## Building from source

All you need is stable Rust ([rustup](https://rustup.rs/)).

```sh
cargo build --release -p dbm-player
cargo run -p dbm-player -- path/to/video.mkv
```

**On Windows**, libmpv and ffmpeg are included in the repository under
[`crates/player/vendor/`](crates/player/vendor/), stored with Git LFS. Run
`git lfs install` before you clone, or you'll get placeholder files instead of
the real ones. That folder's README explains how the player finds them and
what shipping them means for the licence.

**On Linux**, building needs the development packages for fontconfig,
freetype, Wayland, xkbcommon and OpenGL. Running needs a `libmpv.so.2` whose
FFmpeg can decode HEVC. Fedora's standard FFmpeg can't, and those videos play
black with sound. You'll also want `ffmpeg` on your `PATH`. Without it there
are no timeline previews, subtitle sync won't work, and intros and credits are
only found in files whose chapters mark them. The [Flatpak build](#packaging)
takes care of all of this.

**OpenSubtitles** needs an API key. Register one for free at
[opensubtitles.com/consumers](https://www.opensubtitles.com/consumers), then
put it in `crates/player/keys/opensubtitles` (Git ignores that file), or set
`DBM_OPENSUBTITLES_KEY` when you build. See
[`crates/player/keys/`](crates/player/keys/README.md). Without a key, the
player is the same except that there's no *Find subtitles* option.

### Checking a change

CI runs these on Linux and Windows for every push, and a single warning fails
the build:

```sh
cargo fmt --check
cargo clippy -p dbm-player --all-targets -- -D warnings
cargo test -p dbm-player
```

The tests cover everything that doesn't need a window: reading file and track
names, playlist order, the caches, and formatting. For the rest (the
interface, playback, and the platform integrations) the player has a built-in
test harness that you control with environment variables. Some of them run
the player through a scenario and print what happened; others hold one screen
still and save a screenshot. They're all listed at the top of
[`crates/player/src/harness/mod.rs`](crates/player/src/harness/mod.rs).

Run them through `scripts/headless.sh`. It starts a headless compositor (all
it needs is GNOME's `mutter`) so the player's window doesn't pop up on your
desktop:

```sh
scripts/headless.sh env DBM_SHOWCASE=playlist DBM_CAPTURE=playlist.bmp \
    ./target/release/dbm-player path/to/video.mkv
```

If your change affects something the player caches (file lengths, title
tags, where intros and credits are, timeline previews), bump that cache's
version where it's declared, with a line saying what changed. Everything
saved under the old version is then thrown away and worked out again. How
it works is explained at the top of
[`library/cache.rs`](crates/player/src/library/cache.rs).

## Translating

The interface is written in English. Each translation is a single gettext
file, `crates/player/lang/<code>/LC_MESSAGES/dbm-player.po`, that gets built
into the player. There are seven so far: `de`, `es`, `fr`, `it`, `nl`, `pt`
and `sv`. Only the Swedish one has been checked by a native speaker. The
others haven't been reviewed yet, and each file says so at the top, so
corrections are very welcome.

```sh
scripts/translations.py add pl      # start a new language with every entry empty
scripts/translations.py check       # see what each language is missing
scripts/translations.py update      # add entries for new text in the source
```

Fill in each `msgstr` and rebuild. The language then appears under its own
name in *Settings → Language*. Keep every `{}`, because each one is replaced
with a value while the player runs. Leave `msgctxt` alone too: it tells you
where the text is used.

In the code, text is marked for translation with `@tr("…")` in `.slint` files
and `tr("…")` in Rust. Unmarked text stays as written, which is what you want
for key names, file names and the player's name.

## Platforms

Windows and Linux are both supported. macOS isn't planned.

Media keys and the desktop's media controls work on both: through SMTC on
Windows and MPRIS on Linux (see `platform/media_keys`, and run with
`DBM_MPRIS_TEST=1` to watch it work). A few things are still Windows-only and
don't exist on Linux yet:

- dropping files onto the window
- keeping the screen awake while a video plays
- the interface continuing to redraw while you drag the window's edge

## Packaging

### Windows installer

```powershell
.\packaging\build-installer.ps1
```

This builds the release binary, puts libmpv and ffmpeg next to it, and wraps
everything into an NSIS installer in `target/installer/`. You need NSIS
installed (`winget install NSIS.NSIS`). The script will also find the copy
Tauri leaves in `%LOCALAPPDATA%` if that's what you have. The whole installer
is in `packaging/death-by-mpv.nsi`.

CI does the same on every push to `main`. Pushing a `v*` tag also creates a
draft release with the installer and the portable zip attached. The tag has
to match the version in `crates/player/Cargo.toml`.

### Flatpak

```sh
packaging/flatpak/build.sh --bundle
```

This builds the Flatpak from your working tree, installs it for your user,
and writes `target/flatpak/death-by-mpv.flatpak`. CI only builds for Windows,
so this file has to be attached to a release by hand. You need
`flatpak-builder` (or the `org.flatpak.Builder` Flatpak) and the Flathub
remote. The SDK and the Rust extension are downloaded the first time. The
manifest, `packaging/flatpak/io.github.MisterD0ctor.DBM.yml`, builds libass,
libplacebo and libmpv, and uses the runtime's FFmpeg.

The build runs offline, so every Rust crate is listed in
`packaging/flatpak/cargo-sources.json`. Regenerate that file whenever a crate
in `Cargo.lock` is added, removed or updated, using
`flatpak-cargo-generator.py` from
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools):

```sh
python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
```

The player isn't on Flathub. Getting it there would take three changes: the
manifest's `type: dir` source would have to point at a git tag and commit
instead, the metainfo would need screenshots, and Flathub would have to grant
an exception for `--filesystem=host:ro`. Its linter rejects that permission
by default, but the player needs it, because opening one episode means
reading the rest of its folder.

## Licence

**GPL-3.0-or-later.** This wasn't a free choice. The Windows build ships
libmpv and ffmpeg built with an FFmpeg configured as `--enable-gpl
--enable-version3`, which makes them GPLv3-or-later. The player links libmpv,
so a release counts as a single work together with it. Slint is available
under `GPL-3.0-only`, a royalty-free licence or a paid one, and this project
uses the GPL to match.

[LICENSE](LICENSE) has the full text. [THIRD-PARTY.md](THIRD-PARTY.md) lists
what ships with the player, under which terms, and where to get the source.
The licence requires that from anyone who passes the binaries on, including
you if you fork this.

## Layout

| path | what's in it |
|---|---|
| `crates/player/src/` | the application, in folders by concern; the table at the top of `main.rs` says what each one does |
| `crates/player/ui/` | the interface, in Slint |
| `crates/player/shaders/` | the border, blur and glass shaders, and the background shown when nothing is playing |
| `crates/player/lang/` | the translations |
| `crates/player/vendor/` | libmpv and ffmpeg for Windows |
| `packaging/` | the Windows installer, and the Flatpak in `flatpak/` |
| `public/icons/` | the icons, drawn on a 24×24 grid |
| `scripts/` | regenerating the app icons, running the harness headless, and managing translations |
