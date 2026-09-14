//! Paths, defaults, and persistent settings for optionMusic.
//!
//! User config: `~/.option/music/config.toml`

use std::fmt;
use std::fs;
use std::path::{Component, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// `~/.option/music` (migrates from legacy `~/option/music`)
pub fn config_dir() -> PathBuf {
    let _ = option_sdk::App::MUSIC.ensure();
    option_sdk::App::MUSIC.dir()
}

/// `~/.option/music/config.toml`
pub fn config_path() -> PathBuf {
    let _ = option_sdk::App::MUSIC.ensure();
    option_sdk::App::MUSIC.config_toml()
}

/// Raw `desktop_preferences` string from config.toml, without a controller.
/// The desktop app stores its UI state blob (JSON) there.
pub fn desktop_preferences_raw() -> Option<String> {
    let raw = fs::read_to_string(config_path()).ok()?;
    let doc = raw.parse::<toml::Value>().ok()?;
    doc.get("desktop_preferences")?.as_str().map(String::from)
}

/// `~/.option/music/cache` (ensures the whole `~/.option/music` + `cache/` tree)
pub fn cache_dir() -> PathBuf {
    let _ = option_sdk::App::MUSIC.ensure_cache();
    option_sdk::App::MUSIC.cache_dir()
}

/// `~/.option/music/cache/tags`
pub fn tags_cache_dir() -> PathBuf {
    let _ = cache_dir();
    option_sdk::App::MUSIC.path("cache/tags")
}

/// `~/.option/music/cache/covers`
pub fn covers_cache_dir() -> PathBuf {
    let _ = cache_dir();
    option_sdk::App::MUSIC.path("cache/covers")
}

/// Stable FNV-1a 64-bit digest for on-disk cache filenames.
pub fn stable_cache_key(parts: &[&[u8]]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in parts {
        for b in *part {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

/// Default local library: `~/Music`
pub fn default_music_dir() -> PathBuf {
    option_sdk::home_dir().join("Music")
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
    option_sdk::expand_tilde(dir)
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

/// Interactive download wizard UI (`msc dl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DlUiMode {
    /// Arrow keys, checkboxes, cycling options (default).
    #[serde(alias = "arrow")]
    Arrows,
    /// Classic typed prompts.
    #[serde(alias = "typing", alias = "prompt")]
    Type,
}

impl Default for DlUiMode {
    fn default() -> Self {
        Self::Arrows
    }
}

/// How the Artists browser groups tracks (shared CLI + desktop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtistSource {
    /// Group by embedded artist / album-artist tags (default).
    #[serde(alias = "tag", alias = "tags", alias = "meta")]
    Metadata,
    /// Group by parent folder name.
    #[serde(alias = "dir", alias = "directory")]
    Folder,
}

impl Default for ArtistSource {
    fn default() -> Self {
        Self::Metadata
    }
}

impl ArtistSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Folder => "folder",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Metadata => Self::Folder,
            Self::Folder => Self::Metadata,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }
}

impl DlUiMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Arrows => "arrows",
            Self::Type => "type",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Arrows => Self::Type,
            Self::Type => Self::Arrows,
        }
    }

    pub fn prev(self) -> Self {
        self.next()
    }
}

/// Toast anchor (`toast_pos` in config.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ToastPos {
    /// Bottom-center (default).
    #[default]
    #[serde(
        alias = "bottom_center",
        alias = "bottomcenter",
        alias = "bottom",
        alias = "center"
    )]
    BottomCenter,
    #[serde(alias = "bottom_left", alias = "bottomleft", alias = "left")]
    BottomLeft,
    #[serde(alias = "bottom_right", alias = "bottomright", alias = "right")]
    BottomRight,
    #[serde(alias = "top_center", alias = "topcenter", alias = "top")]
    TopCenter,
    #[serde(alias = "top_left", alias = "topleft")]
    TopLeft,
    #[serde(alias = "top_right", alias = "topright")]
    TopRight,
}

impl ToastPos {
    pub fn label(self) -> &'static str {
        match self {
            Self::BottomCenter => "bottom-center",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::TopCenter => "top-center",
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::BottomCenter => Self::BottomLeft,
            Self::BottomLeft => Self::BottomRight,
            Self::BottomRight => Self::TopCenter,
            Self::TopCenter => Self::TopLeft,
            Self::TopLeft => Self::TopRight,
            Self::TopRight => Self::BottomCenter,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::BottomCenter => Self::TopRight,
            Self::BottomLeft => Self::BottomCenter,
            Self::BottomRight => Self::BottomLeft,
            Self::TopCenter => Self::BottomRight,
            Self::TopLeft => Self::TopCenter,
            Self::TopRight => Self::TopLeft,
        }
    }
}

/// yt-dlp 403/PO-token fallback (`youtube:player_client=mweb` retry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DlFallbackMode {
    /// Ask on TTY (`[s/N]`), never auto-retry off-TTY.
    #[default]
    Ask,
    /// Retry automatically without asking.
    Auto,
    /// Never retry with the mweb fallback.
    Off,
}

impl DlFallbackMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Off => "off",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Ask => Self::Auto,
            Self::Auto => Self::Off,
            Self::Off => Self::Ask,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Ask => Self::Off,
            Self::Auto => Self::Ask,
            Self::Off => Self::Auto,
        }
    }
}

/// Docked lyrics strip position (`lyrics_pos` in config.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LyricsPos {
    /// Strip above the "option MUSIC" header block (top of player area).
    #[default]
    Above,
    /// Strip below the footer shortcut chip row (bottom of screen area).
    Below,
    /// Strip hidden — `y` re-opens below.
    Hidden,
}

impl LyricsPos {
    pub fn label(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
            Self::Hidden => "hidden",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Above => Self::Below,
            Self::Below => Self::Hidden,
            Self::Hidden => Self::Above,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Above => Self::Hidden,
            Self::Below => Self::Above,
            Self::Hidden => Self::Below,
        }
    }
}

/// ReplayGain application mode (MPV `replaygain` property).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReplayGainMode {
    #[default]
    Off,
    Track,
    Album,
}

impl ReplayGainMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Track => "track",
            Self::Album => "album",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Track,
            Self::Track => Self::Album,
            Self::Album => Self::Off,
        }
    }

    /// Value for MPV `replaygain` property.
    pub fn mpv_value(self) -> &'static str {
        match self {
            Self::Off => "no",
            Self::Track => "track",
            Self::Album => "album",
        }
    }
}

/// Repeat mode persisted across sessions (`o` cycles off → all → one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::All => "all",
            Self::One => "one",
        }
    }
}

fn default_volume() -> u8 {
    80
}
fn default_speed() -> f64 {
    1.0
}
fn default_pitch() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Allow volume up to 200% (MPV soft gain).
    pub excess_volume: bool,
    /// Low Detail Mode — fewer animations, lighter redraw.
    pub ldm: bool,
    pub accent: Accent,
    pub cava: CavaConfig,
    /// Download wizard UI: `arrows` (default) or `type`.
    #[serde(default)]
    pub dl_ui: DlUiMode,
    /// 403 fallback: `ask` (default) · `auto` · `off`.
    #[serde(default)]
    pub dl_fallback: DlFallbackMode,
    /// Toast anchor: `bottom-center` (default) · `bottom-left` · `bottom-right` · `top-center` · `top-left` · `top-right`.
    #[serde(default)]
    pub toast_pos: ToastPos,
    /// Stack up to 3 toasts; when off (default) only the newest toast shows.
    #[serde(default)]
    pub toast_stack: bool,
    /// Artists browser: metadata tags (default) or folder names.
    #[serde(default)]
    pub artist_source: ArtistSource,
    /// ReplayGain mode applied via MPV.
    #[serde(default)]
    pub replaygain: ReplayGainMode,
    /// Docked lyrics strip: `above` (default) · `below` · `hidden`.
    #[serde(default)]
    pub lyrics_pos: LyricsPos,
    #[serde(default, rename = "folders")]
    pub music_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub favorites: Vec<String>,
    /// Last played track id (absolute path) for session resume.
    #[serde(default)]
    pub resume_track: String,
    /// Playback position in seconds when the session was saved.
    #[serde(default)]
    pub resume_position: f64,
    /// Queue ids restored with the session.
    #[serde(default)]
    pub resume_queue: Vec<String>,
    /// Persist playback prefs + session on quit; `msc play` bare resumes (on by default).
    #[serde(default = "default_true")]
    pub resume: bool,
    /// Discord Rich Presence via local IPC (off by default).
    #[serde(default)]
    pub discord_rpc: bool,
    /// Discord application client id override (empty = built-in).
    #[serde(default)]
    pub discord_rpc_id: String,
    /// Persisted playback prefs (restored on next launch; CLI flags win).
    #[serde(default = "default_volume")]
    pub volume: u8,
    #[serde(default)]
    pub eq: crate::eq::EqPreset,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default = "default_speed")]
    pub speed: f64,
    #[serde(default = "default_pitch")]
    pub pitch: f64,
    #[serde(default)]
    pub smart_shuffle: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            excess_volume: false,
            ldm: false,
            accent: Accent::Default,
            cava: CavaConfig::default(),
            dl_ui: DlUiMode::Arrows,
            dl_fallback: DlFallbackMode::Ask,
            toast_pos: ToastPos::BottomCenter,
            toast_stack: false,
            artist_source: ArtistSource::Metadata,
            replaygain: ReplayGainMode::Off,
            lyrics_pos: LyricsPos::Above,
            music_dirs: Vec::new(),
            favorites: Vec::new(),
            resume_track: String::new(),
            resume_position: 0.0,
            resume_queue: Vec::new(),
            resume: true,
            discord_rpc: false,
            discord_rpc_id: String::new(),
            volume: 80,
            eq: crate::eq::EqPreset::Off,
            repeat: RepeatMode::Off,
            speed: 1.0,
            pitch: 1.0,
            smart_shuffle: false,
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
        let header = "# optionMusic settings — edit carefully or use `c` in the player\n\
# path: ~/.option/music/config.toml\n\n";
        option_sdk::atomic_write(&path, format!("{header}{body}").as_bytes())
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
        assert!(dir.ends_with(std::path::Path::new(".option").join("music")));
    }

    #[test]
    fn sdk_paths_stay_under_option_music() {
        use std::path::Path;
        // Resolved paths must match the SDK surface exactly and keep the
        // historical `~/.option/music/...` layout (old TOMLs stay valid).
        assert_eq!(config_dir(), option_sdk::App::MUSIC.dir());
        assert_eq!(config_path(), option_sdk::App::MUSIC.config_toml());
        assert_eq!(cache_dir(), option_sdk::App::MUSIC.cache_dir());
        assert_eq!(tags_cache_dir(), option_sdk::App::MUSIC.path("cache/tags"));
        assert_eq!(
            covers_cache_dir(),
            option_sdk::App::MUSIC.path("cache/covers")
        );
        let root = Path::new(".option").join("music");
        assert!(config_dir().ends_with(&root));
        assert!(config_path().ends_with(root.join("config.toml")));
        assert!(cache_dir().ends_with(root.join("cache")));
        assert!(tags_cache_dir().ends_with(root.join("cache").join("tags")));
        assert!(covers_cache_dir().ends_with(root.join("cache").join("covers")));
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
        assert!(resolve_music_dir("/tmp/optionmusic_no_such_dir_xyz").is_err());
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
        assert_eq!(
            a,
            Accent::Custom {
                r: 0x7e,
                g: 0xb8,
                b: 0xff
            }
        );
        assert_eq!(a.label(), "#7eb8ff");
    }

    #[test]
    fn cava_rows_normalize() {
        let mut c = CavaConfig {
            style: CavaStyle::Bars,
            rows: 9,
        };
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

    #[test]
    fn dl_ui_defaults_to_arrows() {
        assert_eq!(AppConfig::default().dl_ui, DlUiMode::Arrows);
        let back: AppConfig = toml::from_str("ldm = false").unwrap();
        assert_eq!(back.dl_ui, DlUiMode::Arrows);
    }

    #[test]
    fn dl_ui_parses_aliases() {
        let t: AppConfig = toml::from_str("dl_ui = \"type\"").unwrap();
        assert_eq!(t.dl_ui, DlUiMode::Type);
        let a: AppConfig = toml::from_str("dl_ui = \"arrow\"").unwrap();
        assert_eq!(a.dl_ui, DlUiMode::Arrows);
    }

    #[test]
    fn dl_fallback_defaults_to_ask() {
        use super::DlFallbackMode;
        assert_eq!(AppConfig::default().dl_fallback, DlFallbackMode::Ask);
        let back: AppConfig = toml::from_str("ldm = false").unwrap();
        assert_eq!(back.dl_fallback, DlFallbackMode::Ask);
        let auto: AppConfig = toml::from_str("dl_fallback = \"auto\"").unwrap();
        assert_eq!(auto.dl_fallback, DlFallbackMode::Auto);
    }

    #[test]
    fn toast_pos_defaults_to_bottom_center() {
        assert_eq!(AppConfig::default().toast_pos, ToastPos::BottomCenter);
        let back: AppConfig = toml::from_str("ldm = false").unwrap();
        assert_eq!(back.toast_pos, ToastPos::BottomCenter);
        let tl: AppConfig = toml::from_str("toast_pos = \"bottom-left\"").unwrap();
        assert_eq!(tl.toast_pos, ToastPos::BottomLeft);
        let top: AppConfig = toml::from_str("toast_pos = \"top-center\"").unwrap();
        assert_eq!(top.toast_pos, ToastPos::TopCenter);
    }

    #[test]
    fn toast_pos_cycles() {
        assert_eq!(ToastPos::BottomCenter.next(), ToastPos::BottomLeft);
        assert_eq!(ToastPos::BottomLeft.prev(), ToastPos::BottomCenter);
    }

    #[test]
    fn toast_pos_covers_all_six() {
        // next() walks the full ring, prev() walks it back.
        let mut p = ToastPos::BottomCenter;
        for _ in 0..5 {
            p = p.next();
        }
        assert_eq!(p, ToastPos::TopRight);
        assert_eq!(p.next(), ToastPos::BottomCenter);
        let mut q = ToastPos::BottomCenter;
        for _ in 0..5 {
            q = q.prev();
        }
        assert_eq!(q, ToastPos::BottomLeft);
        assert_eq!(q.prev(), ToastPos::BottomCenter);
        assert_eq!(ToastPos::TopLeft.label(), "top-left");
        assert_eq!(ToastPos::TopRight.label(), "top-right");
    }

    #[test]
    fn toast_pos_parses_new_aliases() {
        let tl: AppConfig = toml::from_str("toast_pos = \"top-left\"").unwrap();
        assert_eq!(tl.toast_pos, ToastPos::TopLeft);
        let tr: AppConfig = toml::from_str("toast_pos = \"topright\"").unwrap();
        assert_eq!(tr.toast_pos, ToastPos::TopRight);
        let tl2: AppConfig = toml::from_str("toast_pos = \"topleft\"").unwrap();
        assert_eq!(tl2.toast_pos, ToastPos::TopLeft);
    }

    #[test]
    fn toast_stack_defaults_to_off() {
        assert!(!AppConfig::default().toast_stack);
        let back: AppConfig = toml::from_str("ldm = false").unwrap();
        assert!(!back.toast_stack);
        let on: AppConfig = toml::from_str("toast_stack = true").unwrap();
        assert!(on.toast_stack);
    }
}
