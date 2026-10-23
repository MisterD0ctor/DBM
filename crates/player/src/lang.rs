//! The language the player speaks.
//!
//! English is what the source says; every other language is a gettext file
//! under `lang/<code>/LC_MESSAGES/dbm-player.po`, read at build time and
//! compiled in. One file per language holds both halves of the interface:
//! the words in `app.slint`, which Slint's own bundling looks up by the
//! component they are in, and the words made here in Rust — a notice, a
//! playlist heading, how much of a film is left — which are the entries
//! under the context `player`, turned into a table by the build script.
//!
//! Adding a language is adding its file. Nothing here or in the interface
//! names one: the settings page lists whatever was built in.
//!
//! Which one is spoken is a number any thread can read, because some of
//! these words are made on the worker — a folder with nothing in it says so
//! from there.

use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};

// `CATALOGUES`: each language's code and its `player` entries, sorted by the
// English they stand for.
include!(concat!(env!("OUT_DIR"), "/catalogues.rs"));

/// Zero for English, or one more than the place in `CATALOGUES`.
static CURRENT: AtomicUsize = AtomicUsize::new(0);
/// Bumped whenever the language changes, so whoever has handed words to the
/// interface can tell they need saying again.
static CHANGES: AtomicUsize = AtomicUsize::new(0);

/// These words in the language being spoken, or as they are where it has
/// none for them.
pub fn tr(text: &'static str) -> &'static str {
    let spoken = CURRENT.load(Ordering::Relaxed);
    let Some((_, words)) = spoken.checked_sub(1).and_then(|i| CATALOGUES.get(i)) else {
        return text;
    };
    words
        .binary_search_by(|(english, _)| (*english).cmp(text))
        .map_or(text, |found| words[found].1)
}

/// Words marked for translation where they are written, in a table that can't
/// call [`tr`] because it is built before any language is spoken. Whoever
/// shows them passes them through `tr` then; `scripts/translations.py` finds
/// them by this mark.
pub const fn marked(text: &'static str) -> &'static str {
    text
}

/// A sentence with its blanks filled in: each `{}` takes the next value.
///
/// By hand rather than `format!`, which needs its pattern at compile time and
/// so could never be handed a translated one.
pub fn fill(pattern: &str, values: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut values = values.iter();
    let mut rest = pattern;
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        if let Some(value) = values.next() {
            out.push_str(&value.to_string());
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// The languages built in besides English, by code.
pub fn codes() -> impl Iterator<Item = &'static str> {
    CATALOGUES.iter().map(|(code, _)| *code)
}

/// The code of the language being spoken: `en`, or one built in.
pub fn spoken() -> &'static str {
    let spoken = CURRENT.load(Ordering::Relaxed);
    spoken
        .checked_sub(1)
        .and_then(|i| CATALOGUES.get(i))
        .map_or("en", |(code, _)| code)
}

/// How many times the language has been set. See `CHANGES`.
pub fn changes() -> usize {
    CHANGES.load(Ordering::Relaxed)
}

/// Speak a language: its code, or nothing for the system's own. One the
/// player was not built with is English.
///
/// **UI thread, after the window exists** — Slint's half of the switch needs
/// a component to have been created.
pub fn speak(choice: &str) {
    let code = if choice.is_empty() {
        system()
    } else {
        choice.to_string()
    };
    let spoken = CATALOGUES
        .iter()
        .position(|(known, _)| *known == code)
        .map_or(0, |place| place + 1);
    CURRENT.store(spoken, Ordering::Relaxed);
    CHANGES.fetch_add(1, Ordering::Relaxed);
    let asked = if spoken == 0 { "" } else { code.as_str() };
    if let Err(e) = slint::select_bundled_translation(asked) {
        eprintln!("dbm: could not switch the interface to {code:?}: {e}");
    }
}

/// The system's language, as the two or three letters before any region.
fn system() -> String {
    sys_locale::get_locale()
        .map(|locale| {
            locale
                .split(['-', '_', '@', '.'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanks_are_filled_in_order() {
        assert_eq!(fill("{} h {} min left", &[&1, &12]), "1 h 12 min left");
        assert_eq!(fill("Season {}", &[&7]), "Season 7");
        assert_eq!(fill("no blanks", &[&7]), "no blanks");
    }

    #[test]
    fn english_is_what_the_source_says() {
        assert_eq!(tr("Sound is back"), "Sound is back");
    }

    #[test]
    fn every_catalogue_is_in_the_order_it_is_searched_in() {
        for (code, words) in CATALOGUES {
            assert!(
                words.windows(2).all(|pair| pair[0].0 < pair[1].0),
                "{code} is out of order"
            );
        }
    }
}
