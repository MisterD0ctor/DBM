//! `DBM_FIND_FAKE`: answer from here instead, so the flow can be driven
//! without a key, a connection or an account. Searches find four subtitles,
//! and downloads are a short one written on the spot. `DBM_FIND_FAKE=spent`
//! starts with the key's own downloads used up, so a download asks for an
//! account; the one account there is is `fake`, with the password `fake`.
//!
//! Only in a build with the `harness` feature. Without it `on` is `false`
//! before the variable is read, and the rest is never reached, so a release
//! carries none of it, while it is still checked in every build.

use std::sync::atomic::{AtomicI64, Ordering};

use zeroize::Zeroizing;

use super::account::Signed;
use super::{Failure, Hit};

/// The account's downloads left today.
static LEFT: AtomicI64 = AtomicI64::new(20);

pub fn on() -> bool {
    cfg!(feature = "harness") && std::env::var_os("DBM_FIND_FAKE").is_some()
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
