//! Searching: what OpenSubtitles holds for a file, asked by its hash and by
//! its name.

use dbm_mpv::MediaPath;
use dbm_naming::{Language, Media};
use serde_json::Value;
use ureq::http::Method;

use super::account::{self, Signed};
use super::hash::hash;
use super::http::{Ask, failure, send_again_once};
use super::{Failure, Hit, fake};

/// The most results shown. Past a screenful the list is releases nobody has.
const MOST: usize = 40;

pub(super) fn look(path: &MediaPath, languages: &[Language]) -> Result<Vec<Hit>, Failure> {
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
fn look_as(
    path: &MediaPath,
    languages: &[Language],
    signed: Option<&Signed>,
) -> Result<Vec<Hit>, Failure> {
    let ask = |query: &[(&str, String)]| ask(query, signed);
    let languages = languages
        .iter()
        .map(Language::tag)
        .collect::<Vec<_>>()
        .join(",");
    let mut hits: Vec<Hit> = Vec::new();
    // By hash first: those are the ones timed for this very file, and they
    // stay at the top of the list whatever their download counts.
    if let Some(hash) = path
        .as_path()
        .is_file()
        .then(|| hash(path.as_path()))
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
fn by_name(path: &MediaPath, languages: &str) -> Vec<(&'static str, String)> {
    let mut query = vec![("languages", languages.to_string())];
    match dbm_naming::parse(path.as_str()) {
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
        method: Method::GET,
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
            let language = Language::parse(text("language"))?;
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
                language,
                release,
                downloads: a.get("download_count").and_then(Value::as_i64).unwrap_or(0),
                exact: flag("moviehash_match"),
                hearing_impaired: flag("hearing_impaired"),
                machine: flag("machine_translated") || flag("ai_translated"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_episode_is_asked_for_by_show_season_and_episode_in_order() {
        let query = by_name(&"/tv/The.Walking.Dead.S04E08.1080p.mkv".into(), "en,sv");
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
        let query = by_name(&"/films/Alien (1979).mkv".into(), "en");
        assert!(query.contains(&("query", "Alien".into())));
        assert!(query.contains(&("year", "1979".into())));
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
