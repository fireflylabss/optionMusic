//! Playback statistics: per-track play counts, total time, last played.
//!
//! Stored as JSON at `~/.option/music/stats.json`. A play is counted when a
//! track plays past ~50% of its duration or 60 seconds, whichever comes first.

use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Position counts as a full play once past 60s or half the track.
pub const COUNT_AFTER_SECS: f64 = 60.0;
/// Fraction of duration that counts as a full play.
pub const COUNT_AFTER_FRACTION: f64 = 0.5;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackStat {
    /// File path (stable id).
    pub id: String,
    /// Display title at last count.
    pub name: String,
    /// Artist at last count (may be empty).
    pub artist: String,
    /// Counted plays.
    pub plays: u64,
    /// Sum of listened seconds across counted plays.
    pub total_secs: u64,
    /// Unix seconds of the last counted play.
    pub last_played: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatsStore {
    #[serde(default)]
    pub tracks: HashMap<String, TrackStat>,
    /// Counted plays across all tracks.
    #[serde(default)]
    pub total_plays: u64,
    /// Listened seconds across counted plays.
    #[serde(default)]
    pub total_secs: u64,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn stats_path() -> std::path::PathBuf {
    crate::config::config_dir().join("stats.json")
}

/// True when `pos_secs` into a `dur_secs` track counts as a play.
/// Unknown duration (`None`/<=0) counts after [`COUNT_AFTER_SECS`].
pub fn should_count_play(pos_secs: f64, dur_secs: Option<f64>) -> bool {
    if pos_secs >= COUNT_AFTER_SECS {
        return true;
    }
    match dur_secs {
        Some(d) if d > 0.0 => pos_secs >= d * COUNT_AFTER_FRACTION,
        _ => false,
    }
}

impl StatsStore {
    pub fn load() -> Self {
        let path = stats_path();
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let dir = crate::config::config_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("create {}", dir.display()))?;
        let path = stats_path();
        let body = serde_json::to_string_pretty(self)?;
        option_sdk::atomic_write(&path, body.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Record one counted play of `listened_secs` for a track (best-effort save).
    pub fn record_play(&mut self, id: &str, name: &str, artist: &str, listened_secs: u64) {
        if id.is_empty() {
            return;
        }
        let entry = self.tracks.entry(id.to_owned()).or_insert_with(|| TrackStat {
            id: id.to_owned(),
            name: name.to_owned(),
            artist: artist.to_owned(),
            plays: 0,
            total_secs: 0,
            last_played: 0,
        });
        entry.name = name.to_owned();
        entry.artist = artist.to_owned();
        entry.plays += 1;
        entry.total_secs += listened_secs;
        entry.last_played = now_unix();
        self.total_plays += 1;
        self.total_secs += listened_secs;
        let _ = self.save();
    }

    /// Top tracks by play count (descending).
    pub fn top_tracks(&self, n: usize) -> Vec<&TrackStat> {
        let mut v: Vec<&TrackStat> = self.tracks.values().collect();
        v.sort_by(|a, b| {
            b.plays
                .cmp(&a.plays)
                .then(b.total_secs.cmp(&a.total_secs))
                .then(a.name.cmp(&b.name))
        });
        v.truncate(n);
        v
    }

    /// Top artists by summed play count (descending). Empty artist → "(unknown)".
    pub fn top_artists(&self, n: usize) -> Vec<(String, u64)> {
        let mut map: HashMap<String, u64> = HashMap::new();
        for t in self.tracks.values() {
            let key = if t.artist.trim().is_empty() {
                "(unknown)".to_owned()
            } else {
                t.artist.clone()
            };
            *map.entry(key).or_default() += t.plays;
        }
        let mut v: Vec<(String, u64)> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unit tests must not pollute the real `~/.option/music/stats.json`.
    struct StatsFileGuard {
        backup: Option<String>,
    }
    impl StatsFileGuard {
        fn take() -> Self {
            Self {
                backup: std::fs::read_to_string(stats_path()).ok(),
            }
        }
    }
    impl Drop for StatsFileGuard {
        fn drop(&mut self) {
            match self.backup.take() {
                Some(body) => {
                    let _ = std::fs::write(stats_path(), body);
                }
                None => {
                    let _ = std::fs::remove_file(stats_path());
                }
            }
        }
    }

    #[test]
    fn counts_past_half_or_sixty_seconds() {
        assert!(should_count_play(61.0, Some(600.0)));
        assert!(should_count_play(100.0, Some(200.0))); // 50%
        assert!(should_count_play(99.0, Some(200.0))); // past 60s
        assert!(!should_count_play(59.0, Some(200.0)));
        assert!(!should_count_play(10.0, Some(200.0)));
        assert!(should_count_play(60.0, None)); // unknown duration
        assert!(!should_count_play(59.0, None));
        assert!(!should_count_play(5.0, Some(0.0)));
    }

    #[test]
    fn record_and_rank_tracks_and_artists() {
        let _guard = StatsFileGuard::take();
        let mut s = StatsStore::default();
        s.record_play("/a.mp3", "A", "Ann", 120);
        s.record_play("/a.mp3", "A", "Ann", 120);
        s.record_play("/b.mp3", "B", "Bob", 30);
        assert_eq!(s.total_plays, 3);
        assert_eq!(s.total_secs, 270);
        let top = s.top_tracks(2);
        assert_eq!(top[0].id, "/a.mp3");
        assert_eq!(top[0].plays, 2);
        let artists = s.top_artists(2);
        assert_eq!(artists[0], ("Ann".to_owned(), 2));
        assert_eq!(artists[1], ("Bob".to_owned(), 1));
    }

    #[test]
    fn ignores_empty_id() {
        let _guard = StatsFileGuard::take();
        let mut s = StatsStore::default();
        s.record_play("", "A", "Ann", 10);
        assert_eq!(s.total_plays, 0);
        assert!(s.tracks.is_empty());
    }
}
