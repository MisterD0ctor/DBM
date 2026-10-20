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
//! `build.rs`), and a build without one has no search. There is no login, so
//! downloads come from the key's small daily allowance, and the player says
//! when it has run out.
//!
//! Everything here blocks on the network, so each search and each download
//! runs on a thread of its own and reports back like any other background
//! result. A generation counter stops a search for a file that has since
//! been replaced, as the sync's does.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;

use crate::lang::tr;
use crate::library::cache::PathMap;
use crate::library::naming::{self, Media};
use crate::worker::{Completion, Reporter};

const API: &str = "https://api.opensubtitles.com/api/v1";
/// Long enough for a slow connection, short enough that a dead one says so
/// while the person is still looking at the row that asked.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The most results shown. Past a screenful the list is releases nobody has.
const MOST: usize = 40;

/// The key the player identifies itself with, if it was built with one.
pub fn key() -> Option<&'static str> {
    option_env!("DBM_OPENSUBTITLES_KEY").filter(|k| !k.trim().is_empty())
}

/// Whether there is any searching to be done at all: a key, or the harness
/// standing in for the service (see `fake_hits`).
pub fn available() -> bool {
    key().is_some() || std::env::var_os("DBM_FIND_FAKE").is_some()
}

/// Downloads kept, by the film they belong to.
static KEPT: PathMap<Vec<String>> = PathMap::record("downloaded-subtitles.json");

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// One subtitle OpenSubtitles holds for the film.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// What to ask the download call for.
    pub file_id: i64,
    /// Their language code: `en`, `sv`, `pt-br`.
    pub language: String,
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
    /// The day's downloads are spent.
    Spent,
    /// Anything else they said, in their words.
    Said(String),
}

impl Failure {
    pub fn sentence(&self) -> String {
        match self {
            Self::Unreachable => tr("Could not reach OpenSubtitles").to_string(),
            Self::Refused => tr("OpenSubtitles refused the player's key").to_string(),
            Self::Spent => tr("Today's subtitle downloads are used up").to_string(),
            Self::Said(text) => text.clone(),
        }
    }
}

/// A search's answer, for the file it was asked about.
pub struct Found {
    pub path: String,
    pub hits: Result<Vec<Hit>, Failure>,
}

/// A download's answer: the subtitle on disk, and its language.
pub struct Fetched {
    pub path: String,
    pub result: Result<(PathBuf, String), Failure>,
}

/// Look for subtitles for `path` in `languages`, on a thread of its own.
pub fn search(path: String, languages: Vec<String>, reporter: Reporter) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        let hits = look(&path, &languages);
        if GENERATION.load(Ordering::SeqCst) == generation {
            reporter.send(Completion::Found(Found { path, hits }));
        }
    });
}

/// Fetch one subtitle for `path`, on a thread of its own, keep it, and say
/// where it is.
pub fn fetch(path: String, hit: Hit, reporter: Reporter) {
    std::thread::spawn(move || {
        let result = download(&path, &hit);
        reporter.send(Completion::Fetched(Fetched { path, result }));
    });
}

/// Subtitles fetched for `path` before that are still there, each with its
/// language, which is in its name (see `keep_as`).
///
/// **Blocking.** Worker thread only.
pub fn kept(path: &str) -> Vec<(PathBuf, String)> {
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
                .map(|code| code.to_string_lossy().into_owned())
                .unwrap_or_default();
            (file, language)
        })
        .collect()
}

/// Forget a subtitle fetched for `path` and delete it, or it would be put
/// back the next time the film opens. Only a file this module fetched, since
/// one beside the film is the person's own.
///
/// **Blocking.** Worker thread only.
pub fn forget(path: &str, file: &str) {
    let mut files = KEPT.load().remove(path).unwrap_or_default();
    let before = files.len();
    files.retain(|kept| kept != file);
    if files.len() == before {
        return;
    }
    KEPT.set(path, files);
    let ours = crate::paths::app_data_dir().join("subtitles");
    if Path::new(file).starts_with(&ours) {
        let _ = std::fs::remove_file(file);
    }
}

// ---------------------------------------------------------------------------
// Searching
// ---------------------------------------------------------------------------

fn look(path: &str, languages: &[String]) -> Result<Vec<Hit>, Failure> {
    if let Some(hits) = fake_hits() {
        return Ok(hits);
    }
    let key = key().ok_or(Failure::Refused)?;
    let languages = languages.join(",");
    let mut hits: Vec<Hit> = Vec::new();
    // By hash first: those are the ones timed for this very file, and they
    // stay at the top of the list whatever their download counts.
    if let Some(hash) = Path::new(path)
        .is_file()
        .then(|| hash(Path::new(path)))
        .flatten()
    {
        let query = [("languages", languages.clone()), ("moviehash", hash)];
        hits.extend(ask(key, &query)?.into_iter().map(|mut hit| {
            hit.exact = true;
            hit
        }));
    }
    let named = by_name(path, &languages);
    for hit in ask(key, &named)? {
        if !hits.iter().any(|h| h.file_id == hit.file_id) {
            hits.push(hit);
        }
    }
    hits.sort_by(|a, b| b.exact.cmp(&a.exact).then(b.downloads.cmp(&a.downloads)));
    hits.truncate(MOST);
    Ok(hits)
}

/// The search by name: a show's season and episode, or a film and its year.
/// In alphabetical order, which is what their API asks of a query string.
fn by_name(path: &str, languages: &str) -> Vec<(&'static str, String)> {
    let mut query = vec![("languages", languages.to_string())];
    match naming::parse(path) {
        Media::Episode {
            show,
            season,
            episode,
            ..
        } => {
            query.push(("episode_number", episode.to_string()));
            query.push(("query", show));
            if let Some(season) = season {
                query.push(("season_number", season.to_string()));
            }
        }
        Media::Movie { title, year } => {
            query.push(("query", title));
            if let Some(year) = year {
                query.push(("year", year.to_string()));
            }
        }
    }
    query.sort_by_key(|(name, _)| *name);
    query
}

fn ask(key: &str, query: &[(&str, String)]) -> Result<Vec<Hit>, Failure> {
    let mut request = agent()
        .get(&format!("{API}/subtitles"))
        .set("Api-Key", key)
        .set("Accept", "application/json");
    for (name, value) in query {
        request = request.query(name, &value.to_lowercase());
    }
    let body: Value = answer(request.call())?
        .into_json()
        .map_err(|_| Failure::Unreachable)?;
    Ok(hits(&body))
}

/// The results in a search's answer.
fn hits(body: &Value) -> Vec<Hit> {
    let Some(data) = body.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|item| {
            let a = item.get("attributes")?;
            let file = a.get("files")?.as_array()?.first()?;
            let file_id = file.get("file_id")?.as_i64()?;
            let text = |name: &str| a.get(name).and_then(Value::as_str).unwrap_or_default();
            let flag = |name: &str| a.get(name).and_then(Value::as_bool).unwrap_or(false);
            let release = match text("release").trim() {
                "" => file
                    .get("file_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                release => release.to_string(),
            };
            Some(Hit {
                file_id,
                language: text("language").to_string(),
                release,
                downloads: a.get("download_count").and_then(Value::as_i64).unwrap_or(0),
                exact: flag("moviehash_match"),
                hearing_impaired: flag("hearing_impaired"),
                machine: flag("machine_translated") || flag("ai_translated"),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Downloading
// ---------------------------------------------------------------------------

fn download(path: &str, hit: &Hit) -> Result<(PathBuf, String), Failure> {
    let file = keep_as(path, hit);
    // Fetched before: no need to spend one of the day's downloads on it.
    if file.is_file() {
        remember(path, &file);
        return Ok((file, hit.language.clone()));
    }
    let bytes = if fake_hits().is_some() {
        fake_subtitle()
    } else {
        let key = key().ok_or(Failure::Refused)?;
        let link: Value = answer(
            agent()
                .post(&format!("{API}/download"))
                .set("Api-Key", key)
                .set("Accept", "application/json")
                .send_json(serde_json::json!({ "file_id": hit.file_id })),
        )?
        .into_json()
        .map_err(|_| Failure::Unreachable)?;
        let Some(url) = link.get("link").and_then(Value::as_str) else {
            return Err(said(&link).unwrap_or(Failure::Unreachable));
        };
        let mut bytes = Vec::new();
        answer(agent().get(url).call())?
            .into_reader()
            .take(16 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Unreachable)?;
        bytes
    };
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&file, bytes).map_err(|e| Failure::Said(e.to_string()))?;
    remember(path, &file);
    Ok((file, hit.language.clone()))
}

/// Where a download for `path` is kept: in the player's own folder, named
/// after the film, the language and the subtitle's own id, so the same one
/// asked for twice is the same file.
fn keep_as(path: &str, hit: &Hit) -> PathBuf {
    let stem = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "subtitles".into());
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " .-_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    crate::paths::app_data_dir()
        .join("subtitles")
        .join(format!("{stem}.{}.{}.srt", hit.language, hit.file_id))
}

fn remember(path: &str, file: &Path) {
    let mut files = KEPT.load().remove(path).unwrap_or_default();
    let file = file.to_string_lossy().into_owned();
    if !files.contains(&file) {
        files.push(file);
        KEPT.set(path, files);
    }
}

// ---------------------------------------------------------------------------
// Talking to them
// ---------------------------------------------------------------------------

fn agent() -> ureq::Agent {
    let builder = ureq::AgentBuilder::new()
        .timeout(TIMEOUT)
        .user_agent(&format!("Death by MPV v{}", env!("CARGO_PKG_VERSION")));
    // Windows' own TLS, which the client has to be handed (see Cargo.toml).
    #[cfg(windows)]
    let builder = match native_tls::TlsConnector::new() {
        Ok(tls) => builder.tls_connector(std::sync::Arc::new(tls)),
        Err(_) => builder,
    };
    builder.build()
}

/// A response worth reading, or what its status means.
fn answer(result: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, Failure> {
    match result {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(code, response)) => {
            let body: Option<Value> = response.into_json().ok();
            Err(match code {
                401 | 403 => Failure::Refused,
                406 | 429 => Failure::Spent,
                _ => body.as_ref().and_then(said).unwrap_or(Failure::Unreachable),
            })
        }
        Err(ureq::Error::Transport(_)) => Err(Failure::Unreachable),
    }
}

/// The message in an answer, if it carries one.
fn said(body: &Value) -> Option<Failure> {
    body.get("message")
        .or_else(|| body.get("errors").and_then(|e| e.get(0)))
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .map(|m| Failure::Said(m.trim().to_string()))
}

// ---------------------------------------------------------------------------
// The file's hash
// ---------------------------------------------------------------------------

/// OpenSubtitles' hash of a file: its size plus the first and last 64 KiB
/// read as little-endian 64-bit words, all added with wrapping. Sixteen hex
/// digits. Cheap (two reads), and the same for every copy of a release.
fn hash(path: &Path) -> Option<String> {
    const CHUNK: u64 = 64 * 1024;
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < CHUNK {
        return None;
    }
    let mut sum = size;
    let mut buf = vec![0u8; CHUNK as usize];
    for at in [0, size - CHUNK] {
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut buf).ok()?;
        for word in buf.as_chunks::<8>().0 {
            sum = sum.wrapping_add(u64::from_le_bytes(*word));
        }
    }
    Some(format!("{sum:016x}"))
}

// ---------------------------------------------------------------------------
// Standing in for them, for the harness
// ---------------------------------------------------------------------------

/// `DBM_FIND_FAKE`: answer searches from here, and downloads with a short
/// subtitle written on the spot, so the flow can be driven without a key or
/// a connection.
fn fake_hits() -> Option<Vec<Hit>> {
    std::env::var_os("DBM_FIND_FAKE")?;
    let hit = |file_id, language: &str, release: &str, downloads, exact| Hit {
        file_id,
        language: language.into(),
        release: release.into(),
        downloads,
        exact,
        hearing_impaired: file_id == 3,
        machine: file_id == 4,
    };
    Some(vec![
        hit(1, "en", "Show.S01E01.1080p.WEB-DL.x265", 48211, true),
        hit(2, "sv", "Show.S01E01.1080p.WEB-DL.x265", 9120, true),
        hit(3, "en", "Show.S01E01.720p.HDTV.x264", 15002, false),
        hit(4, "sv", "Show.S01E01.720p.HDTV.x264", 870, false),
    ])
}

fn fake_subtitle() -> Vec<u8> {
    b"1\n00:00:01,000 --> 00:00:30,000\nFetched from nowhere, for the harness.\n".to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_size_and_both_ends_added_up() {
        let file = std::env::temp_dir().join("dbm-osdb-hash-test.bin");
        // 128 KiB of ones: 16384 words of 0x0101..01 at each end, plus the size.
        std::fs::write(&file, vec![1u8; 128 * 1024]).unwrap();
        let word = u64::from_le_bytes([1; 8]);
        let expected = (128u64 * 1024)
            .wrapping_add(word.wrapping_mul(8192))
            .wrapping_add(word.wrapping_mul(8192));
        assert_eq!(hash(&file), Some(format!("{expected:016x}")));
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn a_file_too_small_to_hash_has_none() {
        let file = std::env::temp_dir().join("dbm-osdb-small-test.bin");
        std::fs::write(&file, b"short").unwrap();
        assert_eq!(hash(&file), None);
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn an_episode_is_asked_for_by_show_season_and_episode_in_order() {
        let query = by_name("/tv/The.Walking.Dead.S04E08.1080p.mkv", "en,sv");
        let names: Vec<&str> = query.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            ["episode_number", "languages", "query", "season_number"]
        );
        assert!(query.contains(&("season_number", "4".into())));
        assert!(query.contains(&("episode_number", "8".into())));
    }

    #[test]
    fn a_film_is_asked_for_by_title_and_year() {
        let query = by_name("/films/Alien (1979).mkv", "en");
        assert!(query.contains(&("query", "Alien".into())));
        assert!(query.contains(&("year", "1979".into())));
    }

    #[test]
    fn a_kept_file_says_its_language_in_its_name() {
        let hit = Hit {
            file_id: 42,
            language: "pt-br".into(),
            release: String::new(),
            downloads: 0,
            exact: false,
            hearing_impaired: false,
            machine: false,
        };
        let file = keep_as("/films/Some Film (2019).mkv", &hit);
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, "Some Film (2019).pt-br.42.srt");
        let stem = file.file_stem().unwrap();
        let code = Path::new(Path::new(stem).file_stem().unwrap())
            .extension()
            .unwrap();
        assert_eq!(code, "pt-br");
    }

    #[test]
    fn a_search_answer_is_read_into_hits() {
        let body = serde_json::json!({ "data": [
            { "attributes": {
                "language": "sv", "release": "Show.S01E01.WEB", "download_count": 120,
                "moviehash_match": true, "hearing_impaired": false,
                "machine_translated": false, "ai_translated": false,
                "files": [{ "file_id": 777, "file_name": "show.srt" }] } },
            { "attributes": { "language": "en", "release": "", "download_count": 5,
                "files": [{ "file_id": 778, "file_name": "show.en.srt" }] } },
            { "attributes": { "language": "en", "files": [] } }
        ]});
        let found = hits(&body);
        assert_eq!(found.len(), 2, "one with no file is not a hit");
        assert_eq!(found[0].file_id, 777);
        assert!(found[0].exact);
        assert_eq!(
            found[1].release, "show.en.srt",
            "no release falls back to the file"
        );
    }
}
