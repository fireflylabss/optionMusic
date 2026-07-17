//! Paths and defaults for optMusic.

use std::path::{Component, PathBuf};

use anyhow::{bail, Context, Result};

/// `~/.config/optmusic`
#[allow(dead_code)]
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("optmusic")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_with_optmusic() {
        let dir = config_dir();
        assert!(dir.ends_with("optmusic"));
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
}
