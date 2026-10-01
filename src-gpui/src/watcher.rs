//! Filesystem watcher for the music library.
//!
//! `notify` events from every scanned directory are forwarded through an
//! mpsc channel, debounced on a GPUI task (~`DEBOUNCE` of quiet), and turned
//! into a rescan on the app thread. All waiting happens on background
//! executor threads — the UI thread never blocks on filesystem events.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{Context, Task};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::view::RootView;

/// Quiet period after the last filesystem event before a rescan fires.
const DEBOUNCE: Duration = Duration::from_millis(1500);
/// Wait between retries while a previous rescan is still in flight.
const BUSY_RETRY: Duration = Duration::from_millis(800);

/// Directories the library scans — mirrors `CoreController::scan_directories`:
/// configured `music_dirs` plus the default `~/Music` when not already listed.
pub(crate) fn watch_dirs(music_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let default = optionmusic::config::default_music_dir();
    let mut dirs = music_dirs.to_vec();
    if !dirs.iter().any(|d| same_dir(d, &default)) && default.is_dir() {
        dirs.push(default);
    }
    dirs
}

/// Path equivalence tolerant to symlinks (`paths_equivalent` in controller).
fn same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Live watcher plus the GPUI pump task. Dropping it (with the view) stops
/// watching; the pump ends when the event channel closes.
pub(crate) struct LibraryWatcher {
    inner: RecommendedWatcher,
    /// Directories already watched — dedup for `watch_dirs` re-syncs.
    watched: Vec<PathBuf>,
    _task: Task<()>,
}

impl LibraryWatcher {
    /// Watch every directory the library scans. Returns `None` when the OS
    /// watcher cannot be created — the app keeps working without auto-rescan.
    pub(crate) fn start(view: &mut RootView, cx: &mut Context<RootView>) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<()>();
        let inner =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if res.is_ok() {
                    let _ = tx.send(());
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("library watcher disabled: {error}");
                    return None;
                }
            };
        let mut watcher = Self {
            inner,
            watched: Vec::new(),
            _task: Self::spawn_pump(cx, rx),
        };
        let dirs = view
            .controller
            .as_ref()
            .map(|c| watch_dirs(&c.config.music_dirs))
            .unwrap_or_default();
        watcher.watch_dirs(dirs);
        Some(watcher)
    }

    /// Watch any not-yet-watched directory, recursively.
    pub(crate) fn watch_dirs(&mut self, dirs: Vec<PathBuf>) {
        for dir in dirs {
            if !dir.is_dir() || self.watched.iter().any(|d| same_dir(d, &dir)) {
                continue;
            }
            if self.inner.watch(&dir, RecursiveMode::Recursive).is_ok() {
                self.watched.push(dir);
            }
        }
    }

    /// Pump task: park a background worker on the event channel, debounce
    /// bursts, then ask the view to rescan (retrying while one is in flight
    /// so no burst is dropped).
    fn spawn_pump(cx: &mut Context<RootView>, rx: Receiver<()>) -> Task<()> {
        let rx = Arc::new(Mutex::new(rx));
        cx.spawn(async move |this, cx| {
            'pump: loop {
                let first = cx
                    .background_executor()
                    .spawn({
                        let rx = Arc::clone(&rx);
                        async move { rx.lock().ok().and_then(|rx| rx.recv().ok()) }
                    })
                    .await;
                if first.is_none() {
                    break 'pump; // channel closed — watcher dropped
                }
                loop {
                    let next = cx
                        .background_executor()
                        .spawn({
                            let rx = Arc::clone(&rx);
                            async move { rx.lock().ok().map(|rx| rx.recv_timeout(DEBOUNCE)) }
                        })
                        .await;
                    match next {
                        // Burst continues — restart the quiet window.
                        Some(Ok(())) => continue,
                        // Quiet — fire the rescan.
                        Some(Err(RecvTimeoutError::Timeout)) => break,
                        _ => break 'pump,
                    }
                }
                loop {
                    match this.update(cx, |view, cx| view.try_begin_watched_rescan(cx)) {
                        Ok(true) => break,
                        Ok(false) => cx.background_executor().timer(BUSY_RETRY).await,
                        Err(_) => break 'pump, // view released
                    }
                }
            }
        })
    }
}
