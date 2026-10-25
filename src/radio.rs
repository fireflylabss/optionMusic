//! Local-library radio: order the whole library as a similarity walk.
//!
//! Each next track is picked by similarity to the *current* one (artist,
//! album, genre, era), weighted by play stats and penalized for anything
//! heard this week — so the queue drifts like a real radio instead of
//! replaying one artist block. Pure ordering logic: `main.rs` hands the
//! resulting order to `run_session` as a normal `Playlist`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::library::Library;
use crate::playlist::Track;
use crate::stats::StatsStore;

// ── Tuning ────────────────────────────────────────────────────
// Similarity weights (higher = stronger pull to keep the vibe).
const W_SAME_ARTIST: f64 = 4.0;
const W_SAME_ALBUM: f64 = 2.0;
const W_GENRE_OVERLAP: f64 = 2.0;
const W_YEAR_CLOSE: f64 = 1.0; // |Δyear| <= YEAR_CLOSE
const W_YEAR_NEAR: f64 = 0.5; // |Δyear| <= YEAR_NEAR
const YEAR_CLOSE: u32 = 3;
const YEAR_NEAR: u32 = 8;
// Familiarity: +0.15 per counted play, capped at 10 plays.
const W_FAMILIAR: f64 = 0.15;
const FAMILIAR_CAP: u64 = 10;
// `--fresh`: bonus for never/rarely played tracks, replaces familiarity.
const W_FRESH: f64 = 1.5;
const FRESH_MAX_PLAYS: u64 = 2;
// Penalty for tracks heard in the last 7 days (skipped in --fresh).
const W_RECENT_PENALTY: f64 = 3.0;
// Penalty for tracks skipped/disliked in the last 7 days (`n` / `d`).
// Stronger than the recency penalty: negative feedback must be felt even
// when the track would otherwise win on similarity. A disliked track counts
// double (see `is_disliked` below).
const W_NEGATIVE_PENALTY: f64 = 6.0;
// Penalty for any candidate whose artist got skip/dislike feedback in the
// last 7 days — the vibe drifts away from penalized artists, not just the
// exact rejected tracks.
const W_NEGATIVE_ARTIST_PENALTY: f64 = 2.0;
// Don't pick the same artist more than this many times in a row while
// alternatives remain — keeps the walk from becoming an artist marathon.
const SAME_ARTIST_RUN_CAP: usize = 2;
// Pick uniformly among the K best candidates each step (radio drift).
const DEFAULT_TOP_K: usize = 5;

/// Options for [`build_order`].
#[derive(Debug, Clone)]
pub struct RadioOpts {
    /// Rediscovery mode: favor rarely/never-played tracks.
    pub fresh: bool,
    /// RNG seed (xorshift) — fixes make the order deterministic.
    pub seed: u64,
    /// How many top candidates each step picks from at random.
    pub top_k: usize,
}

impl Default for RadioOpts {
    fn default() -> Self {
        Self {
            fresh: false,
            seed: 0x9e3779b97f4a7c15,
            top_k: DEFAULT_TOP_K,
        }
    }
}

fn xorshift(state: &mut u64) -> u64 {
    let mut s = *state;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    *state = s;
    s
}

/// Lowercased artist key; `None` for empty / unknown so untagged tracks
/// don't collapse into one giant "same artist" cluster.
fn artist_key(name: &str) -> Option<String> {
    let n = name.trim().to_lowercase();
    if n.is_empty() || n == "unknown artist" {
        None
    } else {
        Some(n)
    }
}

/// Genre tag → token set (`"Rock; Indie/Alt"` → `{rock, indie, alt}`).
fn genre_tokens(genre: Option<&str>) -> HashSet<String> {
    genre
        .into_iter()
        .flat_map(|g| g.split([';', '/', ',']))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn score_candidate(
    cand: usize,
    cur: usize,
    tracks: &[Track],
    artists: &[Option<String>],
    albums: &[Option<String>],
    genres: &[HashSet<String>],
    plays: &[u64],
    is_recent: &[bool],
    is_negative: &[bool],
    is_disliked: &[bool],
    artist_penalized: &[bool],
    fresh: bool,
) -> f64 {
    let mut score = 0.0;
    if let (Some(a), Some(b)) = (&artists[cand], &artists[cur])
        && a == b
    {
        score += W_SAME_ARTIST;
    }
    if let (Some(a), Some(b)) = (&albums[cand], &albums[cur])
        && a == b
    {
        score += W_SAME_ALBUM;
    }
    if genres[cand].iter().any(|g| genres[cur].contains(g)) {
        score += W_GENRE_OVERLAP;
    }
    if let (Some(a), Some(b)) = (tracks[cand].year, tracks[cur].year) {
        let diff = a.abs_diff(b);
        if diff <= YEAR_CLOSE {
            score += W_YEAR_CLOSE;
        } else if diff <= YEAR_NEAR {
            score += W_YEAR_NEAR;
        }
    }
    let p = plays[cand];
    if fresh {
        if p == 0 {
            score += W_FRESH;
        } else if p <= FRESH_MAX_PLAYS {
            score += W_FRESH / 2.0;
        }
    } else {
        score += p.min(FAMILIAR_CAP) as f64 * W_FAMILIAR;
        if is_recent[cand] {
            score -= W_RECENT_PENALTY;
        }
    }
    // Negative radio feedback (`n` skip / `d` dislike) always applies — even
    // in --fresh mode — and outweighs every positive weight combined, so a
    // rejected track (or its artist) loses ties for the next 7 days.
    if is_negative[cand] {
        score -= W_NEGATIVE_PENALTY;
        if is_disliked[cand] {
            score -= W_NEGATIVE_PENALTY;
        }
    }
    if artist_penalized[cand] {
        score -= W_NEGATIVE_ARTIST_PENALTY;
    }
    score
}

/// Order all `tracks` as a radio walk starting at `seed_idx`.
///
/// Greedy: each step scores every unused track against the current one and
/// picks at random among the top-`opts.top_k`. Deterministic for a fixed
/// `opts.seed`. O(n²) scoring — fine for typical libraries (< ~20k tracks);
/// swap the full scan for an artist/genre/decade inverted index if needed.
///
/// `recent_ids` are tracks played this week (mild penalty). `negative_ids`
/// are tracks skipped/disliked this week (strong penalty, always applied —
/// even in `--fresh`). `disliked_ids` is the subset rejected explicitly via
/// `d` (double penalty). `penalized_artists` holds lowercased artists with
/// recent negative feedback (mild per-track penalty).
#[allow(clippy::too_many_arguments)]
pub fn build_order(
    tracks: &[Track],
    artist_of: impl Fn(&Track) -> String,
    stats: &StatsStore,
    recent_ids: &HashSet<String>,
    seed_idx: usize,
    opts: &RadioOpts,
) -> Vec<usize> {
    build_order_full(
        tracks,
        artist_of,
        stats,
        recent_ids,
        &HashSet::new(),
        &HashSet::new(),
        &HashSet::new(),
        seed_idx,
        opts,
    )
}

/// Full variant of [`build_order`] with negative-feedback inputs.
///
/// Prefer [`build_order`] when the caller has no feedback sets; pass the
/// `history::{negative,disliked}_this_week` sets plus
/// `stats.penalized_artists()` here for Radio 2.0 semantics.
#[allow(clippy::too_many_arguments)]
pub fn build_order_full(
    tracks: &[Track],
    artist_of: impl Fn(&Track) -> String,
    stats: &StatsStore,
    recent_ids: &HashSet<String>,
    negative_ids: &HashSet<String>,
    disliked_ids: &HashSet<String>,
    penalized_artists: &HashSet<String>,
    seed_idx: usize,
    opts: &RadioOpts,
) -> Vec<usize> {
    let n = tracks.len();
    if n == 0 {
        return Vec::new();
    }
    let mut rng = opts.seed ^ 0x9e3779b97f4a7c15;

    let artists: Vec<Option<String>> = tracks.iter().map(|t| artist_key(&artist_of(t))).collect();
    let albums: Vec<Option<String>> = tracks
        .iter()
        .map(|t| {
            t.album
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
        })
        .collect();
    let genres: Vec<HashSet<String>> = tracks
        .iter()
        .map(|t| genre_tokens(t.genre.as_deref()))
        .collect();
    let plays: Vec<u64> = tracks
        .iter()
        .map(|t| {
            stats
                .tracks
                .get(t.path.to_string_lossy().as_ref())
                .map(|s| s.plays)
                .unwrap_or(0)
        })
        .collect();
    let is_recent: Vec<bool> = tracks
        .iter()
        .map(|t| recent_ids.contains(t.path.to_string_lossy().as_ref()))
        .collect();
    let is_negative: Vec<bool> = tracks
        .iter()
        .map(|t| negative_ids.contains(t.path.to_string_lossy().as_ref()))
        .collect();
    let is_disliked: Vec<bool> = tracks
        .iter()
        .map(|t| disliked_ids.contains(t.path.to_string_lossy().as_ref()))
        .collect();
    let artist_penalized: Vec<bool> = artists
        .iter()
        .map(|a| a.as_ref().is_some_and(|k| penalized_artists.contains(k)))
        .collect();

    let mut used = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut cur = seed_idx.min(n - 1);
    used[cur] = true;
    order.push(cur);
    let mut run_artist = artists[cur].clone();
    let mut run_len = usize::from(run_artist.is_some());

    while order.len() < n {
        let mut scored: Vec<(f64, usize)> = (0..n)
            .filter(|&i| !used[i])
            .map(|i| {
                (
                    score_candidate(
                        i,
                        cur,
                        tracks,
                        &artists,
                        &albums,
                        &genres,
                        &plays,
                        &is_recent,
                        &is_negative,
                        &is_disliked,
                        &artist_penalized,
                        opts.fresh,
                    ),
                    i,
                )
            })
            .collect();
        if scored.is_empty() {
            break;
        }
        // Run cap: after SAME_ARTIST_RUN_CAP picks of one artist, exclude it
        // while any other artist remains.
        if run_len >= SAME_ARTIST_RUN_CAP
            && let Some(ra) = &run_artist
        {
            let filtered: Vec<(f64, usize)> = scored
                .iter()
                .copied()
                .filter(|(_, i)| artists[*i].as_ref() != Some(ra))
                .collect();
            if !filtered.is_empty() {
                scored = filtered;
            }
        }
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.cmp(&b.1))
        });
        let k = opts.top_k.max(1).min(scored.len());
        let pick = scored[(xorshift(&mut rng) as usize) % k].1;

        if artists[pick].is_some() && artists[pick] == run_artist {
            run_len += 1;
        } else {
            run_artist = artists[pick].clone();
            run_len = usize::from(run_artist.is_some());
        }
        used[pick] = true;
        order.push(pick);
        cur = pick;
    }
    order
}

/// Pick the starting track for a radio session.
///
/// Priority: `--artist`/`--genre` filters (intersected) → positional query
/// (unified library search) → most-played track still in the library →
/// random. `None` when the library is empty or a given seed matches nothing.
pub fn resolve_seed(
    library: &Library,
    query: Option<&str>,
    artist: Option<&str>,
    genre: Option<&str>,
    stats: &StatsStore,
    seed: u64,
) -> Option<usize> {
    resolve_seed_in(
        library.tracks(),
        |t| library.artist_name(t),
        |t| library.album_name(t),
        query,
        artist,
        genre,
        stats,
        seed,
    )
}

/// Track-slice variant of [`resolve_seed`] for callers that own their track
/// list without holding a `Library`.
/// `artist_of`/`album_of` must resolve display names the same way
/// `Library::artist_name`/`album_name` do.
#[allow(clippy::too_many_arguments)]
pub fn resolve_seed_in(
    tracks: &[Track],
    artist_of: impl Fn(&Track) -> String,
    album_of: impl Fn(&Track) -> String,
    query: Option<&str>,
    artist: Option<&str>,
    genre: Option<&str>,
    stats: &StatsStore,
    seed: u64,
) -> Option<usize> {
    if tracks.is_empty() {
        return None;
    }
    let mut rng = seed ^ 0x9e3779b97f4a7c15;
    let pick = |idx: &[usize], rng: &mut u64| -> Option<usize> {
        if idx.is_empty() {
            None
        } else {
            Some(idx[(xorshift(rng) as usize) % idx.len()])
        }
    };

    if artist.is_some() || genre.is_some() {
        let idx: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                genre.is_none_or(|g| {
                    t.genre
                        .as_deref()
                        .map(|tg| tg.eq_ignore_ascii_case(g.trim()))
                        .unwrap_or(false)
                }) && artist.is_none_or(|a| artist_of(t) == a)
            })
            .map(|(i, _)| i)
            .collect();
        return pick(&idx, &mut rng);
    }
    if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
        let idx: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| crate::library::track_matches_query(t, q, &artist_of(t), &album_of(t)))
            .map(|(i, _)| i)
            .collect();
        return pick(&idx, &mut rng);
    }
    // Fallback: most-played track still in the library, else random.
    let index_of: HashMap<String, usize> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.path.to_string_lossy().into_owned(), i))
        .collect();
    for t in stats.top_tracks(50) {
        if let Some(&i) = index_of.get(&t.id) {
            return Some(i);
        }
    }
    Some((xorshift(&mut rng) as usize) % tracks.len())
}

// ── Saved stations ───────────────────────────────────────────────
// A station is just a named radio seed: optional query + artist + genre +
// fresh flag, persisted as one TOML file per station under
// `~/.option/music/stations/<slug>.toml`.

/// Named radio seed saved via `msc radio save NAME`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadioStation {
    /// Display name given on `save`.
    pub name: String,
    /// Positional seed query (title / artist / album / genre search).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// `--artist` seed filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// `--genre` seed filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// `--fresh` rediscovery mode.
    #[serde(default)]
    pub fresh: bool,
}

impl RadioStation {
    /// Short `seed "q" · artist "a" · genre "g" · fresh` description for `ls`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(q) = self
            .query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("\"{q}\""));
        }
        if let Some(a) = self
            .artist
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("artist \"{a}\""));
        }
        if let Some(g) = self
            .genre
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            parts.push(format!("genre \"{g}\""));
        }
        if self.fresh {
            parts.push("fresh".to_owned());
        }
        if parts.is_empty() {
            "library mix".to_owned()
        } else {
            parts.join(" · ")
        }
    }
}

/// Snapshot of the current CLI seed flags — what `msc radio save` stores.
#[derive(Debug, Clone, Default)]
pub struct RadioSeed {
    pub query: Option<String>,
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub fresh: bool,
}

impl RadioSeed {
    pub fn is_empty(&self) -> bool {
        self.query
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none()
            && self
                .artist
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none()
            && self
                .genre
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_none()
            && !self.fresh
    }
}

fn stations_dir() -> PathBuf {
    config::config_dir().join("stations")
}

/// Slug for the station file name (`"Morning Drive"` → `morning-drive`).
/// Rejects path tricks; empty names fall back to `station`.
pub fn station_slug(name: &str) -> Result<String> {
    let mut slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_owned();
    let slug = if slug.is_empty() {
        "station".to_owned()
    } else {
        slug
    };
    if slug == "." || slug == ".." || slug.contains('/') || slug.contains('\\') {
        bail!("invalid station name: {name}");
    }
    Ok(slug)
}

fn station_path(name: &str) -> Result<PathBuf> {
    Ok(stations_dir().join(format!("{}.toml", station_slug(name)?)))
}

/// Persist the current seed flags as a named station (overwrites same name).
pub fn save_station(name: &str, seed: &RadioSeed) -> Result<RadioStation> {
    let name = name.trim();
    if name.is_empty() {
        bail!("station name is empty");
    }
    let station = RadioStation {
        name: name.to_owned(),
        query: seed
            .query
            .clone()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        artist: seed
            .artist
            .clone()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        genre: seed
            .genre
            .clone()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        fresh: seed.fresh,
    };
    let dir = stations_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = station_path(name)?;
    let body = toml::to_string_pretty(&station)?;
    option_sdk::atomic_write(&path, body.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(station)
}

/// Load a station by `save` name (case-insensitive fallback).
pub fn load_station(name: &str) -> Result<RadioStation> {
    let name = name.trim();
    if name.is_empty() {
        bail!("station name is empty");
    }
    if let Ok(path) = station_path(name)
        && path.exists()
    {
        let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        return Ok(toml::from_str(&raw)?);
    }
    // Case-insensitive fallback so `msc radio load Morning` finds `morning`.
    let wanted = name.to_lowercase();
    for station in list_stations()? {
        if station.name.to_lowercase() == wanted {
            return Ok(station);
        }
    }
    bail!("no station named \"{name}\" — see `msc radio ls`")
}

/// Every saved station, sorted case-insensitively by name.
pub fn list_stations() -> Result<Vec<RadioStation>> {
    let dir = stations_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("read {}", dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(station) = toml::from_str::<RadioStation>(&raw)
        {
            out.push(station);
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Delete a saved station by name.
pub fn delete_station(name: &str) -> Result<()> {
    let station = load_station(name)?;
    let path = station_path(&station.name)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// `StatsStore::record_play` writes to the real `~/.option/music/` —
    /// back it up and restore it so tests don't pollute user state.
    struct StatsFileGuard {
        backup: Option<String>,
    }
    impl StatsFileGuard {
        fn take() -> Self {
            Self {
                backup: fs::read_to_string(crate::stats::stats_path()).ok(),
            }
        }
    }
    impl Drop for StatsFileGuard {
        fn drop(&mut self) {
            match self.backup.take() {
                Some(body) => {
                    let _ = fs::write(crate::stats::stats_path(), body);
                }
                None => {
                    let _ = fs::remove_file(crate::stats::stats_path());
                }
            }
        }
    }

    fn track(name: &str, artist: &str, album: &str, genre: &str, year: u32) -> Track {
        let mut t = Track::from_path(PathBuf::from(format!("/music/{name}.mp3")));
        t.artist = if artist.is_empty() {
            None
        } else {
            Some(artist.into())
        };
        t.album = if album.is_empty() {
            None
        } else {
            Some(album.into())
        };
        t.genre = if genre.is_empty() {
            None
        } else {
            Some(genre.into())
        };
        t.year = if year == 0 { None } else { Some(year) };
        t
    }

    fn artist_of(t: &Track) -> String {
        t.artist.clone().unwrap_or_default()
    }

    fn opts(seed: u64, top_k: usize, fresh: bool) -> RadioOpts {
        RadioOpts { fresh, seed, top_k }
    }

    #[test]
    fn covers_every_track_once() {
        let tracks: Vec<Track> = (0..30)
            .map(|i| track(&format!("t{i}"), "A", "alb", "rock", 2000))
            .collect();
        let order = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(7, 5, false),
        );
        assert_eq!(order.len(), 30);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 30);
    }

    #[test]
    fn deterministic_given_seed() {
        let tracks: Vec<Track> = (0..20)
            .map(|i| track(&format!("t{i}"), "A", "alb", "rock", 2000 + i as u32))
            .collect();
        let a = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(42, 5, false),
        );
        let b = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(42, 5, false),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn prefers_same_artist_next() {
        let tracks = vec![
            track("b1", "Y", "", "", 1990),
            track("a1", "X", "", "", 1985),
            track("a2", "X", "", "", 2010),
            track("c1", "Z", "", "", 0),
        ];
        // Seed at b1 (artist Y): with top_k=1 the next pick must be an X track.
        let order = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(1, 1, false),
        );
        assert_eq!(order[0], 0);
        assert!(
            [1, 2].contains(&order[1]),
            "expected an X track, got {}",
            order[1]
        );
    }

    #[test]
    fn genre_overlap_pulls_together() {
        let tracks = vec![
            track("seed", "A", "", "rock", 2000),
            track("kin", "B", "", "rock/indie", 2015),
            track("far", "C", "", "pop", 1980),
        ];
        let order = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(1, 1, false),
        );
        assert_eq!(order[1], 1, "rock/indie should follow the rock seed");
    }

    #[test]
    fn run_cap_forces_artist_switch() {
        let mut tracks: Vec<Track> = (0..4)
            .map(|i| track(&format!("a{i}"), "Same", "alb", "g", 2000))
            .collect();
        tracks.push(track("other", "Other", "x", "g", 2000));
        // Seed inside "Same": after 2 consecutive picks it must jump ship.
        let order = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &HashSet::new(),
            0,
            &opts(1, 3, false),
        );
        let other = 4usize;
        let pos = order.iter().position(|&i| i == other).unwrap();
        assert!(
            pos <= SAME_ARTIST_RUN_CAP,
            "other artist came too late: {order:?}"
        );
    }

    #[test]
    fn recent_tracks_are_penalized() {
        let tracks = vec![
            track("seed", "A", "", "g", 2000),
            track("heard", "B", "", "g", 2000),
            track("fresh", "C", "", "g", 2000),
        ];
        let mut recent = HashSet::new();
        recent.insert("/music/heard.mp3".to_string());
        let order = build_order(
            &tracks,
            artist_of,
            &StatsStore::default(),
            &recent,
            0,
            &opts(1, 1, false),
        );
        assert_eq!(order[1], 2, "recently-heard track should lose the tie");
    }

    #[test]
    fn fresh_mode_prefers_unplayed() {
        let _guard = StatsFileGuard::take();
        let tracks = vec![
            track("seed", "A", "", "g", 2000),
            track("loved", "B", "", "g", 2000),
            track("new", "C", "", "g", 2000),
        ];
        let mut stats = StatsStore::default();
        stats.record_play("/music/loved.mp3", "loved", "B", 100);
        stats.record_play("/music/loved.mp3", "loved", "B", 100);
        let normal = build_order(
            &tracks,
            artist_of,
            &stats,
            &HashSet::new(),
            0,
            &opts(1, 1, false),
        );
        assert_eq!(normal[1], 1, "familiarity should favor the played track");
        let fresh = build_order(
            &tracks,
            artist_of,
            &stats,
            &HashSet::new(),
            0,
            &opts(1, 1, true),
        );
        assert_eq!(fresh[1], 2, "fresh mode should favor the unplayed track");
    }

    // ── resolve_seed (needs a real Library on a temp dir) ─────────

    fn tmp_lib(name: &str, files: &[&str]) -> (PathBuf, Library) {
        let dir =
            std::env::temp_dir().join(format!("stillmusic-radio-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in files {
            fs::write(dir.join(f), "x").unwrap();
        }
        let lib = Library::scan(dir.clone()).unwrap();
        (dir, lib)
    }

    #[test]
    fn seed_by_query_matches_search() {
        let (dir, lib) = tmp_lib("query", &["alpha.mp3", "beta.mp3", "gamma.mp3"]);
        let idx = resolve_seed(&lib, Some("beta"), None, None, &StatsStore::default(), 1).unwrap();
        assert_eq!(lib.tracks()[idx].path.file_stem().unwrap(), "beta");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_no_match_returns_none() {
        let (dir, lib) = tmp_lib("nomatch", &["a.mp3"]);
        assert!(resolve_seed(&lib, Some("zzz"), None, None, &StatsStore::default(), 1).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_fallback_top_played() {
        let _guard = StatsFileGuard::take();
        let (dir, lib) = tmp_lib("top", &["a.mp3", "b.mp3"]);
        let mut stats = StatsStore::default();
        let fav = lib.tracks()[1].path.to_string_lossy().into_owned();
        stats.record_play(&fav, "B", "", 60);
        let idx = resolve_seed(&lib, None, None, None, &stats, 1).unwrap();
        assert_eq!(idx, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seed_random_when_no_stats() {
        let (dir, lib) = tmp_lib("rand", &["a.mp3", "b.mp3"]);
        let idx = resolve_seed(&lib, None, None, None, &StatsStore::default(), 1).unwrap();
        assert!(idx < lib.len());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_library_has_no_seed() {
        let lib = Library::default();
        assert!(resolve_seed(&lib, None, None, None, &StatsStore::default(), 1).is_none());
    }
}
