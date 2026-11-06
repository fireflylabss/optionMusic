//! Named local playlists + M3U import/export.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{self, stable_cache_key};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn playlist_path(id: &str) -> Result<PathBuf> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("invalid playlist id: {id}");
    }
    Ok(playlists_dir().join(format!("{id}.toml")))
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
    let pid = std::process::id().to_le_bytes();
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes();
    let key = stable_cache_key(&[slug.as_bytes(), &now_bytes(), &pid, &counter]);
    format!("{slug}-{key}")
}

fn now_bytes() -> [u8; 16] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
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
        if let Ok(raw) = fs::read_to_string(&path)
            && let Ok(pl) = toml::from_str::<SavedPlaylist>(&raw)
        {
            out.push(pl);
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

pub fn get(id: &str) -> Result<SavedPlaylist> {
    let path = playlist_path(id)?;
    let raw = fs::read_to_string(&path).with_context(|| format!("playlist not found: {id}"))?;
    Ok(toml::from_str(&raw)?)
}

fn write_playlist(pl: &SavedPlaylist) -> Result<()> {
    ensure_dir()?;
    let path = playlist_path(&pl.id)?;
    let body = toml::to_string_pretty(pl)?;
    option_sdk::atomic_write(&path, body.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub fn create(name: &str) -> Result<SavedPlaylist> {
    let name = name.trim();
    if name.is_empty() {
        bail!("playlist name is empty");
    }
    let mut pl = SavedPlaylist {
        id: new_id(name),
        name: name.to_owned(),
        tracks: Vec::new(),
        source: None,
    };
    while playlist_path(&pl.id)?.exists() {
        pl.id = new_id(name);
    }
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
    let path = playlist_path(id)?;
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
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
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
    option_sdk::atomic_write(out, body.as_bytes())
        .with_context(|| format!("write {}", out.display()))?;
    Ok(())
}

/// Write `paths` as an extended M3U with absolute paths, so the file works
/// from any directory. Returns the number of tracks written.
pub fn export_paths_m3u(paths: &[std::path::PathBuf], out: &Path) -> Result<usize> {
    if paths.is_empty() {
        bail!("queue is empty");
    }
    let mut body = String::from("#EXTM3U\n");
    for p in paths {
        let abs = std::path::absolute(p).unwrap_or_else(|_| p.clone());
        let title = abs.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        body.push_str(&format!("#EXTINF:-1,{title}\n{}\n", abs.display()));
    }
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    option_sdk::atomic_write(out, body.as_bytes())
        .with_context(|| format!("write {}", out.display()))?;
    Ok(paths.len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_paths_writes_absolute_extm3u() {
        let dir = std::env::temp_dir().join(format!("still-m3u-{}", std::process::id()));
        let out = dir.join("q.m3u");
        let n = super::export_paths_m3u(&[std::path::PathBuf::from("/a/b/one.mp3")], &out).unwrap();
        assert_eq!(n, 1);
        let body = std::fs::read_to_string(&out).unwrap();
        assert!(body.starts_with("#EXTM3U\n#EXTINF:-1,one\n/a/b/one.mp3"));
        assert!(super::export_paths_m3u(&[], &out).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn playlist_ids_are_single_safe_segments() {
        assert!(playlist_path("../escape").is_err());
        assert!(playlist_path("nested/id").is_err());
        assert!(playlist_path("UPPER").is_err());
        assert!(playlist_path("safe-id-0123").is_ok());
    }

    #[test]
    fn new_ids_include_subsecond_entropy() {
        let first = new_id("Same name");
        let second = new_id("Same name");
        assert_ne!(first, second);
        assert!(first.starts_with("same-name-"));
        assert!(second.starts_with("same-name-"));
    }
}
