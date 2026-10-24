//! Signing in to OpenSubtitles, for the downloads past the key's own.
//!
//! OpenSubtitles' terms for integrators ask for exactly this: a free key's
//! five downloads a day without an account are there so the player works
//! straight after it is installed, and past them the person signs in, so
//! that downloads count against their own account. So nothing asks for an
//! account until a download is refused for want of one.
//!
//! The account is the person's, and the player holds as little of it as it
//! can, for as short a time as it can:
//!
//! - **The password** goes out once, in the body of the sign-in, and the
//!   player's own copies are overwritten as soon as it has gone. It is never
//!   written down: not to disk, not to a log, not to the trace. What Slint
//!   holds of it while it is typed goes when the page does (see
//!   `AccountPage`), which is as far as the player can reach into the
//!   toolkit.
//! - **The token** that comes back lives in memory for this session only,
//!   and is handed back when the person signs out or the player quits, as
//!   OpenSubtitles asks. It runs out by itself a day after it was made.
//! - **Where they go:** only to the API's own two hosts (see [`Host`]), only
//!   over HTTPS, and never through a redirect anywhere else (see `send`). A
//!   sign-in names the host the account uses from then on, and an answer
//!   naming any other is refused rather than followed.
//! - **How often:** sign-ins are held to OpenSubtitles' own limits of one a
//!   second, ten a minute and thirty an hour (see [`may_send`]), and none is
//!   sent but the one the person asked for.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::Engine;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{fake, send, Ask, Failure, Host};
use crate::worker::{Completion, Reporter};

/// How long a token lasts, by OpenSubtitles' word, and the most the player
/// keeps one for whatever it claims.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// How long signing out may hold up quitting. Past it the token runs out on
/// its own schedule instead.
const QUITTING: Duration = Duration::from_secs(3);

/// Who is signed in, as the interface shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    /// What was typed to sign in.
    pub name: String,
    /// Downloads left today, where OpenSubtitles has said.
    pub left: Option<i64>,
}

/// The signed-in account, for one request: where to send it, and the token
/// to send. Overwritten when the request is done with it.
pub(super) struct Signed {
    pub host: Host,
    pub token: Zeroizing<String>,
    /// Which sign-in this came from, so an answer to a request made under one
    /// can't end another made since.
    session: u64,
}

struct Session {
    account: Account,
    host: Host,
    token: Zeroizing<String>,
    expires: SystemTime,
    id: u64,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// The last session's id, counting up, so no two share one.
static SESSIONS: Mutex<u64> = Mutex::new(0);
/// When sign-ins went out, oldest first, back an hour (see [`may_send`]).
static SENT: Mutex<VecDeque<Instant>> = Mutex::new(VecDeque::new());

/// Who is signed in, or nobody. A token past its time is let go here.
pub fn current() -> Option<Account> {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if session
        .as_ref()
        .is_some_and(|s| s.expires <= SystemTime::now())
    {
        *session = None;
    }
    session.as_ref().map(|s| s.account.clone())
}

/// The signed-in account for a request, if there is one still in its time.
pub(super) fn signed() -> Option<Signed> {
    current()?;
    let session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    session.as_ref().map(|s| Signed {
        host: s.host,
        token: s.token.clone(),
        session: s.id,
    })
}

/// OpenSubtitles no longer takes the token a request was sent with, so the
/// person is signed out.
pub(super) fn lapse(signed: &Signed) {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if session.as_ref().is_some_and(|s| s.id == signed.session) {
        *session = None;
    }
}

/// A download said how many the account has left.
pub(super) fn left(signed: &Signed, left: i64) {
    let mut session = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = session.as_mut().filter(|s| s.id == signed.session) {
        s.account.left = Some(left);
    }
}

/// Sign in as `name`, on a thread of its own, and say how it went.
pub fn sign_in(name: String, password: Zeroizing<String>, reporter: Reporter) {
    std::thread::spawn(move || {
        let result = log_in(&name, &password);
        drop(password);
        reporter.send(Completion::SignedIn(result));
    });
}

/// Sign out, handing the token back on a thread of its own. The person is
/// signed out at once, whether or not it arrives.
pub fn sign_out() {
    if let Some(session) = take() {
        std::thread::spawn(move || log_out(session, super::TIMEOUT));
    }
}

/// Sign out as the player quits, waiting [`QUITTING`] at most.
pub fn sign_out_before_quitting() {
    if let Some(session) = take() {
        log_out(session, QUITTING);
    }
}

fn take() -> Option<Session> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).take()
}

fn log_in(name: &str, password: &str) -> Result<Account, Failure> {
    let allowed = may_send(
        &mut SENT.lock().unwrap_or_else(|e| e.into_inner()),
        Instant::now(),
    );
    if !allowed {
        return Err(Failure::Busy);
    }
    if fake::on() {
        return fake::log_in(name, password).map(|token| keep(name, Host::Api, token, None));
    }
    let body = credentials(name, password);
    let reply = send(&Ask {
        method: "POST",
        path: "/login",
        query: &[],
        body: Some(&body),
        account: None,
    })?;
    drop(body);
    match reply.status {
        200..=299 => {}
        401 => return Err(Failure::WrongPassword),
        403 => return Err(Failure::Refused),
        429 => return Err(Failure::Busy),
        _ => return Err(super::said(&reply.body).unwrap_or(Failure::Unreachable)),
    }
    let (host, token) = granted(reply.body)?;
    let Some(host) = host else {
        // Signed in, but told to use a host the player won't send the token
        // to. Handed back to the one it came from, so it isn't left alive.
        let refused = Signed {
            host: Host::Api,
            token,
            session: 0,
        };
        std::thread::spawn(move || hand_back(&refused, super::TIMEOUT));
        return Err(Failure::Unreachable);
    };
    let fresh = Signed {
        host,
        token: token.clone(),
        session: 0,
    };
    let left = downloads_left(&fresh);
    drop(fresh);
    Ok(keep(name, host, token, left))
}

/// Keep a new session, in place of any before it.
fn keep(name: &str, host: Host, token: Zeroizing<String>, left: Option<i64>) -> Account {
    let now = SystemTime::now();
    let id = {
        let mut last = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
        *last += 1;
        *last
    };
    let account = Account {
        name: name.to_string(),
        left,
    };
    let session = Session {
        account: account.clone(),
        host,
        expires: expiry(&token, now),
        token,
        id,
    };
    let before = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(session);
    if let Some(before) = before {
        std::thread::spawn(move || log_out(before, super::TIMEOUT));
    }
    account
}

/// The sign-in's body, written straight into one buffer that is overwritten
/// when it goes. Sized up front for the longest the two could escape to, six
/// bytes for each, so it never grows and leaves a copy behind in memory it
/// gave up.
fn credentials(name: &str, password: &str) -> Zeroizing<Vec<u8>> {
    let mut body = Zeroizing::new(Vec::with_capacity(32 + 6 * (name.len() + password.len())));
    body.extend_from_slice(b"{\"username\":");
    // Writing a string into a Vec can't fail.
    let _ = serde_json::to_writer(&mut *body, name);
    body.extend_from_slice(b",\"password\":");
    let _ = serde_json::to_writer(&mut *body, password);
    body.push(b'}');
    body
}

/// The token a sign-in granted, and the host it is to be used on, or `None`
/// for one the player doesn't send it to. The token is moved out of the
/// answer before anything else is read, so whatever comes of it, the only
/// copy is one that is overwritten.
fn granted(mut body: Value) -> Result<(Option<Host>, Zeroizing<String>), Failure> {
    let token = match body.get_mut("token").map(Value::take) {
        Some(Value::String(token)) => Zeroizing::new(token),
        _ => return Err(Failure::Unreachable),
    };
    if token.is_empty() {
        return Err(Failure::Unreachable);
    }
    // Absent, it is the default the API documents.
    let host = match body.get("base_url").and_then(Value::as_str) {
        None => Some(Host::Api),
        Some(name) => Host::named(name),
    };
    Ok((host, token))
}

/// How many downloads the account has left today, if it says.
fn downloads_left(signed: &Signed) -> Option<i64> {
    let reply = send(&Ask {
        method: "GET",
        path: "/infos/user",
        query: &[],
        body: None,
        account: Some(signed),
    })
    .ok()?;
    if !(200..300).contains(&reply.status) {
        return None;
    }
    reply.body.get("data")?.get("remaining_downloads")?.as_i64()
}

fn log_out(session: Session, timeout: Duration) {
    let signed = Signed {
        host: session.host,
        token: session.token,
        session: session.id,
    };
    hand_back(&signed, timeout);
}

/// End the session `signed` belongs to on OpenSubtitles' side. Nothing is
/// done with the answer: the player has let the token go either way.
fn hand_back(signed: &Signed, timeout: Duration) {
    if fake::on() {
        return;
    }
    let _ = super::send_within(
        &Ask {
            method: "DELETE",
            path: "/logout",
            query: &[],
            body: None,
            account: Some(signed),
        },
        timeout,
    );
}

/// When a token runs out: the time it claims, but never more than [`DAY`]
/// from `now`, which is what OpenSubtitles says one lasts. Read without
/// checking the signature, which only the server can, since this is only
/// when to stop sending it, not whether to believe it.
fn expiry(token: &str, now: SystemTime) -> SystemTime {
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let latest = now + DAY;
    token
        .split('.')
        .nth(1)
        .and_then(|claims| LENIENT.decode(claims).ok())
        .and_then(|claims| serde_json::from_slice::<Value>(&claims).ok())
        .and_then(|claims| claims.get("exp")?.as_u64())
        .map(|exp| UNIX_EPOCH + Duration::from_secs(exp))
        .map_or(latest, |claimed| claimed.min(latest))
}

/// Whether a sign-in may go out at `now`, kept to OpenSubtitles' limits: one
/// a second, ten a minute and thirty an hour. Counted here as well as there,
/// so the person is told to wait rather than the API being told the same
/// thing over and over. A sign-in that may go is counted.
fn may_send(sent: &mut VecDeque<Instant>, now: Instant) -> bool {
    const SECOND: Duration = Duration::from_secs(1);
    const MINUTE: Duration = Duration::from_secs(60);
    const HOUR: Duration = Duration::from_secs(60 * 60);

    while sent
        .front()
        .is_some_and(|at| now.duration_since(*at) >= HOUR)
    {
        sent.pop_front();
    }
    let within = |span: Duration| {
        sent.iter()
            .filter(|at| now.duration_since(**at) < span)
            .count()
    };
    if within(SECOND) >= 1 || within(MINUTE) >= 10 || sent.len() >= 30 {
        return false;
    }
    sent.push_back(now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credentials_are_json_written_without_the_buffer_growing() {
        let name = "kasper \"k\"";
        let password = "p\u{1}ss\\word";
        let body = credentials(name, password);
        let capacity = 32 + 6 * (name.len() + password.len());
        assert_eq!(body.capacity(), capacity, "it never grew");
        let read: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(read["username"], name);
        assert_eq!(read["password"], password);
    }

    #[test]
    fn the_worst_escaping_still_fits() {
        // Every byte a control character, each written as \u00XX.
        let password = "\u{1}".repeat(64);
        let body = credentials("", &password);
        assert_eq!(body.capacity(), 32 + 6 * 64);
    }

    #[test]
    fn a_sign_in_names_only_the_two_api_hosts() {
        let answer = |base: &str| serde_json::json!({ "token": "t", "base_url": base });
        let host = |base: &str| granted(answer(base)).unwrap().0;
        assert_eq!(host("api.opensubtitles.com"), Some(Host::Api));
        assert_eq!(host("vip-api.opensubtitles.com"), Some(Host::Vip));
        for other in [
            "evil.example",
            "api.opensubtitles.com.evil.example",
            "http://api.opensubtitles.com",
            "api.opensubtitles.com:8080",
            "",
        ] {
            assert_eq!(host(other), None, "{other} is refused");
        }
        let unnamed = serde_json::json!({ "token": "t" });
        assert_eq!(granted(unnamed).unwrap().0, Some(Host::Api), "the default");
    }

    #[test]
    fn a_sign_in_without_a_token_is_no_sign_in() {
        assert!(granted(serde_json::json!({ "base_url": "api.opensubtitles.com" })).is_err());
        assert!(granted(serde_json::json!({ "token": "" })).is_err());
        assert!(granted(serde_json::json!({ "token": 42 })).is_err());
    }

    /// A token as OpenSubtitles makes them, claiming `claims`.
    fn token(claims: &str) -> String {
        let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "{}.{}.signature",
            engine.encode(r#"{"typ":"JWT","alg":"HS256"}"#),
            engine.encode(claims)
        )
    }

    #[test]
    fn a_token_runs_out_when_it_says() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let token = token(r#"{"iss":"x","exp":1003600}"#);
        assert_eq!(
            expiry(&token, now),
            UNIX_EPOCH + Duration::from_secs(1_003_600)
        );
    }

    #[test]
    fn a_token_is_kept_a_day_at_most() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let far = token(r#"{"exp":9999999999}"#);
        assert_eq!(expiry(&far, now), now + DAY);
        for unreadable in ["", "no-dots", "a.!!!.c", &token("not json"), &token("{}")] {
            assert_eq!(expiry(unreadable, now), now + DAY, "{unreadable:?}");
        }
    }

    #[test]
    fn sign_ins_are_held_to_a_second_apart() {
        let start = Instant::now();
        let mut sent = VecDeque::new();
        assert!(may_send(&mut sent, start));
        assert!(!may_send(&mut sent, start + Duration::from_millis(500)));
        assert!(may_send(&mut sent, start + Duration::from_secs(1)));
    }

    #[test]
    fn sign_ins_are_held_to_ten_a_minute_and_thirty_an_hour() {
        let start = Instant::now();
        let mut sent = VecDeque::new();
        let at = |s: u64| start + Duration::from_secs(s);
        for s in 0..10 {
            assert!(may_send(&mut sent, at(s * 2)), "the {s}th");
        }
        assert!(!may_send(&mut sent, at(30)), "the eleventh in a minute");
        assert!(may_send(&mut sent, at(61)), "a minute on");
        // The rest of the hour's thirty, seven seconds apart, which no
        // minute's ten stops.
        let mut s = 61;
        while sent.len() < 30 {
            s += 7;
            assert!(may_send(&mut sent, at(s)));
        }
        assert!(
            !may_send(&mut sent, at(s + 61)),
            "the thirty-first in an hour"
        );
        assert!(
            may_send(&mut sent, at(3600 + 61)),
            "once the first is an hour old"
        );
    }
}
