//! Subtitles from OpenSubtitles, for a film that has none worth reading.
//!
//! Two calls to their REST API and a fetch. A search lists what they hold for
//! the file, asked by its hash, which finds subtitles timed for this exact
//! release, and by its name. The person picks one, and the download call
//! hands back a link to it, good for a few hours. The file is kept in the
//! player's own folder rather than beside the film, which may be on a disk
//! the player only reads, and is put back whenever the film opens again.
//!
//! The API wants a key, which identifies the player rather than its user and
//! is registered by whoever builds it. It is read at build time (see
//! `build.rs`), and a build without one has no search. Downloads come from
//! the key's own small allowance until it runs out, and from then on from the
//! person's own account, once they have signed in (see `account`).
//!
//! Everything here blocks on the network, so each search and each download
//! runs on a thread of its own and reports back like any other background
//! result. A [`Supersede`] stops a search for a file that has since been
//! replaced, as the sync's does.

use std::path::{Path, PathBuf};

use dbm_lang::tr;
use dbm_mpv::MediaPath;
use dbm_naming::Language;

use crate::cache::PathMap;
use crate::threads::{ForFile, Reporter, Supersede};

pub mod account;
mod download;
mod fake;
mod hash;
mod http;
mod search;

use account::Account;

/// The key the player identifies itself with, if it was built with one.
pub fn key() -> Option<&'static str> {
    option_env!("DBM_OPENSUBTITLES_KEY").filter(|k| !k.trim().is_empty())
}

/// Whether there is any searching to be done at all: a key, or the harness
/// standing in for the service (see `fake`).
pub fn available() -> bool {
    key().is_some() || fake::on()
}

/// Downloads kept, by the film they belong to.
pub(crate) static KEPT: PathMap<Vec<String>> = PathMap::record("downloaded-subtitles.json");

static SEARCHES: Supersede = Supersede::new();

/// One subtitle OpenSubtitles holds for the film.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// What to ask the download call for.
    pub file_id: i64,
    /// Their language code: `en`, `sv`, `pt-br`.
    pub language: Language,
    /// The release it was timed against, or its file name where it has none.
    pub release: String,
    pub downloads: i64,
    /// Found by the file's own hash: timed for exactly this copy of it.
    pub exact: bool,
    pub hearing_impaired: bool,
    /// Made by a machine rather than a person.
    pub machine: bool,
}

/// What went wrong, so the interface can say it in its own words.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// The network, or their end of it.
    Unreachable,
    /// The key was refused.
    Refused,
    /// The day's downloads are spent: the key's own, or the account's.
    Spent { signed_in: bool },
    /// Too many requests too close together, by their count or the player's.
    Busy,
    /// A sign-in whose name and password don't match.
    WrongPassword,
    /// The account's token is no longer taken, so the person is signed out.
    Lapsed,
    /// Anything else they said, in their words.
    Said(String),
}

impl Failure {
    pub fn sentence(&self) -> String {
        match self {
            Self::Unreachable => tr("Could not reach OpenSubtitles").to_string(),
            Self::Refused => tr("OpenSubtitles refused the player's key").to_string(),
            Self::Spent { signed_in: false } => {
                tr("The downloads without an account are used up for today").to_string()
            }
            Self::Spent { signed_in: true } => {
                tr("Today's subtitle downloads are used up").to_string()
            }
            Self::Busy => tr("OpenSubtitles is busy. Try again in a moment").to_string(),
            Self::WrongPassword => tr("The username or password is wrong").to_string(),
            Self::Lapsed => tr("The sign-in to OpenSubtitles has run out").to_string(),
            Self::Said(text) => text.clone(),
        }
    }

    /// Whether signing in is the way past it.
    pub fn wants_account(&self) -> bool {
        matches!(self, Self::Spent { signed_in: false } | Self::Lapsed)
    }
}

/// A search's answer, for the file it was asked about, and who was signed in
/// once it was done, which holds whatever is playing by then.
pub struct Found {
    pub hits: ForFile<Result<Vec<Hit>, Failure>>,
    pub account: Option<Account>,
}

/// A download's answer: the subtitle on disk, and its language, for the file
/// it was fetched for; and who was signed in once it was done, with what they
/// have left.
pub struct Fetched {
    pub hit: Hit,
    pub result: ForFile<Result<(PathBuf, Language), Failure>>,
    pub account: Option<Account>,
}

/// Look for subtitles for `path` in `languages`, on a thread of its own.
pub fn search(path: MediaPath, languages: Vec<Language>, reporter: Reporter<Found>) {
    let ticket = SEARCHES.next();
    crate::threads::spawn("dbm-search", move || {
        let hits = search::look(&path, &languages);
        if ticket.current() {
            let account = account::current();
            reporter.send(Found {
                hits: ForFile::new(path, hits),
                account,
            });
        }
    });
}

/// Fetch one subtitle for `path`, on a thread of its own, keep it, and say
/// where it is.
pub fn fetch(path: MediaPath, hit: Hit, reporter: Reporter<Fetched>) {
    crate::threads::spawn("dbm-download", move || {
        let result = download::download(&path, &hit);
        let account = account::current();
        reporter.send(Fetched {
            hit,
            result: ForFile::new(path, result),
            account,
        });
    });
}

/// Subtitles fetched for `path` before that are still there, each with the
/// language its name carries, where it carries one (see `download::keep_as`).
///
/// **Blocking.** Worker thread only.
pub fn kept(path: &MediaPath) -> Vec<(PathBuf, Option<Language>)> {
    KEPT.load()
        .remove(path)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|file| file.is_file())
        .map(|file| {
            let language = file
                .file_stem()
                .and_then(|stem| Path::new(stem).file_stem())
                .and_then(|stem| Path::new(stem).extension())
                .and_then(|code| Language::parse(&code.to_string_lossy()));
            (file, language)
        })
        .collect()
}

/// Forget a subtitle fetched for `path` and delete it, or it would be put
/// back the next time the film opens. Only a file this module fetched, since
/// one beside the film is the person's own.
///
/// **Blocking.** Worker thread only.
pub fn forget(path: &MediaPath, file: &str) {
    let mut forgot = false;
    KEPT.edit(|kept| {
        if let Some(files) = kept.get_mut(path) {
            let before = files.len();
            files.retain(|kept| kept != file);
            forgot = files.len() != before;
        }
        forgot
    });
    if !forgot {
        return;
    }
    let ours = crate::paths::app_data_dir().join("subtitles");
    if Path::new(file).starts_with(&ours) {
        let _ = std::fs::remove_file(file);
    }
}
