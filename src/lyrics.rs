//! Synced/plain lyrics: `.lrc` sidecar first, then the lrclib public API.
//!
//! Responses are cached under `~/.option/music/cache/lyrics/`. Everything is
//! best-effort and English-only: failures yield `no lyrics found`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config::{self, stable_cache_key};

#[derive(Debug, Clone, PartialEq)]
pub struct LyricWord {
    pub time: Duration,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LrcLine {
    pub time: Duration,
    pub text: String,
    /// Word-level timings from Enhanced-LRC `<mm:ss.xx> word` tags.
    /// Empty for plain LRC lines (whole-line highlight fallback).
    #[allow(dead_code)]
    pub words: Vec<LyricWord>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedLyrics {
    /// Synced lines (empty when only plain text / none).
    pub lines: Vec<LrcLine>,
    /// Plain-text lines fallback (split from `plainLyrics` / `.txt` / embedded).
    pub plain: Vec<String>,
    /// Where the lyrics came from (`sidecar` · `cache` · `lrclib` · `embedded`).
    pub source: String,
}

impl ResolvedLyrics {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.plain.is_empty()
    }

    /// Index of the line active at `pos` (last line with `time <= pos`).
    pub fn active_index(&self, pos: Duration) -> Option<usize> {
        active_line_index(&self.lines, pos)
    }

    /// Sung word count on line `idx` at `pos` (words with `time <= pos`).
    /// Plain LRC lines (no words) report 0 — callers fall back to line highlight.
    pub fn sung_words(&self, idx: usize, pos: Duration) -> usize {
        self.lines
            .get(idx)
            .map(|l| sung_word_count(l, pos))
            .unwrap_or(0)
    }
}

pub fn lyrics_cache_dir() -> std::path::PathBuf {
    config::cache_dir().join("lyrics")
}

/// Parse `[mm:ss.xx] text` lines. Supports multiple tags per line and
/// `[mm:ss]` / `[mm:ss.xxx]` variants. Non-tag lines are kept as plain text.
pub fn parse_lrc(text: &str) -> Vec<LrcLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Collect leading [..] tags.
        let mut rest = line;
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else {
                break;
            };
            let tag = &stripped[..end];
            if let Some(t) = parse_timestamp(tag) {
                times.push(t);
                rest = stripped[end + 1..].trim_start();
            } else {
                break; // e.g. [ar:artist] metadata — stop tag parsing
            }
        }
        if times.is_empty() {
            continue;
        }
        let text = rest.trim().to_owned();
        if text.is_empty() {
            continue;
        }
        // Enhanced-LRC: inline `<mm:ss.xx> word` tags inside the line body.
        // `<mm:ss>` (no fraction) also parses. Text without inline tags
        // keeps plain-LRC behavior (whole-line timing, no words).
        let (plain_text, words) = parse_inline_words(&text);
        if plain_text.is_empty() && words.is_empty() {
            continue;
        }
        for t in times {
            out.push(LrcLine {
                time: t,
                text: if plain_text.is_empty() {
                    text.clone()
                } else {
                    plain_text.clone()
                },
                words: words.clone(),
            });
        }
    }
    out.sort_by_key(|l| l.time);
    out
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (mm, ss_frac) = tag.split_once(':')?;
    let mins: u64 = mm.trim().parse().ok()?;
    // Fractional part may use '.' or ':' (mm:ss:xx).
    let (secs_str, frac_str) = match ss_frac.split_once('.') {
        Some(p) => p,
        None => match ss_frac.split_once(':') {
            Some(p) => p,
            None => (ss_frac, ""),
        },
    };
    let secs: u64 = secs_str.trim().parse().ok()?;
    let frac: f64 = if frac_str.trim().is_empty() {
        0.0
    } else {
        let digits = frac_str.trim();
        let scale = 10f64.powi(digits.len() as i32);
        digits.parse::<f64>().ok()? / scale
    };
    Some(Duration::from_secs_f64(
        mins as f64 * 60.0 + secs as f64 + frac,
    ))
}

/// Split an LRC line body on inline `<time> word` tags (Enhanced-LRC).
/// Returns `(plain_text_without_tags, words)`. When no inline tag parses,
/// returns `(body, [])` so plain LRC degrades untouched.
fn parse_inline_words(body: &str) -> (String, Vec<LyricWord>) {
    if !body.contains('<') {
        return (body.to_owned(), Vec::new());
    }
    let mut words: Vec<LyricWord> = Vec::new();
    let mut plain = String::new();
    let mut rest = body;
    // Text before the first tag belongs to the line head (kept, untimed).
    let mut head_done = false;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            break;
        };
        let tag = &after[..close];
        let Some(t) = parse_timestamp(tag) else {
            // Not a timestamp (`<3`, HTML-ish) — keep literally and move on.
            if !head_done {
                plain.push_str(&rest[..=open]);
            } else {
                // Inside word runs: keep stray text with the previous word.
                if let Some(last) = words.last_mut() {
                    last.text.push_str(&rest[..=open]);
                    plain.push_str(&rest[..=open]);
                } else {
                    plain.push_str(&rest[..=open]);
                }
            }
            rest = &after[close + 1..];
            continue;
        };
        if !head_done {
            plain.push_str(rest[..open].trim_end());
            head_done = true;
        }
        let seg_start = &after[close + 1..];
        let next_tag = seg_start.find('<');
        let seg = match next_tag {
            Some(i) => &seg_start[..i],
            None => seg_start,
        };
        // Preserve one leading space gap between words for karaoke join.
        let word_text = seg.trim_start_matches([' ', '\t']);
        // Empty segment (back-to-back tags) carries no word — skip it so
        // timing stays attached to real syllables.
        if !word_text.trim().is_empty() {
            // Keep the raw spacing tail except trailing gap (re-added on join).
            let keep = word_text.trim_end().to_owned();
            if !plain.is_empty() && !plain.ends_with(' ') {
                plain.push(' ');
            }
            plain.push_str(&keep);
            words.push(LyricWord {
                time: t,
                text: keep,
            });
        }
        rest = match next_tag {
            Some(i) => &seg_start[i..],
            None => "",
        };
        if rest.is_empty() {
            break;
        }
    }
    if words.is_empty() {
        return (body.to_owned(), Vec::new());
    }
    // Trailing text after the last tag without its own tag stays untimed:
    // glue it onto the final word so nothing is lost.
    let tail = rest.trim();
    if !tail.is_empty() && !tail.contains('<') {
        if let Some(last) = words.last_mut() {
            last.text.push(' ');
            last.text.push_str(tail);
        }
        if !plain.ends_with(' ') {
            plain.push(' ');
        }
        plain.push_str(tail);
    }
    (plain.trim().to_owned(), words)
}

/// Sung words on a line at `pos` (word times `<= pos`).
pub fn sung_word_count(line: &LrcLine, pos: Duration) -> usize {
    line.words.iter().filter(|w| w.time <= pos).count()
}

/// Index of the word active at `pos` (last word with `time <= pos`).
pub fn active_word_index(line: &LrcLine, pos: Duration) -> Option<usize> {
    let mut active = None;
    for (i, w) in line.words.iter().enumerate() {
        if w.time <= pos {
            active = Some(i);
        } else {
            break;
        }
    }
    active
}

/// Fraction of the active word sung at `pos` (0.0 ..= 1.0).
///
/// Interpolates from the word start to the next boundary: the next word
/// on the same line when present, else `next_start` (the next line time).
/// Falls back to a 2s window on the final word so the edge still sweeps.
/// Returns 0.0 before the word starts, 1.0 once past the boundary.
pub fn word_progress_fraction(
    line: &LrcLine,
    idx: usize,
    pos: Duration,
    next_start: Option<Duration>,
) -> f64 {
    let Some(word) = line.words.get(idx) else {
        return 0.0;
    };
    if pos < word.time {
        return 0.0;
    }
    let end = if let Some(nw) = line.words.get(idx + 1) {
        nw.time
    } else if let Some(ns) = next_start {
        ns
    } else {
        word.time + Duration::from_secs_f64(2.0)
    };
    if end <= word.time {
        return 1.0;
    }
    let total = (end - word.time).as_secs_f64();
    if total <= 0.0 {
        return 1.0;
    }
    let done = (pos - word.time).as_secs_f64() / total;
    done.clamp(0.0, 1.0)
}

/// Active word plus its intra-word fraction at `pos`.
/// `None` when no word has started yet. Past the last boundary the
/// fraction saturates at 1.0 (fully sung, no jitter).
pub fn active_word_progress(
    line: &LrcLine,
    pos: Duration,
    next_start: Option<Duration>,
) -> Option<(usize, f64)> {
    let idx = active_word_index(line, pos)?;
    Some((idx, word_progress_fraction(line, idx, pos, next_start)))
}

/// Sung letter count for a word with `chars` letters at `fraction`.
/// Floor step so the bright edge only advances (never flickers back),
/// clamped to `0..=chars`. Geometry-stable: callers paint fixed widths.
pub fn sung_letters(chars: usize, fraction: f64) -> usize {
    if chars == 0 {
        return 0;
    }
    let f = fraction.clamp(0.0, 1.0);
    if f >= 1.0 {
        return chars;
    }
    if f <= 0.0 {
        return 0;
    }
    ((f * chars as f64).floor() as usize).min(chars)
}

/// First-visible row of a lyric window (`shown` rows) holding `active`.
/// Keeps the active line on the second row when possible so one sung
/// line stays above and context below. Short lists pin to 0.
pub fn lyric_window_start(active: Option<usize>, total: usize, shown: usize) -> usize {
    let shown = shown.max(1);
    if total <= shown {
        return 0;
    }
    active
        .unwrap_or(0)
        .saturating_sub(1)
        .min(total.saturating_sub(shown))
}

/// Ease a smooth window offset toward `target` without jumping.
/// Moves a fraction (`rate` per frame, e.g. 0.25 at ~60fps) and snaps
/// when within half a row. LDM callers pass `rate = 1.0` (stepped).
pub fn ease_lyric_offset(current: f64, target: f64, rate: f64) -> f64 {
    if (target - current).abs() < 0.02 {
        return target;
    }
    let rate = rate.clamp(0.0, 1.0);
    let next = current + (target - current) * rate;
    let remaining = (target - next).abs();
    if remaining < 0.02 || (remaining < 0.5 && rate >= 0.99) {
        target
    } else {
        next
    }
}
/// Last line index with `time <= pos`.
pub fn active_line_index(lines: &[LrcLine], pos: Duration) -> Option<usize> {
    let mut active = None;
    for (i, l) in lines.iter().enumerate() {
        if l.time <= pos {
            active = Some(i);
        } else {
            break;
        }
    }
    active
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn cache_key(artist: &str, title: &str, album: &str, duration: Option<f64>) -> String {
    let dur = duration.map(|d| d.round().to_string()).unwrap_or_default();
    stable_cache_key(&[
        artist.as_bytes(),
        title.as_bytes(),
        album.as_bytes(),
        dur.as_bytes(),
        b"lyrics-v1",
    ])
}

/// Fetch lrclib JSON via `curl` (short timeout, no new deps). Returns the raw body.
fn fetch_lrclib(artist: &str, title: &str, album: &str, duration: Option<f64>) -> Option<String> {
    let mut url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}",
        percent_encode(artist),
        percent_encode(title)
    );
    if !album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", percent_encode(album)));
    }
    if let Some(d) = duration
        && d > 0.0
    {
        url.push_str(&format!("&duration={}", d.round() as u64));
    }
    let probe = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "6",
            "--connect-timeout",
            "4",
            "-H",
            "User-Agent: optionmusic",
            &url,
        ])
        .output()
        .ok()?;
    if !probe.status.success() {
        return None;
    }
    let body = String::from_utf8(probe.stdout).ok()?;
    if body.trim().is_empty() || body.contains("\"statusCode\":404") {
        return None;
    }
    Some(body)
}

fn split_plain(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

fn from_lrclib_body(body: &str) -> ResolvedLyrics {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return ResolvedLyrics::empty(),
    };
    let synced = v.get("syncedLyrics").and_then(|s| s.as_str()).unwrap_or("");
    let plain = v.get("plainLyrics").and_then(|s| s.as_str()).unwrap_or("");
    let lines = parse_lrc(synced);
    if !lines.is_empty() {
        return ResolvedLyrics {
            lines,
            plain: Vec::new(),
            source: "lrclib".into(),
        };
    }
    if !plain.trim().is_empty() {
        return ResolvedLyrics {
            lines: Vec::new(),
            plain: split_plain(plain),
            source: "lrclib".into(),
        };
    }
    ResolvedLyrics::empty()
}

/// Resolve lyrics for a track: sidecar `.lrc` → cache → embedded → lrclib.
/// Sidecar `.txt` and embedded text surface as plain lines.
pub fn resolve_lyrics(
    path: &Path,
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) -> ResolvedLyrics {
    // 1. Local `.lrc` sidecar next to the file.
    let stem = path.with_extension("");
    let sidecar = stem.with_extension("lrc");
    if let Ok(text) = fs::read_to_string(&sidecar) {
        let lines = parse_lrc(&text);
        if !lines.is_empty() {
            return ResolvedLyrics {
                lines,
                plain: Vec::new(),
                source: "sidecar".into(),
            };
        }
        let plain = split_plain(&text);
        if !plain.is_empty() {
            return ResolvedLyrics {
                lines: Vec::new(),
                plain,
                source: "sidecar".into(),
            };
        }
    }
    // 2. Cached lrclib response.
    let dir = lyrics_cache_dir();
    let key = cache_key(artist, title, album, duration);
    let cached = dir.join(format!("{key}.json"));
    if let Ok(body) = fs::read_to_string(&cached) {
        let mut r = from_lrclib_body(&body);
        if !r.is_empty() {
            r.source = "cache".into();
            return r;
        }
    }
    // 3. Embedded / `.txt` via existing tag reader.
    let local = crate::meta::read_lyrics(path);
    if !local.text.trim().is_empty() {
        let lines = parse_lrc(&local.text);
        if !lines.is_empty() {
            return ResolvedLyrics {
                lines,
                plain: Vec::new(),
                source: "embedded".into(),
            };
        }
        return ResolvedLyrics {
            lines: Vec::new(),
            plain: split_plain(&local.text),
            source: "embedded".into(),
        };
    }
    // 4. lrclib network (needs artist + title).
    if artist.trim().is_empty() || title.trim().is_empty() {
        return ResolvedLyrics::empty();
    }
    if let Some(body) = fetch_lrclib(artist, title, album, duration) {
        let r = from_lrclib_body(&body);
        if !r.is_empty() {
            let _ = fs::create_dir_all(&dir);
            let _ = option_sdk::atomic_write(&cached, body.as_bytes());
            return r;
        }
    }
    ResolvedLyrics::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc_with_centis() {
        let lines = parse_lrc("[00:12.34] hello\n[01:02.5] world\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time, Duration::from_secs_f64(12.34));
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[1].time, Duration::from_secs_f64(62.5));
    }

    #[test]
    fn parses_multi_tag_and_skips_metadata() {
        let lines =
            parse_lrc("[ar:Someone]\n[00:10.00][00:20.00] chorus\n[ti:Title]\nno tags here\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time, Duration::from_secs(10));
        assert_eq!(lines[1].time, Duration::from_secs(20));
        assert_eq!(lines[0].text, "chorus");
    }

    #[test]
    fn active_line_lookup() {
        let lines = parse_lrc("[00:10.00] a\n[00:20.00] b\n[00:30.00] c\n");
        assert_eq!(active_line_index(&lines, Duration::from_secs(0)), None);
        assert_eq!(active_line_index(&lines, Duration::from_secs(10)), Some(0));
        assert_eq!(active_line_index(&lines, Duration::from_secs(25)), Some(1));
        assert_eq!(active_line_index(&lines, Duration::from_secs(99)), Some(2));
    }

    #[test]
    fn lrclib_body_prefers_synced() {
        let body = r#"{"syncedLyrics":"[00:01.00] hi\n","plainLyrics":"hi"}"#;
        let r = from_lrclib_body(body);
        assert_eq!(r.source, "lrclib");
        assert_eq!(r.lines.len(), 1);
        assert!(r.plain.is_empty());
    }

    #[test]
    fn lrclib_body_falls_back_to_plain() {
        let body = r#"{"syncedLyrics":null,"plainLyrics":"line one\nline two"}"#;
        let r = from_lrclib_body(body);
        assert!(r.lines.is_empty());
        assert_eq!(r.plain, vec!["line one", "line two"]);
    }

    #[test]
    fn parses_enhanced_lrc_word_tags() {
        let lines = parse_lrc("[00:12.00] <00:12.00> Hello <00:12.60> world\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(lines[0].words[0].time, Duration::from_secs_f64(12.0));
        assert_eq!(lines[0].words[0].text, "Hello");
        assert_eq!(lines[0].words[1].time, Duration::from_secs_f64(12.6));
        assert_eq!(lines[0].words[1].text, "world");
    }

    #[test]
    fn enhanced_word_tags_accept_mm_ss_fallback() {
        let lines = parse_lrc("[00:10.00] <00:10> hi <00:12> there\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(lines[0].words[0].time, Duration::from_secs(10));
        assert_eq!(lines[0].words[1].time, Duration::from_secs(12));
        assert_eq!(lines[0].text, "hi there");
    }

    #[test]
    fn plain_lines_have_no_words() {
        let lines = parse_lrc("[00:10.00] just a line\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].words.is_empty());
        assert_eq!(lines[0].text, "just a line");
    }

    #[test]
    fn word_active_lookup_at_positions() {
        let lines = parse_lrc("[00:10.00] <00:10.00> one <00:11.00> two <00:12.00> three\n");
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(active_word_index(l, Duration::from_secs_f64(9.9)), None);
        assert_eq!(active_word_index(l, Duration::from_secs_f64(10.0)), Some(0));
        assert_eq!(active_word_index(l, Duration::from_secs_f64(10.5)), Some(0));
        assert_eq!(active_word_index(l, Duration::from_secs_f64(11.0)), Some(1));
        assert_eq!(active_word_index(l, Duration::from_secs_f64(99.0)), Some(2));
        assert_eq!(sung_word_count(l, Duration::from_secs_f64(10.5)), 1);
        assert_eq!(sung_word_count(l, Duration::from_secs_f64(12.0)), 3);
    }

    #[test]
    fn window_follow_math_pins_and_tracks() {
        // Short list pins to 0.
        assert_eq!(lyric_window_start(Some(2), 2, 3), 0);
        assert_eq!(lyric_window_start(None, 10, 3), 0);
        // Active sits on the second row when room allows.
        assert_eq!(lyric_window_start(Some(0), 10, 3), 0);
        assert_eq!(lyric_window_start(Some(1), 10, 3), 0);
        assert_eq!(lyric_window_start(Some(2), 10, 3), 1);
        assert_eq!(lyric_window_start(Some(5), 10, 3), 4);
        // Tail clamps so no blank rows show.
        assert_eq!(lyric_window_start(Some(9), 10, 3), 7);
        assert_eq!(lyric_window_start(Some(99), 10, 3), 7);
    }

    #[test]
    fn ease_offset_moves_without_jumping() {
        // Small rate never covers the full distance in one step.
        let first = ease_lyric_offset(0.0, 6.0, 0.25);
        assert!(first > 0.0 && first < 6.0);
        assert!((first - 1.5).abs() < 0.001);
        // Monotonic convergence over frames.
        let mut cur = 0.0;
        let mut prev = cur;
        for _ in 0..60 {
            cur = ease_lyric_offset(cur, 6.0, 0.25);
            assert!(cur >= prev);
            prev = cur;
        }
        assert_eq!(cur, 6.0);
        // LDM stepped rate snaps instantly.
        assert_eq!(ease_lyric_offset(0.0, 4.0, 1.0), 4.0);
    }

    #[test]
    fn intra_word_fraction_sweeps_between_words() {
        let lines = parse_lrc("[00:10.00] <00:10.00> one <00:11.00> two <00:12.00> three\n");
        let l = &lines[0];
        // Before start: 0.
        assert_eq!(
            word_progress_fraction(l, 0, Duration::from_secs_f64(9.9), None),
            0.0
        );
        // Word 0 spans 10.0 -> 11.0: halfway at 10.5.
        let half = word_progress_fraction(l, 0, Duration::from_secs_f64(10.5), None);
        assert!((half - 0.5).abs() < 0.001);
        // At the boundary the word is fully sung.
        assert_eq!(
            word_progress_fraction(l, 0, Duration::from_secs_f64(11.0), None),
            1.0
        );
        // Active progress pairs index + fraction.
        let (idx, f) = active_word_progress(l, Duration::from_secs_f64(10.25), None).unwrap();
        assert_eq!(idx, 0);
        assert!((f - 0.25).abs() < 0.001);
        assert_eq!(
            active_word_progress(l, Duration::from_secs_f64(9.9), None),
            None
        );
    }

    #[test]
    fn intra_word_fraction_uses_next_line_then_fallback() {
        let lines = parse_lrc("[00:10.00] <00:10.00> hello\n");
        let l = &lines[0];
        // Last word extends to the next line start.
        let next = Some(Duration::from_secs_f64(12.0));
        let mid = word_progress_fraction(l, 0, Duration::from_secs_f64(11.0), next);
        assert!((mid - 0.5).abs() < 0.001);
        assert_eq!(
            word_progress_fraction(l, 0, Duration::from_secs_f64(12.0), next),
            1.0
        );
        // No next line: 2s fallback window.
        assert_eq!(
            word_progress_fraction(l, 0, Duration::from_secs_f64(12.0), None),
            1.0
        );
        let early = word_progress_fraction(l, 0, Duration::from_secs_f64(10.5), None);
        assert!((early - 0.25).abs() < 0.001);
    }

    #[test]
    fn sung_letters_steps_without_flicker() {
        assert_eq!(sung_letters(5, 0.0), 0);
        assert_eq!(sung_letters(5, 0.39), 1);
        assert_eq!(sung_letters(5, 0.5), 2);
        assert_eq!(sung_letters(5, 0.99), 4);
        assert_eq!(sung_letters(5, 1.0), 5);
        assert_eq!(sung_letters(0, 0.5), 0);
        // Monotonic across a sweep.
        let mut prev = 0;
        for i in 0..=20 {
            let n = sung_letters(6, i as f64 / 20.0);
            assert!(n >= prev);
            prev = n;
        }
        assert_eq!(prev, 6);
    }
}
