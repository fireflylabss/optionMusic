//! Playlist building and track scanning.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

/// Known audio extensions optMusic prefers (MPV handles many more).
const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "wav", "wave", "aac", "m4a", "mp4", "opus", "wma",
    "aiff", "aif", "alac", "webm", "mkv",
];

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
}

impl Track {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Human-friendly name: file stem, falling back to full path.
    pub fn display_name(&self) -> String {
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

#[derive(Debug, Default)]
pub struct Playlist {
    tracks: Vec<Track>,
}

impl Playlist {
    pub fn from_paths(paths: &[PathBuf]) -> Result<Self> {
        let mut tracks = Vec::new();
        for p in paths {
            if p.is_file() {
                if is_audio(p) {
                    tracks.push(Track::new(p.clone()));
                } else {
                    // still try — user may know better
                    tracks.push(Track::new(p.clone()));
                }
            } else if p.is_dir() {
                tracks.extend(scan_path(p, true)?);
            } else {
                anyhow::bail!("path not found: {}", p.display());
            }
        }
        // stable order by path
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        tracks.dedup_by(|a, b| a.path == b.path);
        Ok(Self { tracks })
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn get(&self, index: usize) -> Option<&Track> {
        self.tracks.get(index)
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn shuffle(&mut self) {
        // Simple Fisher–Yates without external rng crate
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::{SystemTime, UNIX_EPOCH};

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);

        let mut state = seed;
        let n = self.tracks.len();
        for i in (1..n).rev() {
            // xorshift-ish
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // mix path hash a bit for extra entropy
            let mut h = DefaultHasher::new();
            self.tracks[i].path.hash(&mut h);
            state = state.wrapping_add(h.finish());
            let j = (state as usize) % (i + 1);
            self.tracks.swap(i, j);
        }
    }
}

/// Scan a path for audio files.
pub fn scan_path(path: &Path, recursive: bool) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();

    if path.is_file() {
        if is_audio(path) {
            tracks.push(Track::new(path.to_path_buf()));
        }
        return Ok(tracks);
    }

    if !path.is_dir() {
        anyhow::bail!("not a file or directory: {}", path.display());
    }

    if recursive {
        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if p.is_file() && is_audio(p) {
                tracks.push(Track::new(p.to_path_buf()));
            }
        }
    } else {
        let rd = std::fs::read_dir(path)
            .with_context(|| format!("cannot read directory {}", path.display()))?;
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && is_audio(&p) {
                tracks.push(Track::new(p));
            }
        }
    }

    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| AUDIO_EXTS.iter().any(|a| a.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_audio_extensions() {
        assert!(is_audio(Path::new("song.mp3")));
        assert!(is_audio(Path::new("Song.FLAC")));
        assert!(is_audio(Path::new("a.Ogg")));
        assert!(!is_audio(Path::new("readme.txt")));
        assert!(!is_audio(Path::new("noext")));
    }

    #[test]
    fn track_display_name() {
        let t = Track::new(PathBuf::from("/music/My Song.mp3"));
        assert_eq!(t.display_name(), "My Song");
    }

    #[test]
    fn shuffle_preserves_length() {
        let mut pl = Playlist {
            tracks: (0..10)
                .map(|i| Track::new(PathBuf::from(format!("t{i}.mp3"))))
                .collect(),
        };
        let len = pl.len();
        pl.shuffle();
        assert_eq!(pl.len(), len);
    }
}
