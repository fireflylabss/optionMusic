//! Equalizer presets for MPV (`af` filters).

/// Built-in EQ cycle (firemusic-style simple presets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqPreset {
    Off,
    Bass,
    Treble,
    Rock,
    Vocal,
    Lofi,
}

impl EqPreset {
    pub const ALL: [EqPreset; 6] = [
        EqPreset::Off,
        EqPreset::Bass,
        EqPreset::Treble,
        EqPreset::Rock,
        EqPreset::Vocal,
        EqPreset::Lofi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EqPreset::Off => "off",
            EqPreset::Bass => "bass+",
            EqPreset::Treble => "treble+",
            EqPreset::Rock => "rock",
            EqPreset::Vocal => "vocal",
            EqPreset::Lofi => "lofi",
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
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
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
}
