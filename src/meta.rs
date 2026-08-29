//! Read/write audio tags, lyrics, and ReplayGain via lofty.

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagType};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{self, stable_cache_key};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub disc_number: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub replaygain_track_gain: Option<f64>,
    #[serde(default)]
    pub replaygain_album_gain: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LyricsKind {
    Embedded,
    Lrc,
    Text,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lyrics {
    pub kind: LyricsKind,
    pub text: String,
}

fn tag_cache_path(path: &Path, mtime: u64, size: u64) -> std::path::PathBuf {
    let key = stable_cache_key(&[
        path.to_string_lossy().as_bytes(),
        mtime.to_string().as_bytes(),
        size.to_string().as_bytes(),
        b"v3",
    ]);
    config::tags_cache_dir().join(format!("{key}.toml"))
}

fn read_tag_cache(path: &Path, mtime: u64, size: u64) -> Option<AudioTags> {
    let cache = tag_cache_path(path, mtime, size);
    let raw = fs::read_to_string(&cache).ok()?;
    toml::from_str(&raw).ok()
}

fn write_tag_cache(path: &Path, mtime: u64, size: u64, tags: &AudioTags) {
    let dir = config::tags_cache_dir();
    let _ = fs::create_dir_all(&dir);
    let cache = tag_cache_path(path, mtime, size);
    if let Ok(body) = toml::to_string(tags) {
        let _ = fs::write(cache, body);
    }
}

fn invalidate_tag_cache(path: &Path, mtime: u64, size: u64) {
    let cache = tag_cache_path(path, mtime, size);
    let _ = fs::remove_file(cache);
}

fn parse_u32_item(tag: &lofty::tag::Tag, key: ItemKey) -> Option<u32> {
    tag.get_string(key)
        .and_then(|s| s.split(['/', ';']).next())
        .and_then(|s| s.trim().parse().ok())
}

fn parse_gain(raw: &str) -> Option<f64> {
    let s = raw
        .trim()
        .trim_end_matches("dB")
        .trim_end_matches("db")
        .trim();
    s.parse().ok()
}

/// Best-effort tag read; missing or unreadable files yield empty tags.
pub fn read_tags(audio: &Path) -> AudioTags {
    let Ok(probe) = Probe::open(audio) else {
        return AudioTags::default();
    };
    let Ok(tagged) = probe.read() else {
        return AudioTags::default();
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return AudioTags::default();
    };

    let title = tag
        .title()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let album = tag
        .album()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let artist = tag
        .artist()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            tag.get_string(ItemKey::AlbumArtist)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        });
    let track_number = tag
        .track()
        .or_else(|| parse_u32_item(tag, ItemKey::TrackNumber));
    let disc_number = tag
        .disk()
        .or_else(|| parse_u32_item(tag, ItemKey::DiscNumber));
    let genre = tag
        .genre()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let year = tag
        .date()
        .map(|y| y.year as u32)
        .or_else(|| parse_u32_item(tag, ItemKey::Year));
    let replaygain_track_gain = tag
        .get_string(ItemKey::ReplayGainTrackGain)
        .and_then(parse_gain);
    let replaygain_album_gain = tag
        .get_string(ItemKey::ReplayGainAlbumGain)
        .and_then(parse_gain);

    AudioTags {
        title,
        artist,
        album,
        track_number,
        disc_number,
        genre,
        year,
        replaygain_track_gain,
        replaygain_album_gain,
    }
}

/// Read tags from disk cache when valid, otherwise parse the file and cache the result.
pub fn read_tags_cached(audio: &Path, mtime: u64, size: u64) -> AudioTags {
    if let Some(cached) = read_tag_cache(audio, mtime, size) {
        return cached;
    }
    let tags = read_tags(audio);
    write_tag_cache(audio, mtime, size, &tags);
    tags
}

/// Write common tags back to the file (title / artist / album / track / disc).
pub fn write_tags(audio: &Path, tags: &AudioTags) -> Result<()> {
    if !audio.is_file() {
        bail!("not a file: {}", audio.display());
    }
    let mut tagged = Probe::open(audio)
        .with_context(|| format!("open {}", audio.display()))?
        .read()
        .with_context(|| format!("read tags {}", audio.display()))?;

    let tag_type = tagged
        .primary_tag()
        .map(|t| t.tag_type())
        .or_else(|| tagged.first_tag().map(|t| t.tag_type()))
        .unwrap_or(TagType::Id3v2);

    if tagged.primary_tag().is_none() && tagged.first_tag().is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }

    {
        let has_primary = tagged.primary_tag().is_some();
        let tag = if has_primary {
            tagged.primary_tag_mut().context("no writable tag")?
        } else {
            tagged.first_tag_mut().context("no writable tag")?
        };

        if let Some(title) = tags.title.as_deref() {
            tag.set_title(title.to_owned());
        }
        if let Some(artist) = tags.artist.as_deref() {
            tag.set_artist(artist.to_owned());
        }
        if let Some(album) = tags.album.as_deref() {
            tag.set_album(album.to_owned());
        }
        if let Some(n) = tags.track_number {
            tag.set_track(n);
        }
        if let Some(n) = tags.disc_number {
            tag.set_disk(n);
        }
    }

    tagged
        .save_to_path(audio, WriteOptions::default())
        .with_context(|| format!("save tags {}", audio.display()))?;

    let (mtime, size) = file_stat(audio);
    invalidate_tag_cache(audio, mtime, size);
    write_tag_cache(audio, mtime, size, tags);
    Ok(())
}

fn file_stat(path: &Path) -> (u64, u64) {
    fs::metadata(path)
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

/// Prefer sidecar `.lrc` / `.txt`, then embedded unsynced lyrics.
pub fn read_lyrics(audio: &Path) -> Lyrics {
    let stem = audio.with_extension("");
    for (ext, kind) in [("lrc", LyricsKind::Lrc), ("txt", LyricsKind::Text)] {
        let side = stem.with_extension(ext);
        if let Ok(text) = fs::read_to_string(&side) {
            let text = text.trim().to_owned();
            if !text.is_empty() {
                return Lyrics { kind, text };
            }
        }
    }

    let Ok(probe) = Probe::open(audio) else {
        return Lyrics {
            kind: LyricsKind::None,
            text: String::new(),
        };
    };
    let Ok(tagged) = probe.read() else {
        return Lyrics {
            kind: LyricsKind::None,
            text: String::new(),
        };
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Lyrics {
            kind: LyricsKind::None,
            text: String::new(),
        };
    };
    if let Some(text) = tag
        .get_string(ItemKey::Lyrics)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
    {
        return Lyrics {
            kind: LyricsKind::Embedded,
            text,
        };
    }
    Lyrics {
        kind: LyricsKind::None,
        text: String::new(),
    }
}

/// Properties useful for duration display in `info` (best-effort).
pub fn duration_secs(audio: &Path) -> Option<f64> {
    let probe = Probe::open(audio).ok()?;
    let tagged = probe.read().ok()?;
    Some(tagged.properties().duration().as_secs_f64())
}
