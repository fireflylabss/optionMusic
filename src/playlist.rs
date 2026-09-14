//! Playlist building and track scanning.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

/// Known audio extensions optionMusic prefers (MPV handles many more).
const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "wav", "wave", "aac", "m4a", "mp4", "opus", "wma", "aiff", "aif",
    "alac", "webm", "mkv",
];

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    /// Cached embedded title (None when missing / unread).
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    /// Whether a cover was found (sidecar or embedded); None until checked.
    pub has_cover: Option<bool>,
    /// Unix seconds of file mtime; `0` when metadata is unavailable.
    pub mtime: u64,
    /// File size in bytes; `0` when metadata is unavailable.
    pub size: u64,
    /// Track length in seconds (`None` until enrichment probes the file).
    pub duration_secs: Option<f64>,
    /// Whether tag enrichment has been attempted for this track.
    pub tags_enriched: bool,
}

impl Track {
    /// Fast path-only scan entry (tags deferred).
    pub fn from_path(path: PathBuf) -> Self {
        let (mtime, size) = file_stat(&path);
        Self::from_path_with_stat(path, mtime, size)
    }

    /// Path-only entry with pre-known mtime/size (used by the library baseline).
    pub fn from_path_with_stat(path: PathBuf, mtime: u64, size: u64) -> Self {
        Self {
            path,
            title: None,
            artist: None,
            album: None,
            track_number: None,
            disc_number: None,
            genre: None,
            year: None,
            has_cover: None,
            mtime,
            size,
            duration_secs: None,
            tags_enriched: false,
        }
    }

    /// Full scan with tag read (CLI / explicit paths).
    pub fn new(path: PathBuf) -> Self {
        let mut track = Self::from_path(path);
        track.enrich_tags();
        track
    }

    /// Load tags from cache or lofty and mark this track enriched.
    pub fn enrich_tags(&mut self) {
        if self.tags_enriched {
            return;
        }
        let tags = crate::meta::read_tags_cached(&self.path, self.mtime, self.size);
        let has_cover = crate::cover::resolve_cover_file(&self.path, self.mtime, self.size)
            .ok()
            .flatten()
            .is_some();
        self.apply_tags(tags, has_cover);
    }

    /// Store already-read tags and mark this track enriched. Shared by
    /// `enrich_tags` and the controller's off-thread `enrich_apply` path.
    pub(crate) fn apply_tags(&mut self, tags: crate::meta::AudioTags, has_cover: bool) {
        self.title = tags.title;
        self.artist = tags.artist;
        self.album = tags.album;
        self.track_number = tags.track_number;
        self.disc_number = tags.disc_number;
        self.genre = tags.genre;
        self.year = tags.year;
        self.duration_secs = tags.duration_secs;
        self.has_cover = Some(has_cover);
        self.tags_enriched = true;
    }

    /// "artist — album" / "artist" / "album" / "" (RPC state line — empty
    /// when untagged so Discord shows just the title, no "local file").
    pub fn artist_album(&self) -> String {
        match (self.artist.as_deref(), self.album.as_deref()) {
            (Some(a), Some(b)) => format!("{a} — {b}"),
            (Some(a), None) => a.to_string(),
            (None, Some(b)) => b.to_string(),
            (None, None) => String::new(),
        }
    }

    /// Human-friendly name: tagged title, else file stem, else full path.
    pub fn display_name(&self) -> String {
        if let Some(title) = self.title.as_ref().filter(|s| !s.is_empty()) {
            return title.clone();
        }
        self.path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// YouTube thumbnail URL derived from a stream URL or a `[id]` yt-dlp-style
    /// filename — the only art Discord can render for local playback.
    /// `mqdefault` is pure 16:9 (no baked-in letterbox bars like `hqdefault`),
    /// so Discord's square center-crop lands cleanly.
    pub fn thumb_url(&self) -> Option<String> {
        yt_video_id(&self.path.to_string_lossy())
            .map(|id| format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"))
    }
}

/// Extract a YouTube video id from a URL (`youtu.be/`, `watch?v=`) or from a
/// `[id]` tag in the file name (yt-dlp's default `%(id)s` convention).
fn yt_video_id(text: &str) -> Option<&str> {
    fn is_id(s: &str) -> bool {
        s.len() == 11
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }
    for marker in ["youtu.be/", "watch?v="] {
        if let Some(i) = text.find(marker) {
            let rest = &text[i + marker.len()..];
            let end = rest.find(['?', '&', '/', ' ']).unwrap_or(rest.len());
            let cand = &rest[..end];
            if is_id(cand) {
                return Some(cand);
            }
        }
    }
    let mut start = 0;
    while let Some(off) = text[start..].find('[') {
        let i = start + off;
        if let Some(j) = text[i..].find(']') {
            let cand = &text[i + 1..i + j];
            if is_id(cand) {
                return Some(cand);
            }
        }
        start = i + 1;
    }
    None
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
                let mut dir_tracks = scan_path(p, true)?;
                for track in &mut dir_tracks {
                    track.enrich_tags();
                }
                tracks.extend(dir_tracks);
            } else {
                anyhow::bail!("path not found: {}", p.display());
            }
        }
        // stable order by path
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        tracks.dedup_by(|a, b| a.path == b.path);
        Ok(Self { tracks })
    }

    /// Playlist from an explicit ordered track list (radio order, etc.).
    pub fn from_tracks(tracks: Vec<Track>) -> Self {
        Self { tracks }
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

    /// Reorder tracks to match `ordered` paths (saved session queue). Paths
    /// that no longer exist drop out; tracks absent from `ordered` keep their
    /// relative order at the end.
    pub fn reorder(&mut self, ordered: &[String]) {
        if ordered.is_empty() {
            return;
        }
        let mut rank = std::collections::HashMap::with_capacity(ordered.len());
        for (i, p) in ordered.iter().enumerate() {
            rank.insert(p.as_str(), i);
        }
        // Stable sort: queued tracks first in saved order; anything not in
        // the saved queue (new files) keeps its relative order at the end.
        self.tracks.sort_by_key(|t| {
            rank.get(t.path.to_string_lossy().as_ref())
                .copied()
                .unwrap_or(usize::MAX)
        });
    }

    /// Rebuild from saved queue paths only — used by session resume.
    /// Returns the index of `resume_track` when found.
    pub fn resume_order(&mut self, queue: &[String], resume_track: &str) -> Option<usize> {
        self.reorder(queue);
        self.tracks
            .iter()
            .position(|t| t.path.to_string_lossy() == resume_track)
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

/// Scan a path for audio files (path index only; tags deferred).
pub fn scan_path(path: &Path, recursive: bool) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();

    if path.is_file() {
        if is_audio(path) {
            tracks.push(Track::from_path(path.to_path_buf()));
        }
        return Ok(tracks);
    }

    if !path.is_dir() {
        anyhow::bail!("not a file or directory: {}", path.display());
    }

    if recursive {
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if p.is_file() && is_audio(p) {
                tracks.push(Track::from_path(p.to_path_buf()));
            }
        }
    } else {
        let rd = std::fs::read_dir(path)
            .with_context(|| format!("cannot read directory {}", path.display()))?;
        for entry in rd.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && is_audio(&p) {
                tracks.push(Track::from_path(p));
            }
        }
    }

    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}

fn file_stat(path: &Path) -> (u64, u64) {
    std::fs::metadata(path)
        .ok()
        .map(|m| {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (mtime, m.len())
        })
        .unwrap_or((0, 0))
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| AUDIO_EXTS.iter().any(|a| a.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

/// Whether `path` is a supported audio extension. Public for the library core.
pub fn is_audio_file(path: &Path) -> bool {
    is_audio(path)
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
    fn yt_id_from_url_and_filename() {
        assert_eq!(
            yt_video_id("https://youtu.be/fCO7f0SmrDc"),
            Some("fCO7f0SmrDc")
        );
        assert_eq!(
            yt_video_id("https://www.youtube.com/watch?v=fCO7f0SmrDc&t=4s"),
            Some("fCO7f0SmrDc")
        );
        assert_eq!(
            yt_video_id("/Music/(G)I-DLE - 'Nxde' MV [fCO7f0SmrDc].mp3"),
            Some("fCO7f0SmrDc")
        );
        assert_eq!(yt_video_id("/Music/ordinary song.mp3"), None);
        assert_eq!(yt_video_id("/Music/[too short].mp3"), None);
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
