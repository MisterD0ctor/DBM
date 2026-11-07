//! Talking to OpenSubtitles: where a request may go, with what, and what its
//! answer means.

use std::time::Duration;

use serde_json::Value;
use ureq::http::{Method, Request, Response};
use ureq::{Agent, Body};
use url::Url;
use zeroize::Zeroizing;

use super::account::Signed;
use super::{Failure, key};

/// Long enough for a slow connection, short enough that a dead one says so
/// while the person is still looking at the row that asked.
pub(super) const TIMEOUT: Duration = Duration::from_secs(15);

/// The hosts the API answers on: everything but the subtitle files
/// themselves goes to one of these, and nothing with the key or a token in it
/// goes anywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Host {
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
    pub(super) fn named(name: &str) -> Option<Self> {
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
pub(super) struct Ask<'a> {
    pub(super) method: Method,
    pub(super) path: &'a str,
    /// In alphabetical order, as their API asks of a query string.
    pub(super) query: &'a [(&'a str, String)],
    pub(super) body: Option<&'a [u8]>,
    /// Sent as this account, on its host, with its token.
    pub(super) account: Option<&'a Signed>,
}

/// What came back: the status, and the body read as JSON, or `Null`.
pub(super) struct Reply {
    pub(super) status: u16,
    pub(super) body: Value,
}

/// Send `ask`, once more after a second if OpenSubtitles' end failed, as
/// they ask clients to. Not for a sign-in, which is never sent twice.
pub(super) fn send_again_once(ask: &Ask) -> Result<Reply, Failure> {
    let reply = send(ask)?;
    if reply.status < 500 || said_invalid(&reply.body) {
        return Ok(reply);
    }
    std::thread::sleep(Duration::from_secs(1));
    send(ask)
}

pub(super) fn send(ask: &Ask) -> Result<Reply, Failure> {
    send_within(ask, TIMEOUT)
}

/// Send `ask` to the API with the player's key, and as the account it names,
/// waiting `timeout` at most for each step.
///
/// Redirects are followed here rather than by the client, and only to the
/// API's own hosts over HTTPS: the client would follow one anywhere, the key
/// with it, and down to plain HTTP. Only a GET is followed at all, so a body,
/// which may be a password, is never sent twice or anywhere new.
pub(super) fn send_within(ask: &Ask, timeout: Duration) -> Result<Reply, Failure> {
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
    let agent = agent(timeout);
    for _ in 0..=REDIRECTS {
        let mut request = Request::builder()
            .method(ask.method.clone())
            .uri(url.as_str())
            .header("Api-Key", key)
            .header("Accept", "application/json");
        if let Some(bearer) = &bearer {
            request = request.header("Authorization", bearer.as_str());
        }
        let sent = match ask.body {
            Some(body) => request
                .header("Content-Type", "application/json")
                .body(body)
                .map(|request| agent.run(request)),
            None => request.body(()).map(|request| agent.run(request)),
        };
        // Either the request couldn't be put together or it got no answer.
        let Ok(Ok(mut response)) = sent else {
            return Err(Failure::Unreachable);
        };
        let status = response.status().as_u16();
        if !(300..400).contains(&status) {
            return Ok(Reply {
                status,
                body: read_json(&mut response),
            });
        }
        url = redirected(&url, location(&response), &ask.method).ok_or(Failure::Unreachable)?;
    }
    Err(Failure::Unreachable)
}

/// Where a redirect from `from` leads, if it is one to follow.
fn redirected(from: &Url, location: Option<&str>, method: &Method) -> Option<Url> {
    let to = from.join(location?).ok()?;
    (*method == Method::GET && Host::of(&to).is_some()).then_some(to)
}

/// Where a redirect says to go.
fn location(response: &Response<Body>) -> Option<&str> {
    response.headers().get("location")?.to_str().ok()
}

/// Fetch a subtitle file from the link the API gave, with neither the key nor
/// a token, which it doesn't need. Redirects are followed anywhere, since the
/// file is on hosts of its own, but only over HTTPS, so what arrives is what
/// was sent.
pub(super) fn fetch_file(mut url: Url) -> Result<Vec<u8>, Failure> {
    let agent = agent(TIMEOUT);
    for _ in 0..=REDIRECTS {
        let mut response = agent
            .get(url.as_str())
            .call()
            .map_err(|_| Failure::Unreachable)?;
        match response.status().as_u16() {
            300..400 => {}
            410 => return Err(Failure::Unreachable),
            400.. => return Err(said(&read_json(&mut response)).unwrap_or(Failure::Unreachable)),
            _ => {
                return response
                    .body_mut()
                    .with_config()
                    .limit(MOST_READ)
                    .read_to_vec()
                    .map_err(|_| Failure::Unreachable);
            }
        }
        url = location(&response)
            .and_then(|to| url.join(to).ok())
            .ok_or(Failure::Unreachable)?;
    }
    Err(Failure::Unreachable)
}

/// A client that follows no redirect itself and refuses plain HTTP, giving
/// up on a request after `timeout`. Every status comes back as an answer, so
/// a refusal's body can still say why.
///
/// No proxy from the environment, which the client would otherwise take: one
/// it can't speak, such as SOCKS, would stop every search.
fn agent(timeout: Duration) -> Agent {
    let config = Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .https_only(true)
        .http_status_as_error(false)
        .proxy(None)
        .user_agent(format!("DBM v{}", env!("CARGO_PKG_VERSION")));
    // Windows' own TLS and its own certificates (see Cargo.toml).
    #[cfg(windows)]
    let config = config.tls_config(
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build(),
    );
    config.build().new_agent()
}

fn read_json(response: &mut Response<Body>) -> Value {
    response
        .body_mut()
        .with_config()
        .limit(MOST_READ)
        .read_json()
        .unwrap_or(Value::Null)
}

/// What a refused request means, by its status and what it said, for an ask
/// made as an account when `signed_in`.
pub(super) fn failure(status: u16, body: &Value, signed_in: bool) -> Failure {
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
pub(super) fn said(body: &Value) -> Option<Failure> {
    body.get("message")
        .or_else(|| body.get("errors").and_then(|e| e.get(0)))
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .map(|m| Failure::Said(m.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let to = |location, method| redirected(&from, Some(location), &method);
        assert_eq!(
            to("/api/v1/subtitles?a=1&b=2", Method::GET).map(String::from),
            Some("https://api.opensubtitles.com/api/v1/subtitles?a=1&b=2".into())
        );
        assert!(to("https://vip-api.opensubtitles.com/api/v1/x", Method::GET).is_some());
        for elsewhere in [
            "http://api.opensubtitles.com/api/v1/x",
            "https://evil.example/",
            "//evil.example/x",
            "https://api.opensubtitles.com:444/",
        ] {
            assert!(to(elsewhere, Method::GET).is_none(), "{elsewhere}");
        }
        assert!(
            to("/api/v1/login", Method::POST).is_none(),
            "a body is never resent"
        );
        assert!(redirected(&from, None, &Method::GET).is_none());
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
}
