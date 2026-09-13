//! Core library/player controller shared by desktop clients.
use crate::{
    config::{self, AppConfig},
    eq::EqPreset,
    player::Player,
    playlist::{self, Track},
};
use anyhow::Result;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder: String,
    /// Embedded / tagged artist (empty when missing).
    pub artist: String,
    /// Embedded / tagged album (empty when missing).
    pub album: String,
    /// Track number from tags when known.
    pub track_number: Option<u32>,
    /// Disc number from tags when known.
    pub disc_number: Option<u32>,
    /// Whether cover art was found (`None` until enrichment).
    pub has_cover: Option<bool>,
    /// Public art URL for remote-derived tracks (YouTube thumbs) — the only
    /// image Discord Rich Presence can render for local playback.
    pub thumb_url: Option<String>,
    /// Track length in seconds (`None` until enrichment probes the file).
    pub duration_secs: Option<f64>,
    /// Unix seconds of file mtime; `0` when metadata is unavailable.
    pub mtime: u64,
    /// File size in bytes; `0` when metadata is unavailable.
    pub size: u64,
}
impl From<&Track> for TrackDto {
    fn from(t: &Track) -> Self {
        let path = t.path.to_string_lossy().into_owned();
        Self {
            id: path.clone(),
            name: t.display_name(),
            folder: t
                .path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            artist: t.artist.clone().unwrap_or_default(),
            album: t.album.clone().unwrap_or_default(),
            track_number: t.track_number,
            disc_number: t.disc_number,
            has_cover: t.has_cover,
            thumb_url: t.thumb_url(),
            path,
            duration_secs: t.duration_secs,
            mtime: t.mtime,
            size: t.size,
        }
    }
}
/// Live transport state for the desktop ticker (no library payload).
///
/// Emitting the full library on every tick (~0.5 MB with a large collection)
/// starved the UI thread and made play clicks appear to do nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LoopMode {
    Off,
    List,
    Track,
}

impl LoopMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::List,
            Self::List => Self::Track,
            Self::Track => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::List => "all",
            Self::Track => "one",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaybackState {
    pub queue: Vec<String>,
    pub current: Option<TrackDto>,
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
    pub stopped: bool,
    pub volume: u8,
    pub muted: bool,
    pub speed: f64,
    pub pitch: f64,
    pub eq: String,
    pub favorites: Vec<String>,
    pub loop_mode: LoopMode,
    pub shuffled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub library: Vec<TrackDto>,
    pub queue: Vec<String>,
    pub current: Option<TrackDto>,
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
    pub stopped: bool,
    pub volume: u8,
    pub muted: bool,
    pub speed: f64,
    pub pitch: f64,
    pub eq: String,
    pub favorites: Vec<String>,
    pub loop_mode: LoopMode,
    pub shuffled: bool,
    pub settings: AppConfig,
    pub desktop_preferences: String,
}

/// Partial library update after background tag enrichment.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEnrichUpdate {
    pub tracks: Vec<TrackDto>,
    pub done: bool,
}

/// Sole owner of the known library, playback rules, queue and persistence.
pub struct CoreController {
    pub config: AppConfig,
    library: Vec<Track>,
    path_index: HashMap<String, usize>,
    queue: VecDeque<String>,
    current: Option<String>,
    player: Option<Player>,
    manually_stopped: bool,
    loop_mode: LoopMode,
    shuffled: bool,
    /// History-aware shuffle: advance avoids recently played tracks.
    smart_shuffle: bool,
    recent: VecDeque<String>,
    desktop_preferences: String,
    /// Wall-clock of last resume write (throttle disk I/O while playing).
    last_resume_save: std::time::Instant,
}
impl Default for CoreController {
    fn default() -> Self {
        Self::new()
    }
}
impl CoreController {
    pub fn new() -> Self {
        Self::with_config(AppConfig::load())
    }
    pub fn with_config(config: AppConfig) -> Self {
        Self {
            config,
            library: Vec::new(),
            path_index: HashMap::new(),
            queue: VecDeque::new(),
            current: None,
            player: None,
            manually_stopped: true,
            loop_mode: LoopMode::Off,
            shuffled: false,
            smart_shuffle: false,
            recent: VecDeque::new(),
            desktop_preferences: Self::load_desktop_preferences(),
            last_resume_save: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(60))
                .unwrap_or_else(std::time::Instant::now),
        }
    }
    fn load_desktop_preferences() -> String {
        std::fs::read_to_string(config::config_path())
            .ok()
            .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
            .and_then(|doc| {
                doc.get("desktop_preferences")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "{}".into())
    }
    fn save_config(&self) -> Result<()> {
        self.config.save()?;
        let path = config::config_path();
        let mut table = match toml::from_str::<toml::Value>(&std::fs::read_to_string(&path)?) {
            Ok(toml::Value::Table(table)) => table,
            _ => Default::default(),
        };
        table.insert(
            "desktop_preferences".into(),
            toml::Value::String(self.desktop_preferences.clone()),
        );
        let doc = toml::Value::Table(table);
        option_sdk::atomic_write(path, toml::to_string_pretty(&doc)?.as_bytes())?;
        Ok(())
    }
    pub fn set_desktop_preferences(&mut self, preferences: String) -> Result<()> {
        self.desktop_preferences = preferences;
        self.save_config()
    }
    pub fn desktop_preferences(&self) -> &str {
        &self.desktop_preferences
    }
    pub fn scan(&mut self, dirs: Option<Vec<PathBuf>>) -> Result<&[Track]> {
        if let Some(dirs) = dirs {
            // A removed folder can remain in the saved desktop configuration.
            // Ignore and prune only missing directories; other resolution errors
            // should still be reported.
            self.config.music_dirs = dirs
                .iter()
                .filter_map(|p| match config::resolve_music_dir(&p.to_string_lossy()) {
                    Ok(path) => Some(Ok(path)),
                    Err(error)
                        if error
                            .to_string()
                            .starts_with("music directory does not exist:") =>
                    {
                        None
                    }
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<_>>()?;
            self.save_config()?;
        }
        let mut all = Vec::new();
        for d in Self::scan_directories(&self.config.music_dirs) {
            if d.exists() {
                all.extend(playlist::scan_path(&d, true)?);
            }
        }
        all.sort_by(|a, b| a.path.cmp(&b.path));
        all.dedup_by(|a, b| a.path == b.path);
        self.library = all;
        self.rebuild_path_index();
        self.shuffled = false;
        Ok(&self.library)
    }

    /// Directories to walk: configured folders plus default `~/Music` when not already listed.
    fn scan_directories(music_dirs: &[PathBuf]) -> Vec<PathBuf> {
        let default = config::default_music_dir();
        let default_present = music_dirs.iter().any(|d| paths_equivalent(d, &default));
        let mut dirs: Vec<PathBuf> = music_dirs.to_vec();
        if !default_present && default.exists() {
            dirs.push(default);
        }
        dirs
    }

    fn rebuild_path_index(&mut self) {
        self.path_index.clear();
        for (i, track) in self.library.iter().enumerate() {
            self.path_index
                .insert(track.path.to_string_lossy().into_owned(), i);
        }
    }

    /// Enrich tags for up to `limit` tracks that have not been processed yet.
    /// Returns DTOs for tracks whose metadata changed.
    pub fn enrich_tags_batch(&mut self, limit: usize) -> LibraryEnrichUpdate {
        let mut updated = Vec::new();
        let mut processed = 0usize;
        for track in self.library.iter_mut() {
            if processed >= limit {
                break;
            }
            if track.tags_enriched {
                continue;
            }
            let before = TrackDto::from(&*track);
            track.enrich_tags();
            processed += 1;
            let after = TrackDto::from(&*track);
            if dto_visible_fields_changed(&before, &after) {
                updated.push(after);
            }
        }
        let done = self.library.iter().all(|t| t.tags_enriched);
        LibraryEnrichUpdate {
            tracks: updated,
            done,
        }
    }

    /// Up to `limit` not-yet-enriched tracks as `(id, path, mtime, size)`
    /// tuples, in library order. Non-mutating: the caller reads tags and
    /// cover art on a background executor (e.g. `meta::read_tags_cached` +
    /// `cover::resolve_cover_file`), then hands results to
    /// [`Self::enrich_apply`] back on the app thread.
    pub fn enrich_prepare(&self, limit: usize) -> Vec<(String, PathBuf, u64, u64)> {
        self.library
            .iter()
            .filter(|t| !t.tags_enriched)
            .take(limit)
            .map(|t| {
                (
                    t.path.to_string_lossy().into_owned(),
                    t.path.clone(),
                    t.mtime,
                    t.size,
                )
            })
            .collect()
    }

    /// Apply tag reads produced off-thread for [`Self::enrich_prepare`] ids.
    /// Each item is `(id, tags, has_cover)`; unknown ids are skipped.
    /// Returns DTOs for tracks whose visible fields changed (same diff rule
    /// as [`Self::enrich_tags_batch`]).
    pub fn enrich_apply(
        &mut self,
        results: Vec<(String, crate::meta::AudioTags, bool)>,
    ) -> Vec<TrackDto> {
        let mut updated = Vec::new();
        for (id, tags, has_cover) in results {
            let Some(&i) = self.path_index.get(&id) else {
                continue;
            };
            let Some(track) = self.library.get_mut(i) else {
                continue;
            };
            let before = TrackDto::from(&*track);
            track.apply_tags(tags, has_cover);
            let after = TrackDto::from(&*track);
            if dto_visible_fields_changed(&before, &after) {
                updated.push(after);
            }
        }
        updated
    }

    pub fn tags_enrichment_pending(&self) -> bool {
        self.library.iter().any(|t| !t.tags_enriched)
    }

    fn track(&self, id: &str) -> Result<&Track> {
        self.path_index
            .get(id)
            .and_then(|&i| self.library.get(i))
            .ok_or_else(|| anyhow::anyhow!("track is not known by core: {id}"))
    }

    fn track_index(&self, id: &str) -> Result<usize> {
        self.path_index
            .get(id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("track is not known by core: {id}"))
    }
    fn player(&mut self) -> Result<&mut Player> {
        if self.player.is_none() {
            let mut p = Player::new(100, 1.0, 0.0)?;
            p.set_volume_max(self.config.volume_max());
            self.player = Some(p);
        }
        Ok(self.player.as_mut().unwrap())
    }
    pub fn play(&mut self, id: &str) -> Result<()> {
        let path = self.track(id)?.path.clone();
        let loop_track = self.loop_mode == LoopMode::Track;
        // Load first so a failed open does not leave "now playing" without audio.
        let rg = self.config.replaygain;
        let player = self.player()?;
        player.set_loop_track(loop_track);
        player.set_replaygain(rg);
        player.play_file(&path)?;
        self.current = Some(id.into());
        self.manually_stopped = false;
        self.queue.retain(|x| x != id);
        self.push_recent(id);
        let _ = crate::history::record_play(id);
        let _ = self.persist_resume(true);
        Ok(())
    }
    pub fn toggle_pause(&mut self) -> Result<bool> {
        // Resume/restart the current track when idle instead of no-oping — the
        // desktop Play button calls this after a track has already been chosen.
        let resume = self.current.is_some()
            && match self.player.as_mut() {
                Some(player) => player.is_idle(),
                None => true,
            };
        if resume {
            let id = self.current.clone().expect("current checked above");
            self.play(&id)?;
            return Ok(false);
        }
        let paused = match self.player.as_mut() {
            Some(player) => player.toggle_pause(),
            None => true,
        };
        let _ = self.persist_resume(true);
        Ok(paused)
    }
    pub fn stop(&mut self) {
        self.manually_stopped = true;
        if let Some(p) = self.player.as_mut() {
            p.stop();
        }
        let _ = self.persist_resume(true);
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<()> {
        let id = self
            .queue
            .pop_front()
            .or_else(|| self.smart_pick())
            .or_else(|| self.next_id())
            .or_else(|| {
                if self.loop_mode == LoopMode::List {
                    self.library
                        .first()
                        .map(|t| t.path.to_string_lossy().into_owned())
                } else {
                    None
                }
            });
        if let Some(id) = id {
            self.play(&id)
        } else {
            Ok(())
        }
    }
    pub fn previous(&mut self) -> Result<()> {
        if let Some(p) = self.player.as_mut()
            && !p.is_idle()
            && p.position() > Duration::from_secs(3)
        {
            p.seek(Duration::ZERO)?;
            return Ok(());
        }
        let id = self.current.clone();
        if let Some(id) = id {
            let i = self.track_index(&id).unwrap_or(0);
            let target = if i > 0 {
                match self.library.get(i - 1) {
                    Some(track) => track.path.to_string_lossy().into_owned(),
                    None => id,
                }
            } else {
                id
            };
            self.play(&target)?;
        }
        Ok(())
    }
    fn next_id(&self) -> Option<String> {
        let i = self
            .current
            .as_ref()
            .and_then(|id| self.track_index(id).ok())
            .map(|i| i + 1)
            .unwrap_or(0);
        self.library
            .get(i)
            .map(|t| t.path.to_string_lossy().into_owned())
    }
    /// Toggle history-aware shuffle. Returns the new state.
    pub fn toggle_smart_shuffle(&mut self) -> bool {
        self.smart_shuffle = !self.smart_shuffle;
        self.smart_shuffle
    }
    pub fn smart_shuffle(&self) -> bool {
        self.smart_shuffle
    }
    fn push_recent(&mut self, id: &str) {
        let cap = crate::smart_shuffle::window_for_len(self.library.len()).max(1);
        self.recent.push_back(id.to_owned());
        while self.recent.len() > cap {
            self.recent.pop_front();
        }
    }
    /// Next track outside the recent window when smart shuffle is on.
    fn smart_pick(&self) -> Option<String> {
        if !self.smart_shuffle || self.library.is_empty() {
            return None;
        }
        let window = crate::smart_shuffle::window_for_len(self.library.len());
        let skip: std::collections::HashSet<&str> = self
            .recent
            .iter()
            .rev()
            .take(window)
            .map(|s| s.as_str())
            .collect();
        let candidates: Vec<usize> = self
            .library
            .iter()
            .enumerate()
            .filter(|(_, t)| !skip.contains(t.path.to_string_lossy().as_ref()))
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        let mut state = seed ^ 0x9e3779b97f4a7c15;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.library
            .get(candidates[(state as usize) % candidates.len()])
            .map(|t| t.path.to_string_lossy().into_owned())
    }
    pub fn seek(&mut self, s: f64) -> Result<()> {
        self.player()?.seek(Duration::from_secs_f64(s.max(0.0)))?;
        let _ = self.persist_resume(true);
        Ok(())
    }
    pub fn set_volume(&mut self, v: u8) {
        if let Ok(p) = self.player() {
            p.set_volume(v);
        }
    }
    pub fn set_excess_volume(&mut self, enabled: bool) -> Result<()> {
        self.config.excess_volume = enabled;
        let max = self.config.volume_max();
        if let Ok(p) = self.player() {
            p.set_volume_max(max);
            if !enabled && p.volume() > 100 {
                p.set_volume(100);
            }
        }
        self.save_config()
    }
    pub fn set_ldm(&mut self, enabled: bool) -> Result<()> {
        self.config.ldm = enabled;
        self.save_config()
    }
    pub fn set_discord_rpc(&mut self, enabled: bool) -> Result<()> {
        self.config.discord_rpc = enabled;
        self.save_config()
    }
    pub fn set_artist_source(&mut self, source: crate::config::ArtistSource) -> Result<()> {
        self.config.artist_source = source;
        self.save_config()
    }
    /// Persist current track / position / queue into shared config.toml.
    pub fn persist_resume(&mut self, force: bool) -> Result<()> {
        let position = self
            .player
            .as_ref()
            .map(|p| p.position().as_secs_f64())
            .unwrap_or(0.0);
        let track = self.current.clone().unwrap_or_default();
        let queue: Vec<String> = self.queue.iter().cloned().collect();
        let changed = track != self.config.resume_track
            || (position - self.config.resume_position).abs() > 1.5
            || queue != self.config.resume_queue;
        if !changed && !force {
            return Ok(());
        }
        if !force && self.last_resume_save.elapsed() < std::time::Duration::from_secs(4) {
            return Ok(());
        }
        self.config.resume_track = track;
        self.config.resume_position = position;
        self.config.resume_queue = queue;
        self.last_resume_save = std::time::Instant::now();
        self.save_config()
    }
    /// Restore the last session (paused at saved position). Call after scan.
    pub fn restore_session(&mut self) -> Result<bool> {
        let id = self.config.resume_track.clone();
        if id.is_empty() {
            return Ok(false);
        }
        if self.track(&id).is_err() {
            return Ok(false);
        }
        let position = self.config.resume_position.max(0.0);
        let queue = self.config.resume_queue.clone();
        self.queue = queue
            .into_iter()
            .filter(|q| self.path_index.contains_key(q))
            .collect();
        let path = self.track(&id)?.path.clone();
        let loop_track = self.loop_mode == LoopMode::Track;
        let player = self.player()?;
        player.set_loop_track(loop_track);
        player.play_file_paused_at(&path, position)?;
        self.current = Some(id);
        self.manually_stopped = false;
        Ok(true)
    }
    pub fn toggle_mute(&mut self) -> bool {
        match self.player() {
            Ok(p) => p.toggle_mute(),
            Err(_) => false,
        }
    }
    pub fn set_eq(&mut self, e: EqPreset) {
        if let Ok(p) = self.player() {
            p.set_eq(e);
        }
    }

    pub fn set_speed(&mut self, speed: f64) -> Result<f64> {
        let p = self.player()?;
        p.set_speed(speed);
        Ok(p.speed())
    }

    pub fn set_pitch(&mut self, pitch: f64) -> Result<f64> {
        let p = self.player()?;
        p.set_pitch(pitch);
        Ok(p.pitch())
    }

    pub fn reset_speed_pitch(&mut self) -> Result<()> {
        self.player()?.reset_speed_pitch();
        Ok(())
    }

    pub fn set_replaygain(&mut self, mode: crate::config::ReplayGainMode) -> Result<()> {
        self.config.replaygain = mode;
        self.config.save()?;
        if let Some(p) = self.player.as_mut() {
            p.set_replaygain(mode);
        }
        Ok(())
    }

    pub fn cycle_loop(&mut self) -> LoopMode {
        self.loop_mode = self.loop_mode.next();
        let loop_track = self.loop_mode == LoopMode::Track;
        if let Ok(p) = self.player() {
            p.set_loop_track(loop_track);
        }
        self.loop_mode
    }
    /// Reorder library for sequential next/previous (catalog UI sorts independently).
    pub fn shuffle(&mut self) {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        for i in (1..self.library.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (seed as usize) % (i + 1);
            self.library.swap(i, j);
        }
        self.rebuild_path_index();
        self.shuffled = true;
    }
    pub fn add_queue(&mut self, id: &str) -> Result<()> {
        self.track(id)?;
        if !self.queue.iter().any(|x| x == id) {
            self.queue.push_back(id.into());
        }
        Ok(())
    }
    pub fn remove_queue(&mut self, id: &str) {
        self.queue.retain(|x| x != id)
    }
    /// Empty the play queue (the current track keeps playing).
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }
    /// Move the queued item at `from` to index `to` (clamped to the tail).
    /// Returns false when `from` is out of bounds.
    pub fn queue_move(&mut self, from: usize, to: usize) -> bool {
        if from >= self.queue.len() {
            return false;
        }
        let to = to.min(self.queue.len() - 1);
        if from == to {
            return true;
        }
        if let Some(item) = self.queue.remove(from) {
            self.queue.insert(to, item);
        }
        true
    }
    /// Position of `id` in the queue, if present.
    pub fn queue_index_of(&self, id: &str) -> Option<usize> {
        self.queue.iter().position(|x| x == id)
    }
    pub fn play_next(&mut self, id: &str) -> Result<()> {
        self.track(id)?;
        self.remove_queue(id);
        self.queue.push_front(id.into());
        Ok(())
    }
    pub fn toggle_favorite(&mut self, id: &str) -> Result<bool> {
        self.track(id)?;
        if let Some(i) = self.config.favorites.iter().position(|x| x == id) {
            self.config.favorites.remove(i);
        } else {
            self.config.favorites.push(id.into());
        };
        self.save_config()?;
        Ok(self.config.favorites.iter().any(|x| x == id))
    }
    pub fn known_path(&self, id: &str) -> Result<&Path> {
        Ok(&self.track(id)?.path)
    }
    /// Album art as a `data:` URL (sidecar image or embedded tag), if any.
    pub fn cover_data_url(&self, id: &str) -> Result<Option<String>> {
        let track = self.track(id)?;
        crate::cover::resolve_cover_data_url(&track.path, track.mtime, track.size)
    }

    /// Album art as a local file path for Tauri `convertFileSrc` (preferred over base64 IPC).
    pub fn cover_file_path(&self, id: &str) -> Result<Option<String>> {
        let track = self.track(id)?;
        Ok(
            crate::cover::resolve_cover_file(&track.path, track.mtime, track.size)?
                .map(|c| c.path.to_string_lossy().into_owned()),
        )
    }
    pub fn snapshot(&mut self) -> Snapshot {
        let playback = self.playback_state();
        Snapshot {
            library: self.library.iter().map(TrackDto::from).collect(),
            queue: playback.queue,
            current: playback.current,
            position: playback.position,
            duration: playback.duration,
            paused: playback.paused,
            stopped: playback.stopped,
            volume: playback.volume,
            muted: playback.muted,
            speed: playback.speed,
            pitch: playback.pitch,
            eq: playback.eq,
            favorites: playback.favorites,
            loop_mode: playback.loop_mode,
            shuffled: playback.shuffled,
            settings: self.config.clone(),
            desktop_preferences: self.desktop_preferences.clone(),
        }
    }

    /// Lightweight transport snapshot for the desktop position ticker.
    pub fn playback_state(&mut self) -> PlaybackState {
        crate::mpv::ensure_c_numeric_locale();
        self.advance_if_finished();
        let current = self
            .current
            .as_ref()
            .and_then(|id| self.track_index(id).ok())
            .and_then(|i| self.library.get(i))
            .map(TrackDto::from);
        let (position, duration, paused, stopped, volume, muted, speed, pitch, eq) =
            if let Some(p) = self.player.as_mut() {
                (
                    p.position().as_secs_f64(),
                    p.duration().map(|d| d.as_secs_f64()),
                    p.is_paused(),
                    p.is_idle(),
                    p.volume(),
                    p.muted(),
                    p.speed(),
                    p.pitch(),
                    p.eq_label().into(),
                )
            } else {
                (0.0, None, true, true, 100, false, 1.0, 1.0, "off".into())
            };
        if current.is_some() && !stopped {
            let _ = self.persist_resume(false);
        }
        PlaybackState {
            queue: self.queue.iter().cloned().collect(),
            current,
            position,
            duration,
            paused,
            stopped,
            volume,
            muted,
            speed,
            pitch,
            eq,
            favorites: self.config.favorites.clone(),
            loop_mode: self.loop_mode,
            shuffled: self.shuffled,
        }
    }

    /// Poll libmpv and apply the same sequential policy as the terminal UI.
    /// This is called by the Tauri position ticker, so EOF advances even when
    /// the frontend sends no further command.
    fn advance_if_finished(&mut self) {
        let ended = self.player.as_mut().is_some_and(|p| p.is_idle())
            && !self.manually_stopped
            && self.current.is_some()
            && self.loop_mode != LoopMode::Track;
        if ended {
            let _ = self.next();
        }
    }

    // ── Playlists ────────────────────────────────────────────────

    pub fn list_playlists(&self) -> Result<Vec<crate::saved_playlists::SavedPlaylist>> {
        crate::saved_playlists::list()
    }

    pub fn create_playlist(&self, name: &str) -> Result<crate::saved_playlists::SavedPlaylist> {
        crate::saved_playlists::create(name)
    }

    pub fn rename_playlist(
        &self,
        id: &str,
        name: &str,
    ) -> Result<crate::saved_playlists::SavedPlaylist> {
        crate::saved_playlists::rename(id, name)
    }

    pub fn delete_playlist(&self, id: &str) -> Result<()> {
        crate::saved_playlists::delete(id)
    }

    pub fn playlist_add(
        &self,
        id: &str,
        track_id: &str,
    ) -> Result<crate::saved_playlists::SavedPlaylist> {
        let _ = self.track(track_id)?;
        crate::saved_playlists::add_track(id, track_id)
    }

    pub fn playlist_remove(
        &self,
        id: &str,
        track_id: &str,
    ) -> Result<crate::saved_playlists::SavedPlaylist> {
        crate::saved_playlists::remove_track(id, track_id)
    }

    pub fn import_m3u(
        &self,
        path: &Path,
        name: Option<&str>,
    ) -> Result<crate::saved_playlists::SavedPlaylist> {
        crate::saved_playlists::import_m3u(path, name)
    }

    pub fn export_m3u(&self, id: &str, path: &Path) -> Result<()> {
        crate::saved_playlists::export_m3u(id, path)
    }

    pub fn play_playlist(&mut self, id: &str) -> Result<()> {
        let pl = crate::saved_playlists::get(id)?;
        if pl.tracks.is_empty() {
            anyhow::bail!("playlist is empty");
        }
        self.queue.clear();
        for track_id in pl.tracks.iter().skip(1) {
            if self.path_index.contains_key(track_id) {
                self.queue.push_back(track_id.clone());
            }
        }
        let first = pl.tracks[0].clone();
        self.play(&first)
    }

    // ── Smart shelves ────────────────────────────────────────────

    pub fn smart_shelf(&self, kind: SmartShelf) -> Vec<TrackDto> {
        match kind {
            SmartShelf::PlayedWeek => {
                let ids = crate::history::played_this_week();
                ids.into_iter()
                    .filter_map(|id| self.track(&id).ok().map(TrackDto::from))
                    .collect()
            }
            SmartShelf::NoCover => self
                .library
                .iter()
                .filter(|t| t.has_cover == Some(false))
                .map(TrackDto::from)
                .collect(),
            SmartShelf::IncompleteAlbums => incomplete_album_tracks(&self.library),
        }
    }

    // ── Tags / lyrics ────────────────────────────────────────────

    pub fn get_track_tags(&self, id: &str) -> Result<crate::meta::AudioTags> {
        let track = self.track(id)?;
        Ok(crate::meta::read_tags_cached(
            &track.path,
            track.mtime,
            track.size,
        ))
    }

    pub fn set_track_tags(&mut self, id: &str, tags: crate::meta::AudioTags) -> Result<TrackDto> {
        let idx = self.track_index(id)?;
        let path = self.library[idx].path.clone();
        crate::meta::write_tags(&path, &tags)?;
        let track = &mut self.library[idx];
        let (mtime, size) = (
            std::fs::metadata(&path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(track.mtime),
            std::fs::metadata(&path)
                .map(|m| m.len())
                .unwrap_or(track.size),
        );
        track.mtime = mtime;
        track.size = size;
        track.title = tags.title.clone();
        track.artist = tags.artist.clone();
        track.album = tags.album.clone();
        track.track_number = tags.track_number;
        track.disc_number = tags.disc_number;
        track.tags_enriched = true;
        Ok(TrackDto::from(&*track))
    }

    pub fn track_lyrics(&self, id: &str) -> Result<crate::meta::Lyrics> {
        let track = self.track(id)?;
        Ok(crate::meta::read_lyrics(&track.path))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartShelf {
    PlayedWeek,
    NoCover,
    IncompleteAlbums,
}

fn incomplete_album_tracks(library: &[Track]) -> Vec<TrackDto> {
    use std::collections::HashMap;
    let mut groups: HashMap<(String, String), Vec<&Track>> = HashMap::new();
    for track in library {
        let album = track.album.clone().unwrap_or_default();
        if album.is_empty() {
            continue;
        }
        let artist = track.artist.clone().unwrap_or_default();
        groups.entry((artist, album)).or_default().push(track);
    }
    let mut out = Vec::new();
    for tracks in groups.values() {
        let numbers: Vec<u32> = tracks.iter().filter_map(|t| t.track_number).collect();
        if numbers.is_empty() {
            continue;
        }
        let max = *numbers.iter().max().unwrap_or(&0);
        if max <= 1 {
            continue;
        }
        let mut present: std::collections::HashSet<u32> = numbers.into_iter().collect();
        let incomplete = (1..=max).any(|n| !present.contains(&n)) || tracks.len() < max as usize;
        if incomplete {
            out.extend(tracks.iter().map(|t| TrackDto::from(*t)));
        }
        let _ = &mut present;
    }
    out.sort_by(|a, b| a.album.cmp(&b.album).then(a.name.cmp(&b.name)));
    out
}

/// Whether enrichment changed a field the UI renders (name/artist/album or
/// the freshly probed duration). Shared by `enrich_tags_batch`/`enrich_apply`.
fn dto_visible_fields_changed(before: &TrackDto, after: &TrackDto) -> bool {
    before.artist != after.artist
        || before.album != after.album
        || before.name != after.name
        || before.duration_secs != after.duration_secs
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::Track;

    #[test]
    fn no_mpv_needed_for_snapshot() {
        let mut c = CoreController::with_config(AppConfig::default());
        assert!(c.snapshot().library.is_empty());
    }
    #[test]
    fn toggle_pause_before_first_play_is_safe() {
        let mut c = CoreController::with_config(AppConfig::default());
        assert!(c.toggle_pause().unwrap());
        assert!(c.snapshot().stopped);
    }
    #[test]
    fn playback_state_omits_library() {
        let mut c = CoreController::with_config(AppConfig::default());
        let state = c.playback_state();
        assert!(state.current.is_none());
        assert!(state.stopped);
        // Full snapshot still exposes an empty library for hydrate.
        assert!(c.snapshot().library.is_empty());
    }
    #[test]
    fn empty_library_is_safe_for_scan_ticker_and_navigation() {
        let mut c = CoreController::with_config(AppConfig::default());
        c.library.clear();
        c.queue.clear();
        assert!(c.next().is_ok());
        assert!(c.previous().is_ok());
        assert!(c.snapshot().library.is_empty());
        // This is the same polling path used by the Tauri position ticker.
        assert!(c.snapshot().library.is_empty());
    }
    #[test]
    fn arbitrary_paths_are_rejected() {
        let mut c = CoreController::with_config(AppConfig::default());
        assert!(c.add_queue("/random.mp3").is_err());
    }

    #[test]
    fn track_dto_uses_scan_mtime() {
        let track = Track {
            path: PathBuf::from("/music/song.mp3"),
            title: Some("Song".into()),
            artist: None,
            album: None,
            track_number: None,
            disc_number: None,
            genre: None,
            year: None,
            has_cover: Some(true),
            mtime: 12_345,
            size: 999,
            duration_secs: Some(61.5),
            tags_enriched: true,
        };
        let dto = TrackDto::from(&track);
        assert_eq!(dto.mtime, 12_345);
        assert_eq!(dto.size, 999);
        assert_eq!(dto.duration_secs, Some(61.5));
        assert_eq!(dto.name, "Song");
    }

    fn controller_with_tracks(paths: &[&str]) -> CoreController {
        let mut c = CoreController::with_config(AppConfig::default());
        c.library = paths
            .iter()
            .map(|p| Track::from_path(PathBuf::from(p)))
            .collect();
        c.rebuild_path_index();
        c
    }

    #[test]
    fn enrich_prepare_returns_only_unenriched() {
        let mut c = controller_with_tracks(&["/m/a.mp3", "/m/b.mp3", "/m/c.mp3"]);
        c.library[1].tags_enriched = true;
        let prep = c.enrich_prepare(8);
        assert_eq!(prep.len(), 2);
        assert_eq!(prep[0].0, "/m/a.mp3");
        assert_eq!(prep[0].1, PathBuf::from("/m/a.mp3"));
        assert_eq!(prep[1].0, "/m/c.mp3");
        // Honors the limit and does not mark anything enriched.
        let prep = c.enrich_prepare(1);
        assert_eq!(prep.len(), 1);
        assert_eq!(prep[0].0, "/m/a.mp3");
        assert!(c.tags_enrichment_pending());
    }

    #[test]
    fn enrich_apply_marks_and_updates() {
        let mut c = controller_with_tracks(&["/m/a.mp3", "/m/b.mp3"]);
        let tags = crate::meta::AudioTags {
            title: Some("Title A".into()),
            artist: Some("Artist A".into()),
            album: Some("Album A".into()),
            track_number: Some(3),
            duration_secs: Some(123.5),
            ..Default::default()
        };
        let updated = c.enrich_apply(vec![
            ("/m/a.mp3".into(), tags, true),
            ("/m/missing.mp3".into(), Default::default(), false),
        ]);
        // Only the known, visibly changed track is reported.
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "/m/a.mp3");
        assert_eq!(updated[0].name, "Title A");
        assert_eq!(updated[0].artist, "Artist A");
        assert_eq!(updated[0].album, "Album A");
        assert_eq!(updated[0].duration_secs, Some(123.5));
        assert_eq!(updated[0].has_cover, Some(true));
        let t = &c.library[0];
        assert!(t.tags_enriched);
        assert_eq!(t.title.as_deref(), Some("Title A"));
        assert_eq!(t.track_number, Some(3));
        assert_eq!(t.duration_secs, Some(123.5));
        assert_eq!(t.has_cover, Some(true));
        assert!(c.tags_enrichment_pending());
        // Empty tags still mark the track done but produce no visible diff.
        let updated = c.enrich_apply(vec![("/m/b.mp3".into(), Default::default(), false)]);
        assert!(updated.is_empty());
        assert!(c.library[1].tags_enriched);
        assert_eq!(c.library[1].has_cover, Some(false));
        assert!(!c.tags_enrichment_pending());
    }

    #[test]
    fn queue_move_and_clear() {
        let mut c = controller_with_tracks(&["/m/a.mp3", "/m/b.mp3", "/m/c.mp3"]);
        for id in ["/m/a.mp3", "/m/b.mp3", "/m/c.mp3"] {
            c.add_queue(id).unwrap();
        }
        assert_eq!(c.queue_index_of("/m/b.mp3"), Some(1));
        assert!(c.queue_move(0, 2));
        assert_eq!(
            c.queue.iter().cloned().collect::<Vec<_>>(),
            vec!["/m/b.mp3", "/m/c.mp3", "/m/a.mp3"]
        );
        assert!(c.queue_move(1, 1)); // same index is a valid no-op
        assert!(!c.queue_move(3, 0)); // out of bounds
        assert!(!c.queue_move(9, 9));
        assert!(c.queue_move(0, 99)); // `to` clamps to the tail
        assert_eq!(
            c.queue.iter().cloned().collect::<Vec<_>>(),
            vec!["/m/c.mp3", "/m/a.mp3", "/m/b.mp3"]
        );
        assert_eq!(c.queue_index_of("/m/b.mp3"), Some(2));
        c.clear_queue();
        assert!(c.queue.is_empty());
        assert_eq!(c.queue_index_of("/m/a.mp3"), None);
        assert!(!c.queue_move(0, 0)); // empty queue: nothing to move
    }

    #[test]
    fn enrich_tags_batch_marks_done() {
        let mut c = CoreController::with_config(AppConfig::default());
        c.library = vec![Track::from_path(PathBuf::from("/nope.mp3"))];
        c.rebuild_path_index();
        let update = c.enrich_tags_batch(8);
        assert!(update.done);
        assert!(c.library[0].tags_enriched);
    }

    #[test]
    fn loop_mode_cycles_off_all_one() {
        let mut m = LoopMode::Off;
        m = m.next();
        assert_eq!(m, LoopMode::List);
        assert_eq!(m.label(), "all");
        m = m.next();
        assert_eq!(m, LoopMode::Track);
        assert_eq!(m.label(), "one");
        m = m.next();
        assert_eq!(m, LoopMode::Off);
        assert_eq!(m.label(), "off");
    }

    #[test]
    fn smart_shuffle_avoids_recent() {
        let mut c = CoreController::with_config(AppConfig::default());
        c.library = (0..12)
            .map(|i| Track::from_path(PathBuf::from(format!("/m/t{i}.mp3"))))
            .collect();
        c.rebuild_path_index();
        assert!(c.toggle_smart_shuffle());
        for i in 1..=8 {
            c.push_recent(&format!("/m/t{i}.mp3"));
        }
        for _ in 0..20 {
            let pick = c.smart_pick().expect("pick");
            assert!(
                ["/m/t0.mp3", "/m/t9.mp3", "/m/t10.mp3", "/m/t11.mp3"].contains(&pick.as_str()),
                "picked recent {pick}"
            );
        }
        c.smart_shuffle = false;
        assert!(c.smart_pick().is_none());
    }
}
