//! Resolve album cover art from sidecar images or embedded tags.

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use lofty::probe::Probe;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{self, stable_cache_key};

const SIDECAR_NAMES: &[&str] = &[
    "cover.jpg",
    "cover.jpeg",
    "cover.png",
    "cover.webp",
    "folder.jpg",
    "folder.jpeg",
    "folder.png",
    "album.jpg",
    "album.jpeg",
    "album.png",
    "front.jpg",
    "front.jpeg",
    "front.png",
    "artwork.jpg",
    "artwork.jpeg",
    "artwork.png",
    "AlbumArt.jpg",
    "AlbumArt.png",
];

/// Resolved cover on disk (sidecar or cached extract). Prefer this over data URLs in IPC.
#[derive(Debug, Clone)]
pub struct CoverFile {
    pub path: PathBuf,
}

/// Prefer a folder sidecar image; otherwise pull embedded front cover.
pub fn resolve_cover_data_url(audio: &Path, mtime: u64, size: u64) -> Result<Option<String>> {
    if let Some(file) = resolve_cover_file(audio, mtime, size)? {
        let bytes = fs::read(&file.path)?;
        let mime = mime_from_path(&file.path);
        return Ok(Some(data_url(mime, &bytes)));
    }
    Ok(None)
}

/// Resolve cover to a local file path (sidecar or disk cache). Uses audio mtime/size when provided.
pub fn resolve_cover_file(audio: &Path, mtime: u64, size: u64) -> Result<Option<CoverFile>> {
    if let Some(path) = find_sidecar_cover(audio) {
        return Ok(Some(CoverFile { path }));
    }
    if let Some(path) = cached_embedded_cover(audio, mtime, size)? {
        return Ok(Some(CoverFile { path }));
    }
    Ok(None)
}

pub fn find_sidecar_cover(audio: &Path) -> Option<PathBuf> {
    let parent = audio.parent()?;

    if let Some(stem) = audio.file_stem().and_then(|s| s.to_str()) {
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let candidate = parent.join(format!("{stem}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    for name in SIDECAR_NAMES {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Case-insensitive scan of common cover basenames in the folder.
    let rd = fs::read_dir(parent).ok()?;
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !matches!(
            ext.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp"
        ) {
            continue;
        }
        let stem_l = stem.to_ascii_lowercase();
        if matches!(
            stem_l.as_str(),
            "cover" | "folder" | "album" | "front" | "artwork" | "albumart" | "albumartsmall"
        ) {
            return Some(path);
        }
    }
    None
}

fn cover_cache_path(audio: &Path, mtime: u64, size: u64, ext: &str) -> PathBuf {
    let key = stable_cache_key(&[
        audio.to_string_lossy().as_bytes(),
        mtime.to_string().as_bytes(),
        size.to_string().as_bytes(),
        b"cover",
    ]);
    config::covers_cache_dir().join(format!("{key}.{ext}"))
}

fn cached_embedded_cover(audio: &Path, mtime: u64, size: u64) -> Result<Option<PathBuf>> {
    let (mtime, size) = if mtime == 0 && size == 0 {
        fs::metadata(audio)
            .ok()
            .map(|m| {
                let mt = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (mt, m.len())
            })
            .unwrap_or((0, 0))
    } else {
        (mtime, size)
    };

    // Probe cache with common extensions before extracting.
    for ext in ["jpg", "jpeg", "png", "webp"] {
        let candidate = cover_cache_path(audio, mtime, size, ext);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    let Some((mime, bytes)) = embedded_cover(audio) else {
        return Ok(None);
    };
    let ext = ext_from_mime(&mime);
    let cache = cover_cache_path(audio, mtime, size, ext);
    let dir = config::covers_cache_dir();
    fs::create_dir_all(&dir)?;
    fs::write(&cache, &bytes)?;
    Ok(Some(cache))
}

fn embedded_cover(audio: &Path) -> Option<(String, Vec<u8>)> {
    let tagged = Probe::open(audio).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let pictures = tag.pictures();
    if pictures.is_empty() {
        return None;
    }
    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first())?;
    let mime = picture
        .mime_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "image/jpeg".into());
    Some((mime, picture.data().to_vec()))
}

fn ext_from_mime(mime: &str) -> &'static str {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

fn mime_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_sidecar_cover_jpg() {
        let dir = std::env::temp_dir().join(format!("optionmusic-cover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("track.mp3");
        let cover = dir.join("cover.jpg");
        fs::write(&audio, b"x").unwrap();
        fs::write(&cover, b"fake").unwrap();
        assert_eq!(find_sidecar_cover(&audio), Some(cover));
        let _ = fs::remove_dir_all(&dir);
    }
}
