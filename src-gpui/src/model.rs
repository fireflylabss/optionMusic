use std::path::PathBuf;

use gpui::{Entity, Pixels, Point, SharedString, Subscription};
use serde::{Deserialize, Serialize};

use optionmusic::controller::{LoopMode, PlaybackState, TrackDto};

use crate::search_input::SearchInput;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Library,
    Artists,
    Albums,
    Playlists,
    Favorites,
    Shelves,
}

impl Page {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Artists => "artists",
            Self::Albums => "albums",
            Self::Playlists => "playlists",
            Self::Favorites => "favorites",
            Self::Shelves => "shelves",
        }
    }

    pub(crate) fn from_slug(slug: &str) -> Self {
        match slug {
            "artists" => Self::Artists,
            "albums" => Self::Albums,
            "playlists" => Self::Playlists,
            "favorites" => Self::Favorites,
            "shelves" => Self::Shelves,
            _ => Self::Library,
        }
    }
}

/// Which list the keyboard `TrackList` context is driving.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusedList {
    Tracks,
    Artists,
    ArtistTracks,
    Albums,
    AlbumTracks,
    Favorites,
    Playlists,
    Shelves,
    ShelfTracks,
    Queue,
}

#[derive(Clone)]
pub(crate) struct ContextMenuState {
    pub(crate) track_id: String,
    pub(crate) position: Point<Pixels>,
    pub(crate) from_queue: bool,
}

/// Anchored "add to playlist" picker opened from the context menu.
#[derive(Clone)]
pub(crate) struct PlaylistPickerState {
    pub(crate) track_id: String,
    pub(crate) position: Point<Pixels>,
    pub(crate) selection: usize,
}

/// What the shared single-line `name_input` is currently editing.
#[derive(Clone)]
pub(crate) enum NameTarget {
    NewPlaylist,
    NewPlaylistForTrack(String),
    RenamePlaylist(String),
}

/// Action executed when the confirm popover is accepted.
#[derive(Clone)]
pub(crate) enum ConfirmAction {
    DeletePlaylist(String),
    ClearQueue,
}

#[derive(Clone)]
pub(crate) struct ConfirmRequest {
    pub(crate) title: SharedString,
    pub(crate) detail: SharedString,
    pub(crate) confirm_label: SharedString,
    pub(crate) action: ConfirmAction,
}

/// Items the track context menu can dispatch, in render order.
/// Separators are visual only and not part of this list.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Play,
    PlayNext,
    QueueAdd,
    QueueRemove,
    Favorite,
    AddToPlaylist,
    EditTags,
    CopyPath,
    Reveal,
}

/// Per-row cover art lookup state (resolved off the app thread).
#[derive(Clone)]
pub(crate) enum CoverSlot {
    Loading,
    Ready(Option<PathBuf>),
}

/// Which bottom panel the stage column shows (`None` = hidden).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StagePanel {
    Queue,
    Lyrics,
}

/// One grouped album for the Albums page index.
#[derive(Clone)]
pub(crate) struct AlbumEntry {
    pub(crate) key: SharedString,
    pub(crate) name: SharedString,
    pub(crate) artist: SharedString,
    pub(crate) track_ids: Vec<String>,
}

/// Open tag editor: one single-line input per editable field.
pub(crate) struct TagEditorState {
    pub(crate) track_id: String,
    pub(crate) title: Entity<SearchInput>,
    pub(crate) artist: Entity<SearchInput>,
    pub(crate) album: Entity<SearchInput>,
    pub(crate) track_number: Entity<SearchInput>,
    pub(crate) disc_number: Entity<SearchInput>,
    pub(crate) year: Entity<SearchInput>,
    /// Keeps the field subscriptions alive for the editor's lifetime.
    pub(crate) _subs: Vec<Subscription>,
}

/// UI state persisted as JSON inside `desktop_preferences` in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DesktopPrefs {
    pub(crate) window_w: f32,
    pub(crate) window_h: f32,
    pub(crate) page: String,
    pub(crate) stage_open: bool,
    /// "queue" | "lyrics" | null (panel hidden).
    pub(crate) panel: Option<String>,
}

impl Default for DesktopPrefs {
    fn default() -> Self {
        Self {
            window_w: 1200.0,
            window_h: 760.0,
            page: "library".into(),
            stage_open: true,
            panel: None,
        }
    }
}

pub(crate) fn empty_playback() -> PlaybackState {
    PlaybackState {
        queue: Vec::new(),
        current: None,
        position: 0.0,
        duration: None,
        paused: true,
        stopped: true,
        volume: 100,
        muted: false,
        speed: 1.0,
        pitch: 1.0,
        eq: "off".into(),
        favorites: Vec::new(),
        loop_mode: LoopMode::Off,
        shuffled: false,
    }
}

pub(crate) fn fmt_time(secs: f64) -> String {
    let secs = secs.max(0.0) as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub(crate) fn cover_glyph(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "o".into())
}

pub(crate) fn folder_label(folder: &str) -> String {
    let normalized = folder.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    parts.last().copied().unwrap_or(folder).to_string()
}

pub(crate) fn track_meta(track: &TrackDto) -> String {
    let artist = track.artist.trim();
    if artist.is_empty() {
        folder_label(&track.folder)
    } else {
        artist.to_string()
    }
}

pub(crate) fn track_artist(track: &TrackDto) -> String {
    let artist = track.artist.trim();
    if artist.is_empty() {
        folder_label(&track.folder)
    } else {
        artist.to_string()
    }
}

/// Display album for a track: tag when present, else the containing folder
/// (mirrors `track_artist`'s fallback so untagged libs still group sanely).
pub(crate) fn track_album(track: &TrackDto) -> String {
    let album = track.album.trim();
    if album.is_empty() {
        folder_label(&track.folder)
    } else {
        album.to_string()
    }
}

/// Stable grouping key for the Albums index: artist + album, NUL-separated.
pub(crate) fn track_album_key(track: &TrackDto) -> String {
    format!("{}\u{1f}{}", track_artist(track), track_album(track))
}

pub(crate) struct SliderDrag(pub(crate) &'static str);
pub(crate) struct ScrollbarDrag(pub(crate) &'static str);
