//! Equalizer presets for MPV (`af` filters).

use serde::{Deserialize, Serialize};

/// Built-in EQ cycle. Existing names stay valid in saved configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EqPreset {
    #[default]
    Off,
    Bass,
    Treble,
    Rock,
    Vocal,
    Lofi,
    Acoustic,
    Jazz,
    Classical,
    Electronic,
    Pop,
    Speech,
    Warm,
}

impl EqPreset {
    pub const ALL: [EqPreset; 13] = [
        EqPreset::Off,
        EqPreset::Bass,
        EqPreset::Treble,
        EqPreset::Rock,
        EqPreset::Vocal,
        EqPreset::Lofi,
        EqPreset::Acoustic,
        EqPreset::Jazz,
        EqPreset::Classical,
        EqPreset::Electronic,
        EqPreset::Pop,
        EqPreset::Speech,
        EqPreset::Warm,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EqPreset::Off => "off",
            EqPreset::Bass => "bass+",
            EqPreset::Treble => "treble+",
            EqPreset::Rock => "rock",
            EqPreset::Vocal => "vocal",
            EqPreset::Lofi => "lofi",
            EqPreset::Acoustic => "acoustic",
            EqPreset::Jazz => "jazz",
            EqPreset::Classical => "classical",
            EqPreset::Electronic => "electronic",
            EqPreset::Pop => "pop",
            EqPreset::Speech => "speech",
            EqPreset::Warm => "warm",
        }
    }

    /// MPV `af` filter string (empty = flat / off).
    pub fn af_filter(self) -> &'static str {
        match self {
            EqPreset::Off => "",
            EqPreset::Bass => "bass=g=10",
            EqPreset::Treble => "treble=g=10",
            EqPreset::Rock => "bass=g=10,treble=g=10",
            EqPreset::Vocal => "equalizer=f=1000:width_type=h:width=200:g=10",
            EqPreset::Lofi => {
                "equalizer=f=300:width_type=h:width=200:g=-10,equalizer=f=3000:width_type=h:width=200:g=-10"
            }
            // These curves shape the tone with cuts, avoiding added boost on mastered audio.
            EqPreset::Acoustic => "bass=f=120:g=-3,equalizer=f=400:width_type=q:width=1:g=-2",
            EqPreset::Jazz => "equalizer=f=350:width_type=q:width=1:g=-2,treble=f=6000:g=-2",
            EqPreset::Classical => "bass=f=100:g=-1,equalizer=f=2500:width_type=q:width=1:g=-1.5",
            EqPreset::Electronic => "equalizer=f=1000:width_type=q:width=0.7:g=-4",
            EqPreset::Pop => "equalizer=f=300:width_type=q:width=1:g=-3,treble=f=8000:g=-1",
            EqPreset::Speech => "bass=f=180:g=-6,treble=f=8000:g=-3",
            EqPreset::Warm => "treble=f=4000:g=-4",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    #[allow(dead_code)]
    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_to_off() {
        let mut p = EqPreset::Off;
        for _ in 0..EqPreset::ALL.len() {
            p = p.next();
        }
        assert_eq!(p, EqPreset::Off);
    }

    #[test]
    fn labels_are_unique() {
        let mut labels: Vec<_> = EqPreset::ALL.iter().map(|p| p.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), EqPreset::ALL.len());
    }

    #[test]
    fn off_has_empty_filter() {
        assert!(EqPreset::Off.af_filter().is_empty());
        assert!(!EqPreset::Bass.af_filter().is_empty());
    }

    #[test]
    fn from_index_wraps() {
        assert_eq!(EqPreset::from_index(0), EqPreset::Off);
        assert_eq!(EqPreset::from_index(EqPreset::ALL.len()), EqPreset::Off);
    }

    #[test]
    fn cycle_visits_every_preset_and_reverses() {
        let mut preset = EqPreset::Off;
        for expected in EqPreset::ALL {
            assert_eq!(preset, expected);
            assert_eq!(preset.next().prev(), preset);
            preset = preset.next();
        }
        assert_eq!(preset, EqPreset::Off);
        assert_eq!(EqPreset::Off.prev(), EqPreset::Warm);
    }

    #[test]
    fn saved_configuration_accepts_every_preset_and_legacy_names() {
        for preset in EqPreset::ALL {
            let json = serde_json::to_string(&preset).unwrap();
            assert_eq!(serde_json::from_str::<EqPreset>(&json).unwrap(), preset);
            let raw = format!("eq = {json}");
            let config: crate::config::AppConfig = toml::from_str(&raw).unwrap();
            assert_eq!(config.eq, preset);
        }
        for (name, preset) in [
            ("off", EqPreset::Off),
            ("bass", EqPreset::Bass),
            ("treble", EqPreset::Treble),
            ("rock", EqPreset::Rock),
            ("vocal", EqPreset::Vocal),
            ("lofi", EqPreset::Lofi),
        ] {
            let config: crate::config::AppConfig =
                toml::from_str(&format!("eq = \"{name}\"")).unwrap();
            assert_eq!(config.eq, preset);
        }
    }
}
