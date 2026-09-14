//! Machine-readable events for non-TTY playback (`--json`).
//!
//! One JSON object per line on stdout so `msc play --json | jq` works while a
//! track is still playing.

use std::time::Duration;

use serde::Serialize;

use crate::playlist::Track;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Queue accepted, before the first track loads.
    Start { tracks: usize },
    /// A track started playing (1-based `index`).
    Track {
        index: usize,
        total: usize,
        path: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        artist: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        album: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration: Option<f64>,
    },
    /// A track stopped, either finished or cut short by a signal.
    TrackEnd {
        index: usize,
        played: f64,
        completed: bool,
    },
    /// The queue ended on its own.
    End { tracks_played: usize },
    /// SIGINT/SIGTERM arrived; playback was stopped cleanly.
    Interrupted { tracks_played: usize },
}

impl Event {
    pub fn track(index: usize, total: usize, track: &Track, duration: Option<Duration>) -> Self {
        Self::Track {
            index,
            total,
            path: track.path.to_string_lossy().into_owned(),
            title: track.display_name(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration: duration.map(|d| d.as_secs_f64()),
        }
    }

    /// One NDJSON line, without the trailing newline.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{\"event\":\"error\"}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn track() -> Track {
        let mut t = Track::from_path(PathBuf::from("/music/a.mp3"));
        t.title = Some("Airbag".into());
        t.artist = Some("Radiohead".into());
        t.album = None;
        t
    }

    #[test]
    fn track_line_is_tagged_and_skips_absent_metadata() {
        let line = Event::track(1, 3, &track(), Some(Duration::from_secs_f64(4.5))).to_line();
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["event"], "track");
        assert_eq!(v["index"], 1);
        assert_eq!(v["total"], 3);
        assert_eq!(v["path"], "/music/a.mp3");
        assert_eq!(v["artist"], "Radiohead");
        assert_eq!(v["duration"], 4.5);
        assert!(v.get("album").is_none(), "absent album must be omitted");
    }

    #[test]
    fn lifecycle_events_are_single_line() {
        for event in [
            Event::Start { tracks: 2 },
            Event::TrackEnd {
                index: 1,
                played: 12.0,
                completed: false,
            },
            Event::End { tracks_played: 2 },
            Event::Interrupted { tracks_played: 1 },
        ] {
            let line = event.to_line();
            assert!(!line.contains('\n'), "{line} must stay on one line");
            assert!(line.starts_with("{\"event\":\""), "{line} must be tagged");
        }
    }
}
