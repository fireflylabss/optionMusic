//! Paths, defaults, and persistent settings for optMusic.
//!
//! User config: `~/option/music/config.toml`

use std::fmt;
use std::fs;
use std::path::{Component, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// `~/option/music`
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("option")
        .join("music")
}

/// `~/option/music/config.toml`
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Default local library: `~/Music`
pub fn default_music_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Music")
}

/// Resolve a music-dir flag. Empty → `~/Music`. Rejects `..` and non-dirs.
pub fn resolve_music_dir(dir: &str) -> Result<PathBuf> {
    if dir.is_empty() {
        return Ok(default_music_dir());
    }

    let path = expand_tilde(dir);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("music directory must not contain '..' components");
    }

    if path.exists() {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        if !canonical.is_dir() {
            bail!("music directory is not a folder: {}", canonical.display());
        }
        return Ok(canonical);
    }

    bail!("music directory does not exist: {}", path.display());
}

fn expand_tilde(dir: &str) -> PathBuf {
    if let Some(rest) = dir.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }
    if dir == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    PathBuf::from(dir)
}

// ── Persistent settings ──────────────────────────────────────────

pub const VOLUME_MAX_NORMAL: u8 = 100;
pub const VOLUME_MAX_EXCESS: u8 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CavaStyle {
    Bars,
    Dense,
    Mirror,
    Dots,
}

impl CavaStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bars => "bars",
            Self::Dense => "dense",
            Self::Mirror => "mirror",
            Self::Dots => "dots",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Bars => Self::Dense,
            Self::Dense => Self::Mirror,
            Self::Mirror => Self::Dots,
            Self::Dots => Self::Bars,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Bars => Self::Dots,
            Self::Dense => Self::Bars,
            Self::Mirror => Self::Dense,
            Self::Dots => Self::Mirror,
        }
    }
}

impl Default for CavaStyle {
    fn default() -> Self {
        Self::Bars
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CavaConfig {
    pub style: CavaStyle,
    /// Vertical bar rows (clamped 3..=7).
    pub rows: u8,
}

impl Default for CavaConfig {
    fn default() -> Self {
        Self {
            style: CavaStyle::Bars,
            rows: 5,
        }
    }
}

impl CavaConfig {
    pub fn normalize(&mut self) {
        self.rows = match self.rows {
            0..=3 => 3,
            4..=5 => 5,
            _ => 7,
        };
    }

    pub fn cycle_rows_up(&mut self) {
        self.rows = match self.rows {
            3 => 5,
            5 => 7,
            _ => 3,
        };
    }

    pub fn cycle_rows_down(&mut self) {
        self.rows = match self.rows {
            7 => 5,
            5 => 3,
            _ => 7,
        };
    }

    pub fn reset_defaults(&mut self) {
        *self = Self::default();
    }
}

/// Accent color: greyscale default, named presets, or `#RRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accent {
    Default,
    Cyan,
    Green,
    Amber,
    Rose,
    Blue,
    Violet,
    Custom { r: u8, g: u8, b: u8 },
}

impl Default for Accent {
    fn default() -> Self {
        Self::Default
    }
}

impl Accent {
    pub fn label(&self) -> String {
        match self {
            Self::Default => "default".into(),
            Self::Cyan => "cyan".into(),
            Self::Green => "green".into(),
            Self::Amber => "amber".into(),
            Self::Rose => "rose".into(),
            Self::Blue => "blue".into(),
            Self::Violet => "violet".into(),
            Self::Custom { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }

    pub fn rgb(&self) -> Option<(u8, u8, u8)> {
        match self {
            Self::Default => None,
            Self::Cyan => Some((120, 210, 230)),
            Self::Green => Some((140, 220, 150)),
            Self::Amber => Some((230, 180, 90)),
            Self::Rose => Some((230, 140, 160)),
            Self::Blue => Some((130, 170, 240)),
            Self::Violet => Some((190, 150, 230)),
            Self::Custom { r, g, b } => Some((*r, *g, *b)),
        }
    }

    pub fn next_preset(&self) -> Self {
        match self {
            Self::Default => Self::Cyan,
            Self::Cyan => Self::Green,
            Self::Green => Self::Amber,
            Self::Amber => Self::Rose,
            Self::Rose => Self::Blue,
            Self::Blue => Self::Violet,
            Self::Violet | Self::Custom { .. } => Self::Default,
        }
    }

    pub fn prev_preset(&self) -> Self {
        match self {
            Self::Default | Self::Custom { .. } => Self::Violet,
            Self::Cyan => Self::Default,
            Self::Green => Self::Cyan,
            Self::Amber => Self::Green,
            Self::Rose => Self::Amber,
            Self::Blue => Self::Rose,
            Self::Violet => Self::Blue,
        }
    }

    fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("default") || s.eq_ignore_ascii_case("white") {
            return Ok(Self::Default);
        }
        if let Some(hex) = s.strip_prefix('#') {
            return parse_hex(hex).map(|(r, g, b)| Self::Custom { r, g, b });
        }
        match s.to_ascii_lowercase().as_str() {
            "cyan" => Ok(Self::Cyan),
            "green" => Ok(Self::Green),
            "amber" => Ok(Self::Amber),
            "rose" => Ok(Self::Rose),
            "blue" => Ok(Self::Blue),
            "violet" | "purple" => Ok(Self::Violet),
            other => Err(format!("unknown accent color: {other}")),
        }
    }
}

impl fmt::Display for Accent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl Serialize for Accent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.label())
    }
}

impl<'de> Deserialize<'de> for Accent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_hex(hex: &str) -> Result<(u8, u8, u8), String> {
    let hex = hex.trim();
    if hex.len() != 6 {
        return Err("accent hex must be #RRGGBB".into());
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "bad accent hex")?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "bad accent hex")?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "bad accent hex")?;
    Ok((r, g, b))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Allow volume up to 200% (MPV soft gain).
    pub excess_volume: bool,
    /// Low Detail Mode — fewer animations, lighter redraw.
    pub ldm: bool,
    pub accent: Accent,
    pub cava: CavaConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            excess_volume: false,
            ldm: false,
            accent: Accent::Default,
            cava: CavaConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(raw) => match toml::from_str::<AppConfig>(&raw) {
                Ok(mut cfg) => {
                    cfg.cava.normalize();
                    cfg
                }
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create config dir {}", dir.display()))?;
        let path = config_path();
        let body = toml::to_string_pretty(self).context("serialize config")?;
        let header = "# optMusic settings — edit carefully or use `c` in the player\n\
# path: ~/option/music/config.toml\n\n";
        fs::write(&path, format!("{header}{body}"))
            .with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }

    pub fn volume_max(&self) -> u8 {
        if self.excess_volume {
            VOLUME_MAX_EXCESS
        } else {
            VOLUME_MAX_NORMAL
        }
    }

    pub fn reset_all(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_with_option_music() {
        let dir = config_dir();
        assert!(dir.ends_with("music"));
        assert!(dir.to_string_lossy().contains("option"));
    }

    #[test]
    fn default_music_dir_ends_with_music() {
        assert!(default_music_dir().ends_with("Music"));
    }

    #[test]
    fn resolve_empty_returns_default() {
        assert_eq!(resolve_music_dir("").unwrap(), default_music_dir());
    }

    #[test]
    fn resolve_rejects_parent_components() {
        assert!(resolve_music_dir("../Music").is_err());
    }

    #[test]
    fn resolve_rejects_missing() {
        assert!(resolve_music_dir("/tmp/optmusic_no_such_dir_xyz").is_err());
    }

    #[test]
    fn resolve_accepts_existing_dir() {
        assert!(resolve_music_dir("src").is_ok());
    }

    #[test]
    fn expand_tilde_home() {
        let p = expand_tilde("~/Music");
        assert!(p.ends_with("Music"));
        assert!(!p.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn accent_roundtrip_hex() {
        let a = Accent::parse("#7eb8ff").unwrap();
        assert_eq!(a, Accent::Custom { r: 0x7e, g: 0xb8, b: 0xff });
        assert_eq!(a.label(), "#7eb8ff");
    }

    #[test]
    fn cava_rows_normalize() {
        let mut c = CavaConfig { style: CavaStyle::Bars, rows: 9 };
        c.normalize();
        assert_eq!(c.rows, 7);
    }

    #[test]
    fn toml_roundtrip_defaults() {
        let cfg = AppConfig::default();
        let s = toml::to_string(&cfg).unwrap();
        let back: AppConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, back);
    }
}
