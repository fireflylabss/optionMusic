//! Opt-in ListenBrainz scrobbling (`listenbrainz_token` in config.toml).
//!
//! Privacy first: **no network traffic happens unless the user configures a
//! non-empty `listenbrainz_token`** (empty is the default = off). Every public
//! entry point checks the token before touching the network, failures are
//! silent by design — scrobbling must never break playback — and only the
//! outcome counters in `stats.json` (`scrobbles_ok` / `scrobble_errors`,
//! shown by `msc stats`) record what happened.
//!
//! Endpoints (all `POST https://api.listenbrainz.org/1/submit-listens`):
//! - `playing_now` — sent when a track starts ("played").
//! - `single` — sent once a play counts (past ~50% / 60s, "completed").

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Submit endpoint for both `single` listens and `playing_now` notifications.
pub const SUBMIT_URL: &str = "https://api.listenbrainz.org/1/submit-listens";

/// Outcome of a submission attempt. `Disabled` / `Skipped` never touch the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No token configured — scrobbling is off.
    Disabled,
    /// Token set but artist/title unusable (blank) — nothing sent.
    Skipped,
    /// ListenBrainz answered `{"status":"ok",...}`.
    Sent,
    /// Network error, bad token, or a non-`ok` answer. Silent to the user.
    Failed,
}

/// True when the token opts in (non-empty after trimming).
pub fn enabled(token: &str) -> bool {
    !token.trim().is_empty()
}

/// Token from shared config, trimmed. Empty means scrobbling is off.
///
/// Never logged or printed — treat it like a password.
pub fn configured_token() -> String {
    crate::config::AppConfig::load()
        .listenbrainz_token
        .trim()
        .to_owned()
}

/// `Authorization: Token <token>` header value.
pub fn auth_header(token: &str) -> String {
    format!("Token {}", token.trim())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn clean(field: &str) -> Option<String> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[derive(Debug, Clone, Serialize)]
struct SubmitBody {
    listen_type: String,
    payload: Vec<Listen>,
}

#[derive(Debug, Clone, Serialize)]
struct Listen {
    #[serde(skip_serializing_if = "Option::is_none")]
    listened_at: Option<u64>,
    track_metadata: TrackMetadata,
}

#[derive(Debug, Clone, Serialize)]
struct TrackMetadata {
    artist_name: String,
    track_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_name: Option<String>,
}

/// Build a `single` (completed listen) payload. `None` when artist/title are blank.
fn single_payload(
    artist: &str,
    title: &str,
    album: Option<&str>,
    listened_at: u64,
) -> Option<SubmitBody> {
    let (artist_name, track_name) = (clean(artist)?, clean(title)?);
    Some(SubmitBody {
        listen_type: "single".into(),
        payload: vec![Listen {
            listened_at: Some(listened_at),
            track_metadata: TrackMetadata {
                artist_name,
                track_name,
                release_name: album.and_then(clean),
            },
        }],
    })
}

/// Build a `playing_now` payload. `None` when artist/title are blank.
fn playing_now_payload(artist: &str, title: &str, album: Option<&str>) -> Option<SubmitBody> {
    let (artist_name, track_name) = (clean(artist)?, clean(title)?);
    Some(SubmitBody {
        listen_type: "playing_now".into(),
        payload: vec![Listen {
            listened_at: None,
            track_metadata: TrackMetadata {
                artist_name,
                track_name,
                release_name: album.and_then(clean),
            },
        }],
    })
}

fn post(token: &str, body: &SubmitBody) -> Outcome {
    let Ok(json) = serde_json::to_string(body) else {
        return Outcome::Failed;
    };
    // Short timeouts: a hanging scrobble must not stall playback.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(4)))
        .timeout_global(Some(Duration::from_secs(8)))
        .user_agent(concat!("stillmusic/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let Ok(mut resp) = agent
        .post(SUBMIT_URL)
        .header("Authorization", auth_header(token))
        .header("Content-Type", "application/json")
        .send(json)
    else {
        return Outcome::Failed;
    };
    let Ok(text) = resp.body_mut().read_to_string() else {
        return Outcome::Failed;
    };
    // ListenBrainz answers `{"status":"ok",...}` on acceptance.
    let ok = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_owned))
        .is_some_and(|s| s.eq_ignore_ascii_case("ok"));
    if ok { Outcome::Sent } else { Outcome::Failed }
}

/// Submit a completed listen (`single`).
///
/// Returns `None` when scrobbling is disabled or the metadata is unusable
/// (no network, nothing to count in stats); otherwise `Some(sent)`.
pub fn notify_listen(artist: &str, title: &str, album: Option<&str>) -> Option<bool> {
    if cfg!(test) {
        return None;
    }
    let token = configured_token();
    if !enabled(&token) {
        return None;
    }
    let body = single_payload(artist, title, album, now_unix())?;
    Some(matches!(post(&token, &body), Outcome::Sent))
}

/// Fire a `playing_now` notification. Best-effort and silent: no token (the
/// default) means returning immediately without any network.
pub fn notify_now_playing(artist: &str, title: &str, album: Option<&str>) {
    if cfg!(test) {
        return;
    }
    let token = configured_token();
    if !enabled(&token) {
        return;
    }
    let Some(body) = playing_now_payload(artist, title, album) else {
        return;
    };
    let _ = post(&token, &body);
}

/// `playing_now` from a file path (history hook): embedded tags when present,
/// filename stem as the title fallback. Skips silently without an artist.
pub fn notify_now_playing_for_path(path: &Path) {
    if cfg!(test) {
        return;
    }
    let token = configured_token();
    if !enabled(&token) {
        return;
    }
    let tags = crate::meta::read_tags(path);
    let title = tags
        .title
        .as_deref()
        .and_then(clean)
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).and_then(clean))
        .unwrap_or_default();
    let artist = tags.artist.as_deref().and_then(clean).unwrap_or_default();
    if artist.is_empty() || title.is_empty() {
        return;
    }
    notify_now_playing(&artist, &title, tags.album.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_token() {
        assert!(!enabled(""));
        assert!(!enabled("   "));
        assert!(!enabled("\n\t "));
        assert!(enabled("abc123"));
        assert!(enabled("  abc123  "));
    }

    #[test]
    fn auth_header_uses_token_scheme_and_trims() {
        assert_eq!(auth_header("abc123"), "Token abc123");
        assert_eq!(auth_header("  abc123\n"), "Token abc123");
    }

    #[test]
    fn single_payload_shape() {
        let body = single_payload("Ann", "Airbag", Some("OK Computer"), 1_700_000_000).unwrap();
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["listen_type"], "single");
        assert_eq!(v["payload"][0]["listened_at"], 1_700_000_000);
        assert_eq!(v["payload"][0]["track_metadata"]["artist_name"], "Ann");
        assert_eq!(v["payload"][0]["track_metadata"]["track_name"], "Airbag");
        assert_eq!(
            v["payload"][0]["track_metadata"]["release_name"],
            "OK Computer"
        );
    }

    #[test]
    fn single_payload_omits_blank_album() {
        let body = single_payload("Ann", "Airbag", Some("  "), 42).unwrap();
        let v = serde_json::to_value(&body).unwrap();
        assert!(
            v["payload"][0]["track_metadata"]
                .get("release_name")
                .is_none()
        );
    }

    #[test]
    fn playing_now_has_no_listened_at() {
        let body = playing_now_payload("Ann", "Airbag", None).unwrap();
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["listen_type"], "playing_now");
        assert!(v["payload"][0].get("listened_at").is_none());
        assert_eq!(v["payload"][0]["track_metadata"]["artist_name"], "Ann");
    }

    #[test]
    fn blank_metadata_builds_nothing_and_sends_nothing() {
        assert!(single_payload("", "Airbag", None, 1).is_none());
        assert!(single_payload("Ann", "  ", None, 1).is_none());
        assert!(playing_now_payload("", "", None).is_none());
    }

    #[test]
    fn submit_endpoint_is_listenbrainz() {
        assert_eq!(SUBMIT_URL, "https://api.listenbrainz.org/1/submit-listens");
    }
}
