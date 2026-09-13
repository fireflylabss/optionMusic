use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Context, Entity, ExternalPaths, FocusHandle, PathPromptOptions, Pixels, Point, Render, Role,
    ScrollHandle, ScrollStrategy, SharedString, Subscription, Task, UniformListScrollHandle,
    Window, deferred, div, prelude::*, px,
};
use optionmusic::config::{ArtistSource, ReplayGainMode};
use optionmusic::controller::{CoreController, PlaybackState, SmartShelf, TrackDto};
use optionmusic::eq::EqPreset;
use optionmusic::rpc::Rpc;
use optionmusic::saved_playlists::SavedPlaylist;

use crate::model::*;
use crate::search_input::{SearchEvent, SearchInput};
use crate::theme::*;
use crate::{
    BlurList, ClearQueue, CycleLoop, DismissOverlay, FavoriteCurrent, FocusList, ListActivate,
    ListDown, ListFirst, ListLast, ListUp, MenuActivate, MenuDown, MenuLeft, MenuRight, MenuUp,
    Mute, NavAlbums, NavArtists, NavFavorites, NavLibrary, NavPlaylists, NavShelves, Next,
    PlayPause, Previous, QueueItemDown, QueueItemUp, QueueJump, SeekBack, SeekForward, Shuffle,
    SliderEnd, SliderHome, SliderLeft, SliderRight, Stop, ToggleLyrics, ToggleQueue, ToggleSearch,
    ToggleStage, VolumeDown, VolumeUp,
};

pub(crate) struct RootView {
    pub(crate) controller: Option<CoreController>,
    pub(crate) library: Vec<TrackDto>,
    /// Library filtered by `search_query` (regenerated only in `apply_filter`).
    pub(crate) filtered_library: Rc<[TrackDto]>,
    pub(crate) artists_index: Vec<(SharedString, usize)>,
    pub(crate) filtered_artists: Vec<(SharedString, usize)>,
    pub(crate) selected_artist: Option<SharedString>,
    pub(crate) artist_tracks: Rc<[TrackDto]>,
    pub(crate) albums_index: Vec<AlbumEntry>,
    pub(crate) filtered_albums: Vec<AlbumEntry>,
    pub(crate) selected_album: Option<SharedString>,
    pub(crate) album_tracks: Rc<[TrackDto]>,
    pub(crate) playlists: Vec<SavedPlaylist>,
    pub(crate) filtered_playlists: Vec<SavedPlaylist>,
    pub(crate) filtered_favorites: Rc<[TrackDto]>,
    pub(crate) selected_shelf: Option<SmartShelf>,
    pub(crate) shelf_tracks: Rc<[TrackDto]>,
    pub(crate) playback: PlaybackState,
    /// Discord Rich Presence client — lazily connects, deduped internally,
    /// cleared on drop (window close / quit).
    pub(crate) rpc: Rpc,
    pub(crate) status: SharedString,
    pub(crate) status_generation: u64,
    pub(crate) search_input: Entity<SearchInput>,
    pub(crate) search_active: bool,
    /// Lowercased copy of the search input — readable without cx.
    pub(crate) search_query: String,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) artist_scroll_handle: UniformListScrollHandle,
    pub(crate) album_scroll_handle: UniformListScrollHandle,
    pub(crate) fav_scroll_handle: UniformListScrollHandle,
    pub(crate) shelf_scroll_handle: UniformListScrollHandle,
    pub(crate) playlist_scroll_handle: UniformListScrollHandle,
    pub(crate) queue_scroll_handle: UniformListScrollHandle,
    /// Scroll state for the playlist picker's overflow list.
    pub(crate) picker_scroll: ScrollHandle,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) list_focus: FocusHandle,
    pub(crate) overlay_focus: FocusHandle,
    pub(crate) seek_focus: FocusHandle,
    pub(crate) volume_focus: FocusHandle,
    pub(crate) focused_list: Option<FocusedList>,
    pub(crate) focused_row: Option<usize>,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) menu_selection: usize,
    pub(crate) playlist_picker: Option<PlaylistPickerState>,
    pub(crate) pending_confirm: Option<ConfirmRequest>,
    /// Which button Enter triggers in `pending_confirm` (Left/Right swap it).
    pub(crate) confirm_accept_selected: bool,
    pub(crate) name_input: Entity<SearchInput>,
    pub(crate) name_target: Option<NameTarget>,
    pub(crate) tag_editor: Option<TagEditorState>,
    pub(crate) settings_open: bool,
    pub(crate) page: Page,
    pub(crate) stage_open: bool,
    pub(crate) stage_panel: Option<StagePanel>,
    pub(crate) current_cover: Option<PathBuf>,
    pub(crate) cover_track_id: Option<String>,
    /// Per-track cover cache shared by rows/hero/player; resolved off-thread.
    pub(crate) covers: RefCell<HashMap<String, CoverSlot>>,
    pub(crate) cover_tasks: RefCell<Vec<Task<()>>>,
    /// track id (path) → index into `library`, for queue/row lookups.
    pub(crate) track_index_by_id: HashMap<String, usize>,
    pub(crate) folders_count_cache: usize,
    pub(crate) lyrics: Option<SharedString>,
    pub(crate) lyrics_track: Option<String>,
    pub(crate) lyrics_scroll: ScrollHandle,
    pub(crate) drop_hover: bool,
    pub(crate) _scan_task: Option<Task<()>>,
    pub(crate) _poll_task: Option<Task<()>>,
    pub(crate) _cover_task: Option<Task<()>>,
    pub(crate) _status_task: Option<Task<()>>,
    pub(crate) _open_task: Option<Task<()>>,
    pub(crate) _enrich_task: Option<Task<()>>,
    pub(crate) _lyrics_task: Option<Task<()>>,
    pub(crate) _prefs_task: Option<Task<()>>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

impl RootView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let search_input = cx.new(SearchInput::new);
        let search_subscription = cx.subscribe(
            &search_input,
            |this, _input, event: &SearchEvent, cx| match event {
                SearchEvent::Changed => this.apply_filter(cx),
                SearchEvent::Submit => this.play_first_filtered(cx),
                SearchEvent::Dismiss => this.close_search(cx),
                // Tab leaves the input and enters the page's list.
                SearchEvent::TabNext => this.focus_current_list_detached(cx),
                SearchEvent::TabPrev => {}
            },
        );
        let name_input = cx.new(|cx| SearchInput::with_placeholder(cx, "Playlist name…"));
        let name_subscription = cx.subscribe(
            &name_input,
            |this, _input, event: &SearchEvent, cx| match event {
                SearchEvent::Changed => {}
                SearchEvent::Submit => this.on_name_submit(cx),
                SearchEvent::Dismiss => this.on_name_dismiss(cx),
                SearchEvent::TabNext | SearchEvent::TabPrev => {}
            },
        );
        let mut view = Self {
            controller: None,
            library: Vec::new(),
            filtered_library: Rc::from(Vec::new()),
            artists_index: Vec::new(),
            filtered_artists: Vec::new(),
            selected_artist: None,
            artist_tracks: Rc::from(Vec::new()),
            albums_index: Vec::new(),
            filtered_albums: Vec::new(),
            selected_album: None,
            album_tracks: Rc::from(Vec::new()),
            playlists: Vec::new(),
            filtered_playlists: Vec::new(),
            filtered_favorites: Rc::from(Vec::new()),
            selected_shelf: None,
            shelf_tracks: Rc::from(Vec::new()),
            playback: empty_playback(),
            rpc: Rpc::new(),
            status: "Scanning library…".into(),
            status_generation: 0,
            search_input,
            search_active: false,
            search_query: String::new(),
            scroll_handle: UniformListScrollHandle::new(),
            artist_scroll_handle: UniformListScrollHandle::new(),
            album_scroll_handle: UniformListScrollHandle::new(),
            fav_scroll_handle: UniformListScrollHandle::new(),
            shelf_scroll_handle: UniformListScrollHandle::new(),
            playlist_scroll_handle: UniformListScrollHandle::new(),
            queue_scroll_handle: UniformListScrollHandle::new(),
            picker_scroll: ScrollHandle::new(),
            focus_handle,
            list_focus: cx.focus_handle(),
            overlay_focus: cx.focus_handle(),
            seek_focus: cx.focus_handle(),
            volume_focus: cx.focus_handle(),
            focused_list: None,
            focused_row: None,
            context_menu: None,
            menu_selection: 0,
            playlist_picker: None,
            pending_confirm: None,
            confirm_accept_selected: true,
            name_input,
            name_target: None,
            tag_editor: None,
            settings_open: false,
            page: Page::Library,
            stage_open: true,
            stage_panel: None,
            current_cover: None,
            cover_track_id: None,
            covers: RefCell::new(HashMap::new()),
            cover_tasks: RefCell::new(Vec::new()),
            track_index_by_id: HashMap::new(),
            folders_count_cache: 0,
            lyrics: None,
            lyrics_track: None,
            lyrics_scroll: ScrollHandle::new(),
            drop_hover: false,
            _scan_task: None,
            _poll_task: None,
            _cover_task: None,
            _status_task: None,
            _open_task: None,
            _enrich_task: None,
            _lyrics_task: None,
            _prefs_task: None,
            _subscriptions: vec![search_subscription, name_subscription],
        };
        view.start_scan(cx);
        view
    }

    pub(crate) fn start_scan(&mut self, cx: &mut Context<Self>) {
        self._scan_task = Some(cx.spawn(async move |this, cx| {
            let mut controller = cx
                .background_executor()
                .spawn(async move {
                    let mut controller = CoreController::new();
                    let _ = controller.scan(None);
                    // Restore last session (queue/current/position) off-thread —
                    // it creates the MPV player and touches the resume blob.
                    let _ = controller.restore_session();
                    controller
                })
                .await;

            this.update(cx, move |view, cx| {
                view.playback = controller.playback_state();
                view.library = controller.snapshot().library;
                view.controller = Some(controller);
                view.refresh_playlists();
                view.apply_desktop_prefs(cx);
                view.rebuild_indexes();
                view.apply_filter(cx);
                view.set_status(
                    format!(
                        "{} tracks · {} folders",
                        view.library.len(),
                        view.folders_count_cache
                    ),
                    cx,
                );
                view.update_playback_and_cover(cx);
                view.start_enrichment(cx);
                view.start_poll(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    /// Pump deferred tag enrichment: `enrich_prepare` on the app thread, tag +
    /// cover reads on the background executor, `enrich_apply` back here.
    /// Loops until the controller reports no pending tracks.
    pub(crate) fn start_enrichment(&mut self, cx: &mut Context<Self>) {
        self._enrich_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let batch = this
                    .update(cx, |view, _| {
                        view.controller
                            .as_ref()
                            .map(|c| c.enrich_prepare(200))
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                if batch.is_empty() {
                    break;
                }
                let results = cx
                    .background_executor()
                    .spawn(async move {
                        batch
                            .into_iter()
                            .map(|(id, path, mtime, size)| {
                                let tags = optionmusic::meta::read_tags_cached(&path, mtime, size);
                                let has_cover =
                                    optionmusic::cover::resolve_cover_file(&path, mtime, size)
                                        .ok()
                                        .flatten()
                                        .is_some();
                                (id, tags, has_cover)
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                let more = this
                    .update(cx, |view, cx| {
                        let updated = view
                            .controller
                            .as_mut()
                            .map(|c| c.enrich_apply(results))
                            .unwrap_or_default();
                        if !updated.is_empty() {
                            let by_id: HashMap<&str, &TrackDto> =
                                updated.iter().map(|t| (t.id.as_str(), t)).collect();
                            for track in view.library.iter_mut() {
                                if let Some(u) = by_id.get(track.id.as_str()) {
                                    *track = (*u).clone();
                                }
                            }
                            // Cover paths may now exist — drop stale "no cover"
                            // results so rows retry through `cover_for`.
                            let mut covers = view.covers.borrow_mut();
                            for t in &updated {
                                if t.has_cover == Some(true)
                                    && matches!(covers.get(&t.id), Some(CoverSlot::Ready(None)))
                                {
                                    covers.remove(&t.id);
                                }
                            }
                            drop(covers);
                            view.rebuild_indexes();
                            view.apply_filter(cx);
                        }
                        view.controller
                            .as_ref()
                            .is_some_and(|c| c.tags_enrichment_pending())
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
            let _ = this.update(cx, |view, cx| {
                view.set_status("Metadata ready", cx);
            });
        }));
    }

    pub(crate) fn start_poll(&mut self, cx: &mut Context<Self>) {
        self._poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                this.update(cx, |view, cx| {
                    let changed = view
                        .controller
                        .as_mut()
                        .map(|controller| controller.playback_state())
                        .is_some_and(|next| {
                            if next != view.playback {
                                view.playback = next;
                                true
                            } else {
                                false
                            }
                        });
                    if changed {
                        view.spawn_cover_load(cx);
                        cx.notify();
                    }
                    view.sync_presence(cx);
                })
                .ok();
            }
        }));
    }

    /// Push the current playback state to Discord Rich Presence. Called every
    /// poll tick — `Rpc` dedupes pushes and throttles reconnects internally.
    /// Follows the `discord_rpc` config toggle live.
    pub(crate) fn sync_presence(&mut self, cx: &mut Context<Self>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if let Some(warn) = self.rpc.sync(
            controller.config.discord_rpc,
            &controller.config.discord_rpc_id,
        ) {
            self.set_status(warn, cx);
        }
        match self.playback.current.as_ref() {
            Some(track) if !self.playback.stopped => {
                // Same state line as the CLI: "artist — album" (either side
                // may be empty for untagged tracks).
                let state = match (track.artist.trim(), track.album.trim()) {
                    ("", "") => String::new(),
                    (a, "") => a.to_string(),
                    ("", b) => b.to_string(),
                    (a, b) => format!("{a} — {b}"),
                };
                self.rpc.update(
                    &track.name,
                    &state,
                    track.thumb_url.as_deref(),
                    Duration::from_secs_f64(self.playback.position),
                    self.playback.duration.map(Duration::from_secs_f64),
                    self.playback.paused,
                );
            }
            // Stopped / empty queue: drop the presence instead of lingering.
            _ => self.rpc.clear(),
        }
    }

    pub(crate) fn update_playback_and_cover(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            self.playback = controller.playback_state();
        }
        self.spawn_cover_load(cx);
    }

    pub(crate) fn spawn_cover_load(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.playback.current.as_ref() else {
            self.current_cover = None;
            self.cover_track_id = None;
            self.lyrics = None;
            self.lyrics_track = None;
            return;
        };
        if self.cover_track_id.as_ref() == Some(&track.id) {
            return;
        }
        let id = track.id.clone();
        let path = PathBuf::from(&track.path);
        let mtime = track.mtime;
        let size = track.size;
        self.cover_track_id = Some(id.clone());
        self._cover_task = Some(cx.spawn(async move |this, cx| {
            let cover_path = cx
                .background_executor()
                .spawn(async move {
                    optionmusic::cover::resolve_cover_file(&path, mtime, size)
                        .ok()
                        .flatten()
                        .map(|cover| cover.path)
                })
                .await;
            this.update(cx, |view, _| {
                // Warm the shared row cache so the playing track's row reuses it.
                view.covers
                    .borrow_mut()
                    .insert(id.clone(), CoverSlot::Ready(cover_path.clone()));
                if view.cover_track_id.as_ref() == Some(&id) {
                    view.current_cover = cover_path;
                }
            })
            .ok();
        }));
        self.load_lyrics(cx);
    }

    /// Load lyrics for the current track (single file read; cached by meta).
    pub(crate) fn load_lyrics(&mut self, cx: &mut Context<Self>) {
        let Some(track) = self.playback.current.clone() else {
            self.lyrics = None;
            self.lyrics_track = None;
            return;
        };
        if self.lyrics_track.as_ref() == Some(&track.id) {
            return;
        }
        self.lyrics_track = Some(track.id.clone());
        self.lyrics = None;
        let lyrics = self
            .controller
            .as_ref()
            .and_then(|c| c.track_lyrics(&track.id).ok());
        self.lyrics = lyrics.and_then(|l| {
            if l.text.trim().is_empty() {
                None
            } else {
                Some(SharedString::from(l.text))
            }
        });
        cx.notify();
    }

    /// Shared row-cover lookup: returns the resolved path when ready, else
    /// kicks a bounded background resolve (inserting `CoverSlot::Loading`).
    /// Call from renderers; results land in `covers` and notify on completion.
    pub(crate) fn cover_for(&self, track: &TrackDto, cx: &mut Context<Self>) -> Option<PathBuf> {
        const MAX_INFLIGHT: usize = 8;
        if let Some(slot) = self.covers.borrow().get(&track.id) {
            return match slot {
                CoverSlot::Ready(path) => path.clone(),
                CoverSlot::Loading => None,
            };
        }
        if track.has_cover == Some(false) {
            self.covers
                .borrow_mut()
                .insert(track.id.clone(), CoverSlot::Ready(None));
            return None;
        }
        let inflight = self
            .covers
            .borrow()
            .values()
            .filter(|s| matches!(s, CoverSlot::Loading))
            .count();
        if inflight >= MAX_INFLIGHT {
            return None;
        }
        self.covers
            .borrow_mut()
            .insert(track.id.clone(), CoverSlot::Loading);
        let id = track.id.clone();
        let path = PathBuf::from(&track.path);
        let (mtime, size) = (track.mtime, track.size);
        let task = cx.spawn(async move |this, cx| {
            let cover = cx
                .background_executor()
                .spawn(async move {
                    optionmusic::cover::resolve_cover_file(&path, mtime, size)
                        .ok()
                        .flatten()
                        .map(|c| c.path)
                })
                .await;
            this.update(cx, |view, cx| {
                view.covers.borrow_mut().insert(id, CoverSlot::Ready(cover));
                cx.notify();
            })
            .ok();
        });
        self.cover_tasks.borrow_mut().push(task);
        None
    }

    pub(crate) fn track_by_id(&self, id: &str) -> Option<&TrackDto> {
        self.track_index_by_id
            .get(id)
            .and_then(|&i| self.library.get(i))
    }

    pub(crate) fn matches_query(&self, text: &str) -> bool {
        self.search_query.is_empty() || text.to_lowercase().contains(&self.search_query)
    }

    pub(crate) fn track_matches_query(&self, track: &TrackDto) -> bool {
        self.search_query.is_empty()
            || self.matches_query(&track.name)
            || self.matches_query(&track.artist)
            || self.matches_query(&track.album)
            || self.matches_query(&track.folder)
    }

    pub(crate) fn set_status(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = message.into();
        self.status_generation += 1;
        let generation = self.status_generation;
        self._status_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            this.update(cx, |view, cx| {
                if view.status_generation == generation {
                    view.status = "".into();
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Rebuild every query-filtered view of the library. Runs only when the
    /// query or the underlying data changes — renderers read the cached
    /// `filtered_*`/`Rc` fields and never re-filter per frame.
    pub(crate) fn apply_filter(&mut self, cx: &mut Context<Self>) {
        self.search_query = self.search_input.read(cx).content.to_lowercase();

        self.filtered_library = Rc::from(
            self.library
                .iter()
                .filter(|t| self.track_matches_query(t))
                .cloned()
                .collect::<Vec<_>>(),
        );
        self.filtered_artists = self
            .artists_index
            .iter()
            .filter(|(name, _)| self.matches_query(name))
            .cloned()
            .collect();
        self.filtered_albums = self
            .albums_index
            .iter()
            .filter(|a| self.matches_query(&a.name) || self.matches_query(&a.artist))
            .cloned()
            .collect();
        self.filtered_playlists = self
            .playlists
            .iter()
            .filter(|p| self.matches_query(&p.name))
            .cloned()
            .collect();
        let favorites: HashSet<&String> = self.playback.favorites.iter().collect();
        self.filtered_favorites = Rc::from(
            self.library
                .iter()
                .filter(|t| favorites.contains(&t.id) && self.track_matches_query(t))
                .cloned()
                .collect::<Vec<_>>(),
        );
        // Detail lists keep their selection and get filtered in place.
        if let Some(artist) = self.selected_artist.clone() {
            self.artist_tracks = Rc::from(
                self.library
                    .iter()
                    .filter(|t| track_artist(t) == artist.as_ref() && self.track_matches_query(t))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(shelf) = self.selected_shelf {
            let full = self
                .controller
                .as_ref()
                .map(|c| c.smart_shelf(shelf))
                .unwrap_or_default();
            self.shelf_tracks = Rc::from(
                full.into_iter()
                    .filter(|t| self.track_matches_query(t))
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(key) = self.selected_album.clone() {
            self.album_tracks = Rc::from(
                self.library
                    .iter()
                    .filter(|t| track_album_key(t) == key.as_str() && self.track_matches_query(t))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
        if let (Some(list), Some(row)) = (self.focused_list, self.focused_row)
            && row >= self.list_len(list)
        {
            self.focused_row = None;
        }
        cx.notify();
    }

    pub(crate) fn refresh_playlists(&mut self) {
        if let Some(controller) = self.controller.as_mut() {
            self.playlists = controller.list_playlists().unwrap_or_default();
            self.filtered_playlists = self
                .playlists
                .iter()
                .filter(|p| self.matches_query(&p.name))
                .cloned()
                .collect();
        }
    }

    pub(crate) fn select_shelf(&mut self, shelf: SmartShelf, cx: &mut Context<Self>) {
        self.selected_shelf = Some(shelf);
        self.focused_list = Some(FocusedList::ShelfTracks);
        self.focused_row = None;
        self.apply_filter(cx);
    }

    pub(crate) fn clear_shelf(&mut self, cx: &mut Context<Self>) {
        self.selected_shelf = None;
        self.shelf_tracks = Rc::from(Vec::new());
        self.focused_list = Some(FocusedList::Shelves);
        self.focused_row = None;
        cx.notify();
    }

    pub(crate) fn rebuild_indexes(&mut self) {
        let mut artists: HashMap<SharedString, usize> = HashMap::new();
        let mut folders: HashSet<&str> = HashSet::new();
        let mut albums: HashMap<String, AlbumEntry> = HashMap::new();
        self.track_index_by_id.clear();
        for (i, track) in self.library.iter().enumerate() {
            self.track_index_by_id.insert(track.id.clone(), i);
            folders.insert(track.folder.as_str());
            let artist_key: SharedString = track_artist(track).into();
            *artists.entry(artist_key).or_insert(0) += 1;
            let key = track_album_key(track);
            let entry = albums.entry(key.clone()).or_insert_with(|| AlbumEntry {
                key: key.clone().into(),
                name: track_album(track).into(),
                artist: track_artist(track).into(),
                track_ids: Vec::new(),
            });
            entry.track_ids.push(track.id.clone());
        }
        self.folders_count_cache = folders.len();
        let mut artists_index: Vec<(SharedString, usize)> = artists.into_iter().collect();
        artists_index.sort_by(|a, b| a.0.cmp(&b.0));
        self.artists_index = artists_index;
        // Album tracks sorted by disc/track number for sensible "Play album".
        let mut albums_index: Vec<AlbumEntry> = albums.into_values().collect();
        for album in &mut albums_index {
            album.track_ids.sort_by_key(|id| {
                self.track_by_id(id)
                    .map(|t| (t.disc_number.unwrap_or(0), t.track_number.unwrap_or(0)))
                    .unwrap_or_default()
            });
        }
        albums_index.sort_by(|a, b| a.name.cmp(&b.name).then(a.artist.cmp(&b.artist)));
        self.albums_index = albums_index;

        if let Some(artist) = self.selected_artist.as_ref()
            && !self.artists_index.iter().any(|(name, _)| name == artist)
        {
            self.selected_artist = None;
            self.artist_tracks = Rc::from(Vec::new());
        }
        if let Some(key) = self.selected_album.as_ref()
            && !self.albums_index.iter().any(|a| &a.key == key)
        {
            self.selected_album = None;
            self.album_tracks = Rc::from(Vec::new());
        }
    }

    pub(crate) fn select_artist(&mut self, name: SharedString, cx: &mut Context<Self>) {
        self.selected_artist = Some(name);
        self.focused_list = Some(FocusedList::ArtistTracks);
        self.focused_row = None;
        self.apply_filter(cx);
    }

    pub(crate) fn clear_artist(&mut self, cx: &mut Context<Self>) {
        self.selected_artist = None;
        self.artist_tracks = Rc::from(Vec::new());
        self.focused_list = Some(FocusedList::Artists);
        self.focused_row = None;
        cx.notify();
    }

    pub(crate) fn select_album(&mut self, key: SharedString, cx: &mut Context<Self>) {
        self.selected_album = Some(key);
        self.focused_list = Some(FocusedList::AlbumTracks);
        self.focused_row = None;
        self.apply_filter(cx);
    }

    pub(crate) fn clear_album(&mut self, cx: &mut Context<Self>) {
        self.selected_album = None;
        self.album_tracks = Rc::from(Vec::new());
        self.focused_list = Some(FocusedList::Albums);
        self.focused_row = None;
        cx.notify();
    }

    /// Play an album: first track now, the rest appended to the queue.
    pub(crate) fn play_album(&mut self, track_ids: Vec<String>, cx: &mut Context<Self>) {
        let mut ids = track_ids.into_iter();
        let Some(first) = ids.next() else {
            return;
        };
        if let Some(controller) = self.controller.as_mut() {
            controller.clear_queue();
            for id in ids {
                let _ = controller.add_queue(&id);
            }
        }
        self.play_track(&first, cx);
        self.stage_panel = Some(StagePanel::Queue);
    }

    /// Replace one track's DTO everywhere it appears (tag edits, enrichment).
    pub(crate) fn update_track_dto(&mut self, dto: TrackDto, cx: &mut Context<Self>) {
        if let Some(&i) = self.track_index_by_id.get(&dto.id)
            && let Some(slot) = self.library.get_mut(i)
        {
            *slot = dto.clone();
        }
        self.covers.borrow_mut().remove(&dto.id);
        self.rebuild_indexes();
        self.apply_filter(cx);
    }

    pub(crate) fn play_first_filtered(&mut self, cx: &mut Context<Self>) {
        if let Some(track) = self.filtered_library.first() {
            let id = track.id.clone();
            self.play_track(&id, cx);
        }
    }

    pub(crate) fn play_track(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.play(id)
        {
            self.set_status(format!("Play failed: {error}"), cx);
            return;
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_play_pause(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.toggle_pause()
        {
            eprintln!("toggle_pause failed: {error}");
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_next(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.next()
        {
            eprintln!("next failed: {error}");
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_previous(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.previous()
        {
            eprintln!("previous failed: {error}");
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_stop(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            controller.stop();
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_cycle_loop(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            controller.cycle_loop();
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_shuffle(&mut self, cx: &mut Context<Self>) {
        let library = self.controller.as_mut().map(|controller| {
            controller.shuffle();
            controller.snapshot().library
        });
        if let Some(library) = library {
            self.library = library;
            self.apply_filter(cx);
            self.rebuild_indexes();
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_volume_up(&mut self, cx: &mut Context<Self>) {
        let volume = self.playback.volume.saturating_add(5).min(100);
        if let Some(controller) = self.controller.as_mut() {
            controller.set_volume(volume);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_volume_down(&mut self, cx: &mut Context<Self>) {
        let volume = self.playback.volume.saturating_sub(5);
        if let Some(controller) = self.controller.as_mut() {
            controller.set_volume(volume);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_toggle_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            controller.toggle_mute();
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_toggle_favorite(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.toggle_favorite(id)
        {
            eprintln!("toggle favorite failed: {error}");
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_add_queue(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.add_queue(id)
        {
            eprintln!("add queue failed: {error}");
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_play_next(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.play_next(id)
        {
            self.set_status(format!("Play next failed: {error}"), cx);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_remove_queue(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            controller.remove_queue(id);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_toggle_queue(&mut self, cx: &mut Context<Self>) {
        self.stage_panel = match self.stage_panel {
            Some(StagePanel::Queue) => None,
            _ => Some(StagePanel::Queue),
        };
        cx.notify();
    }

    pub(crate) fn do_toggle_stage(&mut self, cx: &mut Context<Self>) {
        self.stage_open = !self.stage_open;
        cx.notify();
    }

    pub(crate) fn do_toggle_lyrics(&mut self, cx: &mut Context<Self>) {
        self.stage_panel = match self.stage_panel {
            Some(StagePanel::Lyrics) => None,
            _ => Some(StagePanel::Lyrics),
        };
        if matches!(self.stage_panel, Some(StagePanel::Lyrics)) {
            self.load_lyrics(cx);
        }
        cx.notify();
    }

    pub(crate) fn do_rescan(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            if let Err(error) = controller.scan(None) {
                self.set_status(format!("Scan failed: {error}"), cx);
            } else {
                self.library = controller.snapshot().library;
                self.refresh_playlists();
                self.rebuild_indexes();
                self.apply_filter(cx);
                self.set_status(
                    format!(
                        "{} tracks · {} folders",
                        self.library.len(),
                        self.folders_count_cache
                    ),
                    cx,
                );
                self.start_enrichment(cx);
            }
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_seek(&mut self, fraction: f64, cx: &mut Context<Self>) {
        let Some(duration) = self.playback.duration.filter(|d| *d > 0.0) else {
            return;
        };
        let position = duration * fraction.clamp(0.0, 1.0);
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.seek(position)
        {
            eprintln!("seek failed: {error}");
        }
        self.playback.position = position;
        cx.notify();
    }

    pub(crate) fn do_set_volume(&mut self, fraction: f64, cx: &mut Context<Self>) {
        let volume = (fraction.clamp(0.0, 1.0) * 100.0).round() as u8;
        if volume == self.playback.volume {
            return;
        }
        if let Some(controller) = self.controller.as_mut() {
            controller.set_volume(volume);
        }
        self.playback.volume = volume;
        cx.notify();
    }

    // ── Sound controls (settings popover) ────────────────────────

    pub(crate) fn do_set_eq(&mut self, preset: EqPreset, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            controller.set_eq(preset);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_nudge_speed(&mut self, delta: f64, cx: &mut Context<Self>) {
        let next = (self.playback.speed + delta).clamp(0.5, 2.0);
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.set_speed(next);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_nudge_pitch(&mut self, delta: f64, cx: &mut Context<Self>) {
        let next = (self.playback.pitch + delta).clamp(-12.0, 12.0);
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.set_pitch(next);
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_reset_speed_pitch(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let _ = controller.reset_speed_pitch();
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    pub(crate) fn do_cycle_replaygain(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let next = match controller.config.replaygain {
                ReplayGainMode::Off => ReplayGainMode::Track,
                ReplayGainMode::Track => ReplayGainMode::Album,
                ReplayGainMode::Album => ReplayGainMode::Off,
            };
            let _ = controller.set_replaygain(next);
        }
        cx.notify();
    }

    pub(crate) fn do_toggle_excess_volume(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let next = !controller.config.excess_volume;
            let _ = controller.set_excess_volume(next);
        }
        cx.notify();
    }

    pub(crate) fn do_cycle_artist_source(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let next = match controller.config.artist_source {
                ArtistSource::Metadata => ArtistSource::Folder,
                ArtistSource::Folder => ArtistSource::Metadata,
            };
            let _ = controller.set_artist_source(next);
        }
        // Artist grouping changes — rebuild indexes and re-filter.
        self.rebuild_indexes();
        self.apply_filter(cx);
        cx.notify();
    }

    pub(crate) fn do_toggle_ldm(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let next = !controller.config.ldm;
            let _ = controller.set_ldm(next);
        }
        cx.notify();
    }

    pub(crate) fn do_toggle_discord_rpc(&mut self, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            let next = !controller.config.discord_rpc;
            if controller.set_discord_rpc(next).is_err() {
                return;
            }
            let id = controller.config.discord_rpc_id.clone();
            // Apply now rather than waiting a poll tick, so toggling off
            // drops the presence immediately.
            if let Some(warn) = self.rpc.sync(next, &id) {
                self.set_status(warn, cx);
            } else {
                self.set_status(
                    if next {
                        "discord rpc · on"
                    } else {
                        "discord rpc · off"
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Search filters whatever page is active — no forced Library jump.
    pub(crate) fn do_toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_active = !self.search_active;
        if self.search_active {
            let focus = self.search_input.read(cx).focus_handle.clone();
            focus.focus(window, cx);
        } else {
            self.search_input.update(cx, |input, cx| input.clear(cx));
            self.focus_handle.focus(window, cx);
        }
        self.apply_filter(cx);
        cx.notify();
    }

    pub(crate) fn close_search(&mut self, cx: &mut Context<Self>) {
        self.search_active = false;
        self.search_input.update(cx, |input, cx| input.clear(cx));
        self.apply_filter(cx);
    }

    pub(crate) fn open_context_menu(
        &mut self,
        track_id: String,
        position: Point<Pixels>,
        from_queue: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenuState {
            track_id,
            position,
            from_queue,
        });
        self.menu_selection = 0;
        self.playlist_picker = None;
        self.settings_open = false;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn close_overlays(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.menu_selection = 0;
        self.playlist_picker = None;
        self.pending_confirm = None;
        self.settings_open = false;
        if self.name_target.is_some() {
            self.name_target = None;
            self.name_input.update(cx, |input, cx| input.clear(cx));
        }
        cx.notify();
    }

    // ── Context menu (keyboard navigable) ────────────────────────

    /// Menu entries for the open context menu, in render order.
    pub(crate) fn menu_items(&self) -> Vec<MenuAction> {
        let Some(menu) = self.context_menu.as_ref() else {
            return Vec::new();
        };
        let mut items = vec![
            MenuAction::Play,
            MenuAction::PlayNext,
            if menu.from_queue {
                MenuAction::QueueRemove
            } else {
                MenuAction::QueueAdd
            },
            MenuAction::Favorite,
            MenuAction::AddToPlaylist,
            MenuAction::EditTags,
            MenuAction::CopyPath,
        ];
        if self
            .track_by_id(&menu.track_id)
            .is_some_and(|t| !t.path.is_empty())
        {
            items.push(MenuAction::Reveal);
        }
        items
    }

    pub(crate) fn menu_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.playlist_picker.is_some() {
            return self.picker_move(delta, cx);
        }
        let len = self.menu_items().len();
        if len == 0 {
            return;
        }
        let next = (self.menu_selection as isize + delta).clamp(0, len as isize - 1) as usize;
        self.menu_selection = next;
        cx.notify();
    }

    pub(crate) fn menu_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_confirm.is_some() {
            if self.confirm_accept_selected {
                self.confirm_accept(window, cx);
            } else {
                self.confirm_cancel(window, cx);
            }
            return;
        }
        if self.playlist_picker.is_some() {
            return self.picker_activate(window, cx);
        }
        let Some(action) = self.menu_items().get(self.menu_selection).copied() else {
            return;
        };
        self.dispatch_menu_action(action, window, cx);
    }

    pub(crate) fn dispatch_menu_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.clone() else {
            return;
        };
        let id = menu.track_id.clone();
        match action {
            MenuAction::Play => {
                self.context_menu = None;
                self.play_track(&id, cx);
            }
            MenuAction::PlayNext => {
                self.context_menu = None;
                self.do_play_next(&id, cx);
            }
            MenuAction::QueueAdd => {
                self.context_menu = None;
                self.do_add_queue(&id, cx);
            }
            MenuAction::QueueRemove => {
                self.context_menu = None;
                self.do_remove_queue(&id, cx);
            }
            MenuAction::Favorite => {
                self.context_menu = None;
                self.do_toggle_favorite(&id, cx);
            }
            MenuAction::AddToPlaylist => {
                self.open_playlist_picker(id, menu.position, window, cx);
            }
            MenuAction::EditTags => {
                self.context_menu = None;
                self.open_tag_editor(id, window, cx);
            }
            MenuAction::CopyPath => {
                if let Some(track) = self.track_by_id(&id) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(track.path.clone()));
                    self.set_status("Path copied", cx);
                }
                self.context_menu = None;
                cx.notify();
            }
            MenuAction::Reveal => {
                self.context_menu = None;
                self.reveal_track(&id, cx);
            }
        }
    }

    /// Reveal the track in the OS file manager (GPUI handles each platform).
    pub(crate) fn reveal_track(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(track) = self.track_by_id(id) {
            cx.reveal_path(std::path::Path::new(&track.path));
        }
    }

    // ── Playlist picker + name prompt ────────────────────────────

    pub(crate) fn open_playlist_picker(
        &mut self,
        track_id: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playlist_picker = Some(PlaylistPickerState {
            track_id,
            position,
            selection: 0,
        });
        self.context_menu = None;
        self.picker_scroll.set_offset(Point::new(px(0.0), px(0.0)));
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Picker rows: one per playlist + a trailing "New playlist…" item.
    pub(crate) fn picker_len(&self) -> usize {
        self.playlists.len() + 1
    }

    pub(crate) fn picker_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.picker_len();
        if len == 0 {
            return;
        }
        let Some(picker) = self.playlist_picker.as_mut() else {
            return;
        };
        picker.selection = (picker.selection as isize + delta).clamp(0, len as isize - 1) as usize;
        // Keep the keyboard-selected row visible inside the scrollable list.
        if picker.selection < self.playlists.len() {
            self.picker_scroll.scroll_to_item(picker.selection);
        }
        cx.notify();
    }

    pub(crate) fn picker_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.playlist_picker.clone() else {
            return;
        };
        if picker.selection >= self.playlists.len() {
            // "New playlist…" → inline name prompt for this track.
            self.playlist_picker = None;
            self.open_name_prompt(NameTarget::NewPlaylistForTrack(picker.track_id), window, cx);
            return;
        }
        let Some(playlist) = self.playlists.get(picker.selection).cloned() else {
            return;
        };
        self.playlist_picker = None;
        if let Some(controller) = self.controller.as_ref() {
            match controller.playlist_add(&playlist.id, &picker.track_id) {
                Ok(_) => {
                    self.refresh_playlists();
                    self.set_status(format!("Added to {}", playlist.name), cx);
                }
                Err(error) => self.set_status(format!("Add failed: {error}"), cx),
            }
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Show the shared single-line input for playlist create/rename flows.
    pub(crate) fn open_name_prompt(
        &mut self,
        target: NameTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefill = match &target {
            NameTarget::RenamePlaylist(id) => self
                .playlists
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.name_target = Some(target);
        self.playlist_picker = None;
        self.context_menu = None;
        let input = self.name_input.clone();
        input.update(cx, |input, cx| {
            input.set_content(prefill, cx);
        });
        let focus = input.read(cx).focus_handle.clone();
        focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn on_name_submit(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).content.trim().to_string();
        let Some(target) = self.name_target.take() else {
            return;
        };
        self.name_input.update(cx, |input, cx| input.clear(cx));
        if name.is_empty() {
            cx.notify();
            return;
        }
        // Run controller ops first (immutable borrow), then update state.
        let outcome: Result<String, String> = match &target {
            NameTarget::NewPlaylist | NameTarget::NewPlaylistForTrack(_) => {
                match self
                    .controller
                    .as_ref()
                    .and_then(|c| c.create_playlist(&name).ok())
                {
                    Some(playlist) => {
                        if let NameTarget::NewPlaylistForTrack(track_id) = &target {
                            match self
                                .controller
                                .as_ref()
                                .map(|c| c.playlist_add(&playlist.id, track_id))
                            {
                                Some(Ok(_)) => Ok(format!("Added to {name}")),
                                Some(Err(e)) => Err(format!("Add failed: {e}")),
                                None => Err("No library".into()),
                            }
                        } else {
                            Ok(format!("Playlist “{name}” created"))
                        }
                    }
                    None => Err("Create failed".into()),
                }
            }
            NameTarget::RenamePlaylist(id) => match self
                .controller
                .as_ref()
                .map(|c| c.rename_playlist(id, &name))
            {
                Some(Ok(_)) => Ok("Playlist renamed".into()),
                Some(Err(e)) => Err(format!("Rename failed: {e}")),
                None => Err("No library".into()),
            },
        };
        match outcome {
            Ok(message) => {
                self.refresh_playlists();
                self.set_status(message, cx);
            }
            Err(message) => self.set_status(message, cx),
        }
        self.refocus_root(cx);
        cx.notify();
    }

    pub(crate) fn on_name_dismiss(&mut self, cx: &mut Context<Self>) {
        self.name_target = None;
        self.name_input.update(cx, |input, cx| input.clear(cx));
        self.refocus_root(cx);
        cx.notify();
    }

    /// Refocus the root handle without holding a `Window` (submit callbacks
    /// don't receive one).
    fn refocus_root(&mut self, cx: &mut Context<Self>) {
        let handle = self.focus_handle.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
    }

    // ── Confirm dialog ───────────────────────────────────────────

    pub(crate) fn request_confirm(&mut self, req: ConfirmRequest, cx: &mut Context<Self>) {
        self.pending_confirm = Some(req);
        self.confirm_accept_selected = true;
        cx.notify();
    }

    pub(crate) fn confirm_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_confirm = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn confirm_accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(req) = self.pending_confirm.take() else {
            return;
        };
        match req.action {
            ConfirmAction::DeletePlaylist(id) => self.delete_playlist(id, cx),
            ConfirmAction::ClearQueue => {
                if let Some(controller) = self.controller.as_mut() {
                    controller.clear_queue();
                }
                self.update_playback_and_cover(cx);
                self.set_status("Queue cleared", cx);
            }
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    // ── Queue management ─────────────────────────────────────────

    pub(crate) fn queue_item_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.focused_list != Some(FocusedList::Queue) {
            return;
        }
        let Some(from) = self.focused_row else {
            return;
        };
        self.queue_move_at(from, delta, cx);
    }

    /// Move queue row `from` by `delta` (shared by keyboard and row buttons).
    pub(crate) fn queue_move_at(&mut self, from: usize, delta: isize, cx: &mut Context<Self>) {
        let len = self.playback.queue.len();
        if len == 0 {
            return;
        }
        let to = (from as isize + delta).clamp(0, len as isize - 1) as usize;
        if to == from {
            return;
        }
        if let Some(controller) = self.controller.as_mut()
            && controller.queue_move(from, to)
        {
            if self.focused_list == Some(FocusedList::Queue) {
                self.focused_row = Some(to);
            }
            self.queue_scroll_handle
                .scroll_to_item(to, ScrollStrategy::Nearest);
            self.update_playback_and_cover(cx);
        }
        cx.notify();
    }

    pub(crate) fn queue_jump_to_current(&mut self, cx: &mut Context<Self>) {
        let Some(current) = self.playback.current.as_ref().map(|t| t.id.clone()) else {
            return;
        };
        let index = self
            .controller
            .as_ref()
            .and_then(|c| c.queue_index_of(&current))
            .or_else(|| self.playback.queue.iter().position(|id| id == &current));
        // The current track is popped off the queue once playing — jump to the
        // first queued item instead when it's not in the list.
        let row = index.unwrap_or(0);
        self.focused_list = Some(FocusedList::Queue);
        self.focused_row = Some(row);
        self.queue_scroll_handle
            .scroll_to_item(row, ScrollStrategy::Nearest);
        cx.notify();
    }

    // ── Tag editor ───────────────────────────────────────────────

    pub(crate) fn open_tag_editor(
        &mut self,
        track_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tags = self
            .controller
            .as_ref()
            .and_then(|c| c.get_track_tags(&track_id).ok())
            .unwrap_or_default();
        self.close_overlays(cx);
        let mk = |cx: &mut Context<Self>, placeholder: &str, value: String| {
            let input = cx.new(|cx| SearchInput::with_placeholder(cx, placeholder));
            if !value.is_empty() {
                input.update(cx, |i, cx| i.set_content(value, cx));
            }
            let sub = cx.subscribe(&input, |this, input, event: &SearchEvent, cx| match event {
                SearchEvent::Changed => {}
                SearchEvent::Submit => this.save_tag_editor(cx),
                SearchEvent::Dismiss => this.close_tag_editor(cx),
                SearchEvent::TabNext => this.tag_input_cycle(input.clone(), 1, cx),
                SearchEvent::TabPrev => this.tag_input_cycle(input.clone(), -1, cx),
            });
            (input, sub)
        };
        let (title, s1) = mk(cx, "Title", tags.title.unwrap_or_default());
        let (artist, s2) = mk(cx, "Artist", tags.artist.unwrap_or_default());
        let (album, s3) = mk(cx, "Album", tags.album.unwrap_or_default());
        let (track_number, s4) = mk(
            cx,
            "Track #",
            tags.track_number.map(|n| n.to_string()).unwrap_or_default(),
        );
        let (disc_number, s5) = mk(
            cx,
            "Disc #",
            tags.disc_number.map(|n| n.to_string()).unwrap_or_default(),
        );
        let (year, s6) = mk(
            cx,
            "Year",
            tags.year.map(|n| n.to_string()).unwrap_or_default(),
        );
        let focus = title.read(cx).focus_handle.clone();
        self.tag_editor = Some(TagEditorState {
            track_id,
            title,
            artist,
            album,
            track_number,
            disc_number,
            year,
            _subs: vec![s1, s2, s3, s4, s5, s6],
        });
        focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn close_tag_editor(&mut self, cx: &mut Context<Self>) {
        if self.tag_editor.take().is_some() {
            self.refocus_root(cx);
            cx.notify();
        }
    }

    /// Move focus between the tag editor's fields (Tab / Shift-Tab, wraps).
    fn tag_input_cycle(&mut self, from: Entity<SearchInput>, dir: isize, cx: &mut Context<Self>) {
        let Some(editor) = self.tag_editor.as_ref() else {
            return;
        };
        let fields = [
            &editor.title,
            &editor.artist,
            &editor.album,
            &editor.track_number,
            &editor.disc_number,
            &editor.year,
        ];
        let Some(index) = fields
            .iter()
            .position(|f| f.entity_id() == from.entity_id())
        else {
            return;
        };
        let next = (index as isize + dir).rem_euclid(fields.len() as isize) as usize;
        let handle = fields[next].read(cx).focus_handle.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
    }

    pub(crate) fn save_tag_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.tag_editor.take() else {
            return;
        };
        let text = |e: &Entity<SearchInput>| {
            let value = e.read(cx).content.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        };
        let num = |e: &Entity<SearchInput>| e.read(cx).content.trim().parse::<u32>().ok();
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut tags = controller
            .get_track_tags(&editor.track_id)
            .unwrap_or_default();
        tags.title = text(&editor.title);
        tags.artist = text(&editor.artist);
        tags.album = text(&editor.album);
        tags.track_number = num(&editor.track_number);
        tags.disc_number = num(&editor.disc_number);
        tags.year = num(&editor.year);
        match controller.set_track_tags(&editor.track_id, tags) {
            Ok(dto) => {
                self.update_track_dto(dto, cx);
                self.set_status("Tags saved", cx);
            }
            Err(error) => self.set_status(format!("Save failed: {error}"), cx),
        }
        self.refocus_root(cx);
        cx.notify();
    }

    // ── Seek / transport extras ──────────────────────────────────

    pub(crate) fn do_seek_by(&mut self, delta_secs: f64, cx: &mut Context<Self>) {
        let Some(duration) = self.playback.duration.filter(|d| *d > 0.0) else {
            return;
        };
        let position = (self.playback.position + delta_secs).clamp(0.0, duration);
        if let Some(controller) = self.controller.as_mut()
            && let Err(error) = controller.seek(position)
        {
            eprintln!("seek failed: {error}");
        }
        self.playback.position = position;
        cx.notify();
    }

    pub(crate) fn do_favorite_current(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.playback.current.as_ref().map(|t| t.id.clone()) else {
            return;
        };
        self.do_toggle_favorite(&id, cx);
    }

    // ── Focus management ─────────────────────────────────────────

    /// The list Tab enters for the active page.
    fn page_list(&self) -> FocusedList {
        match self.page {
            Page::Library => FocusedList::Tracks,
            Page::Artists => {
                if self.selected_artist.is_some() {
                    FocusedList::ArtistTracks
                } else {
                    FocusedList::Artists
                }
            }
            Page::Albums => {
                if self.selected_album.is_some() {
                    FocusedList::AlbumTracks
                } else {
                    FocusedList::Albums
                }
            }
            Page::Playlists => FocusedList::Playlists,
            Page::Favorites => FocusedList::Favorites,
            Page::Shelves => {
                if self.selected_shelf.is_some() {
                    FocusedList::ShelfTracks
                } else {
                    FocusedList::Shelves
                }
            }
        }
    }

    /// Point `focused_list`/`focused_row` at the page's list and scroll to it.
    fn aim_page_list(&mut self) {
        let list = self.page_list();
        self.focused_list = Some(list);
        if self
            .focused_row
            .is_none_or(|row| row >= self.list_len(list))
        {
            self.focused_row = if self.list_len(list) > 0 {
                Some(0)
            } else {
                None
            };
        }
        if let Some(row) = self.focused_row {
            self.list_scroll_handle(list)
                .scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    /// Tab: enter the list that makes sense for the active page.
    pub(crate) fn focus_current_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.aim_page_list();
        self.list_focus.focus(window, cx);
        cx.notify();
    }

    /// `focus_current_list` where no `Window` is available (event
    /// subscriptions) — focuses `list_focus` in every window, like
    /// `refocus_root` does for the root handle.
    fn focus_current_list_detached(&mut self, cx: &mut Context<Self>) {
        self.aim_page_list();
        let handle = self.list_focus.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
        cx.notify();
    }

    /// Esc inside a TrackList: release the list back to the root context.
    pub(crate) fn blur_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_list = None;
        self.focused_row = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    // ── Slider keyboard (dispatched by which handle is focused) ──

    pub(crate) fn slider_nudge(&mut self, delta: f64, cx: &mut Context<Self>, window: &Window) {
        if self.seek_focus.is_focused(window) {
            let duration = self.playback.duration.unwrap_or(0.0);
            if duration > 0.0 {
                self.do_seek(
                    (self.playback.position / duration) + delta / duration.max(1.0),
                    cx,
                );
            }
        } else if self.volume_focus.is_focused(window) {
            self.do_set_volume((self.playback.volume as f64 / 100.0) + delta, cx);
        }
    }

    pub(crate) fn slider_edge(&mut self, home: bool, cx: &mut Context<Self>, window: &Window) {
        if self.seek_focus.is_focused(window) {
            self.do_seek(if home { 0.0 } else { 1.0 }, cx);
        } else if self.volume_focus.is_focused(window) {
            self.do_set_volume(if home { 0.0 } else { 1.0 }, cx);
        }
    }

    // ── Drag & drop ──────────────────────────────────────────────

    pub(crate) fn handle_dropped_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drop_hover = false;
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                dirs.push(path);
            } else if optionmusic::playlist::is_audio_file(&path) {
                files.push(path);
            }
        }
        let had_dirs = !dirs.is_empty();
        if had_dirs {
            let count = dirs.len();
            self.apply_new_dirs(dirs, cx);
            self.set_status(
                format!(
                    "Added {count} folder{} — rescanning",
                    if count == 1 { "" } else { "s" }
                ),
                cx,
            );
        }
        if !files.is_empty() {
            let first = files[0].to_string_lossy().into_owned();
            // Dropped files may not be in the library yet — queue what the
            // engine knows; unknown paths get indexed on the next scan.
            if self.track_by_id(&first).is_some() {
                self.play_track(&first, cx);
                for path in files.iter().skip(1) {
                    let id = path.to_string_lossy().into_owned();
                    if self.track_by_id(&id).is_some() {
                        self.do_add_queue(&id, cx);
                    }
                }
            } else {
                self.set_status(
                    "Dropped files are outside the library — drop folders to add them",
                    cx,
                );
            }
        }
        if !had_dirs && files.is_empty() {
            self.set_status("Nothing to add — drop music files or folders", cx);
        }
        window.refresh();
        cx.notify();
    }

    /// Merge folders into `music_dirs`, rescan, kick enrichment.
    pub(crate) fn apply_new_dirs(&mut self, dirs: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        let mut music_dirs = controller.config.music_dirs.clone();
        for path in dirs {
            if !music_dirs.iter().any(|dir| dir == &path) {
                music_dirs.push(path);
            }
        }
        match controller.scan(Some(music_dirs)) {
            Ok(_) => {
                self.library = controller.snapshot().library;
                self.refresh_playlists();
                self.rebuild_indexes();
                self.apply_filter(cx);
                self.set_status(
                    format!(
                        "{} tracks · {} folders",
                        self.library.len(),
                        self.folders_count_cache
                    ),
                    cx,
                );
                self.start_enrichment(cx);
            }
            Err(error) => self.set_status(format!("Scan failed: {error}"), cx),
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    // ── Desktop preferences (JSON blob in config.toml) ───────────

    /// Apply persisted UI state after the first scan (page/stage/panel).
    pub(crate) fn apply_desktop_prefs(&mut self, cx: &mut Context<Self>) {
        let prefs: DesktopPrefs = self
            .controller
            .as_ref()
            .and_then(|c| serde_json::from_str(c.desktop_preferences()).ok())
            .unwrap_or_default();
        self.page = Page::from_slug(&prefs.page);
        self.stage_open = prefs.stage_open;
        self.stage_panel = match prefs.panel.as_deref() {
            Some("queue") => Some(StagePanel::Queue),
            Some("lyrics") => Some(StagePanel::Lyrics),
            _ => None,
        };
        cx.notify();
    }

    /// Debounced persist of window + UI state into `desktop_preferences`.
    pub(crate) fn save_desktop_prefs(&mut self, window: &Window, cx: &mut Context<Self>) {
        let bounds = window.bounds();
        let prefs = DesktopPrefs {
            window_w: f32::from(bounds.size.width),
            window_h: f32::from(bounds.size.height),
            page: self.page.slug().into(),
            stage_open: self.stage_open,
            panel: self.stage_panel.map(|p| match p {
                StagePanel::Queue => "queue".into(),
                StagePanel::Lyrics => "lyrics".into(),
            }),
        };
        self._prefs_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(600))
                .await;
            this.update(cx, |view, _| {
                if let Ok(json) = serde_json::to_string(&prefs)
                    && let Some(controller) = view.controller.as_mut()
                {
                    let _ = controller.set_desktop_preferences(json);
                }
            })
            .ok();
        }));
    }

    pub(crate) fn add_folders(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add folders to your library".into()),
        });
        self._open_task = Some(cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.set_status(format!("Open failed: {error}"), cx);
                    })
                    .ok();
                    return;
                }
                Err(_) => return,
            };
            this.update(cx, |view, cx| view.apply_new_dirs(picked, cx))
                .ok();
        }));
    }

    pub(crate) fn list_scroll_handle(&self, list: FocusedList) -> UniformListScrollHandle {
        match list {
            FocusedList::Tracks => self.scroll_handle.clone(),
            FocusedList::Artists | FocusedList::ArtistTracks => self.artist_scroll_handle.clone(),
            FocusedList::Albums | FocusedList::AlbumTracks => self.album_scroll_handle.clone(),
            FocusedList::Favorites => self.fav_scroll_handle.clone(),
            FocusedList::Shelves | FocusedList::ShelfTracks => self.shelf_scroll_handle.clone(),
            FocusedList::Playlists => self.playlist_scroll_handle.clone(),
            FocusedList::Queue => self.queue_scroll_handle.clone(),
        }
    }

    pub(crate) fn list_len(&self, list: FocusedList) -> usize {
        match list {
            FocusedList::Tracks => self.filtered_library.len(),
            FocusedList::Artists => self.filtered_artists.len(),
            FocusedList::ArtistTracks => self.artist_tracks.len(),
            FocusedList::Albums => self.filtered_albums.len(),
            FocusedList::AlbumTracks => self.album_tracks.len(),
            FocusedList::Favorites => self.filtered_favorites.len(),
            FocusedList::Playlists => self.filtered_playlists.len(),
            FocusedList::Shelves => 3,
            FocusedList::ShelfTracks => self.shelf_tracks.len(),
            FocusedList::Queue => self.playback.queue.len(),
        }
    }

    pub(crate) fn focus_list(
        &mut self,
        list: FocusedList,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focused_list = Some(list);
        self.focused_row = Some(row);
        self.list_focus.focus(window, cx);
    }

    pub(crate) fn list_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(list) = self.focused_list else {
            return;
        };
        let len = self.list_len(list);
        if len == 0 {
            return;
        }
        let current = self.focused_row.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        self.focused_row = Some(next);
        self.list_scroll_handle(list)
            .scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub(crate) fn list_jump(&mut self, edge: isize, cx: &mut Context<Self>) {
        let Some(list) = self.focused_list else {
            return;
        };
        let len = self.list_len(list);
        if len == 0 {
            return;
        }
        let next = if edge < 0 { 0 } else { len - 1 };
        self.focused_row = Some(next);
        self.list_scroll_handle(list)
            .scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub(crate) fn list_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(list) = self.focused_list else {
            return;
        };
        let Some(row) = self.focused_row else {
            return;
        };
        match list {
            FocusedList::Tracks => {
                if let Some(track) = self.filtered_library.get(row) {
                    let id = track.id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::ArtistTracks => {
                if let Some(track) = self.artist_tracks.get(row) {
                    let id = track.id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::Favorites => {
                if let Some(track) = self.filtered_favorites.get(row) {
                    let id = track.id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::ShelfTracks => {
                if let Some(track) = self.shelf_tracks.get(row) {
                    let id = track.id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::Queue => {
                if let Some(id) = self.playback.queue.get(row) {
                    let id = id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::Artists => {
                if let Some((name, _)) = self.filtered_artists.get(row) {
                    let name = name.clone();
                    self.select_artist(name, cx);
                }
            }
            FocusedList::Albums => {
                if let Some(album) = self.filtered_albums.get(row) {
                    let key = album.key.clone();
                    self.select_album(key, cx);
                }
            }
            FocusedList::AlbumTracks => {
                if let Some(track) = self.album_tracks.get(row) {
                    let id = track.id.clone();
                    self.play_track(&id, cx);
                }
            }
            FocusedList::Playlists => {
                if let Some(playlist) = self.filtered_playlists.get(row) {
                    self.play_playlist(playlist.id.clone(), window, cx);
                }
            }
            FocusedList::Shelves => {
                let shelf = match row {
                    0 => Some(SmartShelf::PlayedWeek),
                    1 => Some(SmartShelf::NoCover),
                    2 => Some(SmartShelf::IncompleteAlbums),
                    _ => None,
                };
                if let Some(shelf) = shelf {
                    self.select_shelf(shelf, cx);
                }
            }
        }
    }

    pub(crate) fn play_playlist(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(controller) = self.controller.as_mut() {
            match controller.play_playlist(&id) {
                Ok(()) => {
                    let name = self
                        .playlists
                        .iter()
                        .find(|playlist| playlist.id == id)
                        .map(|playlist| playlist.name.clone())
                        .unwrap_or_else(|| "playlist".into());
                    self.set_status(format!("Playing {name}"), cx);
                    self.stage_panel = Some(StagePanel::Queue);
                }
                Err(error) => self.set_status(format!("Playlist failed: {error}"), cx),
            }
        }
        self.update_playback_and_cover(cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// "New playlist" button — opens the shared name prompt.
    pub(crate) fn new_playlist_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_name_prompt(NameTarget::NewPlaylist, window, cx);
    }

    /// Pencil on a playlist row — rename via the shared name prompt.
    pub(crate) fn rename_playlist_prompt(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_name_prompt(NameTarget::RenamePlaylist(id), window, cx);
    }

    /// Import `.m3u`/`.m3u8` files picked by the user into saved playlists.
    pub(crate) fn import_m3u_flow(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import playlists".into()),
        });
        self._open_task = Some(cx.spawn(async move |this, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return,
            };
            this.update(cx, |view, cx| {
                let mut imported = 0usize;
                let mut last_error: Option<String> = None;
                for path in &paths {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    if ext != "m3u" && ext != "m3u8" {
                        continue;
                    }
                    match view.controller.as_ref().map(|c| c.import_m3u(path, None)) {
                        Some(Ok(_)) => imported += 1,
                        Some(Err(e)) => last_error = Some(e.to_string()),
                        None => {}
                    }
                }
                if imported > 0 {
                    view.refresh_playlists();
                    view.set_status(
                        format!(
                            "Imported {imported} playlist{}",
                            if imported == 1 { "" } else { "s" }
                        ),
                        cx,
                    );
                } else if let Some(error) = last_error {
                    view.set_status(format!("Import failed: {error}"), cx);
                } else if !paths.is_empty() {
                    view.set_status("No .m3u files selected", cx);
                }
            })
            .ok();
        }));
    }

    /// Export one playlist to a `.m3u` path chosen via the save dialog.
    pub(crate) fn export_m3u_flow(&mut self, id: String, cx: &mut Context<Self>) {
        let name = self
            .playlists
            .iter()
            .find(|p| p.id == id)
            .map(|p| format!("{}.m3u", p.name));
        let receiver = cx.prompt_for_new_path(
            std::path::Path::new("."),
            name.as_deref().or(Some("playlist.m3u")),
        );
        self._open_task = Some(cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(path)) => path,
                _ => None,
            };
            this.update(cx, |view, cx| {
                let Some(path) = path else {
                    return;
                };
                match view.controller.as_ref().map(|c| c.export_m3u(&id, &path)) {
                    Some(Ok(_)) => view.set_status(format!("Exported to {}", path.display()), cx),
                    Some(Err(e)) => view.set_status(format!("Export failed: {e}"), cx),
                    None => {}
                }
            })
            .ok();
        }));
    }

    pub(crate) fn delete_playlist(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(controller) = self.controller.as_mut() {
            match controller.delete_playlist(&id) {
                Ok(()) => {
                    self.refresh_playlists();
                    self.set_status("Playlist deleted", cx);
                }
                Err(error) => self.set_status(format!("Delete failed: {error}"), cx),
            }
        }
        cx.notify();
    }

    pub(crate) fn navigate(&mut self, page: Page, window: &Window, cx: &mut Context<Self>) {
        self.page = page;
        self.selected_artist = None;
        self.selected_album = None;
        self.selected_shelf = None;
        self.shelf_tracks = Rc::from(Vec::new());
        self.artist_tracks = Rc::from(Vec::new());
        self.album_tracks = Rc::from(Vec::new());
        self.focused_list = None;
        self.focused_row = None;
        // Search stays open and keeps filtering the newly selected page.
        self.save_desktop_prefs(window, cx);
        cx.notify();
    }

    pub(crate) fn play_pause(
        &mut self,
        _: &PlayPause,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_play_pause(cx);
    }

    pub(crate) fn next_track(&mut self, _: &Next, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_next(cx);
    }

    pub(crate) fn previous_track(
        &mut self,
        _: &Previous,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_previous(cx);
    }

    pub(crate) fn stop(&mut self, _: &Stop, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_stop(cx);
    }

    pub(crate) fn cycle_loop(
        &mut self,
        _: &CycleLoop,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_cycle_loop(cx);
    }

    pub(crate) fn shuffle(&mut self, _: &Shuffle, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_shuffle(cx);
    }

    pub(crate) fn toggle_queue(
        &mut self,
        _: &ToggleQueue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_toggle_queue(cx);
        self.save_desktop_prefs(window, cx);
    }

    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_toggle_search(window, cx);
    }

    pub(crate) fn list_up(&mut self, _: &ListUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.list_move(-1, cx);
    }

    pub(crate) fn list_down(&mut self, _: &ListDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.list_move(1, cx);
    }

    pub(crate) fn list_first(
        &mut self,
        _: &ListFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list_jump(-1, cx);
    }

    pub(crate) fn list_last(&mut self, _: &ListLast, _window: &mut Window, cx: &mut Context<Self>) {
        self.list_jump(1, cx);
    }

    pub(crate) fn on_list_activate(
        &mut self,
        _: &ListActivate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list_activate(window, cx);
    }

    pub(crate) fn dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(cx);
        self.focus_handle.focus(window, cx);
    }

    pub(crate) fn volume_up(&mut self, _: &VolumeUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_volume_up(cx);
    }

    pub(crate) fn volume_down(
        &mut self,
        _: &VolumeDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_volume_down(cx);
    }

    pub(crate) fn mute(&mut self, _: &Mute, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_toggle_mute(cx);
    }

    pub(crate) fn favorite_current(
        &mut self,
        _: &FavoriteCurrent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_favorite_current(cx);
    }

    pub(crate) fn seek_back(&mut self, _: &SeekBack, _window: &mut Window, cx: &mut Context<Self>) {
        self.do_seek_by(-5.0, cx);
    }

    pub(crate) fn seek_forward(
        &mut self,
        _: &SeekForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_seek_by(5.0, cx);
    }

    pub(crate) fn focus_list_tab(
        &mut self,
        _: &FocusList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_current_list(window, cx);
    }

    pub(crate) fn blur_list_action(
        &mut self,
        _: &BlurList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.blur_list(window, cx);
    }

    pub(crate) fn nav_library(&mut self, _: &NavLibrary, w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Library, w, cx);
    }

    pub(crate) fn nav_artists(&mut self, _: &NavArtists, w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Artists, w, cx);
    }

    pub(crate) fn nav_albums(&mut self, _: &NavAlbums, w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Albums, w, cx);
    }

    pub(crate) fn nav_playlists(
        &mut self,
        _: &NavPlaylists,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(Page::Playlists, w, cx);
    }

    pub(crate) fn nav_favorites(
        &mut self,
        _: &NavFavorites,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(Page::Favorites, w, cx);
    }

    pub(crate) fn nav_shelves(&mut self, _: &NavShelves, w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Shelves, w, cx);
    }

    pub(crate) fn toggle_stage(
        &mut self,
        _: &ToggleStage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_toggle_stage(cx);
        self.save_desktop_prefs(window, cx);
    }

    pub(crate) fn toggle_lyrics(
        &mut self,
        _: &ToggleLyrics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_toggle_lyrics(cx);
        self.save_desktop_prefs(window, cx);
    }

    pub(crate) fn menu_up(&mut self, _: &MenuUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.menu_move(-1, cx);
    }

    pub(crate) fn menu_down(&mut self, _: &MenuDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.menu_move(1, cx);
    }

    /// Left/Right (and Shift-Tab/Tab) pick between the confirm dialog's
    /// Cancel/Accept buttons; ignored by other overlays.
    pub(crate) fn menu_left(&mut self, _: &MenuLeft, _w: &mut Window, cx: &mut Context<Self>) {
        if self.pending_confirm.is_some() {
            self.confirm_accept_selected = false;
            cx.notify();
        }
    }

    pub(crate) fn menu_right(&mut self, _: &MenuRight, _w: &mut Window, cx: &mut Context<Self>) {
        if self.pending_confirm.is_some() {
            self.confirm_accept_selected = true;
            cx.notify();
        }
    }

    pub(crate) fn menu_enter(
        &mut self,
        _: &MenuActivate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_activate(window, cx);
    }

    pub(crate) fn slider_left(
        &mut self,
        _: &SliderLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slider_nudge(-0.05, cx, window);
    }

    pub(crate) fn slider_right(
        &mut self,
        _: &SliderRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slider_nudge(0.05, cx, window);
    }

    pub(crate) fn slider_home(
        &mut self,
        _: &SliderHome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slider_edge(true, cx, window);
    }

    pub(crate) fn slider_end(
        &mut self,
        _: &SliderEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slider_edge(false, cx, window);
    }

    pub(crate) fn queue_item_up(
        &mut self,
        _: &QueueItemUp,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_item_move(-1, cx);
    }

    pub(crate) fn queue_item_down(
        &mut self,
        _: &QueueItemDown,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_item_move(1, cx);
    }

    pub(crate) fn queue_jump(&mut self, _: &QueueJump, _w: &mut Window, cx: &mut Context<Self>) {
        self.queue_jump_to_current(cx);
    }

    pub(crate) fn clear_queue(
        &mut self,
        _: &ClearQueue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.playback.queue.is_empty() {
            return;
        }
        self.request_confirm(
            ConfirmRequest {
                title: "Clear queue".into(),
                detail: format!("{} tracks will be removed", self.playback.queue.len()).into(),
                confirm_label: "Clear".into(),
                action: ConfirmAction::ClearQueue,
            },
            cx,
        );
        self.overlay_focus.focus(window, cx);
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = tokens();
        // Height available to the stage column (viewport minus titlebar and
        // player bar) — the hero cover is sized from it.
        let stage_h = f32::from(window.viewport_size().height) - 118.0;

        div()
            .id("optionmusic-root")
            .accessibility_id("optionmusic.application")
            .role(Role::Application)
            .aria_label("optionMusic")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::play_pause))
            .on_action(cx.listener(Self::next_track))
            .on_action(cx.listener(Self::previous_track))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::cycle_loop))
            .on_action(cx.listener(Self::shuffle))
            .on_action(cx.listener(Self::toggle_queue))
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::volume_up))
            .on_action(cx.listener(Self::volume_down))
            .on_action(cx.listener(Self::mute))
            .on_action(cx.listener(Self::favorite_current))
            .on_action(cx.listener(Self::seek_back))
            .on_action(cx.listener(Self::seek_forward))
            .on_action(cx.listener(Self::focus_list_tab))
            .on_action(cx.listener(Self::blur_list_action))
            .on_action(cx.listener(Self::nav_library))
            .on_action(cx.listener(Self::nav_artists))
            .on_action(cx.listener(Self::nav_albums))
            .on_action(cx.listener(Self::nav_playlists))
            .on_action(cx.listener(Self::nav_favorites))
            .on_action(cx.listener(Self::nav_shelves))
            .on_action(cx.listener(Self::toggle_stage))
            .on_action(cx.listener(Self::toggle_lyrics))
            .on_action(cx.listener(Self::menu_up))
            .on_action(cx.listener(Self::menu_down))
            .on_action(cx.listener(Self::menu_left))
            .on_action(cx.listener(Self::menu_right))
            .on_action(cx.listener(Self::menu_enter))
            .on_action(cx.listener(Self::slider_left))
            .on_action(cx.listener(Self::slider_right))
            .on_action(cx.listener(Self::slider_home))
            .on_action(cx.listener(Self::slider_end))
            .on_action(cx.listener(Self::queue_item_up))
            .on_action(cx.listener(Self::queue_item_down))
            .on_action(cx.listener(Self::queue_jump))
            .on_action(cx.listener(Self::clear_queue))
            .on_drag_move::<ExternalPaths>(cx.listener(
                |this: &mut RootView,
                 _event: &gpui::DragMoveEvent<ExternalPaths>,
                 _window: &mut Window,
                 cx: &mut Context<RootView>| {
                    if !this.drop_hover {
                        this.drop_hover = true;
                        cx.notify();
                    }
                },
            ))
            .on_drop::<ExternalPaths>(cx.listener(
                |this: &mut RootView,
                 paths: &ExternalPaths,
                 window: &mut Window,
                 cx: &mut Context<RootView>| {
                    this.handle_dropped_paths(paths.paths().to_vec(), window, cx);
                },
            ))
            .size_full()
            .flex()
            .flex_col()
            .bg(tokens.bg)
            .text_color(tokens.ink)
            .font_family(
                "Inter, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
            )
            .child(self.titlebar(window, tokens, cx))
            .child(
                div()
                    .id("shell")
                    .accessibility_id("optionmusic.shell")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(tokens, cx))
                    .child(
                        div()
                            .id("workspace")
                            .accessibility_id("optionmusic.workspace")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h_0()
                            .child(
                                div()
                                    .id("frame")
                                    .accessibility_id("optionmusic.frame")
                                    .flex()
                                    .flex_1()
                                    .min_h_0()
                                    .child(self.catalog(tokens, cx).flex_1().min_w(px(0.0)))
                                    .when(self.stage_open, |this| {
                                        this.child(self.stage(tokens, stage_h, cx))
                                    }),
                            )
                            .child(self.player_bar(tokens, cx)),
                    ),
            )
            .when(self.drop_hover, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(6.0))
                        .border_2()
                        .border_color(tokens.white.opacity(0.5)),
                )
            })
            .when_some(self.context_menu.clone(), |this, menu| {
                this.child(deferred(self.overlay_scrim(cx, false)))
                    .child(deferred(self.context_menu_panel(menu, tokens, cx)).with_priority(1))
            })
            .when_some(self.playlist_picker.clone(), |this, picker| {
                this.child(deferred(self.overlay_scrim(cx, false))).child(
                    deferred(self.playlist_picker_panel(picker, tokens, cx)).with_priority(1),
                )
            })
            .when(self.settings_open, |this| {
                this.child(deferred(self.overlay_scrim(cx, true)))
                    .child(deferred(self.settings_dialog(tokens, cx)).with_priority(1))
            })
            .when_some(self.name_target.clone(), |this, target| {
                this.child(deferred(self.overlay_scrim(cx, true))).child(
                    deferred(self.name_prompt_panel(target, window, tokens, cx)).with_priority(2),
                )
            })
            .when(self.tag_editor.is_some(), |this| {
                this.child(deferred(self.overlay_scrim(cx, true)))
                    .child(deferred(self.tag_editor_dialog(window, tokens, cx)).with_priority(2))
            })
            .when_some(self.pending_confirm.clone(), |this, request| {
                this.child(deferred(self.overlay_scrim(cx, true)))
                    .child(deferred(self.confirm_dialog(request, tokens, cx)).with_priority(3))
            })
            .when(!self.status.is_empty(), |this| {
                this.child(deferred(self.status_toast(tokens, cx)).with_priority(4))
            })
    }
}
