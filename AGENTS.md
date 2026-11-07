# Working on DBM

DBM is a Rust video player: a Slint interface over libmpv, with its own OpenGL
passes in between, split across the crates in `crates/`.

A lot of the recent work in this tree has been replacing checks that a value
is sound with types that cannot be unsound — `git log --oneline v1.1.0..HEAD`
is the series. New code is expected to keep that up, because the checks are
the part reviewers forget and the types are the part they cannot.

The second half of this file is the other kind of rule: the classes of bug
that have been fixed here more than once, most of them in code an agent
wrote. Each one names its commits; `git show` them before writing something
of the same kind.

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

`None` answers one question. `poll_event` answered `None` both for an empty
queue and for an event it skips, so every drain stopped at the first skipped
one (`4db1c4f`); `Reader::get` answered it for a missing value and for a read
mpv refused (`3fa5028`); a length of zero meant both "not known yet" and
"none" (`bb7a826`, `c36f566`). If an absence can have two causes the code
treats differently, it is `Result<Option<T>, _>` or an enum.

### Failures are types

`platform::Error`, `gpu::pipeline::Error` and `mpv::Error`, not
`Result<_, String>`. If two failures are handled differently, they are two
variants; if they are not, they can share one.

### A thread rule is a handle

Which thread may make a call is said by the handle it holds, not by a comment
above the call. mpv is handed out as `Mpv`, whose calls return at once;
`Reader`, which may block and so stays off the frame path; and `Events`, the
one reader of the queue, through `&mut self`. When the queue was a safe method
on a shared handle, with a comment saying one thread kept to it, it was a
data race (`52ad5a4`, `3300d34`).

### A list written twice drifts

An observed property missing from a hand-written `ALL` was never observed
(`4b42d38`); a settings key missing from its roster lost its slider or
indexed the wrong cell (`75364a9`); a reply missing from its roster was never
recognised (`be94f2e`). An enum, its `ALL`, its names and its table come from
one macro over one list, as `replies!` and `observed!` do. A thing is
addressed by its key, never by its position in a list: a row that carried a
registry index moved a different setting once the order changed (`f2ea097`,
`230018c`).

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

## What has broken before

### Nothing may wait for a frame

A frame is drawn only while something on screen changes: a minimised window,
a paused film or an idle mpv draws none. Work run only from the frame went
unheard while minimised (`f9f90ad`); a drain that stopped early was rescued
by the next frame until there was none (`4db1c4f`); a re-read asked for "on
the next frame" never came on a paused film (`fcfac5b`). Slint itself reads
`mouse-cursor` only while routing a pointer event, so a binding that hid the
pointer three seconds after the last movement never once took effect
(`7fe7705`).

- Run work from the event that makes it necessary — mpv's wakeup, the
  worker's, a Slint callback — through `Player`. The frame draws; it does not
  decide.
- A drain empties its queue.
- Work that follows a change runs once per change, not on every frame while
  a condition holds (`26476f2`), and a batch is written once, not once per
  item (`0e8e385`).
- Do not read an event out of the time between two frames: a window that
  stalls looks like a seek (`05ed3af`).
- Test it by starving frames: `DBM_IDLE_TEST` minimises the window.

### End a state on the event that proves it is over

The most repeated bug here is clearing or setting a fact on an event that
usually comes with the real one. A failure was cleared when the file's path
appeared, and mpv reports a path for a file it then fails to play: the fix
(`d72f4d4`) was itself corrected the next day (`c1dff56`). "Opening …" ended
when a file existed, which over a film already playing was at once
(`c94a769`). The last file's chapters were read against the next because its
list arrives after its index (`69b79d7`). A drag ended on `clicked`, which a
release off the track never fires (`3848051`).

- Before writing the line that clears a state, name the event that proves it
  is over and use that one: `FileLoaded`, not the path; the pointer coming
  up, not `clicked`; the reply to this request, not the next reply.
- What belonged to the last file goes when the file changes, all at once, in
  `follow_file`, rather than piece by piece as the next file's facts arrive.
- mpv carries a setting changed during one file into the next unless
  `reset-on-next-file` names it (`784ec8a`).

### A request in flight ends on a refusal too

Whatever waits for an answer from mpv also has to end on an error and on no
answer at all. A refused seek still counted as in flight, and the scrubber
waited on it for the rest of the drag (`affd926`); a start flag waiting for a
list's reply stood when the list never reached mpv, and the next list's reply
took it (`bc3b254`); a move past the end of the playlist is refused as
`Reply::Move`, and has to end the save that waited on it (`34199a4`). Keep
the wait inside the request's own value, as `Opening` keeps `Asked`, match
the failed reply as well as the success, and say what clears the wait if mpv
never answers.

### Make the new one before letting go of the old

Every resource bug here was on the failure path. A resize deleted the old
texture before the new allocation was known to have worked, while Slint still
held its name (`b978ed7`). A GL program that failed to build leaked the
shaders before it (`c87a04b`). Dropping a `Child` neither kills nor waits for
it, so called-off ffmpeg runs lived on as zombies (`e326019`). A release
through `&self` let a second release, or a draw after one, compile
(`11dfd0f`). Unloading libmpv at exit let one of its script threads jump into
unmapped memory (`7732e57`).

- Allocate and check the replacement, then release the old; on failure the
  old is untouched.
- Release by value, `fn release(self, gl)`, so a released thing cannot be
  used again.
- What owns a process, thread or library handle lets go of it in `Drop`, as
  `ffmpeg::Children` kills and waits.
- Every early return in a constructor releases what was made before it.
- A failure the window cannot provoke gets a hook behind the `harness`
  feature, as `pipeline::refuse_next_allocation` does.

### A file on disk is shared state

Each of these was found by asking what a crash, a second open or a second
copy of the player would do:

- Write with `paths::write_atomic`, a scratch file renamed over the old, never
  in place: a half-written settings file lost every setting (`bc96f40`). A
  file that cannot be read is moved aside with `paths::set_aside`, not written
  over.
- Read, change and write a shared map under one lock, `PathMap::edit`; a `get`
  and then a `set` can lose a write made between them (`4c471dc`).
- A scratch file is named per process and per use (`paths::playlist_m3u`): a
  shared one was written over before mpv had read it (`e566870`).
- A cached answer is kept with everything it depends on — the file's
  modification time and size as well as its path, so a replaced file does not
  inherit the old one's crop (`7beb50c`) — and the cache's version goes up
  when the way it is worked out changes.
- A write that waits, behind a debounce or a queue, is flushed at quit and
  numbered, so an older one never lands on a newer (`53a854e`).

### One file has one path

A file reached two ways was two files to the player, with two resume points
and two watched marks, and it took three fixes: links (`58a4db7`), files found
inside an opened folder (`5ea39f2`) and the Flatpak's document portal
(`d6fd28a`). The type holds it now. A `MediaPath` is made only from a
`RealPath`, whose every link has been followed, from mpv reporting one it was
given, or read back from the player's own records; `for_test` exists only for
tests, behind `dbm-mpv`'s `testing` feature. A folder is listed through
`RealPath::entries`, which costs a lookup only for a link. A path a person
hands over goes through `playlist::prepare`, the one caller of the portal
lookup. A callback that needs the file playing reads `App::playing`, never the
interface's text.

No answer may depend on the order `read_dir` or a `HashMap` happens to yield;
sort first (`094aa6c`).

### Values from outside are checked where they come in

The command line, a settings file, mpv, OpenSubtitles and the window system
have each handed the player a value its code assumed it would not get:

- A file name need not be Unicode: `args_os`, `OsString` and `Path`, since
  `args` panicked before the window opened (`5521f6e`).
- `"nan"` and `"inf"` parse as `f64`, and `clamp` passes NaN through; a number
  read from a file is finite or skipped (`ee7354c`).
- Text from a service is filtered before it becomes part of a file name
  (`9dbc4b6`).
- In a shader, floor what goes into `pow`, `sqrt` or a divisor, since rounding
  carries a value a hair past its range (`3120004`).
- A wheel sends sideways and fractional steps: add them up to a notch, and
  leave the other axis alone (`147c80f`). A letter under Ctrl or the system
  key belongs to other programs (`c3b13fc`).
- The window manager changes what the player believes it set: fullscreen is
  read from the window, not from the last toggle (`583a45a`).

### A surface on its way out shows and does what it offered

A pill, tip or row that is fading out still draws and still takes presses.
Each time one read the live state instead, it changed its words mid-fade
(`829afc9`), played the next episode on the second press of a double-click
(`0bfe020`), or flashed as a panel closed (`4605c3b`). Keep what was offered
when it was shown, draw that until it is gone, and make a press on an offer
that has ended do nothing. The keyboard's ring follows the same rule: an
offer coming up takes it only when no panel holds it (`1714cc2`), and every
way of closing a panel goes through `close-panel` (`4b3b903`).

### Lay out from where a box is, not where it is going

In Slint, the value an animated property is heading for and the value it has
now agree only at rest. Pages sized from the height a panel was animating
towards threw their rows about mid-move (`f9f76ba`, `09865d2`), and a page
built only while it was the one asked for went blank as it slid away
(`fdfc8ca`). Read the box's own `height`, keep a page built for as long as it
is on screen, and capture frames across the move, since none of these shows
at rest.

### Words are made where they are shown

A sentence made on the worker, or kept from when it was made, stays in the
language of that moment (`7c1860f`, `e43c58e`, `613cfc5`, `3f0dfb3`). Carry a
typed reason out of the library and turn it into words in the interface when
it is drawn. Text gets the width its words ask for, not a fixed one: German
elided a 260 px menu (`e6572b3`). A number and its unit are one pattern in
every language (`c045206`). The harness speaks the system's language, so a
check never compares against English.

### What mpv and Slint do is measured, not assumed

Several bugs were a reasonable guess about a library that turned out false:
mpv saves a resume point only on `quit`, `stop` and `loadfile` (`34199a4`);
`sub-visibility` can be on with no track selected (`3d239b3`); a blocking
read overtakes writes still queued (`b47672f`); mpv's GL renderer keeps a
fence per frame that the render API never frees (`def065e`); a Slint `Timer`
that was never started does nothing on `restart` (`5a17516`). Before relying
on such a behaviour, try it — a ctypes script against libmpv, or a harness
exercise — and say at the call what it relies on.

### A fix's exercise fails before the fix

Run the new check on the code before the change and see it fail. Ask the outside
world, not the player's own flag: `DBM_CURSOR_TEST` asks Windows whether the
pointer is drawn, because `pointer-hidden` was set correctly for months while
the pointer never hid (`7fe7705`). Reproduce the state a person was in, not
the one a fresh file arrives in: the first attempt at `3d239b3` photographed
a correct menu. And a check waits for what it expects to hold, with
`Verdict::expect` or `expect_after` and the seconds of film it needs, rather
than reading a moment after asking — the nightly runner on one core is slower
than any desktop (`a9f8872`, `c5a3361`).

## Before you add a handler, a wait or a write

1. **What starts it, and does that come while no frame is drawn?**
2. **What ends it, and is that the event that proves it, or one that usually
   comes with it?**
3. **If it waits for mpv: what ends the wait on a refusal, and on silence?**
4. **If it lets go of something: is the replacement made and checked first,
   and is every early return covered?**
5. **If it writes a file: what does a crash halfway, a second open or a
   second copy of the player do to it?**
6. **If it reads from outside: what is the strangest value that can arrive?**
7. **If it shows words: in which language, at what width, and after the
   language changes?**
8. **If it fades: what does it show and do on its way out?**

## Where these live

| Pattern | Home |
|---|---|
| `ForFile<T>` | `crates/library/src/threads.rs` |
| `Size`, `Rect<U>`, `Region` | `crates/gpu/src/size.rs`, `crates/gpu/src/rect.rs` |
| Player lifecycle (`Video`) | `crates/player/src/driver/mod.rs` |
| Per-file and per-open state | `crates/player/src/player/` |
| Typed mpv properties | `crates/mpv/src/prop.rs` |
| `Mpv`, `Reader`, `Events` | `crates/mpv/src/lib.rs`, `crates/mpv/src/events.rs` |
| Command replies (`replies!`) | `crates/player/src/playback/commands.rs` |
| Platform failures | `crates/player/src/platform/mod.rs` |
| `write_atomic`, `set_aside`, `playlist_m3u` | `crates/library/src/paths.rs` |
| `PathMap::edit`, `set_many`, cache versions | `crates/library/src/cache.rs` |
| `MediaPath`, `RealPath` | `crates/mpv/src/media_path.rs` |
| Ways in (`prepare`), the portal | `crates/library/src/playlist.rs`, `crates/library/src/documents.rs` |
| `ffmpeg::Children` | `crates/library/src/ffmpeg.rs` |
| `Verdict::expect`, `expect_after` | `crates/player/src/harness/script.rs` |

## Keeping it honest

`cargo clippy --workspace --all-targets --all-features -- -D warnings`, the
test suite and `scripts/smoke.sh` are the floor. A driver failure the window
cannot provoke gets a test-only hook behind the `harness` feature, as
`dbm_gpu::pipeline::refuse_next_allocation` does, rather than an untested
branch.
