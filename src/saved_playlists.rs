//! Named local playlists + M3U import/export.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{self, stable_cache_key};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPlaylist {
    pub id: String,
    pub name: String,
    /// Absolute track path ids.
    pub tracks: Vec<String>,
    /// Optional original M3U path when imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn playlists_dir() -> PathBuf {
    config::config_dir().join("playlists")
}

fn playlist_path(id: &str) -> PathBuf {
    playlists_dir().join(format!("{id}.toml"))
}

fn ensure_dir() -> Result<()> {
    fs::create_dir_all(playlists_dir())
        .with_context(|| format!("create {}", playlists_dir().display()))
}

fn new_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "playlist" } else { slug };
    let key = stable_cache_key(&[slug.as_bytes(), &now_bytes()]);
    format!("{slug}-{}", &key[..8])
}

fn now_bytes() -> [u8; 8] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    n.to_le_bytes()
}

pub fn list() -> Result<Vec<SavedPlaylist>> {
    ensure_dir()?;
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(playlists_dir()) else {
        return Ok(out);
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(pl) = toml::from_str::<SavedPlaylist>(&raw) {
                out.push(pl);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn get(id: &str) -> Result<SavedPlaylist> {
    let path = playlist_path(id);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("playlist not found: {id}"))?;
    Ok(toml::from_str(&raw)?)
}

fn write_playlist(pl: &SavedPlaylist) -> Result<()> {
    ensure_dir()?;
    let path = playlist_path(&pl.id);
    let body = toml::to_string_pretty(pl)?;
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn create(name: &str) -> Result<SavedPlaylist> {
    let name = name.trim();
    if name.is_empty() {
        bail!("playlist name is empty");
    }
    let pl = SavedPlaylist {
        id: new_id(name),
        name: name.to_owned(),
        tracks: Vec::new(),
        source: None,
    };
    write_playlist(&pl)?;
    Ok(pl)
}

pub fn rename(id: &str, name: &str) -> Result<SavedPlaylist> {
    let mut pl = get(id)?;
    let name = name.trim();
    if name.is_empty() {
        bail!("playlist name is empty");
    }
    pl.name = name.to_owned();
    write_playlist(&pl)?;
    Ok(pl)
}

pub fn delete(id: &str) -> Result<()> {
    let path = playlist_path(id);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn add_track(id: &str, track_id: &str) -> Result<SavedPlaylist> {
    let mut pl = get(id)?;
    if !pl.tracks.iter().any(|t| t == track_id) {
        pl.tracks.push(track_id.to_owned());
        write_playlist(&pl)?;
    }
    Ok(pl)
}

pub fn remove_track(id: &str, track_id: &str) -> Result<SavedPlaylist> {
    let mut pl = get(id)?;
    pl.tracks.retain(|t| t != track_id);
    write_playlist(&pl)?;
    Ok(pl)
}

/// Parse an M3U / M3U8 file into absolute paths (relative lines resolve against the M3U dir).
pub fn parse_m3u(path: &Path) -> Result<Vec<PathBuf>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tracks = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let candidate = if Path::new(line).is_absolute() {
            PathBuf::from(line)
        } else {
            base.join(line)
        };
        if candidate.exists() {
            tracks.push(candidate);
        }
    }
    Ok(tracks)
}

pub fn import_m3u(path: &Path, name: Option<&str>) -> Result<SavedPlaylist> {
    let paths = parse_m3u(path)?;
    if paths.is_empty() {
        bail!("no playable paths found in {}", path.display());
    }
    let name = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported")
                .to_owned()
        });
    let mut pl = create(&name)?;
    pl.tracks = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    pl.source = Some(path.to_string_lossy().into_owned());
    write_playlist(&pl)?;
    Ok(pl)
}

pub fn export_m3u(id: &str, out: &Path) -> Result<()> {
    let pl = get(id)?;
    let mut body = String::from("#EXTM3U\n");
    for track in &pl.tracks {
        body.push_str(track);
        body.push('\n');
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, body).with_context(|| format!("write {}", out.display()))?;
    Ok(())
}
