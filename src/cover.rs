//! Resolve album cover art from sidecar images or embedded tags.

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::file::TaggedFileExt;
use std::path::{Path, PathBuf};

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

/// Prefer a folder sidecar image; otherwise pull embedded front cover.
pub fn resolve_cover_data_url(audio: &Path) -> Result<Option<String>> {
    if let Some(path) = find_sidecar_cover(audio) {
        let bytes = std::fs::read(&path)?;
        let mime = mime_from_path(&path);
        return Ok(Some(data_url(mime, &bytes)));
    }
    if let Some((mime, bytes)) = embedded_cover(audio) {
        return Ok(Some(data_url(&mime, &bytes)));
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
    let rd = std::fs::read_dir(parent).ok()?;
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
        let dir = std::env::temp_dir().join(format!("optmusic-cover-{}", std::process::id()));
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
