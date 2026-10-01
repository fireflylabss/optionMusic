//! Now Playing bridge: publishes the current track to the OS media surface
//! (Control Center / Touch Bar / AirPods / media keys) and feeds remote
//! commands back into the player.
//!
//! macOS uses `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter`; other
//! platforms get no-op stubs so the call sites stay unconditional.

/// A remote-command event produced by the OS and drained on the app thread.
#[derive(Clone, Copy)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum RemoteCommand {
    TogglePause,
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    /// Absolute position in seconds (`changePlaybackPositionCommand`).
    Seek(f64),
}

#[cfg(target_os = "macos")]
pub(crate) use macos::NowPlaying;
#[cfg(not(target_os = "macos"))]
pub(crate) use stub::NowPlaying;

#[cfg(not(target_os = "macos"))]
mod stub {
    use std::path::Path;

    use optionmusic::controller::PlaybackState;

    use super::RemoteCommand;

    pub(crate) struct NowPlaying;

    impl NowPlaying {
        pub(crate) fn new() -> Self {
            Self
        }

        pub(crate) fn sync(&mut self, _playback: &PlaybackState, _cover: Option<&Path>) {}

        pub(crate) fn poll_commands(&mut self) -> Vec<RemoteCommand> {
            Vec::new()
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    // objc 0.2's msg_send!/class! expansions contain cfg(feature="cargo-clippy")
    // probes that trip check-cfg at the call site.
    #![allow(unexpected_cfgs)]

    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use block::{ConcreteBlock, RcBlock};
    use objc::runtime::Object;
    #[cfg(target_arch = "x86_64")]
    use objc::runtime::Sel;
    use objc::{class, msg_send, sel, sel_impl};

    use optionmusic::controller::PlaybackState;

    use super::RemoteCommand;
    use crate::model::{track_album, track_artist};

    const MP_NOW_PLAYING_STATE_PLAYING: i64 = 1;
    const MP_NOW_PLAYING_STATE_PAUSED: i64 = 2;
    const MP_NOW_PLAYING_STATE_STOPPED: i64 = 3;
    const MP_REMOTE_STATUS_SUCCESS: isize = 0;
    const MP_REMOTE_STATUS_NO_SUCH_CONTENT: isize = 1;
    const MP_MEDIA_TYPE_AUDIO: i64 = 1;
    const NS_UTF8_STRING_ENCODING: usize = 4;
    /// Republish elapsed time once the real position drifts this far from the
    /// value the OS is extrapolating from our last report (seeks, stalls).
    const DRIFT_TOLERANCE_SECS: f64 = 0.75;

    #[link(name = "MediaPlayer", kind = "framework")]
    unsafe extern "C" {
        static MPMediaItemPropertyTitle: *const Object;
        static MPMediaItemPropertyArtist: *const Object;
        static MPMediaItemPropertyAlbumTitle: *const Object;
        static MPMediaItemPropertyPlaybackDuration: *const Object;
        static MPMediaItemPropertyArtwork: *const Object;
        static MPNowPlayingInfoPropertyElapsedPlaybackTime: *const Object;
        static MPNowPlayingInfoPropertyPlaybackRate: *const Object;
        static MPNowPlayingInfoPropertyMediaType: *const Object;
    }

    /// Command handlers are ObjC blocks registered once per process. They can
    /// run on any thread MediaRemote picks, so they only look up the current
    /// owner (the view that most recently published Now Playing metadata) and
    /// enqueue — the app thread drains via `poll_commands`.
    static COMMAND_OWNER: Mutex<Option<(u64, Sender<RemoteCommand>)>> = Mutex::new(None);
    static REGISTERED: OnceLock<()> = OnceLock::new();
    static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(1);

    /// `RcBlock`s are `!Send`, so they live in thread-local storage for the
    /// lifetime of the app thread that registers them.
    type CommandBlock = RcBlock<(*mut Object,), isize>;
    thread_local! {
        static REMOTE_BLOCKS: RefCell<Vec<CommandBlock>> = const { RefCell::new(Vec::new()) };
    }

    unsafe fn ns_string(text: &str) -> *mut Object {
        unsafe {
            let obj: *mut Object = msg_send![class!(NSString), alloc];
            let obj: *mut Object = msg_send![
                obj,
                initWithBytes: text.as_ptr() as *const c_void
                length: text.len()
                encoding: NS_UTF8_STRING_ENCODING
            ];
            msg_send![obj, autorelease]
        }
    }

    unsafe fn ns_number(value: f64) -> *mut Object {
        unsafe { msg_send![class!(NSNumber), numberWithDouble: value] }
    }

    /// The view whose Now Playing metadata is on screen owns the media keys;
    /// remote commands are delivered to whichever instance last published.
    fn claim_command_ownership(id: u64, tx: &Sender<RemoteCommand>) {
        if let Ok(mut guard) = COMMAND_OWNER.lock() {
            *guard = Some((id, tx.clone()));
        }
    }

    fn release_command_ownership(id: u64) -> bool {
        match COMMAND_OWNER.lock() {
            Ok(mut guard) if matches!(&*guard, Some((owner, _)) if *owner == id) => {
                *guard = None;
                true
            }
            _ => false,
        }
    }

    fn dispatch(command: RemoteCommand) -> isize {
        let tx = COMMAND_OWNER
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|(_, tx)| tx.clone()));
        match tx {
            Some(tx) if tx.send(command).is_ok() => MP_REMOTE_STATUS_SUCCESS,
            _ => MP_REMOTE_STATUS_NO_SUCH_CONTENT,
        }
    }

    fn handler_block(command: RemoteCommand) -> CommandBlock {
        ConcreteBlock::new(move |_event: *mut Object| -> isize { dispatch(command) }).copy()
    }

    #[cfg(target_arch = "x86_64")]
    fn position_time(event: *mut Object) -> f64 {
        // x86-64 requires objc_msgSend_fpret for double-returning messages.
        unsafe extern "C" {
            fn objc_msgSend_fpret(receiver: *mut Object, sel: Sel, ...) -> f64;
        }
        unsafe { objc_msgSend_fpret(event, sel!(positionTime)) }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn position_time(event: *mut Object) -> f64 {
        unsafe { msg_send![event, positionTime] }
    }

    unsafe fn install(command: *mut Object, block: &CommandBlock) {
        unsafe {
            let _: () = msg_send![command, setEnabled: true];
            let _: *mut Object = msg_send![command, addTargetWithHandler: &**block];
        }
    }

    /// Registers every remote command optionMusic supports. Handlers enqueue
    /// onto the channel of whichever `NowPlaying` currently owns playback.
    fn register_remote_commands() {
        unsafe {
            let center: *mut Object = msg_send![class!(MPRemoteCommandCenter), sharedCommandCenter];
            if center.is_null() {
                return;
            }
            let mut blocks = Vec::new();

            let play: *mut Object = msg_send![center, playCommand];
            blocks.push(handler_block(RemoteCommand::Play));
            install(play, blocks.last().unwrap());

            let pause: *mut Object = msg_send![center, pauseCommand];
            blocks.push(handler_block(RemoteCommand::Pause));
            install(pause, blocks.last().unwrap());

            let toggle: *mut Object = msg_send![center, togglePlayPauseCommand];
            blocks.push(handler_block(RemoteCommand::TogglePause));
            install(toggle, blocks.last().unwrap());

            let stop: *mut Object = msg_send![center, stopCommand];
            blocks.push(handler_block(RemoteCommand::Stop));
            install(stop, blocks.last().unwrap());

            let next: *mut Object = msg_send![center, nextTrackCommand];
            blocks.push(handler_block(RemoteCommand::Next));
            install(next, blocks.last().unwrap());

            let previous: *mut Object = msg_send![center, previousTrackCommand];
            blocks.push(handler_block(RemoteCommand::Previous));
            install(previous, blocks.last().unwrap());

            let seek: *mut Object = msg_send![center, changePlaybackPositionCommand];
            blocks.push(
                ConcreteBlock::new(|event: *mut Object| -> isize {
                    let position = position_time(event);
                    if !position.is_finite() || position < 0.0 {
                        return MP_REMOTE_STATUS_NO_SUCH_CONTENT;
                    }
                    dispatch(RemoteCommand::Seek(position))
                })
                .copy(),
            );
            install(seek, blocks.last().unwrap());

            REMOTE_BLOCKS.with(|stored| *stored.borrow_mut() = blocks);
        }
    }

    #[derive(PartialEq)]
    struct Metadata {
        track_id: String,
        title: String,
        artist: String,
        album: String,
        duration_secs: Option<u64>,
        speed_bits: u64,
        paused: bool,
        cover: Option<PathBuf>,
    }

    unsafe fn artwork(path: &Path) -> *mut Object {
        unsafe {
            let file = ns_string(&path.to_string_lossy());
            let image: *mut Object = msg_send![class!(NSImage), alloc];
            let image: *mut Object = msg_send![image, initWithContentsOfFile: file];
            if image.is_null() {
                return ptr::null_mut();
            }
            let image: *mut Object = msg_send![image, autorelease];
            let artwork: *mut Object = msg_send![class!(MPMediaItemArtwork), alloc];
            let artwork: *mut Object = msg_send![artwork, initWithImage: image];
            msg_send![artwork, autorelease]
        }
    }

    unsafe fn set_object(dict: *mut Object, value: *mut Object, key: *const Object) {
        unsafe {
            let _: () = msg_send![dict, setObject: value forKey: key];
        }
    }

    unsafe fn publish(meta: &Metadata, elapsed: f64, rate: f64) {
        unsafe {
            let center: *mut Object = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            let dict: *mut Object = msg_send![class!(NSMutableDictionary), dictionary];
            set_object(dict, ns_string(&meta.title), MPMediaItemPropertyTitle);
            if !meta.artist.is_empty() {
                set_object(dict, ns_string(&meta.artist), MPMediaItemPropertyArtist);
            }
            if !meta.album.is_empty() {
                set_object(dict, ns_string(&meta.album), MPMediaItemPropertyAlbumTitle);
            }
            if let Some(duration) = meta.duration_secs {
                set_object(
                    dict,
                    ns_number(duration as f64),
                    MPMediaItemPropertyPlaybackDuration,
                );
            }
            if let Some(cover) = meta.cover.as_deref() {
                let artwork = artwork(cover);
                if !artwork.is_null() {
                    set_object(dict, artwork, MPMediaItemPropertyArtwork);
                }
            }
            set_object(
                dict,
                ns_number(elapsed),
                MPNowPlayingInfoPropertyElapsedPlaybackTime,
            );
            set_object(dict, ns_number(rate), MPNowPlayingInfoPropertyPlaybackRate);
            set_object(
                dict,
                msg_send![class!(NSNumber), numberWithInteger: MP_MEDIA_TYPE_AUDIO],
                MPNowPlayingInfoPropertyMediaType,
            );
            let _: () = msg_send![center, setNowPlayingInfo: dict];
            let state = if meta.paused {
                MP_NOW_PLAYING_STATE_PAUSED
            } else {
                MP_NOW_PLAYING_STATE_PLAYING
            };
            let _: () = msg_send![center, setPlaybackState: state];
        }
    }

    unsafe fn clear_now_playing() {
        unsafe {
            let center: *mut Object = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            let _: () = msg_send![center, setNowPlayingInfo: ptr::null::<Object>()];
            let _: () = msg_send![center, setPlaybackState: MP_NOW_PLAYING_STATE_STOPPED];
        }
    }

    /// Per-window Now Playing bridge. Owns the command channel; the view polls
    /// it from its playback tick and `sync` republishes on real changes only.
    pub(crate) struct NowPlaying {
        id: u64,
        tx: Sender<RemoteCommand>,
        rx: Receiver<RemoteCommand>,
        published: Option<Metadata>,
        published_elapsed: f64,
        published_rate: f64,
        published_at: Instant,
        cleared: bool,
    }

    impl NowPlaying {
        pub(crate) fn new() -> Self {
            REGISTERED.get_or_init(register_remote_commands);
            let (tx, rx) = channel();
            Self {
                id: NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed),
                tx,
                rx,
                published: None,
                published_elapsed: 0.0,
                published_rate: 0.0,
                published_at: Instant::now(),
                cleared: true,
            }
        }

        /// Publish to `MPNowPlayingInfoCenter`. Dedupes: sends on metadata,
        /// play/pause, artwork or drift (seek) changes only — the OS
        /// extrapolates the scrubber from rate + elapsed in between.
        pub(crate) fn sync(&mut self, playback: &PlaybackState, cover: Option<&Path>) {
            let Some(track) = playback.current.as_ref().filter(|_| !playback.stopped) else {
                if !self.cleared {
                    unsafe { clear_now_playing() };
                    self.cleared = true;
                    self.published = None;
                }
                return;
            };
            let meta = Metadata {
                track_id: track.id.clone(),
                title: track.name.clone(),
                artist: track_artist(track),
                album: track_album(track),
                duration_secs: playback.duration.map(|d| d.max(0.0) as u64),
                speed_bits: playback.speed.to_bits(),
                paused: playback.paused,
                cover: cover.map(Path::to_path_buf),
            };
            let rate = if playback.paused { 0.0 } else { playback.speed };
            let expected = self.published_elapsed
                + self.published_rate * self.published_at.elapsed().as_secs_f64();
            let drift = (playback.position - expected).abs();
            if self.published.as_ref() == Some(&meta) && drift < DRIFT_TOLERANCE_SECS {
                return;
            }
            unsafe { publish(&meta, playback.position, rate) };
            self.published_elapsed = playback.position;
            self.published_rate = rate;
            self.published_at = Instant::now();
            self.published = Some(meta);
            self.cleared = false;
            claim_command_ownership(self.id, &self.tx);
        }

        /// Remote commands enqueued since the last poll, in arrival order.
        pub(crate) fn poll_commands(&mut self) -> Vec<RemoteCommand> {
            let mut out = Vec::new();
            while let Ok(command) = self.rx.try_recv() {
                out.push(command);
            }
            out
        }
    }

    impl Drop for NowPlaying {
        fn drop(&mut self) {
            // Only scrub the OS surface when we were the last publisher —
            // another window's metadata may be on screen.
            if release_command_ownership(self.id) && !self.cleared {
                unsafe { clear_now_playing() };
            }
        }
    }
}
