//! Audio playback engine for optMusic (libmpv / MPV).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use libmpv2::Mpv;
use libmpv2::events::Event as MpvEvent;
use libmpv2::mpv_end_file_reason;

use crate::config::VOLUME_MAX_NORMAL;
use crate::eq::EqPreset;
use crate::mpv::{MpvConfig, create_player};

const SEEK_SHORT_SECS: i64 = 5;
const SEEK_LONG_SECS: i64 = 60;
const VOLUME_STEP: u8 = 5;
const SPEED_STEP: f64 = 0.1;
const PITCH_STEP: f64 = 0.05;

/// Thin wrapper around an MPV instance for sequential file playback.
pub struct Player {
    mpv: Mpv,
    volume: u8,
    /// Soft ceiling — 100 normally, 200 when excess volume is enabled.
    volume_max: u8,
    muted: bool,
    speed: f64,
    pitch: f64,
    eq: EqPreset,
    /// Soft stop (no active file / held).
    stopped: bool,
    /// Track finished naturally (eof) — cleared on next load.
    finished: bool,
    /// MPV loop-file (repeat current track).
    loop_track: bool,
}

impl Player {
    pub fn new(volume: u8, speed: f64, crossfade: f64) -> Result<Self> {
        let volume = volume.min(VOLUME_MAX_NORMAL);
        let config = MpvConfig::for_cli(volume as f64, speed, false, crossfade);
        let mpv = create_player(&config)?;
        let _ = mpv.set_property("pitch", 1.0);

        Ok(Self {
            mpv,
            volume,
            volume_max: VOLUME_MAX_NORMAL,
            muted: false,
            speed: config.speed,
            pitch: 1.0,
            eq: EqPreset::Off,
            stopped: true,
            finished: false,
            loop_track: false,
        })
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn pitch(&self) -> f64 {
        self.pitch
    }

    pub fn eq_label(&self) -> &'static str {
        self.eq.label()
    }

    /// Update the soft volume ceiling (100 or 200). Clamps current volume if needed.
    pub fn set_volume_max(&mut self, max: u8) {
        self.volume_max = max.clamp(VOLUME_MAX_NORMAL, crate::config::VOLUME_MAX_EXCESS);
        if self.volume > self.volume_max {
            self.set_volume(self.volume_max);
        }
    }

    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(self.volume_max);
        let _ = self.mpv.set_property("volume", self.volume as f64);
    }

    pub fn volume_up(&mut self, step: u8) -> u8 {
        let v = (self.volume as u16 + step as u16).min(self.volume_max as u16) as u8;
        self.set_volume(v);
        self.volume
    }

    pub fn volume_down(&mut self, step: u8) -> u8 {
        let v = self.volume.saturating_sub(step);
        self.set_volume(v);
        self.volume
    }

    pub fn volume_step_up(&mut self) -> u8 {
        self.volume_up(VOLUME_STEP)
    }

    pub fn volume_step_down(&mut self) -> u8 {
        self.volume_down(VOLUME_STEP)
    }

    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        let _ = self.mpv.set_property("mute", self.muted);
        self.muted
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.1, 10.0);
        let _ = self.mpv.set_property("speed", self.speed);
    }

    pub fn speed_up(&mut self) -> f64 {
        self.set_speed(self.speed + SPEED_STEP);
        self.speed
    }

    pub fn speed_down(&mut self) -> f64 {
        self.set_speed(self.speed - SPEED_STEP);
        self.speed
    }

    pub fn set_pitch(&mut self, pitch: f64) {
        self.pitch = pitch.clamp(0.5, 2.0);
        let _ = self.mpv.set_property("pitch", self.pitch);
    }

    pub fn pitch_up(&mut self) -> f64 {
        self.set_pitch(self.pitch + PITCH_STEP);
        self.pitch
    }

    pub fn pitch_down(&mut self) -> f64 {
        self.set_pitch(self.pitch - PITCH_STEP);
        self.pitch
    }

    pub fn reset_speed_pitch(&mut self) {
        self.set_speed(1.0);
        self.set_pitch(1.0);
    }

    pub fn cycle_eq(&mut self) -> EqPreset {
        self.eq = self.eq.next();
        let _ = self.mpv.set_property("af", self.eq.af_filter());
        self.eq
    }

    pub fn set_eq(&mut self, eq: EqPreset) {
        self.eq = eq;
        let _ = self.mpv.set_property("af", self.eq.af_filter());
    }

    /// Repeat the current file (`loop-file=inf`).
    pub fn set_loop_track(&mut self, on: bool) {
        self.loop_track = on;
        let _ = self
            .mpv
            .set_property("loop-file", if on { "inf" } else { "no" });
    }

    pub fn loop_track(&self) -> bool {
        self.loop_track
    }

    /// Stop current playback and start playing `path`.
    pub fn play_file(&mut self, path: &Path) -> Result<()> {
        let path_str = path
            .to_str()
            .with_context(|| format!("non-UTF8 path: {}", path.display()))?;
        self.play_uri(path_str)
            .with_context(|| format!("cannot load {}", path.display()))
    }

    /// Load a local file paused at `position` seconds (session resume).
    pub fn play_file_paused_at(&mut self, path: &Path, position: f64) -> Result<()> {
        let path_str = path
            .to_str()
            .with_context(|| format!("non-UTF8 path: {}", path.display()))?;
        self.play_uri_at(path_str, true, position.max(0.0))
            .with_context(|| format!("cannot resume {}", path.display()))
    }

    /// Play a local path or remote URL (YouTube / SoundCloud via mpv+yt-dlp).
    pub fn play_uri(&mut self, uri: &str) -> Result<()> {
        self.play_uri_at(uri, false, 0.0)
    }

    fn play_uri_at(&mut self, uri: &str, paused: bool, position: f64) -> Result<()> {
        // GTK may flip LC_NUMERIC after init; keep C for libmpv property I/O.
        crate::mpv::ensure_c_numeric_locale();
        self.drain_events();
        self.mpv
            .command("loadfile", &[uri, "replace"])
            .map_err(|e| anyhow::anyhow!("cannot load {uri}: {e:?}"))?;
        // Drop EndFile(Stop) from the replace so next/prev aren't treated as eof.
        self.drain_events();

        let _ = self.mpv.set_property("volume", self.volume as f64);
        let _ = self.mpv.set_property("mute", self.muted);
        let _ = self.mpv.set_property("speed", self.speed);
        let _ = self.mpv.set_property("pitch", self.pitch);
        let _ = self.mpv.set_property("af", self.eq.af_filter());
        let _ = self
            .mpv
            .set_property("loop-file", if self.loop_track { "inf" } else { "no" });
        let _ = self.mpv.set_property("pause", paused);
        if position > 0.05 {
            let pos = format!("{position:.3}");
            let _ = self.mpv.command("seek", &[&pos, "absolute"]);
        }

        self.stopped = false;
        self.finished = false;
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) {
        if self.stopped {
            return;
        }
        let _ = self.mpv.set_property("pause", paused);
    }

    pub fn toggle_pause(&mut self) -> bool {
        if self.stopped {
            return true;
        }
        let paused: bool = self.mpv.get_property("pause").unwrap_or(false);
        let next = !paused;
        let _ = self.mpv.set_property("pause", next);
        next
    }

    pub fn is_paused(&self) -> bool {
        if self.stopped {
            return true;
        }
        self.mpv.get_property("pause").unwrap_or(false)
    }

    pub fn stop(&mut self) {
        let _ = self.mpv.command("stop", &[]);
        self.stopped = true;
        self.finished = false;
    }

    /// True when nothing is playing (stopped or track ended).
    pub fn is_idle(&mut self) -> bool {
        self.poll_events();
        if self.stopped || self.finished {
            return true;
        }
        // Property fallback if EndFile was missed (e.g. wrapped as Err).
        let eof: bool = self.mpv.get_property("eof-reached").unwrap_or(false);
        if eof {
            self.finished = true;
            return true;
        }
        false
    }

    pub fn position(&self) -> Duration {
        if self.stopped {
            return Duration::ZERO;
        }
        let secs: f64 = self.mpv.get_property("time-pos").unwrap_or(0.0);
        Duration::from_secs_f64(secs.max(0.0))
    }

    pub fn duration(&self) -> Option<Duration> {
        if self.stopped {
            return None;
        }
        let secs: f64 = self.mpv.get_property("duration").unwrap_or(0.0);
        if secs > 0.0 {
            Some(Duration::from_secs_f64(secs))
        } else {
            None
        }
    }

    pub fn seek_relative_secs(&mut self, delta: i64) {
        if self.stopped {
            return;
        }
        let arg = delta.to_string();
        let _ = self.mpv.command("seek", &[&arg, "relative"]);
    }

    pub fn seek_short_forward(&mut self) {
        self.seek_relative_secs(SEEK_SHORT_SECS);
    }

    pub fn seek_short_back(&mut self) {
        self.seek_relative_secs(-SEEK_SHORT_SECS);
    }

    pub fn seek_long_forward(&mut self) {
        self.seek_relative_secs(SEEK_LONG_SECS);
    }

    pub fn seek_long_back(&mut self) {
        self.seek_relative_secs(-SEEK_LONG_SECS);
    }

    pub fn seek(&mut self, pos: Duration) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        let secs = pos.as_secs_f64().to_string();
        self.mpv
            .command("seek", &[&secs, "absolute"])
            .map_err(|e| anyhow::anyhow!("seek failed: {e:?}"))?;
        Ok(())
    }

    /// Seek to a fraction of the current track duration (0.0 ..= 1.0).
    pub fn seek_ratio(&mut self, ratio: f64) -> Result<()> {
        let Some(dur) = self.duration() else {
            return Ok(());
        };
        let pos = Duration::from_secs_f64(dur.as_secs_f64() * ratio.clamp(0.0, 1.0));
        self.seek(pos)
    }

    fn poll_events(&mut self) {
        while let Some(ev) = self.mpv.wait_event(0.0) {
            match ev {
                // Only natural EOF advances the playlist. `Stop` fires on
                // loadfile-replace / stop and must not look like track end
                // (that was breaking n/p and ◂/▸).
                Ok(MpvEvent::EndFile(reason)) => {
                    if !self.stopped && reason == mpv_end_file_reason::Eof {
                        self.finished = true;
                    }
                }
                Ok(MpvEvent::Shutdown) => {
                    self.stopped = true;
                    self.finished = true;
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    fn drain_events(&mut self) {
        while self.mpv.wait_event(0.0).is_some() {}
    }
}

/// Probe duration of a file via a short-lived MPV instance (ao=null).
pub fn probe_duration(path: &Path) -> Option<Duration> {
    let path_str = path.to_str()?;
    let mut mpv = Mpv::new().ok()?;
    let _ = mpv.set_property("video", "no");
    let _ = mpv.set_property("ao", "null");
    let _ = mpv.set_property("terminal", "no");
    let _ = mpv.set_property("pause", true);
    let _ = mpv.set_property("idle", "yes");
    mpv.command("loadfile", &[path_str, "replace"]).ok()?;

    for _ in 0..80 {
        match mpv.wait_event(0.05) {
            Some(Ok(MpvEvent::FileLoaded)) | Some(Ok(MpvEvent::PlaybackRestart)) => {}
            Some(Ok(MpvEvent::EndFile(_))) | Some(Ok(MpvEvent::Shutdown)) => break,
            _ => {}
        }
        let d: f64 = mpv.get_property("duration").unwrap_or(0.0);
        if d > 0.0 {
            return Some(Duration::from_secs_f64(d));
        }
    }
    let d: f64 = mpv.get_property("duration").unwrap_or(0.0);
    if d > 0.0 {
        Some(Duration::from_secs_f64(d))
    } else {
        None
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.mpv.command("stop", &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_constants_are_sensible() {
        assert_eq!(SEEK_SHORT_SECS, 5);
        assert_eq!(SEEK_LONG_SECS, 60);
        assert!(SEEK_LONG_SECS > SEEK_SHORT_SECS);
    }
}
