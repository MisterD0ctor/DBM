# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

<!-- Not one of the schema's four values. This is a native Windows/Linux
     application — Rust, Slint, OpenGL, libmpv's render API — and recording it
     as `web` would point every later command at browsers, breakpoints and CSS
     that do not exist here. -->

## Users

One person: the developer, who is also the player's primary user. Releases are
published publicly from CI on version tags and anyone may use them, but the
audience is not the design constraint — decisions answer to whether he would
use it, not to a general public.

The situation is a personal library on local disk, watched on a desktop machine
with a keyboard and mouse: films, and series in folders of episodes named the
way scene and fansub releases name them.

## Product Purpose

A desktop video player built on libmpv whose interface is the reason it exists.
mpv already plays everything; what this adds is a shell worth looking at and an
absence of configuration. Success is that it is the player he actually opens.

## Positioning

mpv's video is rendered **into the application's own GL context** as a texture,
rather than into a sibling window the interface can never see. That single
decision is the product:

- the liquid glass genuinely refracts the frame behind it, rather than
  approximating it;
- the ambient border is the frame's own edges extended into the letterbox,
  computed from the live texture;
- it removed the forked mpv the previous build needed to paint that border
  through a `//!HOOK BORDER` shader stage.

A player that composites its interface over a separate video surface cannot
truthfully claim any of this.

## Operating Context

- Opening one file adopts its folder siblings as the playlist; opening a folder
  scans it recursively.
- Position, track selection and subtitle delay are remembered per file through
  mpv's own watch-later mechanism.
- Interface settings persist to `%APPDATA%/Death by MPV/settings.conf` as plain
  text, editable by hand while tuning.
- Seek-preview thumbnails are cached per file under `previews/`, keyed on path
  and modification time.
- Runs on a high-refresh display; the interface is expected to move at the
  display's rate while video runs at the film's.

## Capabilities and Constraints

**Migration complete.** The original build was Tauri 2 + Leptos (`src/`,
`src-tauri/`), with mpv in a sibling window the WebView could never sample. It
was replaced by a native Rust + Slint application in `crates/player/` on branch
`slint-rewrite`, and removed once that one did everything it did. The history
has it if it is ever wanted.

**Installs per user.** `packaging/death-by-mpv.nsi` is a hand-written NSIS
installer: `%LOCALAPPDATA%\Programs`, HKCU only, no administrator prompt. It
registers the player as *a* handler for the formats it can open and lists it
in Settings > Default apps, but takes no file type by itself — Windows 10 and
later forbid that, and it would be rude in any case. CI publishes it alongside
a portable zip for anyone who would rather not install anything. Unsigned, so
SmartScreen warns on first run until the binary earns reputation or a
certificate is bought.

**Ships its own dependencies.** libmpv and ffmpeg are bundled binaries, not
prerequisites — nothing is asked of the user's machine. They live in
`crates/player/vendor/`, tracked in Git LFS, and are looked for beside the
executable first and in that directory second, so a checkout and a packaged
build both find them. ffmpeg exists for seek-preview thumbnails only.

**Platforms.** Windows ships now. Linux is planned and the code is kept honest
for it — Windows-only work such as the modal resize-loop hook stays isolated
behind `cfg` — but it is not verified per change. macOS is explicitly out of
scope.

**Standing rules**, stated by the user and treated as binding:

- Nothing blocks the frame path. No synchronous mpv property read, no
  synchronous command, no file I/O. Anything that can block goes to a worker or
  its own thread.
- Settings always save.
- Systems stay compartmentalised, so each can be reasoned about on its own.
- Icons are drawn for 24×24 and displayed at that size.

**Undecided or not yet built:** code signing, an update mechanism, MPRIS and
XDND as the Linux answers to the media keys and the drop target, and language
endonyms from a real locale database (currently a hand-written 42-language
table with ISO 639-2 folding).

## Brand Commitments

- Name: **Death by MPV** (DBM). Bundle identifier `com.kaspe.deathbympv`.
- Logo: `public/death-by-mpv.svg`. Application icons: `crates/player/icons/`.
- An icon set of 54 SVGs in `public/icons/`, drawn by the author at 24×24 and
  wholly the project's own. Future work extends this set rather than
  substituting a library — the look depends on them being one family.

## Evidence on Hand

- Public repository `github.com/MisterD0ctor/DBM`; GitHub Actions builds on
  push, pull request and `v*` tags, currently Windows-only.
- Version 0.2.0.
- The logo and icon set named above.
- Licensed GPL-3.0-or-later, forced by Slint's open-source terms and by the
  GPLv3 builds of libmpv and ffmpeg that ship with it; `THIRD-PARTY.md`
  records the versions, build origins and source locations.
- There are **no** users, testimonials, benchmarks, pricing or download
  figures. None exist and none may be invented.

## Product Principles

1. **The interface is the point.** When a change would make the player faster
   or simpler but less itself, the look wins. The render architecture was
   rebuilt to make that possible; it is not decoration to be trimmed.
2. **Nothing blocks the frame.** Smoothness is a feature of this product, not
   an implementation detail — the interface moves at the display's rate whatever
   the film is doing.
3. **Correct without being asked.** What a file needs — what it is called, what
   tracks it has, where you stopped — is worked out, not configured.
4. **Guess well, and never lie about it.** Filenames, track names and container
   metadata are inconsistent; the player interprets them generously, and where
   it cannot, it says so plainly rather than showing something invented.
5. **One system per problem.** Each concern is reasoned about alone, which is
   what keeps a real-time render loop and a desktop interface in one binary
   tractable.
