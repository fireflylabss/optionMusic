//! Thin MPV helpers (libmpv2).

use anyhow::Result;
use libmpv2::Mpv;

#[derive(Debug, Clone)]
pub struct MpvConfig {
    pub volume: f64,
    pub speed: f64,
    pub loop_file: bool,
    pub crossfade: f64,
}

impl Default for MpvConfig {
    fn default() -> Self {
        Self {
            volume: 80.0,
            speed: 1.0,
            loop_file: false,
            crossfade: 0.0,
        }
    }
}

impl MpvConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn volume(mut self, volume: f64) -> Self {
        self.volume = volume.clamp(0.0, 100.0);
        self
    }

    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed.clamp(0.1, 10.0);
        self
    }

    pub fn loop_file(mut self, loop_file: bool) -> Self {
        self.loop_file = loop_file;
        self
    }

    pub fn crossfade(mut self, crossfade: f64) -> Self {
        self.crossfade = crossfade.max(0.0);
        self
    }

    pub fn for_cli(volume: f64, speed: f64, loop_file: bool, crossfade: f64) -> Self {
        Self::new()
            .volume(volume)
            .speed(speed)
            .loop_file(loop_file)
            .crossfade(crossfade)
    }
}

/// Create an audio-only MPV instance.
pub fn create_player(config: &MpvConfig) -> Result<Mpv> {
    let mpv = Mpv::new().map_err(|e| {
        anyhow::anyhow!("failed to initialize libmpv (is libmpv installed?): {e:?}")
    })?;

    let _ = mpv.set_property("video", "no");
    let _ = mpv.set_property("terminal", "no");
    let _ = mpv.set_property("input-default-bindings", "no");
    let _ = mpv.set_property("input-vo-keyboard", "no");
    let _ = mpv.set_property("osc", "no");
    let _ = mpv.set_property("volume", config.volume);
    let _ = mpv.set_property("speed", config.speed);
    // Do not keep-open: we advance the playlist ourselves on EndFile / eof.

    if config.loop_file {
        let _ = mpv.set_property("loop-file", "inf");
    }

    if config.crossfade > 0.0 {
        let _ = mpv.set_property("audio-fade", config.crossfade);
    }

    Ok(mpv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_clamps_volume() {
        assert_eq!(MpvConfig::new().volume(150.0).volume, 100.0);
        assert_eq!(MpvConfig::new().volume(-10.0).volume, 0.0);
    }

    #[test]
    fn config_clamps_speed() {
        assert_eq!(MpvConfig::new().speed(20.0).speed, 10.0);
        assert_eq!(MpvConfig::new().speed(0.01).speed, 0.1);
    }

    #[test]
    fn for_cli_preserves_values() {
        let cfg = MpvConfig::for_cli(50.0, 1.5, true, 2.0);
        assert_eq!(cfg.volume, 50.0);
        assert_eq!(cfg.speed, 1.5);
        assert!(cfg.loop_file);
        assert_eq!(cfg.crossfade, 2.0);
    }

    #[test]
    fn crossfade_non_negative() {
        assert_eq!(MpvConfig::new().crossfade(-1.0).crossfade, 0.0);
    }
}
