//! Downloading: asking for a subtitle's link, fetching it, and keeping it
//! in the player's own folder.

use std::path::{Path, PathBuf};

use dbm_mpv::MediaPath;
use dbm_naming::Language;
use serde_json::Value;
use ureq::http::Method;
use url::Url;

use super::account::{self, Signed};
use super::http::{Ask, failure, fetch_file, said, send_again_once};
use super::{Failure, Hit, KEPT, fake};

pub(super) fn download(path: &MediaPath, hit: &Hit) -> Result<(PathBuf, Language), Failure> {
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
    // Whole or not at all, since a file found here is never fetched again.
    crate::paths::write_atomic(&file, bytes).map_err(|e| Failure::Said(e.to_string()))?;
    remember(path, &file);
    Ok((file, hit.language.clone()))
}

/// Where a download for `path` is kept: in the player's own folder, named
/// after the film, the language and the subtitle's own id, so the same one
/// asked for twice is the same file.
fn keep_as(path: &MediaPath, hit: &Hit) -> PathBuf {
    let stem = path
        .as_path()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "subtitles".into());
    // The language is the service's word, so it is filtered too, and loses
    // its dots, since `kept` reads the code back from between two.
    let language = safe(hit.language.tag()).replace('.', "_");
    crate::paths::app_data_dir().join("subtitles").join(format!(
        "{}.{language}.{}.srt",
        safe(&stem),
        hit.file_id
    ))
}

/// `text` with anything that can't go in a file name on every system made
/// an underscore.
fn safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || " .-_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Where to fetch `hit` from, which costs one of the day's downloads: the
/// key's own, or the account's when `signed` is.
fn link(hit: &Hit, signed: Option<&Signed>) -> Result<Url, Failure> {
    let body = serde_json::to_vec(&serde_json::json!({ "file_id": hit.file_id }))
        .map_err(|_| Failure::Unreachable)?;
    let reply = send_again_once(&Ask {
        method: Method::POST,
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

fn remember(path: &MediaPath, file: &Path) {
    let file = file.to_string_lossy().into_owned();
    KEPT.edit(|kept| {
        let files = kept.entry(path.clone()).or_default();
        let new = !files.contains(&file);
        if new {
            files.push(file);
        }
        new
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_file_says_its_language_in_its_name() {
        let hit = Hit {
            file_id: 42,
            language: Language::parse("pt-br").expect("a language"),
            release: String::new(),
            downloads: 0,
            exact: false,
            hearing_impaired: false,
            machine: false,
        };
        let file = keep_as(&"/films/Some Film (2019).mkv".into(), &hit);
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, "Some Film (2019).pt-br.42.srt");
        let stem = file.file_stem().unwrap();
        let code = Path::new(Path::new(stem).file_stem().unwrap())
            .extension()
            .unwrap();
        assert_eq!(code, "pt-br");

        // A code that would leave the folder, or move the dots, can't.
        let odd = Hit {
            language: Language::parse("../x.y").expect("a language"),
            ..hit
        };
        let file = keep_as(&"/films/Some Film (2019).mkv".into(), &odd);
        assert_eq!(
            file.file_name().unwrap().to_string_lossy(),
            "Some Film (2019).___x_y.42.srt"
        );
        assert_eq!(file.parent().unwrap().file_name().unwrap(), "subtitles");
    }
}
