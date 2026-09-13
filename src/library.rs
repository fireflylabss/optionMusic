//! Reusable library core for the CLI (and future desktop): scanning,
//! incremental refresh, grouping, filtering and search.
//!
//! `Library` owns the music index under a root directory. It is the single
//! source of truth for `msc library` and `msc browse`; the desktop keeps its
//! own `CoreController` until the Tauri shell is removed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::{ArtistSource, AppConfig};
use crate::playlist::{Playlist, Track};

/// Delta reported by [`Library::refresh`] since the last scan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanResult {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub total: usize,
}

/// Grouping entry for the artist level of the browse tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistGroup {
    pub name: String,
    /// Number of distinct album names under this artist.
    pub albums: usize,
    pub tracks: usize,
}

/// Grouping entry for the album level of the browse tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumGroup {
    pub name: String,
    pub year: Option<u32>,
    pub tracks: usize,
}

/// Owner of the music index. Scans eagerly on construction, then refreshes
/// incrementally via `mtime`/`size` (no full re-parse of unchanged files).
pub struct Library {
    root: PathBuf,
    tracks: Vec<Track>,
    /// Walk errors (e.g. permission denied subdirectories) surfaced distinctly.
    pub walk_errors: Vec<String>,
    artist_source: ArtistSource,
    favorites: Vec<String>,
    baseline: Option<Vec<Track>>,
}

/// Persistent snapshot of the index (path + mtime + size) so `msc library
/// refresh` can detect renames/removals across CLI invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Baseline {
    root: String,
    tracks: Vec<BaselineTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BaselineTrack {
    path: String,
    mtime: u64,
    size: u64,
}

impl Baseline {
    fn load(root: &Path) -> Option<Baseline> {
        let path = baseline_path(root);
        let raw = std::fs::read_to_string(&path).ok()?;
        let b: Baseline = toml::from_str(&raw).ok()?;
        if b.root == root.to_string_lossy() {
            Some(b)
        } else {
            None
        }
    }

    fn save(&self) {
        let path = baseline_path(Path::new(&self.root));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(body) = toml::to_string(self) {
            let _ = option_sdk::atomic_write(path, body.as_bytes());
        }
    }

    fn into_tracks(self) -> Vec<Track> {
        self.tracks
            .into_iter()
            .map(|t| Track::from_path_with_stat(PathBuf::from(t.path), t.mtime, t.size))
            .collect()
    }
}

fn baseline_path(root: &Path) -> PathBuf {
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| format!("-{s}"))
        .unwrap_or_default();
    crate::config::cache_dir().join(format!("library-baseline{name}.toml"))
}

impl Library {
    /// Scan `root` once, indexing and tag-enriching every audio file.
    pub fn scan(root: PathBuf) -> Result<Self> {
        let (paths, walk_errors) = walk_audio(&root)?;
        let mut tracks: Vec<Track> = paths.into_iter().map(Track::new).collect();
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        tracks.dedup_by(|a, b| a.path == b.path);
        let config = AppConfig::load();
        let baseline = Baseline::load(&root).map(Baseline::into_tracks);
        Ok(Self {
            root,
            tracks,
            walk_errors,
            artist_source: config.artist_source,
            favorites: config.favorites.clone(),
            baseline,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Track> {
        self.tracks.get(index)
    }

    /// How artists are derived: embedded metadata or parent folder name.
    pub fn artist_source(&self) -> ArtistSource {
        self.artist_source
    }

    /// Artist name for a track, honoring `artist_source`.
    pub fn artist_name(&self, track: &Track) -> String {
        match self.artist_source {
            ArtistSource::Metadata => track
                .artist
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(str::trim)
                .unwrap_or("Unknown Artist")
                .to_owned(),
            ArtistSource::Folder => track
                .path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| "Unknown Artist".into()),
        }
    }

    /// Album name for a track (falls back to "Unknown Album").
    pub fn album_name(&self, track: &Track) -> String {
        track
            .album
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(str::trim)
            .unwrap_or("Unknown Album")
            .to_owned()
    }

    /// Incremental refresh: prune removed paths, re-scan changed files,
    /// add new files. Unchanged tracks keep their cached tags untouched.
    ///
    /// Comparison baseline is the persisted index (path + mtime + size), so
    /// renames and removals made between CLI invocations are detected too.
    pub fn refresh(&mut self) -> Result<ScanResult> {
        let (paths, walk_errors) = walk_audio(&self.root)?;
        self.walk_errors = walk_errors;

        let disk: HashMap<PathBuf, (u64, u64)> = paths
            .into_iter()
            .map(|p| {
                let (mtime, size) = file_stat(&p);
                (p, (mtime, size))
            })
            .collect();

        let baseline = self
            .baseline
            .take()
            .unwrap_or_else(|| std::mem::take(&mut self.tracks));
        let fresh: HashMap<PathBuf, Track> = self
            .tracks
            .drain(..)
            .map(|t| (t.path.clone(), t))
            .collect();

        let mut added = 0usize;
        let mut removed = 0usize;
        let mut changed = 0usize;
        let mut next: Vec<Track> = Vec::with_capacity(disk.len().max(baseline.len()));

        for old in baseline {
            match disk.get(&old.path) {
                Some(&(mtime, size)) if mtime == old.mtime && size == old.size => {
                    // Prefer the freshly scanned (enriched) track when present.
                    if let Some(f) = fresh.get(&old.path) {
                        next.push(f.clone());
                    } else {
                        next.push(old);
                    }
                }
                Some(_) => {
                    changed += 1;
                    next.push(Track::new(old.path));
                }
                None => {
                    removed += 1;
                }
            }
        }

        let known: HashSet<PathBuf> = next.iter().map(|t| t.path.clone()).collect();
        for (path, _) in disk.iter() {
            if !known.contains(path) {
                added += 1;
                next.push(Track::new(path.clone()));
            }
        }

        next.sort_by(|a, b| a.path.cmp(&b.path));
        next.dedup_by(|a, b| a.path == b.path);

        // Persist the new baseline for the next invocation.
        let baseline = Baseline {
            root: self.root.to_string_lossy().into_owned(),
            tracks: next
                .iter()
                .map(|t| BaselineTrack {
                    path: t.path.to_string_lossy().into_owned(),
                    mtime: t.mtime,
                    size: t.size,
                })
                .collect(),
        };
        baseline.save();

        self.tracks = next;
        self.baseline = None;
        Ok(ScanResult {
            added,
            removed,
            changed,
            total: self.tracks.len(),
        })
    }

    // ── Grouping ──────────────────────────────────────────────

    /// Group a track index slice by artist (metadata or folder). Sorted by name.
    pub fn artists(&self, indices: &[usize]) -> Vec<ArtistGroup> {
        let mut groups: BTreeMap<String, (HashSet<String>, usize)> = BTreeMap::new();
        for &i in indices {
            let Some(t) = self.tracks.get(i) else { continue };
            let name = self.artist_name(t);
            let entry = groups.entry(name).or_default();
            entry.0.insert(self.album_name(t));
            entry.1 += 1;
        }
        groups
            .into_iter()
            .map(|(name, (albums, count))| ArtistGroup {
                name,
                albums: albums.len(),
                tracks: count,
            })
            .collect()
    }

    /// Group a track index slice by album. Sorted by name (year shown when present).
    pub fn albums(&self, indices: &[usize]) -> Vec<AlbumGroup> {
        let mut groups: BTreeMap<String, (Option<u32>, usize)> = BTreeMap::new();
        for &i in indices {
            let Some(t) = self.tracks.get(i) else { continue };
            let name = self.album_name(t);
            let entry = groups.entry(name).or_default();
            if entry.0.is_none() {
                entry.0 = t.year;
            }
            entry.1 += 1;
        }
        groups
            .into_iter()
            .map(|(name, (year, count))| AlbumGroup {
                name,
                year,
                tracks: count,
            })
            .collect()
    }

    /// Track indices belonging to one artist, grouped into albums for the tree.
    pub fn albums_of_artist(&self, indices: &[usize], artist: &str) -> Vec<AlbumGroup> {
        let artist_idx: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| self.tracks.get(i).is_some_and(|t| self.artist_name(t) == artist))
            .collect();
        self.albums(&artist_idx)
    }

    /// Track indices for one album of one artist, album/track ordered.
    pub fn tracks_of_album(&self, indices: &[usize], artist: &str, album: &str) -> Vec<usize> {
        let mut out = indices
            .iter()
            .copied()
            .filter(|&i| {
                self.tracks.get(i).is_some_and(|t| {
                    self.artist_name(t) == artist && self.album_name(t) == album
                })
            })
            .collect::<Vec<_>>();
        out.sort_by(|&a, &b| {
            let (a, b) = (&self.tracks[a], &self.tracks[b]);
            a.disc_number
                .cmp(&b.disc_number)
                .then_with(|| a.track_number.cmp(&b.track_number))
                .then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    /// Track indices for one album across every artist (for a plain album list).
    pub fn tracks_of_album_all(&self, indices: &[usize], album: &str) -> Vec<usize> {
        let mut out = indices
            .iter()
            .copied()
            .filter(|&i| self.tracks.get(i).is_some_and(|t| self.album_name(t) == album))
            .collect::<Vec<_>>();
        out.sort_by(|&a, &b| {
            let (a, b) = (&self.tracks[a], &self.tracks[b]);
            a.track_number
                .cmp(&b.track_number)
                .then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    // ── Filtering ─────────────────────────────────────────────

    /// Predicate-based filter over the whole library, returning indices.
    pub fn filter(
        &self,
        genre: Option<&str>,
        year: Option<u32>,
        artist: Option<&str>,
        album: Option<&str>,
    ) -> Vec<usize> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                genre.map_or(true, |g| {
                    t.genre
                        .as_deref()
                        .map(|tg| tg.eq_ignore_ascii_case(g.trim()))
                        .unwrap_or(false)
                }) && year.map_or(true, |y| t.year == Some(y))
                    && artist.map_or(true, |a| self.artist_name(t) == a)
                    && album.map_or(true, |al| self.album_name(t) == al)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Subset matching genre/year, ordered by `(album, track_number)` —
    /// the "smart playlist" building block.
    pub fn tracks_matching(&self, genre: Option<&str>, year: Option<u32>) -> Vec<usize> {
        let mut out = self.filter(genre, year, None, None);
        out.sort_by(|&a, &b| {
            let (a, b) = (&self.tracks[a], &self.tracks[b]);
            self.album_name(a)
                .cmp(&self.album_name(b))
                .then_with(|| a.track_number.cmp(&b.track_number))
                .then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    /// Track indices honoring genre/year filters plus an optional search query
    /// and favorites-only mode — the base set for the browse tree.
    pub fn matching(
        &self,
        genre: Option<&str>,
        year: Option<u32>,
        query: Option<&str>,
        favorites_only: bool,
    ) -> Vec<usize> {
        let q = query.map(str::trim).filter(|s| !s.is_empty());
        let out = self.tracks.iter().enumerate().filter(|(_, t)| {
            if let Some(g) = genre {
                if !t
                    .genre
                    .as_deref()
                    .map(|tg| tg.eq_ignore_ascii_case(g.trim()))
                    .unwrap_or(false)
                {
                    return false;
                }
            }
            if let Some(y) = year {
                if t.year != Some(y) {
                    return false;
                }
            }
            if favorites_only && !self.is_favorite(&t.path) {
                return false;
            }
            if let Some(q) = q {
                if !self.matches_query(t, q) {
                    return false;
                }
            }
            true
        });
        out.map(|(i, _)| i).collect()
    }

    fn matches_query(&self, track: &Track, query: &str) -> bool {
        let hay = [
            track.title.as_deref().unwrap_or(""),
            track.artist.as_deref().unwrap_or(""),
            track.album.as_deref().unwrap_or(""),
            track.genre.as_deref().unwrap_or(""),
            &self.artist_name(track),
            &self.album_name(track),
            &track.year.map(|y| y.to_string()).unwrap_or_default(),
            &track
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase(),
        ]
        .join(" ")
        .to_lowercase();
        query
            .split_whitespace()
            .all(|tok| hay.contains(&tok.to_lowercase()))
    }

    /// Distinct genres present (sorted, case-preserving first occurrence).
    pub fn genres(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for t in &self.tracks {
            if let Some(g) = t.genre.as_deref().filter(|s| !s.trim().is_empty()) {
                let key = g.trim().to_lowercase();
                if seen.insert(key) {
                    out.push(g.trim().to_owned());
                }
            }
        }
        out.sort();
        out
    }

    /// Distinct years present, sorted ascending.
    pub fn years(&self) -> Vec<u32> {
        let mut set: HashSet<u32> = self.tracks.iter().filter_map(|t| t.year).collect();
        let mut out: Vec<u32> = set.drain().collect();
        out.sort();
        out
    }

    // ── Search ────────────────────────────────────────────────

    /// Unified search across name / artist / album / genre / year.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let q = query.trim();
        if q.is_empty() {
            return (0..self.tracks.len()).collect();
        }
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| self.matches_query(t, q))
            .map(|(i, _)| i)
            .collect()
    }

    // ── Favorites ─────────────────────────────────────────────

    /// Track ids are absolute path strings (same convention as the desktop).
    pub fn favorites(&self) -> &[String] {
        &self.favorites
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        let id = path.to_string_lossy();
        self.favorites.iter().any(|f| *f == id)
    }

    /// Toggle favorite by path; persists via `AppConfig`. Returns new state.
    pub fn toggle_favorite(&mut self, path: &Path) -> Result<bool> {
        let id = path.to_string_lossy().into_owned();
        let on = if let Some(i) = self.favorites.iter().position(|f| f == &id) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(id);
            true
        };
        let mut config = AppConfig::load();
        config.favorites = self.favorites.clone();
        config.save()?;
        Ok(on)
    }

    /// The whole library as a playable `Playlist` (paths only).
    pub fn to_playlist(&self) -> Playlist {
        let paths: Vec<PathBuf> = self.tracks.iter().map(|t| t.path.clone()).collect();
        Playlist::from_paths(&paths).unwrap_or_default()
    }
}

impl Default for Library {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            tracks: Vec::new(),
            walk_errors: Vec::new(),
            artist_source: ArtistSource::Metadata,
            favorites: Vec::new(),
            baseline: None,
        }
    }
}

/// Walk `root` collecting audio paths plus human-readable walk errors
/// (e.g. permission-denied subtrees) so they can be surfaced distinctly.
fn walk_audio(root: &Path) -> Result<(Vec<PathBuf>, Vec<String>)> {
    if root.is_file() {
        if crate::playlist::is_audio_file(root) {
            return Ok((vec![root.to_path_buf()], Vec::new()));
        }
        return Ok((Vec::new(), Vec::new()));
    }
    if !root.is_dir() {
        anyhow::bail!("not a file or directory: {}", root.display());
    }

    let mut paths = Vec::new();
    let mut errors = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        match entry {
            Ok(e) => {
                let p = e.path();
                if p.is_file() && crate::playlist::is_audio_file(p) {
                    paths.push(p.to_path_buf());
                }
            }
            Err(err) => {
                let msg = format!(
                    "{} ({})",
                    err.path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| root.display().to_string()),
                    err.io_error()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "walk error".into())
                );
                if !errors.contains(&msg) {
                    errors.push(msg);
                }
            }
        }
    }
    Ok((paths, errors))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "optionmusic-lib-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scan_indexes_audio_only() {
        let dir = tmp_tree("scan");
        touch(&dir.join("a.mp3"), "x");
        touch(&dir.join("b.flac"), "y");
        touch(&dir.join("notes.txt"), "z");
        let lib = Library::scan(dir.clone()).unwrap();
        assert_eq!(lib.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_detects_add_remove_change() {
        let dir = tmp_tree("refresh");
        touch(&dir.join("keep.mp3"), "v1");
        touch(&dir.join("gone.mp3"), "g");
        let mut lib = Library::scan(dir.clone()).unwrap();
        assert_eq!(lib.len(), 2);

        // changed: rewrite body of keep.mp3
        touch(&dir.join("keep.mp3"), "v2-very-different-bytes");
        // added
        touch(&dir.join("new.mp3"), "n");
        // removed
        fs::remove_file(dir.join("gone.mp3")).unwrap();

        let res = lib.refresh().unwrap();
        assert_eq!(res.added, 1, "new.mp3 added");
        assert_eq!(res.removed, 1, "gone.mp3 removed");
        assert_eq!(res.changed, 1, "keep.mp3 changed");
        assert_eq!(res.total, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_is_idempotent() {
        let dir = tmp_tree("idem");
        touch(&dir.join("a.mp3"), "x");
        touch(&dir.join("b.mp3"), "y");
        let mut lib = Library::scan(dir.clone()).unwrap();
        let first = lib.refresh().unwrap();
        assert_eq!((first.added, first.removed, first.changed), (0, 0, 0));
        assert_eq!(first.total, 2);
        let again = lib.refresh().unwrap();
        assert_eq!((again.added, again.removed, again.changed), (0, 0, 0));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn genres_and_years_collect() {
        let dir = tmp_tree("genres");
        let _ = Library::scan(dir.clone()).unwrap();
        assert!(Library::scan(dir.clone()).unwrap().genres().is_empty());
        assert!(Library::scan(dir.clone()).unwrap().years().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn filter_combines_predicates() {
        let dir = tmp_tree("filter");
        touch(&dir.join("rock.mp3"), "r");
        let lib = Library::scan(dir.clone()).unwrap();
        assert!(lib.filter(Some("Rock"), None, None, None).is_empty());
        assert!(lib.filter(None, Some(1999), None, None).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_rejects_missing_root() {
        assert!(Library::scan(PathBuf::from("/tmp/optionmusic_nope_xyz")).is_err());
    }
}
