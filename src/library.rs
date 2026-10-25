//! Library core for the CLI: scanning, incremental refresh, grouping,
//! filtering and search.
//!
//! `Library` owns the music index under a root directory. It is the single
//! source of truth for `msc library`, `msc browse` and `msc radio`.
//!
//! The index is persisted in SQLite at `~/.option/music/cache/library.db`
//! (table `tracks`). `scan` hydrates tag metadata from the index when a file
//! is unchanged (no lofty re-parse); `refresh` re-parses only new/changed
//! files (by `mtime` + `size`), prunes removed paths, and bulk-writes the
//! delta in one transaction so 10k+ track libraries stay fast.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::Deserialize;

use crate::config::{AppConfig, ArtistSource};
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
}

/// SQLite cache for the library index.
///
/// Location: `~/.option/music/cache/library.db`, table `tracks`:
/// `path` PK, `mtime`, `size`, `title`, `artist`, `album`, `genre`,
/// `year`, `duration`.
pub const LIBRARY_DB_FILENAME: &str = "library.db";

/// DDL for the persisted library index. `tracks.path` is the absolute path
/// primary key; one row describes one audio file on disk.
pub const LIBRARY_SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS tracks (
    path     TEXT PRIMARY KEY,
    mtime    INTEGER NOT NULL,
    size     INTEGER NOT NULL,
    title    TEXT,
    artist   TEXT,
    album    TEXT,
    genre    TEXT,
    year     INTEGER,
    duration REAL
);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tracks_mtime ON tracks(mtime);
";

/// Schema version written to `meta(schema_version)` by [`open_db`].
pub const LIBRARY_SCHEMA_VERSION: i64 = 1;

impl Library {
    /// Scan `root`, hydrating unchanged files from the SQLite index so a
    /// second invocation parses zero lofty tags. Files missing from the
    /// index (or whose `mtime`/`size` drifted) are parsed and upserted;
    /// files absent from disk are pruned lazily on [`Library::refresh`]
    /// and eagerly for single-file roots.
    pub fn scan(root: PathBuf) -> Result<Self> {
        let (paths, walk_errors) = walk_audio(&root)?;
        let mut cached = load_cached_tracks();
        migrate_legacy_baseline(&root, &mut cached);
        let mut tracks: Vec<Track> = Vec::with_capacity(paths.len());
        let mut fresh: Vec<Track> = Vec::new();
        for path in &paths {
            let (mtime, size) = file_stat(path);
            match cached.remove(path) {
                Some(hit) if hit.mtime == mtime && hit.size == size => tracks.push(hit),
                _ => {
                    let track = Track::new(path.clone());
                    fresh.push(track.clone());
                    tracks.push(track);
                }
            }
        }
        tracks.sort_by(|a, b| a.path.cmp(&b.path));
        tracks.dedup_by(|a, b| a.path == b.path);
        if !fresh.is_empty() {
            save_cached_tracks(&fresh);
        }
        let config = AppConfig::load();
        Ok(Self {
            root,
            tracks,
            walk_errors,
            artist_source: config.artist_source,
            favorites: config.favorites.clone(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Scan several roots into one library (multi-folder `music_dirs`).
    /// Tracks merge sorted + deduped by path; the first root stays the
    /// display root. Empty input behaves like scanning a missing dir.
    pub fn scan_many(roots: &[PathBuf]) -> Result<Self> {
        let Some((first, rest)) = roots.split_first() else {
            anyhow::bail!("no library folders configured");
        };
        let mut lib = Self::scan(first.clone())?;
        for root in rest {
            let other = Self::scan(root.clone())?;
            lib.walk_errors.extend(other.walk_errors);
            lib.tracks.extend(other.tracks);
        }
        lib.tracks.sort_by(|a, b| a.path.cmp(&b.path));
        lib.tracks.dedup_by(|a, b| a.path == b.path);
        Ok(lib)
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
    /// add new files. Unchanged tracks keep their cached tags untouched —
    /// zero lofty re-parse for files whose `mtime`+`size` match the SQLite
    /// row — and the delta (upserts + deletes) commits in one transaction.
    ///
    /// Comparison baseline is the persisted SQLite index, so renames and
    /// removals made between CLI invocations are detected too.
    pub fn refresh(&mut self) -> Result<ScanResult> {
        let root = self.root.clone();
        self.refresh_roots(std::slice::from_ref(&root))
    }

    /// Refresh across several roots (multi-folder libraries). Same delta
    /// semantics as [`Library::refresh`], merged over all roots.
    pub fn refresh_roots(&mut self, roots: &[PathBuf]) -> Result<ScanResult> {
        let mut disk: HashMap<PathBuf, (u64, u64)> = HashMap::new();
        let mut walk_errors: Vec<String> = Vec::new();
        for root in roots {
            let (paths, errors) = walk_audio(root)?;
            walk_errors.extend(errors);
            for p in paths {
                let (mtime, size) = file_stat(&p);
                disk.insert(p, (mtime, size));
            }
        }
        self.walk_errors = walk_errors;

        let mut cached = load_cached_tracks();
        migrate_legacy_baseline(&self.root, &mut cached);

        let mut added = 0usize;
        let mut removed = 0usize;
        let mut changed = 0usize;
        let mut next: Vec<Track> = Vec::with_capacity(disk.len());
        let mut upsert: Vec<Track> = Vec::new();

        for (path, (mtime, size)) in &disk {
            match cached.remove(path) {
                Some(hit) if hit.mtime == *mtime && hit.size == *size => next.push(hit),
                Some(old) => {
                    // Same path, stat drifted → edited file or tag rewrite.
                    let _ = old;
                    changed += 1;
                    let track = Track::new(path.clone());
                    upsert.push(track.clone());
                    next.push(track);
                }
                None => {
                    added += 1;
                    let track = Track::new(path.clone());
                    upsert.push(track.clone());
                    next.push(track);
                }
            }
        }

        // Anything left in the index under these roots that is no longer
        // on disk is a removal. (Rows for other roots stay untouched so
        // two music dirs can share one `library.db`.)
        let mut pruned: Vec<PathBuf> = Vec::new();
        for (path, _) in cached {
            if roots.iter().any(|r| path_starts_with_root(&path, r)) {
                removed += 1;
                pruned.push(path);
            }
        }

        next.sort_by(|a, b| a.path.cmp(&b.path));
        next.dedup_by(|a, b| a.path == b.path);

        if !upsert.is_empty() || !pruned.is_empty() {
            persist_delta(&upsert, &pruned);
        }

        self.tracks = next;
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
            let Some(t) = self.tracks.get(i) else {
                continue;
            };
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
            let Some(t) = self.tracks.get(i) else {
                continue;
            };
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
            .filter(|&i| {
                self.tracks
                    .get(i)
                    .is_some_and(|t| self.artist_name(t) == artist)
            })
            .collect();
        self.albums(&artist_idx)
    }

    /// Track indices for one album of one artist, album/track ordered.
    pub fn tracks_of_album(&self, indices: &[usize], artist: &str, album: &str) -> Vec<usize> {
        let mut out = indices
            .iter()
            .copied()
            .filter(|&i| {
                self.tracks
                    .get(i)
                    .is_some_and(|t| self.artist_name(t) == artist && self.album_name(t) == album)
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
            .filter(|&i| {
                self.tracks
                    .get(i)
                    .is_some_and(|t| self.album_name(t) == album)
            })
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
                genre.is_none_or(|g| {
                    t.genre
                        .as_deref()
                        .map(|tg| tg.eq_ignore_ascii_case(g.trim()))
                        .unwrap_or(false)
                }) && year.is_none_or(|y| t.year == Some(y))
                    && artist.is_none_or(|a| self.artist_name(t) == a)
                    && album.is_none_or(|al| self.album_name(t) == al)
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
            if let Some(g) = genre
                && !t
                    .genre
                    .as_deref()
                    .map(|tg| tg.eq_ignore_ascii_case(g.trim()))
                    .unwrap_or(false)
            {
                return false;
            }
            if let Some(y) = year
                && t.year != Some(y)
            {
                return false;
            }
            if favorites_only && !self.is_favorite(&t.path) {
                return false;
            }
            if let Some(q) = q
                && !self.matches_query(t, q)
            {
                return false;
            }
            true
        });
        out.map(|(i, _)| i).collect()
    }

    fn matches_query(&self, track: &Track, query: &str) -> bool {
        track_matches_query(
            track,
            query,
            &self.artist_name(track),
            &self.album_name(track),
        )
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

    /// Track ids are absolute path strings.
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
        }
    }
}

/// Token-based search shared by `Library::search`/`matching` and radio seed
/// resolution: every whitespace-separated token must appear in the combined
/// haystack (case-insensitive). `artist`/`album` are the display names
/// resolved by the caller's artist/album naming rules.
pub(crate) fn track_matches_query(track: &Track, query: &str, artist: &str, album: &str) -> bool {
    let hay = [
        track.title.as_deref().unwrap_or(""),
        track.artist.as_deref().unwrap_or(""),
        track.album.as_deref().unwrap_or(""),
        track.genre.as_deref().unwrap_or(""),
        artist,
        album,
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

// ── SQLite index (`~/.option/music/cache/library.db`) ───────────────

/// Absolute path of the SQLite index file. Created lazily on first write;
/// readers tolerate a missing file (cold cache → full parse).
pub fn library_db_path() -> PathBuf {
    crate::config::cache_dir().join(LIBRARY_DB_FILENAME)
}

/// Open (creating parent dirs) and migrate the index to [`LIBRARY_SCHEMA`].
/// Returns `None` when SQLite itself is unusable so callers fall back to a
/// full in-memory parse instead of failing the command.
fn open_db() -> Option<Connection> {
    let path = library_db_path();
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return None;
    }
    let conn = Connection::open(&path).ok()?;
    // WAL keeps concurrent readers (browse + player) from blocking writers;
    // synchronous=NORMAL + a busy timeout ride out lock contention.
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let _ = conn.pragma_update(None, "synchronous", "NORMAL");
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    conn.execute_batch(LIBRARY_SCHEMA).ok()?;
    let _ = conn.execute(
        "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![LIBRARY_SCHEMA_VERSION.to_string()],
    );
    Some(conn)
}

/// Load every cached row as path → enriched `Track` (tags hydrated, marked
/// enriched so [`Track::enrich_tags`] is a no-op for unchanged files).
fn load_cached_tracks() -> HashMap<PathBuf, Track> {
    let path = library_db_path();
    if !path.is_file() {
        return HashMap::new();
    }
    let Some(conn) = open_db() else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    let mut stmt = match conn.prepare(
        "SELECT path, mtime, size, title, artist, album, genre, year, duration
         FROM tracks",
    ) {
        Ok(s) => s,
        Err(_) => return out,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, Option<f64>>(8)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for row in rows.filter_map(|r| r.ok()) {
        let (spath, mtime, size, title, artist, album, genre, year, duration) = row;
        let mut t = Track::from_path_with_stat(
            PathBuf::from(spath),
            mtime.max(0) as u64,
            size.max(0) as u64,
        );
        t.title = title.filter(|s| !s.is_empty());
        t.artist = artist.filter(|s| !s.is_empty());
        t.album = album.filter(|s| !s.is_empty());
        t.genre = genre.filter(|s| !s.is_empty());
        t.year = year
            .and_then(|y| u32::try_from(y.max(0)).ok())
            .filter(|y| *y > 0);
        t.duration_secs = duration.filter(|d| d.is_finite() && *d > 0.0);
        // Cover presence is resolved lazily (sidecar check is cheap); mark
        // unknown so browse/player probe on demand.
        t.has_cover = None;
        t.tags_enriched = true;
        out.insert(t.path.clone(), t);
    }
    out
}

/// Upsert freshly parsed tracks into the index. Best-effort: a failed write
/// only costs the next invocation a re-parse, never a command failure.
fn save_cached_tracks(tracks: &[Track]) {
    if tracks.is_empty() {
        return;
    }
    let Some(mut conn) = open_db() else { return };
    // One transaction: 10k cold-start rows commit in ms, not seconds.
    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(_) => return,
    };
    {
        let mut stmt = match tx.prepare(
            "INSERT INTO tracks
             (path, mtime, size, title, artist, album, genre, year, duration)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
               mtime = excluded.mtime, size = excluded.size,
               title = excluded.title, artist = excluded.artist,
               album = excluded.album, genre = excluded.genre,
               year = excluded.year, duration = excluded.duration",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        for t in tracks {
            let _ = stmt.execute(params![
                t.path.to_string_lossy().as_ref(),
                t.mtime as i64,
                t.size as i64,
                t.title.as_deref(),
                t.artist.as_deref(),
                t.album.as_deref(),
                t.genre.as_deref(),
                t.year.map(|y| y as i64),
                t.duration_secs,
            ]);
        }
    }
    let _ = tx.commit();
}

/// Apply an incremental delta — upsert new/changed rows, delete pruned
/// paths — in a single transaction so 10k-file refreshes stay O(delta),
/// not O(library), on the write path.
fn persist_delta(upsert: &[Track], pruned: &[PathBuf]) {
    if upsert.is_empty() && pruned.is_empty() {
        return;
    }
    let Some(mut conn) = open_db() else { return };
    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(_) => return,
    };
    {
        let mut ins = match tx.prepare(
            "INSERT INTO tracks
             (path, mtime, size, title, artist, album, genre, year, duration)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
               mtime = excluded.mtime, size = excluded.size,
               title = excluded.title, artist = excluded.artist,
               album = excluded.album, genre = excluded.genre,
               year = excluded.year, duration = excluded.duration",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        for t in upsert {
            let _ = ins.execute(params![
                t.path.to_string_lossy().as_ref(),
                t.mtime as i64,
                t.size as i64,
                t.title.as_deref(),
                t.artist.as_deref(),
                t.album.as_deref(),
                t.genre.as_deref(),
                t.year.map(|y| y as i64),
                t.duration_secs,
            ]);
        }
        drop(ins);
        if !pruned.is_empty() {
            let mut del = match tx.prepare("DELETE FROM tracks WHERE path = ?1") {
                Ok(s) => s,
                Err(_) => return,
            };
            for p in pruned {
                let _ = del.execute(params![p.to_string_lossy().as_ref()]);
            }
        }
    }
    let _ = tx.commit();
}

/// Whether `path` lives under `root` (or *is* a file root). Guards the
/// shared-DB prune so `msc -m ~/Music` never deletes rows owned by
/// `msc -m ~/Podcasts`.
fn path_starts_with_root(path: &Path, root: &Path) -> bool {
    if root.is_file() {
        return path == root;
    }
    path.starts_with(root)
}

/// One-time migration of the pre-SQLite per-root TOML baselines
/// (`library-baseline*.toml`: path + mtime + size only, no tags) into
/// `library.db`.
///
/// Rows are inserted with NULL tags — the next `scan`/`refresh` parses tags
/// for exactly those rows and upserts them. No data is lost: every known
/// path survives as a row; tag enrichment just happens lazily. Legacy files
/// are removed after a successful import so the migration runs once.
fn migrate_legacy_baseline(root: &Path, cached: &mut HashMap<PathBuf, Track>) {
    let names = legacy_baseline_paths(root);
    if names.iter().all(|p| !p.is_file()) {
        return;
    }
    #[derive(Debug, Deserialize)]
    struct LegacyBaseline {
        #[serde(default)]
        root: String,
        #[serde(default)]
        tracks: Vec<LegacyTrack>,
    }
    #[derive(Debug, Deserialize)]
    struct LegacyTrack {
        #[serde(default)]
        path: String,
        #[serde(default)]
        mtime: u64,
        #[serde(default)]
        size: u64,
    }
    let mut imported: Vec<Track> = Vec::new();
    let mut consumed: Vec<PathBuf> = Vec::new();
    for legacy in &names {
        let Ok(raw) = std::fs::read_to_string(legacy) else {
            continue;
        };
        // Tolerate both TOML (current writer) and JSON (older cache shape):
        // either way only path/mtime/size survive, tags re-parse lazily.
        let parsed: Option<LegacyBaseline> = toml::from_str(&raw)
            .ok()
            .or_else(|| serde_json::from_str(&raw).ok());
        let Some(base) = parsed else { continue };
        if !base.root.is_empty() && Path::new(&base.root) != root {
            continue;
        }
        for t in base.tracks {
            if t.path.is_empty() {
                continue;
            }
            let p = PathBuf::from(&t.path);
            if cached.contains_key(&p) || imported.iter().any(|x: &Track| x.path == p) {
                continue;
            }
            imported.push(Track::from_path_with_stat(p, t.mtime, t.size));
        }
        consumed.push(legacy.clone());
    }
    if imported.is_empty() {
        for legacy in consumed {
            let _ = std::fs::remove_file(&legacy);
        }
        return;
    }
    // Persist as tag-less rows; do not overwrite richer SQLite rows.
    if open_db().is_some() {
        save_cached_tracks(&imported);
        for t in imported {
            cached.entry(t.path.clone()).or_insert(t);
        }
        for legacy in consumed {
            let _ = std::fs::remove_file(&legacy);
        }
    }
}

/// Candidate legacy baseline files for `root`: the current per-root TOML
/// name plus the historical JSON name (`library-cache*.json`) so both
/// migrate without data loss.
fn legacy_baseline_paths(root: &Path) -> Vec<PathBuf> {
    let stem = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| format!("-{s}"))
        .unwrap_or_default();
    let dir = crate::config::cache_dir();
    vec![
        dir.join(format!("library-baseline{stem}.toml")),
        dir.join(format!("library-cache{stem}.json")),
        dir.join("library-baseline.toml"),
        dir.join("library-cache.json"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Serialize library tests: they share the real `library.db` path via
    /// `OPTION_HOME` isolation would need process-global env mutation, so a
    /// mutex keeps parallel test threads from interleaving DB state.
    static LIB_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Redirect `OPTION_HOME` + `HOME` at `~/.option/music/cache/library.db`
    /// to a scratch dir. Returns the guard (lock + env restore on drop).
    struct LibraryDbGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        old_option_home: Option<std::ffi::OsString>,
        old_home: Option<std::ffi::OsString>,
        _dir: PathBuf,
    }

    impl LibraryDbGuard {
        fn take(name: &str) -> Self {
            let lock = LIB_TEST_LOCK.lock().unwrap();
            let dir = std::env::temp_dir()
                .join(format!("stillmusic-libdb-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join(".option").join("music").join("cache")).unwrap();
            let old_option_home = std::env::var_os("OPTION_HOME");
            let old_home = std::env::var_os("HOME");
            // SAFETY: tests run sequentially behind LIB_TEST_LOCK.
            unsafe {
                std::env::set_var("OPTION_HOME", dir.join(".option"));
                std::env::set_var("HOME", &dir);
            }
            Self {
                _lock: lock,
                old_option_home,
                old_home,
                _dir: dir,
            }
        }
    }

    impl Drop for LibraryDbGuard {
        fn drop(&mut self) {
            // SAFETY: still holding LIB_TEST_LOCK; no other test observes env.
            unsafe {
                match self.old_option_home.take() {
                    Some(v) => std::env::set_var("OPTION_HOME", v),
                    None => std::env::remove_var("OPTION_HOME"),
                }
                match self.old_home.take() {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    fn tmp_tree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stillmusic-lib-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scan_indexes_audio_only() {
        let _db = LibraryDbGuard::take("scan");
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
        let _db = LibraryDbGuard::take("refresh");
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
        let _db = LibraryDbGuard::take("idem");
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
    fn sqlite_index_persists_across_invocations() {
        // Simulates two CLI runs: scan → drop → scan again. The second scan
        // must hydrate from library.db (same mtime/size → same tags object
        // without re-parse) and refresh must report zero delta.
        let _db = LibraryDbGuard::take("persist");
        assert!(!library_db_path().is_file());
        let dir = tmp_tree("persist");
        touch(&dir.join("a.mp3"), "x");
        let lib = Library::scan(dir.clone()).unwrap();
        assert_eq!(lib.len(), 1);
        assert!(library_db_path().is_file());
        drop(lib);
        let lib2 = Library::scan(dir.clone()).unwrap();
        assert_eq!(lib2.len(), 1);
        assert!(lib2.tracks()[0].tags_enriched);
        let mut lib2 = lib2;
        let res = lib2.refresh().unwrap();
        assert_eq!(
            (res.added, res.removed, res.changed, res.total),
            (0, 0, 0, 1)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sqlite_migrates_legacy_baseline_without_loss() {
        // Legacy TOML baseline (path+mtime+size only) must become rows in
        // library.db; orphan legacy files are consumed afterwards.
        let _db = LibraryDbGuard::take("migrate");
        let dir = tmp_tree("migrate");
        touch(&dir.join("old.mp3"), "data");
        let (mtime, size) = file_stat(&dir.join("old.mp3"));
        let stem = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("migrate");
        let legacy = crate::config::cache_dir().join(format!("library-baseline-{stem}.toml"));
        fs::write(
            &legacy,
            toml::to_string(&serde_json::json!({
                "root": dir.to_string_lossy(),
                "tracks": [{
                    "path": dir.join("old.mp3").to_string_lossy(),
                    "mtime": mtime,
                    "size": size,
                }],
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(!library_db_path().is_file());
        let lib = Library::scan(dir.clone()).unwrap();
        assert_eq!(lib.len(), 1);
        assert!(
            library_db_path().is_file(),
            "migration must create library.db"
        );
        assert!(!legacy.is_file(), "legacy baseline consumed after import");
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
        assert!(Library::scan(PathBuf::from("/tmp/stillmusic_nope_xyz")).is_err());
    }

    #[test]
    fn scan_many_merges_roots_sorted_deduped() {
        let _db = LibraryDbGuard::take("many");
        let a = tmp_tree("many-a");
        let b = tmp_tree("many-b");
        touch(&a.join("one.mp3"), "1");
        touch(&b.join("two.mp3"), "2");
        let lib = Library::scan_many(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(lib.len(), 2);
        assert_eq!(lib.root(), &a);
        let paths: Vec<_> = lib.tracks().iter().map(|t| t.path.clone()).collect();
        assert_eq!(paths, {
            let mut sorted = vec![a.join("one.mp3"), b.join("two.mp3")];
            sorted.sort();
            sorted
        });
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn scan_many_rejects_empty() {
        let _db = LibraryDbGuard::take("many-empty");
        assert!(Library::scan_many(&[]).is_err());
    }

    #[test]
    fn refresh_roots_covers_all_folders() {
        let _db = LibraryDbGuard::take("many-refresh");
        let a = tmp_tree("many-r-a");
        let b = tmp_tree("many-r-b");
        touch(&a.join("one.mp3"), "1");
        touch(&b.join("two.mp3"), "2");
        let mut lib = Library::scan_many(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(lib.len(), 2);
        touch(&b.join("three.mp3"), "3");
        let res = lib.refresh_roots(&[a.clone(), b.clone()]).unwrap();
        assert_eq!(
            (res.added, res.removed, res.changed, res.total),
            (1, 0, 0, 3)
        );
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }
}
