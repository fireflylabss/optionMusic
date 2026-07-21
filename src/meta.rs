//! Read common audio tags (title / artist / album) via lofty.

use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct AudioTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
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

    let title = tag.title().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let album = tag.album().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
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

    AudioTags {
        title,
        artist,
        album,
    }
}
