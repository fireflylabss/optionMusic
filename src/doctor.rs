//! `msc doctor` — report which external tools the player found on `PATH`.

use std::path::PathBuf;

use crate::which;

/// What a tool is needed for. Playback itself needs nothing here: libmpv is
/// linked into the binary, so a running `doctor` already proves it resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// `msc dl` cannot work without it.
    Downloads,
    /// A single optional feature degrades without it.
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub tool: &'static str,
    pub need: Need,
    /// What stops working without it.
    pub purpose: &'static str,
    pub found: Option<PathBuf>,
    /// Shown only when the tool is missing.
    pub hint: &'static str,
}

/// Everything the player shells out to, in report order.
pub fn checks() -> Vec<Check> {
    let probe = |tool: &'static str, need, purpose, hint| Check {
        tool,
        need,
        purpose,
        found: which::lookup(tool),
        hint,
    };
    vec![
        probe(
            "yt-dlp",
            Need::Downloads,
            "`msc dl` downloads",
            "Arch: sudo pacman -S yt-dlp · https://github.com/yt-dlp/yt-dlp#installation",
        ),
        probe(
            "ffmpeg",
            Need::Downloads,
            "audio extraction / muxing for `msc dl`",
            "Arch: sudo pacman -S ffmpeg",
        ),
        probe(
            "cava",
            Need::Optional,
            "spectrum bars (`--cava`, or `v` in the TUI)",
            "Arch: sudo pacman -S cava · https://github.com/karlstav/cava",
        ),
        probe(
            "mpv",
            Need::Optional,
            "not used for playback (libmpv is linked in), handy for debugging",
            "Arch: sudo pacman -S mpv",
        ),
    ]
}

/// Whether `msc dl` can run end to end.
pub fn downloads_ready(checks: &[Check]) -> bool {
    !checks
        .iter()
        .any(|c| c.need == Need::Downloads && c.found.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_explains_itself() {
        for c in checks() {
            assert!(!c.purpose.is_empty(), "{} lacks a purpose", c.tool);
            assert!(!c.hint.is_empty(), "{} lacks an install hint", c.tool);
        }
    }

    #[test]
    fn downloads_need_both_yt_dlp_and_ffmpeg() {
        let mut cs = checks();
        for c in &mut cs {
            c.found = Some(PathBuf::from("/usr/bin").join(c.tool));
        }
        assert!(downloads_ready(&cs));
        for tool in ["yt-dlp", "ffmpeg"] {
            let mut degraded = cs.clone();
            degraded.iter_mut().find(|c| c.tool == tool).unwrap().found = None;
            assert!(!downloads_ready(&degraded), "{tool} must gate downloads");
        }
        // Optional tools never gate downloads.
        let mut no_cava = cs.clone();
        no_cava.iter_mut().find(|c| c.tool == "cava").unwrap().found = None;
        assert!(downloads_ready(&no_cava));
    }
}
