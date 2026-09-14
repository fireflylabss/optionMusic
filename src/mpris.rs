//! MPRIS2 (`org.mpris.MediaPlayer2`) on the session bus — opt-out via `mpris`.
//!
//! Purely local: the desktop already routes the multimedia keys
//! (`XF86AudioPlay`, `XF86AudioNext`, …) and panel widgets to whichever player
//! owns an MPRIS name, so exporting the interface is what makes those keys and
//! `playerctl` drive optionMusic — no key grabbing of our own.
//!
//! The bus lives on its own connection thread; incoming methods only queue
//! [`Command`]s, which the TUI loop drains with [`Mpris::take_commands`] and
//! runs through the same code paths as the keyboard. Non-Linux targets get the
//! same API as no-ops.

use std::time::Duration;

/// Remote control request coming from the bus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Quit,
    /// Relative seek (positive = forward).
    Seek(Duration, bool),
    /// Absolute seek.
    SetPosition(Duration),
    /// Volume as a 0..=100 percentage (MPRIS sends 0.0..=1.0).
    SetVolume(u8),
}

/// What the player currently is, flattened for the bus.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Absolute path of the current file (exported as a `file://` URL).
    pub path: String,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub paused: bool,
    /// Nothing loaded / stopped via `s`.
    pub stopped: bool,
    pub volume: u8,
    pub can_next: bool,
    pub can_prev: bool,
}

impl Status {
    /// `PlaybackStatus` property value.
    pub fn playback_status(&self) -> &'static str {
        if self.stopped {
            "Stopped"
        } else if self.paused {
            "Paused"
        } else {
            "Playing"
        }
    }

    /// Fields that make up the `Metadata` property — used to dedupe signals.
    fn metadata_key(&self) -> (&str, &str, &str, &str, u64) {
        (
            &self.title,
            &self.artist,
            &self.album,
            &self.path,
            self.duration.unwrap_or_default().as_micros() as u64,
        )
    }
}

#[cfg(target_os = "linux")]
mod bus {
    use super::{Command, Status};
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use zbus::zvariant::{ObjectPath, Value};

    pub(super) type Shared = Arc<Mutex<Status>>;
    pub(super) type Queue = Arc<Mutex<VecDeque<Command>>>;

    /// Bounded so a client hammering the bus while the loop is busy cannot
    /// grow the queue without limit.
    const QUEUE_MAX: usize = 32;

    pub(super) fn push(queue: &Queue, cmd: Command) {
        if let Ok(mut q) = queue.lock()
            && q.len() < QUEUE_MAX
        {
            q.push_back(cmd);
        }
    }

    fn read(state: &Shared) -> Status {
        state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// `Metadata` for the current track. `mpris:trackid` has to be a valid
    /// object path, so the file path itself cannot be used.
    pub(super) fn metadata(status: &Status) -> HashMap<String, Value<'static>> {
        let mut map: HashMap<String, Value<'static>> = HashMap::new();
        let id = if status.path.is_empty() {
            ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack")
        } else {
            ObjectPath::from_static_str_unchecked("/one/hory/optionMusic/track/current")
        };
        map.insert("mpris:trackid".into(), Value::from(id));
        if !status.title.is_empty() {
            map.insert("xesam:title".into(), Value::from(status.title.clone()));
        }
        if !status.artist.is_empty() {
            map.insert(
                "xesam:artist".into(),
                Value::from(vec![status.artist.clone()]),
            );
        }
        if !status.album.is_empty() {
            map.insert("xesam:album".into(), Value::from(status.album.clone()));
        }
        if !status.path.is_empty() {
            map.insert("xesam:url".into(), Value::from(file_url(&status.path)));
        }
        if let Some(dur) = status.duration {
            map.insert("mpris:length".into(), Value::from(dur.as_micros() as i64));
        }
        map
    }

    /// `file://` URL with the path percent-encoded (RFC 3986 unreserved set
    /// plus the separators a path needs).
    pub(super) fn file_url(path: &str) -> String {
        let mut out = String::from("file://");
        for byte in path.as_bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                    out.push(*byte as char)
                }
                _ => out.push_str(&format!("%{byte:02X}")),
            }
        }
        out
    }

    pub(super) struct Root {
        pub queue: Queue,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        fn raise(&self) {}

        fn quit(&self) {
            push(&self.queue, Command::Quit);
        }

        #[zbus(property)]
        fn can_quit(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_raise(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn has_track_list(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn identity(&self) -> &str {
            "optionMusic"
        }

        #[zbus(property)]
        fn desktop_entry(&self) -> &str {
            "optionmusic"
        }

        #[zbus(property)]
        fn supported_uri_schemes(&self) -> Vec<String> {
            vec!["file".into()]
        }

        #[zbus(property)]
        fn supported_mime_types(&self) -> Vec<String> {
            Vec::new()
        }
    }

    pub(super) struct Player {
        pub state: Shared,
        pub queue: Queue,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        fn play(&self) {
            push(&self.queue, Command::Play);
        }

        fn pause(&self) {
            push(&self.queue, Command::Pause);
        }

        fn play_pause(&self) {
            push(&self.queue, Command::PlayPause);
        }

        fn stop(&self) {
            push(&self.queue, Command::Stop);
        }

        fn next(&self) {
            push(&self.queue, Command::Next);
        }

        fn previous(&self) {
            push(&self.queue, Command::Previous);
        }

        fn seek(&self, offset_micros: i64) {
            let forward = offset_micros >= 0;
            let delta = Duration::from_micros(offset_micros.unsigned_abs());
            push(&self.queue, Command::Seek(delta, forward));
        }

        fn set_position(&self, _track: ObjectPath<'_>, position_micros: i64) {
            let target = Duration::from_micros(position_micros.max(0) as u64);
            push(&self.queue, Command::SetPosition(target));
        }

        fn open_uri(&self, _uri: String) {}

        #[zbus(property)]
        fn playback_status(&self) -> String {
            read(&self.state).playback_status().to_string()
        }

        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, Value<'static>> {
            metadata(&read(&self.state))
        }

        #[zbus(property)]
        fn position(&self) -> i64 {
            read(&self.state).position.as_micros() as i64
        }

        #[zbus(property)]
        fn volume(&self) -> f64 {
            f64::from(read(&self.state).volume) / 100.0
        }

        #[zbus(property)]
        fn set_volume(&self, volume: f64) {
            let pct = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
            push(&self.queue, Command::SetVolume(pct));
        }

        #[zbus(property)]
        fn rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn set_rate(&self, _rate: f64) {}

        #[zbus(property)]
        fn minimum_rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn maximum_rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            read(&self.state).can_next
        }

        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            read(&self.state).can_prev
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_seek(&self) -> bool {
            !read(&self.state).stopped
        }

        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
    }
}

/// MPRIS server handle. Every method no-ops while disabled (or off Linux), so
/// callers can `sync` + `update` unconditionally each frame.
pub struct Mpris {
    enabled: bool,
    #[cfg(target_os = "linux")]
    conn: Option<zbus::blocking::Connection>,
    #[cfg(target_os = "linux")]
    state: bus::Shared,
    #[cfg(target_os = "linux")]
    queue: bus::Queue,
    /// Last published status — dedupes property-changed signals.
    published: Option<Status>,
}

impl Default for Mpris {
    fn default() -> Self {
        Self::new()
    }
}

impl Mpris {
    pub fn new() -> Self {
        Self {
            enabled: false,
            #[cfg(target_os = "linux")]
            conn: None,
            #[cfg(target_os = "linux")]
            state: Default::default(),
            #[cfg(target_os = "linux")]
            queue: Default::default(),
            published: None,
        }
    }

    /// Whether the bus name is currently owned.
    pub fn active(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.conn.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Follow the config toggle live. Returns a one-shot warning when the
    /// interface was requested but the session bus is unreachable.
    pub fn sync(&mut self, enabled: bool) -> Option<&'static str> {
        if enabled == self.enabled {
            return None;
        }
        self.enabled = enabled;
        #[cfg(target_os = "linux")]
        {
            if !enabled {
                self.conn = None;
                self.published = None;
                return None;
            }
            match self.serve() {
                Ok(()) => None,
                Err(_) => {
                    self.enabled = false;
                    Some("mpris · no session bus")
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn serve(&mut self) -> zbus::Result<()> {
        // Unique-per-process name, as the spec requires when several
        // instances of one player can run at once.
        let name = format!(
            "org.mpris.MediaPlayer2.optionmusic.instance{}",
            std::process::id()
        );
        let conn = zbus::blocking::connection::Builder::session()?
            .serve_at(
                "/org/mpris/MediaPlayer2",
                bus::Root {
                    queue: self.queue.clone(),
                },
            )?
            .serve_at(
                "/org/mpris/MediaPlayer2",
                bus::Player {
                    state: self.state.clone(),
                    queue: self.queue.clone(),
                },
            )?
            .name(name)?
            .build()?;
        self.conn = Some(conn);
        Ok(())
    }

    /// Publish the current state, emitting `PropertiesChanged` only for the
    /// properties that actually moved. Safe to call every frame.
    pub fn update(&mut self, status: Status) {
        if !self.enabled {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            if let Ok(mut shared) = self.state.lock() {
                *shared = status.clone();
            }
            let Some(conn) = self.conn.as_ref() else {
                return;
            };
            let Ok(iface) = conn
                .object_server()
                .interface::<_, bus::Player>("/org/mpris/MediaPlayer2")
            else {
                return;
            };
            let previous = self.published.take();
            let emitter = iface.signal_emitter();
            let player = iface.get();
            let metadata_moved = previous
                .as_ref()
                .is_none_or(|p| p.metadata_key() != status.metadata_key());
            let playback_moved = previous
                .as_ref()
                .is_none_or(|p| p.playback_status() != status.playback_status());
            let volume_moved = previous.as_ref().is_none_or(|p| p.volume != status.volume);
            let caps_moved = previous
                .as_ref()
                .is_none_or(|p| (p.can_next, p.can_prev) != (status.can_next, status.can_prev));
            if metadata_moved {
                let _ = zbus::block_on(player.metadata_changed(emitter));
            }
            if playback_moved {
                let _ = zbus::block_on(player.playback_status_changed(emitter));
            }
            if volume_moved {
                let _ = zbus::block_on(player.volume_changed(emitter));
            }
            if caps_moved {
                let _ = zbus::block_on(player.can_go_next_changed(emitter));
                let _ = zbus::block_on(player.can_go_previous_changed(emitter));
            }
            drop(player);
            self.published = Some(status);
        }
        #[cfg(not(target_os = "linux"))]
        {
            self.published = Some(status);
        }
    }

    /// Drain the commands received since the last call.
    pub fn take_commands(&mut self) -> Vec<Command> {
        #[cfg(target_os = "linux")]
        {
            if !self.enabled {
                return Vec::new();
            }
            match self.queue.lock() {
                Ok(mut q) => q.drain(..).collect(),
                Err(_) => Vec::new(),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Vec::new()
        }
    }

    /// Drop the bus name (on quit).
    pub fn clear(&mut self) {
        self.enabled = false;
        self.published = None;
        #[cfg(target_os = "linux")]
        {
            self.conn = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> Status {
        Status {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            path: "/home/u/Music/a b.mp3".into(),
            position: Duration::from_secs(3),
            duration: Some(Duration::from_secs(200)),
            paused: false,
            stopped: false,
            volume: 80,
            can_next: true,
            can_prev: false,
        }
    }

    #[test]
    fn playback_status_maps_stopped_before_paused() {
        let mut s = playing();
        assert_eq!(s.playback_status(), "Playing");
        s.paused = true;
        assert_eq!(s.playback_status(), "Paused");
        s.stopped = true;
        assert_eq!(s.playback_status(), "Stopped");
    }

    #[test]
    fn metadata_key_ignores_position_and_pause() {
        let a = playing();
        let mut b = a.clone();
        b.position = Duration::from_secs(120);
        b.paused = true;
        assert_eq!(a.metadata_key(), b.metadata_key());
        b.title = "Other".into();
        assert_ne!(a.metadata_key(), b.metadata_key());
    }

    #[test]
    fn disabled_mpris_owns_nothing_and_yields_no_commands() {
        let mut mpris = Mpris::new();
        assert!(!mpris.active());
        mpris.update(playing());
        assert!(mpris.take_commands().is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn file_url_escapes_spaces_and_keeps_separators() {
        assert_eq!(
            super::bus::file_url("/home/u/My Music/a+b.mp3"),
            "file:///home/u/My%20Music/a%2Bb.mp3"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn metadata_carries_title_length_and_url() {
        let map = super::bus::metadata(&playing());
        assert!(map.contains_key("mpris:trackid"));
        assert_eq!(
            map.get("mpris:length"),
            Some(&zbus::zvariant::Value::from(200_000_000i64))
        );
        assert_eq!(
            map.get("xesam:url"),
            Some(&zbus::zvariant::Value::from(
                "file:///home/u/Music/a%20b.mp3".to_string()
            ))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn empty_track_reports_the_no_track_id() {
        let map = super::bus::metadata(&Status::default());
        assert!(!map.contains_key("xesam:title"));
        assert!(!map.contains_key("mpris:length"));
        assert!(map.contains_key("mpris:trackid"));
    }
}
