//! Core library/player controller shared by desktop clients.
use crate::{
    config::{self, AppConfig},
    eq::EqPreset,
    player::Player,
    playlist::{self, Track},
};
use anyhow::Result;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct TrackDto {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder: String,
    /// Embedded / tagged artist (empty when missing).
    pub artist: String,
    /// Embedded / tagged album (empty when missing).
    pub album: String,
    /// Unix seconds of file mtime; `0` when metadata is unavailable.
    pub mtime: u64,
}
impl From<&Track> for TrackDto {
    fn from(t: &Track) -> Self {
        let path = t.path.to_string_lossy().into_owned();
        let mtime = std::fs::metadata(&t.path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
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
            path,
            mtime,
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
            Self::List => "list",
            Self::Track => "track",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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

/// Sole owner of the known library, playback rules, queue and persistence.
pub struct CoreController {
    pub config: AppConfig,
    library: Vec<Track>,
    queue: VecDeque<String>,
    current: Option<String>,
    player: Option<Player>,
    manually_stopped: bool,
    loop_mode: LoopMode,
    shuffled: bool,
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
            queue: VecDeque::new(),
            current: None,
            player: None,
            manually_stopped: true,
            loop_mode: LoopMode::Off,
            shuffled: false,
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
        std::fs::write(path, toml::to_string_pretty(&doc)?)?;
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
        for d in self
            .config
            .music_dirs
            .iter()
            .chain(std::iter::once(&config::default_music_dir()))
        {
            if d.exists() {
                all.extend(playlist::scan_path(d, true)?);
            }
        }
        all.sort_by(|a, b| a.path.cmp(&b.path));
        all.dedup_by(|a, b| a.path == b.path);
        self.library = all;
        self.shuffled = false;
        Ok(&self.library)
    }
    fn track(&self, id: &str) -> Result<&Track> {
        self.library
            .iter()
            .find(|t| t.path.to_string_lossy() == id)
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
        let player = self.player()?;
        player.set_loop_track(loop_track);
        player.play_file(&path)?;
        self.current = Some(id.into());
        self.manually_stopped = false;
        self.queue.retain(|x| x != id);
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
    pub fn next(&mut self) -> Result<()> {
        let id = self.queue.pop_front().or_else(|| self.next_id()).or_else(|| {
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
        if let Some(p) = self.player.as_mut() {
            if !p.is_idle() && p.position() > Duration::from_secs(3) {
                p.seek(Duration::ZERO)?;
                return Ok(());
            }
        }
        let id = self.current.clone();
        if let Some(id) = id {
            let i = self
                .library
                .iter()
                .position(|t| t.path.to_string_lossy() == id)
                .unwrap_or(0);
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
            .and_then(|id| {
                self.library
                    .iter()
                    .position(|t| t.path.to_string_lossy() == *id)
            })
            .map(|i| i + 1)
            .unwrap_or(0);
        self.library
            .get(i)
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
            .filter(|q| self.library.iter().any(|t| t.path.to_string_lossy() == *q))
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
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            let j = (seed as usize) % (i + 1);
            self.library.swap(i, j);
        }
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
        let path = self.known_path(id)?;
        crate::cover::resolve_cover_data_url(path)
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
            .and_then(|id| {
                self.library
                    .iter()
                    .find(|t| t.path.to_string_lossy() == *id)
            })
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
}
#[cfg(test)]
mod tests {
    use super::*;
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
}
