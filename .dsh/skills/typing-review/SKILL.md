---
name: typing-review
description: Review a DBM change for states the types still allow and the code never handles — parallel Options, booleans that mean three things, facts about a file that outlive the file, and unnamed sizes or units.
whenToUse: Use when a change adds or moves state in crates/player or crates/gpu — a new field, flag, Option, tuple, or a second boolean beside an existing one — or carries a path, an id, a size or a duration, and again before committing it.
---

# Reviewing a change for representable states

The rules themselves are in `AGENTS.md` at the repository root: read the
section headed by the shape you are adding. This is the pass to run over the
diff.

## 1. Read the diff as a set of new facts

```sh
git diff -U0 | grep -E '^\+' | grep -vE '^\+\+\+'
```

List what the change makes true, not what it does. "The panel remembers the
file it was opened for" is a fact; `Option<MediaPath>` beside the panel's
state is how it is stored.

## 2. Scan for the shapes

```sh
# states that can disagree
git diff -U0 | grep -nE '^\+.*(Option<|: bool|: Option)'
# unnamed sizes and rectangles
git diff -U0 | grep -nE '^\+.*\(u32, u32\)|^\+.*\(f32, f32\)'
# answers compared to what they are about
git diff -U0 | grep -nE '^\+.*(\.path ==|== Some\(&?path|\.id ==)'
# failures flattened to text
git diff -U0 | grep -nE '^\+.*Result<[^>]*, *String>'
```

Any hit is a question, not a verdict: `Mouse::hidden`, mpv's `pause` and a
settings toggle are genuine two-state facts. A `(f32, f32)` that is a
direction vector is not a size.

## 3. Answer the questions for every new field

1. What is it about — a file, an open, a panel, a window?
2. Can it be set while that thing is absent or replaced?
3. How many combinations of the struct's flags can occur, and how many does
   the code handle?
4. Does any reader compare a path or id to know the value is current?
5. Is it a size, count, position, duration or rate, and does its type say so?

`AGENTS.md` has the fallback answers: the fact moves into the type, the flags
become an enum, the reader loses its comparison.

## 4. Fix in this order

1. Move the fact into the type that owns it (`Option<PerFile>`,
   `Asked { m3u, start }`, `ForFile<T>`).
2. Replace the flags with an enum whose variants are the valid states.
3. Name the number or the unit (`Size`, `Rect<U>`, `Rate`).
4. Only then add a guard clause, and say in a comment why the type could not
   carry it — that comment is the next reviewer's clue that the type is
   still wrong.

Do not add a `debug_assert!` or a test that a state cannot happen when the
state can still be constructed.

## 5. Check it the usual way

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

A change to a state machine that the tests cannot reach wants a harness
exercise (`crates/player/src/harness/`, run with `scripts/smoke.sh`), and a
failure the window cannot provoke wants a hook behind the `harness` feature,
as `dbm_gpu::pipeline::refuse_next_allocation` is.
