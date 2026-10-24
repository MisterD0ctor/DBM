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
//! result. A generation counter stops a search for a file that has since
//! been replaced, as the sync's does.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use url::Url;
use zeroize::Zeroizing;

use crate::lang::tr;
use crate::library::cache::PathMap;
use crate::library::naming::{self, Media};
use crate::worker::{Completion, Reporter};

pub mod account;

use account::{Account, Signed};

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
/// standing in for the service (see `fake`).
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
/// once it was done.
pub struct Found {
    pub path: String,
    pub hits: Result<Vec<Hit>, Failure>,
    pub account: Option<Account>,
}

/// A download's answer: the subtitle on disk, and its language; and who was
/// signed in once it was done, with what they have left.
pub struct Fetched {
    pub path: String,
    pub hit: Hit,
    pub result: Result<(PathBuf, String), Failure>,
    pub account: Option<Account>,
}

/// Look for subtitles for `path` in `languages`, on a thread of its own.
pub fn search(path: String, languages: Vec<String>, reporter: Reporter) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        let hits = look(&path, &languages);
        if GENERATION.load(Ordering::SeqCst) == generation {
            let account = account::current();
            reporter.send(Completion::Found(Found {
                path,
                hits,
                account,
            }));
        }
    });
}

/// Fetch one subtitle for `path`, on a thread of its own, keep it, and say
/// where it is.
pub fn fetch(path: String, hit: Hit, reporter: Reporter) {
    std::thread::spawn(move || {
        let result = download(&path, &hit);
        let account = account::current();
        reporter.send(Completion::Fetched(Fetched {
            path,
            hit,
            result,
            account,
        }));
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
    if fake::on() {
        return Ok(fake::hits());
    }
    // Searching needs no account, but OpenSubtitles asks for the token on
    // every request once there is one, and the VIP host refuses those
    // without it. One it no longer takes is let go, and the search asked
    // again without it.
    let signed = account::signed();
    match look_as(path, languages, signed.as_ref()) {
        Err(Failure::Lapsed) => {
            if let Some(signed) = &signed {
                account::lapse(signed);
            }
            look_as(path, languages, None)
        }
        result => result,
    }
}

/// The search, as `signed` or as nobody.
fn look_as(path: &str, languages: &[String], signed: Option<&Signed>) -> Result<Vec<Hit>, Failure> {
    let ask = |query: &[(&str, String)]| ask(query, signed);
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
        hits.extend(ask(&query)?.into_iter().map(|mut hit| {
            hit.exact = true;
            hit
        }));
    }
    let named = by_name(path, &languages);
    for hit in ask(&named)? {
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

fn ask(query: &[(&str, String)], signed: Option<&Signed>) -> Result<Vec<Hit>, Failure> {
    let reply = send_again_once(&Ask {
        method: "GET",
        path: "/subtitles",
        query,
        body: None,
        account: signed,
    })?;
    match reply.status {
        200..=299 => Ok(hits(&reply.body)),
        status => Err(failure(status, &reply.body, signed.is_some())),
    }
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
    let signed = account::signed();
    let bytes = if fake::on() {
        fake::download(signed.as_ref())?
    } else {
        let url = link(hit, signed.as_ref())?;
        fetch_file(url)?
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

/// Where to fetch `hit` from, which costs one of the day's downloads: the
/// key's own, or the account's when `signed` is.
fn link(hit: &Hit, signed: Option<&Signed>) -> Result<Url, Failure> {
    let body = serde_json::to_vec(&serde_json::json!({ "file_id": hit.file_id }))
        .map_err(|_| Failure::Unreachable)?;
    let reply = send_again_once(&Ask {
        method: "POST",
        path: "/download",
        query: &[],
        body: Some(&body),
        account: signed,
    })?;
    if !(200..300).contains(&reply.status) {
        let why = failure(reply.status, &reply.body, signed.is_some());
        if let (Failure::Lapsed, Some(signed)) = (&why, signed) {
            account::lapse(signed);
        }
        return Err(why);
    }
    if let (Some(signed), Some(left)) =
        (signed, reply.body.get("remaining").and_then(Value::as_i64))
    {
        account::left(signed, left);
    }
    reply
        .body
        .get("link")
        .and_then(Value::as_str)
        .and_then(|link| Url::parse(link).ok())
        .filter(|link| link.scheme() == "https")
        .ok_or_else(|| said(&reply.body).unwrap_or(Failure::Unreachable))
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

/// The hosts the API answers on: everything but the subtitle files
/// themselves goes to one of these, and nothing with the key or a token in it
/// goes anywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Api,
    /// Where a sign-in sends a VIP account, which must then send its token
    /// with every request.
    Vip,
}

impl Host {
    fn name(self) -> &'static str {
        match self {
            Self::Api => "api.opensubtitles.com",
            Self::Vip => "vip-api.opensubtitles.com",
        }
    }

    /// The host a sign-in names, if it is one of these.
    fn named(name: &str) -> Option<Self> {
        [Self::Api, Self::Vip]
            .into_iter()
            .find(|host| host.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The host `url` is on, if it is one of these, over HTTPS on its own
    /// port.
    fn of(url: &Url) -> Option<Self> {
        if url.scheme() != "https" || url.port().is_some() {
            return None;
        }
        Self::named(url.host_str()?)
    }

    fn base(self) -> String {
        format!("https://{}/api/v1", self.name())
    }
}

/// The most redirects followed for one request.
const REDIRECTS: usize = 5;
/// The most of any answer read: well past a subtitle file, and further than
/// anything that is one.
const MOST_READ: u64 = 16 * 1024 * 1024;

/// One request to the API.
struct Ask<'a> {
    method: &'static str,
    path: &'a str,
    /// In alphabetical order, as their API asks of a query string.
    query: &'a [(&'a str, String)],
    body: Option<&'a [u8]>,
    /// Sent as this account, on its host, with its token.
    account: Option<&'a Signed>,
}

/// What came back: the status, and the body read as JSON, or `Null`.
struct Reply {
    status: u16,
    body: Value,
}

/// Send `ask`, once more after a second if OpenSubtitles' end failed, as
/// they ask clients to. Not for a sign-in, which is never sent twice.
fn send_again_once(ask: &Ask) -> Result<Reply, Failure> {
    let reply = send(ask)?;
    if reply.status < 500 || said_invalid(&reply.body) {
        return Ok(reply);
    }
    std::thread::sleep(Duration::from_secs(1));
    send(ask)
}

fn send(ask: &Ask) -> Result<Reply, Failure> {
    send_within(ask, TIMEOUT)
}

/// Send `ask` to the API with the player's key, and as the account it names,
/// waiting `timeout` at most for each step.
///
/// Redirects are followed here rather than by the client, and only to the
/// API's own hosts over HTTPS: the client would follow one anywhere, the key
/// with it, and down to plain HTTP. Only a GET is followed at all, so a body,
/// which may be a password, is never sent twice or anywhere new.
fn send_within(ask: &Ask, timeout: Duration) -> Result<Reply, Failure> {
    let key = key().ok_or(Failure::Refused)?;
    let host = ask.account.map_or(Host::Api, |signed| signed.host);
    let mut url =
        Url::parse(&format!("{}{}", host.base(), ask.path)).map_err(|_| Failure::Unreachable)?;
    if !ask.query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in ask.query {
            pairs.append_pair(name, &value.to_lowercase());
        }
    }
    let bearer = ask
        .account
        .map(|signed| Zeroizing::new(format!("Bearer {}", signed.token.as_str())));
    let agent = agent(timeout).redirects(0).build();
    for _ in 0..=REDIRECTS {
        let mut request = agent
            .request_url(ask.method, &url)
            .set("Api-Key", key)
            .set("Accept", "application/json");
        if let Some(bearer) = &bearer {
            request = request.set("Authorization", bearer);
        }
        let result = match ask.body {
            Some(body) => request
                .set("Content-Type", "application/json")
                .send_bytes(body),
            None => request.call(),
        };
        let response = match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(_)) => return Err(Failure::Unreachable),
        };
        let status = response.status();
        if !(300..400).contains(&status) {
            return Ok(Reply {
                status,
                body: read_json(response),
            });
        }
        url = redirected(&url, response.header("location"), ask.method)
            .ok_or(Failure::Unreachable)?;
    }
    Err(Failure::Unreachable)
}

/// Where a redirect from `from` leads, if it is one to follow.
fn redirected(from: &Url, location: Option<&str>, method: &str) -> Option<Url> {
    let to = from.join(location?).ok()?;
    (method == "GET" && Host::of(&to).is_some()).then_some(to)
}

/// Fetch a subtitle file from the link the API gave, with neither the key nor
/// a token, which it doesn't need. Redirects are followed anywhere, since the
/// file is on hosts of its own, but only over HTTPS, so what arrives is what
/// was sent.
fn fetch_file(mut url: Url) -> Result<Vec<u8>, Failure> {
    let agent = agent(TIMEOUT).redirects(0).build();
    for _ in 0..=REDIRECTS {
        let response = match agent.request_url("GET", &url).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(410, _)) => return Err(Failure::Unreachable),
            Err(ureq::Error::Status(_, response)) => {
                return Err(said(&read_json(response)).unwrap_or(Failure::Unreachable))
            }
            Err(ureq::Error::Transport(_)) => return Err(Failure::Unreachable),
        };
        if !(300..400).contains(&response.status()) {
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(MOST_READ)
                .read_to_end(&mut bytes)
                .map_err(|_| Failure::Unreachable)?;
            return Ok(bytes);
        }
        url = response
            .header("location")
            .and_then(|to| url.join(to).ok())
            .filter(|to| to.scheme() == "https")
            .ok_or(Failure::Unreachable)?;
    }
    Err(Failure::Unreachable)
}

fn agent(timeout: Duration) -> ureq::AgentBuilder {
    let builder = ureq::AgentBuilder::new()
        .timeout(timeout)
        .user_agent(&format!("Death by MPV v{}", env!("CARGO_PKG_VERSION")));
    // Windows' own TLS, which the client has to be handed (see Cargo.toml).
    #[cfg(windows)]
    let builder = match native_tls::TlsConnector::new() {
        Ok(tls) => builder.tls_connector(std::sync::Arc::new(tls)),
        Err(_) => builder,
    };
    builder
}

fn read_json(response: ureq::Response) -> Value {
    serde_json::from_reader(response.into_reader().take(MOST_READ)).unwrap_or(Value::Null)
}

/// What a refused request means, by its status and what it said, for an ask
/// made as an account when `signed_in`.
fn failure(status: u16, body: &Value, signed_in: bool) -> Failure {
    match status {
        401 if signed_in => Failure::Lapsed,
        401 | 403 => Failure::Refused,
        // Download quota and a token it won't take share the status, and
        // only the answer tells them apart: a quota comes with a count.
        406 if body.get("remaining").is_some() || body.get("reset_time_utc").is_some() => {
            Failure::Spent { signed_in }
        }
        406 | 500.. if signed_in && said_invalid(body) => Failure::Lapsed,
        429 => Failure::Busy,
        500.. => Failure::Unreachable,
        _ => said(body).unwrap_or(Failure::Unreachable),
    }
}

/// Whether the answer says the token is invalid, which is how OpenSubtitles
/// reports one past its time: *invalid token*, or *invalid* alone.
fn said_invalid(body: &Value) -> bool {
    body.get("message")
        .and_then(Value::as_str)
        .map(|message| message.trim().to_lowercase())
        .is_some_and(|message| {
            message == "invalid" || (message.contains("invalid") && message.contains("token"))
        })
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

/// `DBM_FIND_FAKE`: answer from here instead, so the flow can be driven
/// without a key, a connection or an account. Searches find four subtitles,
/// and downloads are a short one written on the spot. `DBM_FIND_FAKE=spent`
/// starts with the key's own downloads used up, so a download asks for an
/// account; the one account there is is `fake`, with the password `fake`.
mod fake {
    use std::sync::atomic::{AtomicI64, Ordering};

    use zeroize::Zeroizing;

    use super::account::Signed;
    use super::{Failure, Hit};

    /// The account's downloads left today.
    static LEFT: AtomicI64 = AtomicI64::new(20);

    pub fn on() -> bool {
        std::env::var_os("DBM_FIND_FAKE").is_some()
    }

    fn spent() -> bool {
        std::env::var_os("DBM_FIND_FAKE").is_some_and(|v| v == "spent")
    }

    pub fn hits() -> Vec<Hit> {
        let hit = |file_id, language: &str, release: &str, downloads, exact| Hit {
            file_id,
            language: language.into(),
            release: release.into(),
            downloads,
            exact,
            hearing_impaired: file_id == 3,
            machine: file_id == 4,
        };
        vec![
            hit(1, "en", "Show.S01E01.1080p.WEB-DL.x265", 48211, true),
            hit(2, "sv", "Show.S01E01.1080p.WEB-DL.x265", 9120, true),
            hit(3, "en", "Show.S01E01.720p.HDTV.x264", 15002, false),
            hit(4, "sv", "Show.S01E01.720p.HDTV.x264", 870, false),
        ]
    }

    pub fn download(signed: Option<&Signed>) -> Result<Vec<u8>, Failure> {
        match signed {
            None if spent() => return Err(Failure::Spent { signed_in: false }),
            None => {}
            Some(signed) => {
                let left = LEFT.fetch_sub(1, Ordering::SeqCst) - 1;
                if left < 0 {
                    return Err(Failure::Spent { signed_in: true });
                }
                super::account::left(signed, left);
            }
        }
        Ok(b"1\n00:00:01,000 --> 00:00:30,000\nFetched from nowhere, for the harness.\n".to_vec())
    }

    pub fn log_in(name: &str, password: &str) -> Result<Zeroizing<String>, Failure> {
        if name == "fake" && password == "fake" {
            Ok(Zeroizing::new("fake-token".into()))
        } else {
            Err(Failure::WrongPassword)
        }
    }
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
    fn requests_go_only_to_the_api_hosts_over_https() {
        let url = |u: &str| Url::parse(u).unwrap();
        assert_eq!(
            Host::of(&url("https://api.opensubtitles.com/api/v1/x")),
            Some(Host::Api)
        );
        assert_eq!(
            Host::of(&url("https://VIP-API.opensubtitles.com/")),
            Some(Host::Vip)
        );
        for other in [
            "http://api.opensubtitles.com/",
            "https://api.opensubtitles.com:8443/",
            "https://evil.example/",
            "https://api.opensubtitles.com.evil.example/",
            "https://api.opensubtitles.com@evil.example/",
        ] {
            assert_eq!(Host::of(&url(other)), None, "{other}");
        }
    }

    #[test]
    fn only_a_get_follows_a_redirect_and_only_within_the_api() {
        let from = Url::parse("https://api.opensubtitles.com/api/v1/subtitles?a=1").unwrap();
        let to = |location, method| redirected(&from, Some(location), method);
        assert_eq!(
            to("/api/v1/subtitles?a=1&b=2", "GET").map(String::from),
            Some("https://api.opensubtitles.com/api/v1/subtitles?a=1&b=2".into())
        );
        assert!(to("https://vip-api.opensubtitles.com/api/v1/x", "GET").is_some());
        for elsewhere in [
            "http://api.opensubtitles.com/api/v1/x",
            "https://evil.example/",
            "//evil.example/x",
            "https://api.opensubtitles.com:444/",
        ] {
            assert!(to(elsewhere, "GET").is_none(), "{elsewhere}");
        }
        assert!(
            to("/api/v1/login", "POST").is_none(),
            "a body is never resent"
        );
        assert!(redirected(&from, None, "GET").is_none());
    }

    #[test]
    fn a_used_up_quota_is_told_from_the_other_refusals() {
        let quota = serde_json::json!({
            "requests": 21, "remaining": -1,
            "message": "You have downloaded your allowed 20 subtitles for 24h.",
            "reset_time_utc": "2022-01-30T06:00:53.000Z",
        });
        assert_eq!(
            failure(406, &quota, false),
            Failure::Spent { signed_in: false }
        );
        assert_eq!(
            failure(406, &quota, true),
            Failure::Spent { signed_in: true }
        );
        let token = serde_json::json!({ "message": "invalid token" });
        assert_eq!(failure(406, &token, true), Failure::Lapsed);
        assert_eq!(
            failure(500, &serde_json::json!({ "message": "invalid" }), true),
            Failure::Lapsed
        );
        let file = serde_json::json!({ "message": "Invalid file_id", "status": 406 });
        assert_eq!(
            failure(406, &file, true),
            Failure::Said("Invalid file_id".into())
        );
        let throttled = serde_json::json!({ "message": "Throttle limit reached. Retry later." });
        assert_eq!(
            failure(429, &throttled, false),
            Failure::Busy,
            "not a spent quota"
        );
        assert_eq!(failure(401, &Value::Null, true), Failure::Lapsed);
        assert_eq!(failure(401, &Value::Null, false), Failure::Refused);
        assert_eq!(failure(403, &Value::Null, true), Failure::Refused);
        assert_eq!(failure(503, &Value::Null, true), Failure::Unreachable);
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
