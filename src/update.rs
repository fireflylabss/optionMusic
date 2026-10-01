//! Update check for the desktop shell: asks GitHub for the latest published
//! release of `fireflylabss/optionMusic` and compares it with the running
//! build. Everything here is plain data and blocking calls — callers run it
//! off the UI thread and decide how to surface the result.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::config::cache_dir;

/// GitHub releases API endpoint — 15 newest entries is plenty for picking.
const RELEASES_URL: &str =
    "https://api.github.com/repos/fireflylabss/optionMusic/releases?per_page=15";
/// Release pages link out to `/releases/tag/<tag>`.
const RELEASES_PAGE: &str = "https://github.com/fireflylabss/optionMusic/releases";

/// Minimum gap between automatic checks — one per day.
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

/// Parsed `vX.Y.Z[m][-channel]` tag. CLI, desktop and mixed cuts share one
/// scheme, so desktop scope is decided by the version line, not a prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `m` before the channel marks a mixed CLI + desktop cut.
    pub mixed: bool,
    /// Channel label without the leading `-` (`""` when the tag has none).
    pub channel: String,
}

/// A published GitHub release reduced to what the update UI needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Tag as published, e.g. `v0.2.17-beta`.
    pub tag: String,
    /// Release page URL — the Download button opens this.
    pub url: String,
    pub version: TagVersion,
}

/// Outcome of an update check — also the UI state for the updates row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No check has run yet, or the last one failed silently.
    Unknown,
    /// A check request is in flight.
    Checking,
    /// The latest published release is at or behind the running build.
    /// Holds the newest tag seen.
    UpToDate(String),
    /// A newer desktop-scoped (or latest overall) release exists.
    Available(ReleaseInfo),
}

#[derive(Deserialize)]
struct ReleaseDto {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
}

/// Parse `v0.2.17-beta`, `v0.2.12m-beta`, `0.1.7`, … (`v` is optional).
/// Returns `None` for anything that is not a numeric `X.Y.Z` version.
pub fn parse_tag(tag: &str) -> Option<TagVersion> {
    let stem = tag.strip_prefix('v').unwrap_or(tag);
    let (base, channel) = match stem.split_once('-') {
        Some((base, channel)) => (base, channel),
        None => (stem, ""),
    };
    let mut parts = base.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let mut patch = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    // A trailing `m` on the patch number marks a mixed CLI + desktop cut.
    let mixed = patch.ends_with('m');
    if mixed {
        patch = &patch[..patch.len() - 1];
    }
    Some(TagVersion {
        major,
        minor,
        patch: patch.parse().ok()?,
        mixed,
        channel: channel.to_string(),
    })
}

/// Desktop cuts share the running build's `major.minor` line; mixed `m`
/// cuts ship both surfaces. No dedicated desktop tag prefix exists yet.
pub fn is_desktop_scoped(tag: &TagVersion, current: &TagVersion) -> bool {
    tag.mixed || (tag.major, tag.minor) == (current.major, current.minor)
}

/// Release-channel ordering: `alpha` < `beta` < `stable` / no suffix.
fn channel_rank(channel: &str) -> (u8, &str) {
    match channel {
        "" | "stable" => (2, ""),
        "beta" => (1, channel),
        _ => (0, channel),
    }
}

/// `latest` is newer than `current` on (major, minor, patch, channel).
/// The mixed marker does not affect ordering — only version numbers do.
pub fn is_newer(latest: &TagVersion, current: &TagVersion) -> bool {
    (
        latest.major,
        latest.minor,
        latest.patch,
        channel_rank(&latest.channel),
    ) > (
        current.major,
        current.minor,
        current.patch,
        channel_rank(&current.channel),
    )
}

/// First (newest) desktop-scoped release in `body`; when none exists, the
/// newest release overall. `body` is the releases API JSON array.
pub fn pick_release(body: &str, current: &TagVersion) -> Option<ReleaseInfo> {
    let releases: Vec<ReleaseDto> = serde_json::from_str(body).ok()?;
    let mut first: Option<ReleaseInfo> = None;
    for dto in releases.iter().filter(|dto| !dto.draft) {
        let Some(version) = parse_tag(&dto.tag_name) else {
            continue;
        };
        let info = ReleaseInfo {
            tag: dto.tag_name.clone(),
            url: if dto.html_url.is_empty() {
                format!("{RELEASES_PAGE}/tag/{}", dto.tag_name)
            } else {
                dto.html_url.clone()
            },
            version: version.clone(),
        };
        if first.is_none() {
            first = Some(info.clone());
        }
        if is_desktop_scoped(&version, current) {
            return Some(info);
        }
    }
    first
}

fn http_get(url: &str) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(4)))
        .timeout_global(Some(Duration::from_secs(6)))
        .user_agent(concat!("optionmusic/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()
}

fn stamp_path() -> PathBuf {
    cache_dir().join("update_check")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Whether an automatic check is due — at most one per `CHECK_INTERVAL_SECS`.
/// A missing or unreadable stamp counts as due.
pub fn check_due() -> bool {
    let Ok(raw) = fs::read_to_string(stamp_path()) else {
        return true;
    };
    let Ok(ts) = raw.trim().parse::<u64>() else {
        return true;
    };
    now_secs().saturating_sub(ts) >= CHECK_INTERVAL_SECS
}

/// Record a completed check attempt — the interval gates request rate, not
/// outcomes, so failures are stamped too.
pub fn mark_checked() {
    let _ = fs::write(stamp_path(), now_secs().to_string());
}

/// One blocking update check — run it off the UI thread. Stamps the rate
/// file whether the request succeeds or fails; failures come back as
/// `UpdateStatus::Unknown` so callers can stay silent.
pub fn check_now(current_tag: &str) -> UpdateStatus {
    mark_checked();
    let Some(current) = parse_tag(current_tag) else {
        return UpdateStatus::Unknown;
    };
    let Some(body) = http_get(RELEASES_URL) else {
        return UpdateStatus::Unknown;
    };
    match pick_release(&body, &current) {
        Some(info) if is_newer(&info.version, &current) => UpdateStatus::Available(info),
        Some(info) => UpdateStatus::UpToDate(info.tag),
        None => UpdateStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_option_tags() {
        let beta = parse_tag("v0.2.17-beta").unwrap();
        assert_eq!((beta.major, beta.minor, beta.patch), (0, 2, 17));
        assert_eq!(beta.channel, "beta");
        let mixed = parse_tag("v0.2.12m-beta").unwrap();
        assert!(mixed.mixed);
        assert_eq!((mixed.major, mixed.minor, mixed.patch), (0, 2, 12));
        let plain = parse_tag("0.1.7").unwrap();
        assert_eq!(plain.channel, "");
        assert!(!plain.mixed);
        assert_eq!(parse_tag("v0.2.15-stable").unwrap().channel, "stable");
        assert!(parse_tag("release-notes").is_none());
        assert!(parse_tag("v0.2").is_none());
    }

    #[test]
    fn ordering_matches_release_channels() {
        let current = parse_tag("0.1.7").unwrap();
        assert!(is_newer(&parse_tag("v0.2.17-beta").unwrap(), &current));
        assert!(!is_newer(&parse_tag("v0.1.7").unwrap(), &current));
        assert!(is_newer(&parse_tag("v0.1.8-beta").unwrap(), &current));
        // Channel settles equal version numbers: beta < stable.
        assert!(is_newer(
            &parse_tag("v0.1.8").unwrap(),
            &parse_tag("0.1.8-beta").unwrap()
        ));
        assert!(!is_newer(
            &parse_tag("v0.1.8-alpha").unwrap(),
            &parse_tag("0.1.8-beta").unwrap()
        ));
    }

    #[test]
    fn prefers_desktop_scoped_release() {
        let body = r#"[
            {"tag_name": "v0.2.17-beta", "html_url": "https://x/cli", "draft": false},
            {"tag_name": "v0.1.8-beta", "html_url": "https://x/desk", "draft": false}
        ]"#;
        let current = parse_tag("0.1.7").unwrap();
        assert_eq!(pick_release(body, &current).unwrap().tag, "v0.1.8-beta");
    }

    #[test]
    fn mixed_cuts_count_as_desktop_scoped() {
        let body = r#"[
            {"tag_name": "v0.2.17-beta", "html_url": "https://x/cli", "draft": false},
            {"tag_name": "v0.2.12m-beta", "html_url": "https://x/mix", "draft": false}
        ]"#;
        let current = parse_tag("0.1.7").unwrap();
        assert_eq!(pick_release(body, &current).unwrap().tag, "v0.2.12m-beta");
    }

    #[test]
    fn falls_back_to_latest_overall() {
        let body = r#"[
            {"tag_name": "v0.2.16-beta", "html_url": "https://x/draft", "draft": true},
            {"tag_name": "v0.2.17-beta", "html_url": "https://x/cli", "draft": false}
        ]"#;
        let current = parse_tag("0.1.7").unwrap();
        // Drafts are skipped; the newest published tag wins when nothing is
        // desktop-scoped.
        assert_eq!(pick_release(body, &current).unwrap().tag, "v0.2.17-beta");
        assert!(pick_release("not json", &current).is_none());
        assert!(pick_release("[]", &current).is_none());
    }
}
