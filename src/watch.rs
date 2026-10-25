//! Filesystem watcher for library folders (`notify` crate).
//!
//! `spawn_watcher` observes the configured roots and calls `on_change` (debounced)
//! whenever audio files are created, removed, renamed or modified. Callers
//! typically run `Library::refresh_roots` inside the callback. Watching is
//! best-effort: missing folders are skipped, a dead watcher just stops firing.

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Audio extensions that count as library changes.
fn is_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "ogg" | "oga" | "opus" | "wav" | "m4a" | "aac" | "wma" | "aiff" | "aif"
    )
}

/// True when a notify event should trigger a library refresh.
fn event_matters(kind: &EventKind, paths: &[PathBuf]) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
            paths.iter().any(|p| is_audio(p))
        }
        _ => false,
    }
}

/// Watch `roots` in a background thread; invoke `on_change` (debounced 1s)
/// on every relevant burst. Returns the thread handle; dropping is not
/// supported — the watcher lives until process exit.
pub fn spawn_watcher<F>(roots: Vec<PathBuf>, mut on_change: F) -> Result<thread::JoinHandle<()>>
where
    F: FnMut() + Send + 'static,
{
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())?;
    for root in &roots {
        if root.is_dir() {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
    }
    std::mem::forget(watcher);
    Ok(thread::spawn(move || {
        let mut pending: Option<Instant> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(event)) => {
                    if event_matters(&event.kind, &event.paths) {
                        pending = Some(Instant::now());
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if let Some(since) = pending
                && since.elapsed() >= Duration::from_secs(1)
            {
                pending = None;
                on_change();
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind};

    #[test]
    fn audio_extensions_recognized() {
        assert!(is_audio(Path::new("/m/a.mp3")));
        assert!(is_audio(Path::new("/m/b.FLAC")));
        assert!(is_audio(Path::new("/m/c.ogg")));
        assert!(!is_audio(Path::new("/m/notes.txt")));
        assert!(!is_audio(Path::new("/m/noext")));
    }

    #[test]
    fn only_audio_events_matter() {
        let audio = vec![PathBuf::from("/m/a.mp3")];
        let text = vec![PathBuf::from("/m/notes.txt")];
        assert!(event_matters(&EventKind::Create(CreateKind::File), &audio));
        assert!(event_matters(
            &EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            &audio
        ));
        assert!(!event_matters(&EventKind::Create(CreateKind::File), &text));
        assert!(!event_matters(
            &EventKind::Access(notify::event::AccessKind::Any),
            &audio
        ));
    }
}
