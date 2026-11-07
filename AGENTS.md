# Working on DBM

DBM is a Rust video player: a Slint interface over libmpv, with its own OpenGL
passes in between, split across the crates in `crates/`.

A lot of the recent work in this tree has been replacing checks that a value
is sound with types that cannot be unsound — `git log --oneline v1.1.0..HEAD`
is the series. New code is expected to keep that up, because the checks are
the part reviewers forget and the types are the part they cannot.

## The one rule

If two values can disagree, or a value can outlive the thing it describes,
the compiler should be the one to notice. A guard clause is the fallback, not
the first move.

## What that looks like here

### A fact about a file carries the file

`PerFile` was a path beside the flags for what had been done for it, so a
flag could exist with no file playing, and every reader checked the path
first. It is `Option<PerFile>` now, and its `path` is not optional.

Answers that arrive from a worker are `dbm_library::threads::ForFile<T>`: the
value and the path it is about, with `if_playing(playing)` to take it only
for the file still playing. A `(MediaPath, T)` tuple, or a struct with
`path: Option<MediaPath>` beside `T`, is that idea half-done.

The same applies to a start that belongs to a list mpv has been handed:
`Opening` keeps `Asked { m3u, start }` rather than a `starting` flag beside
the queue, so there is no start left standing when a list never arrives.

### A thing that is only sometimes there is an enum, not two options

`Driver` held `gpu: Option<Gpu>`, `failure: Option<Failure>` and the file to
open as three independent fields: a context and a failure could both be set,
and a second setup would open the file again. It is one `Video`:
`Waiting { file }`, `Live(Box<Gpu>)`, `Failed(Failure)`.

The same shape recurs in `pipeline::Backdrop` (`Off`, `Alone`, `OverFilm` —
two booleans allowed a fourth state nothing meant) and `pipeline::Sizes`
(`Allocated { resized }` or `Refused`; a refused allocation has no useful
"resized" answer).

Ask of a new struct: *does it have a value for a state the program will never
be in, and never handle?*

### A pair of booleans at a call site is an enum

`show_backdrop(true, false)` says nothing about which half is which. If a
parameter would need a comment at the call site to be read, give it a name —
an enum, or a constructor that names the cases.

Exception: a boolean that mirrors one property of something external — mpv's
`pause`, a settings toggle, whether the pointer is hidden — is a two-state
fact, and stays one.

### Sizes have a type, and units are part of it

`dbm_gpu::size::Size` is a width and height in physical pixels. Positions are
`Rect<Normalised>`, `Rect<Logical>` or `Rect<Pixels>`; whole-pixel rectangles
are `Region`. Do not pass `(u32, u32)` for a size, a pair of floats for a
rectangle, or a fraction where a percentage is expected.

### A number means one thing

Keep seconds `f64`, `fraction` in 0..1, `percent_pos` in 0..100, and a `Rate`
(which cannot be zero) apart by name. `MediaPath`, `EntryId`, `TrackChoice`
and `NonZeroU32` exist so that a path is not a string and an id is not a
path: prefer a newtype to a comment that says what a number is.

### Partial knowledge is a type

`OsdParts` holds the six pieces of mpv's layout and produces an `OsdDims`
only when every one has arrived; until then there is no layout, which is not
the same as one with zeroes. Where a caller could otherwise read a
half-filled version, make the half-filled one unrepresentable.

### Failures are types

`platform::Error`, `gpu::pipeline::Error` and `mpv::Error`, not
`Result<_, String>`. If two failures are handled differently, they are two
variants; if they are not, they can share one.

### A shader trick used twice is one chunk

The ambient border dithers its gradient, and the glass rim needed the same
grain: `crates/gpu/shaders/dither.frag` is spliced into both by
`program_with!`, so the two cannot drift and only one of them get fixed.
Anything else two passes must agree about belongs somewhere shared too.

## Before you add a field or a state

Run this over the change:

1. **What is this about?** A file, a panel, a load, a window? Then that thing
   travels with it.
2. **Can it be set while that thing is absent?** If yes, it belongs inside an
   `Option` or enum of the thing, not beside it.
3. **How many combinations of this struct's flags are valid?** If fewer than
   the arithmetic allows, the struct is wrong.
4. **Would a caller have to compare a path or an id to know it is current?**
   That comparison belongs in a type (`ForFile`) or in the constructor.
5. **Is the number a size, a count, a position, a duration or a rate?** Use
   the type that says so.
6. **Does the behaviour have a test?** A type that makes a bug unrepresentable
   needs none; anything whose behaviour changed does.

## Where these live

| Pattern | Home |
|---|---|
| `ForFile<T>` | `crates/library/src/threads.rs` |
| `Size`, `Rect<U>`, `Region` | `crates/gpu/src/size.rs`, `crates/gpu/src/rect.rs` |
| Player lifecycle (`Video`) | `crates/player/src/driver/mod.rs` |
| Per-file and per-open state | `crates/player/src/player/` |
| Typed mpv properties | `crates/mpv/src/prop.rs` |
| Platform failures | `crates/player/src/platform/mod.rs` |

## Keeping it honest

`cargo clippy --workspace --all-targets --all-features -- -D warnings`, the
test suite and `scripts/smoke.sh` are the floor. A driver failure the window
cannot provoke gets a test-only hook behind the `harness` feature, as
`dbm_gpu::pipeline::refuse_next_allocation` does, rather than an untested
branch.
