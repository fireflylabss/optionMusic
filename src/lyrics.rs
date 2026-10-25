//! Synced/plain lyrics: managed selection, local files, then online providers.
//!
//! Responses are cached under `~/.option/music/cache/lyrics/`. Everything is
//! best-effort: playback failures yield `no lyrics found`; manager errors are explicit.

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

#[derive(Debug, Clone, Default, PartialEq)]
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
    if secs >= 60 {
        return None;
    }
    let digits = frac_str.trim();
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut nanos = 0u32;
    for index in 0..9 {
        nanos = nanos * 10
            + digits
                .as_bytes()
                .get(index)
                .map_or(0, |byte| (byte - b'0') as u32);
    }
    let seconds = mins.checked_mul(60)?.checked_add(secs)?;
    Some(Duration::new(seconds, nanos))
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
        word.time.saturating_add(Duration::from_secs(2))
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

pub(crate) fn percent_encode(s: &str) -> String {
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

/// lrclib `GET /api/get` URL for a track.
fn lrclib_url(artist: &str, title: &str, album: &str, duration: Option<f64>) -> String {
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
    url
}

fn http_get(url: &str) -> Option<String> {
    http_get_with(url, &[])
}

fn http_get_with(url: &str, headers: &[(&str, &str)]) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_global(Some(Duration::from_secs(5)))
        .user_agent(concat!("stillmusic/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut req = agent.get(url);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    let body = req.call().ok()?.body_mut().read_to_string().ok()?;
    if body.trim().is_empty() || body.contains("\"statusCode\":404") {
        return None;
    }
    Some(body)
}

/// Fetch lrclib JSON over HTTPS. Exact `/api/get` first, then `/api/search`
/// when the signature misses (different album tag, live vs studio length).
fn fetch_lrclib(artist: &str, title: &str, album: &str, duration: Option<f64>) -> Option<String> {
    let exact = http_get(&lrclib_url(artist, title, album, duration));
    if exact
        .as_deref()
        .is_some_and(|body| !from_lrclib_body(body).is_empty())
    {
        return exact;
    }
    let q = format!("{} {}", artist.trim(), title.trim());
    let url = format!(
        "https://lrclib.net/api/search?q={}",
        percent_encode(q.trim())
    );
    let body = http_get(&url)?;
    pick_lrclib_search(&body, duration)
}

/// Choose one record from an lrclib search array. Prefer synced lyrics, then
/// the duration closest to the file when we know it.
fn pick_lrclib_search(body: &str, duration: Option<f64>) -> Option<String> {
    let hits: Vec<serde_json::Value> = serde_json::from_str(body).ok()?;
    let mut best: Option<(i32, &serde_json::Value)> = None;
    for hit in &hits {
        let synced = hit
            .get("syncedLyrics")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let plain = hit
            .get("plainLyrics")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        if synced.trim().is_empty() && plain.trim().is_empty() {
            continue;
        }
        let mut score = if synced.trim().is_empty() { 0 } else { 100 };
        if let (Some(want), Some(got)) = (
            duration.filter(|d| *d > 0.0),
            hit.get("duration").and_then(|d| d.as_f64()),
        ) {
            let delta = (want - got).abs();
            if delta <= 2.0 {
                score += 40;
            } else if delta <= 8.0 {
                score += 15;
            } else if delta > 30.0 {
                score -= 30;
            }
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, hit));
        }
    }
    let (_, hit) = best?;
    serde_json::to_string(hit).ok()
}

fn split_plain(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn from_lrclib_body(body: &str) -> ResolvedLyrics {
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

/// Resolve lyrics for a track: sidecar `.lrc` → cache → embedded → online.
/// Online order is lrclib, then NetEase, then lyrics.ovh.
/// Sidecar `.txt` and embedded text surface as plain lines.
///
/// Blocks on the network when only a provider can answer; interactive callers
/// should use [`LyricsLoader`] instead.
pub fn resolve_lyrics(
    path: &Path,
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) -> ResolvedLyrics {
    match resolve_local(path, artist, title, album, duration) {
        Some(r) => r,
        None => fetch_remote(artist, title, album, duration).unwrap_or_else(ResolvedLyrics::empty),
    }
}

/// Lyrics reachable without network I/O: sidecar → cache → embedded tags.
/// `None` means only an online provider can still answer.
pub fn resolve_local(
    path: &Path,
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) -> Option<ResolvedLyrics> {
    if let Ok(Some(lyrics)) = read_managed(path) {
        return Some(lyrics);
    }
    // 1. Local `.lrc` sidecar next to the file.
    let sidecar = path.with_extension("lrc");
    if let Ok(text) = fs::read_to_string(&sidecar) {
        let lines = parse_lrc(&text);
        if !lines.is_empty() {
            return Some(ResolvedLyrics {
                lines,
                plain: Vec::new(),
                source: "sidecar".into(),
            });
        }
        let plain = split_plain(&text);
        if !plain.is_empty() {
            return Some(ResolvedLyrics {
                lines: Vec::new(),
                plain,
                source: "sidecar".into(),
            });
        }
    }
    // 2. Cached provider response (lrclib, netease, or ovh).
    let cached = cache_path(artist, title, album, duration);
    if let Ok(body) = fs::read_to_string(&cached) {
        let r = from_stored_body(&body);
        if !r.is_empty() {
            return Some(r);
        }
    }
    // 3. Embedded / `.txt` via existing tag reader.
    let local = crate::meta::read_lyrics(path);
    if !local.text.trim().is_empty() {
        let lines = parse_lrc(&local.text);
        if !lines.is_empty() {
            return Some(ResolvedLyrics {
                lines,
                plain: Vec::new(),
                source: "embedded".into(),
            });
        }
        return Some(ResolvedLyrics {
            lines: Vec::new(),
            plain: split_plain(&local.text),
            source: "embedded".into(),
        });
    }
    None
}

pub(crate) fn cache_path(
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) -> std::path::PathBuf {
    let key = cache_key(artist, title, album, duration);
    lyrics_cache_dir().join(format!("{key}.json"))
}

/// Managed selections survive cache cleanup and never modify the music file.
pub fn managed_path(path: &Path) -> std::path::PathBuf {
    managed_path_in(&config::config_dir().join("lyrics"), path)
}

fn managed_path_in(root: &Path, path: &Path) -> std::path::PathBuf {
    let identity = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let key = stable_cache_key(&[
        identity.as_os_str().as_encoded_bytes(),
        b"lyrics-selection-v1",
    ]);
    root.join(format!("{key}.json"))
}

fn read_managed_file(path: &Path) -> anyhow::Result<Option<ResolvedLyrics>> {
    use anyhow::Context;
    let body = match fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Cannot read saved lyrics"),
    };
    let document: serde_json::Value = serde_json::from_str(&body)
        .context("Saved lyrics JSON is malformed; reimport or remove the selection")?;
    anyhow::ensure!(
        document.get("version").and_then(|v| v.as_u64()) == Some(1),
        "Unsupported saved lyrics version"
    );
    let lyrics = from_stored_body(&body);
    anyhow::ensure!(!lyrics.is_empty(), "Saved lyrics contain no usable text");
    Ok(Some(lyrics))
}

pub fn read_managed(track: &Path) -> anyhow::Result<Option<ResolvedLyrics>> {
    read_managed_file(&managed_path(track))
}

pub fn save_managed(track: &Path, lyrics: &ResolvedLyrics) -> anyhow::Result<()> {
    save_managed_file(&managed_path(track), lyrics)
}

fn save_managed_file(path: &Path, lyrics: &ResolvedLyrics) -> anyhow::Result<()> {
    anyhow::ensure!(!lyrics.is_empty(), "Cannot save empty lyrics");
    let body = serde_json::json!({
        "version": 1,
        "source": lyrics.source,
        "syncedLyrics": lrc_from_lines(&lyrics.lines),
        "plainLyrics": lyrics.plain.join("\n"),
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    option_sdk::atomic_write(path, serde_json::to_vec_pretty(&body)?.as_slice())?;
    Ok(())
}

/// Online lookup. Synced lyrics win: lrclib, then NetEase. lyrics.ovh is
/// plain text and only used when neither synced source hits. A title is
/// enough for lrclib and NetEase; ovh also needs an artist.
pub fn fetch_remote(
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) -> Option<ResolvedLyrics> {
    if title.trim().is_empty() {
        return None;
    }
    let mut plain: Option<ResolvedLyrics> = None;
    if let Some(body) = fetch_lrclib(artist, title, album, duration) {
        let got = from_lrclib_body(&body);
        if !got.lines.is_empty() {
            store_cache(&got, artist, title, album, duration);
            return Some(got);
        }
        if !got.is_empty() {
            plain = Some(got);
        }
    }
    if let Some(got) = fetch_netease(artist, title, duration) {
        if !got.lines.is_empty() {
            store_cache(&got, artist, title, album, duration);
            return Some(got);
        }
        if plain.is_none() && !got.is_empty() {
            plain = Some(got);
        }
    }
    if plain.is_none() {
        plain = fetch_ovh(artist, title);
    }
    plain.inspect(|got| {
        store_cache(got, artist, title, album, duration);
    })
}

fn store_cache(
    lyrics: &ResolvedLyrics,
    artist: &str,
    title: &str,
    album: &str,
    duration: Option<f64>,
) {
    let body = serde_json::json!({
        "source": lyrics.source,
        "syncedLyrics": lrc_from_lines(&lyrics.lines),
        "plainLyrics": lyrics.plain.join("\n"),
    });
    let cached = cache_path(artist, title, album, duration);
    if let Some(dir) = cached.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&body) {
        let _ = option_sdk::atomic_write(&cached, text.as_bytes());
    }
}

pub(crate) fn from_stored_body(body: &str) -> ResolvedLyrics {
    let mut lyrics = from_lrclib_body(body);
    if lyrics.is_empty() {
        return lyrics;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(source) = v.get("source").and_then(|s| s.as_str())
        && !source.is_empty()
        && source != "lrclib"
    {
        lyrics.source = source.to_string();
    }
    lyrics
}

pub(crate) fn lrc_from_lines(lines: &[LrcLine]) -> String {
    let mut out = String::new();
    for line in lines {
        let stamp = lrc_stamp(line.time);
        out.push('[');
        out.push_str(&stamp);
        out.push(']');
        if line.words.is_empty() {
            out.push(' ');
            out.push_str(&line.text);
        } else {
            for word in &line.words {
                out.push_str(" <");
                out.push_str(&lrc_stamp(word.time));
                out.push_str("> ");
                out.push_str(&word.text);
            }
        }
        out.push('\n');
    }
    out
}

fn lrc_stamp(time: Duration) -> String {
    let ms = time.as_millis();
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) as f64 / 1000.0;
    format!("{minutes:02}:{seconds:05.2}")
}

/// NetEase song id whose title and duration best match the file.
fn pick_netease_id(body: &str, title: &str, duration: Option<f64>) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let songs = v.pointer("/result/songs")?.as_array()?;
    let mut best: Option<(i32, i64)> = None;
    for song in songs {
        let Some(id) = song.get("id").and_then(|id| id.as_i64()) else {
            continue;
        };
        let name = song.get("name").and_then(|s| s.as_str()).unwrap_or("");
        let mut score = if name.eq_ignore_ascii_case(title.trim()) {
            50
        } else if name.to_lowercase().contains(&title.trim().to_lowercase()) {
            15
        } else {
            0
        };
        if let (Some(want), Some(got_ms)) = (
            duration.filter(|d| *d > 0.0),
            song.get("duration").and_then(|d| d.as_f64()),
        ) {
            let delta = (want - got_ms / 1000.0).abs();
            if delta <= 3.0 {
                score += 40;
            } else if delta <= 10.0 {
                score += 10;
            } else if delta > 45.0 {
                score -= 25;
            }
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}

fn fetch_netease(artist: &str, title: &str, duration: Option<f64>) -> Option<ResolvedLyrics> {
    let q = format!("{} {}", artist.trim(), title.trim());
    let search = format!(
        "https://music.163.com/api/search/get?s={}&type=1&limit=6&offset=0",
        percent_encode(q.trim())
    );
    let body = http_get_with(
        &search,
        &[
            ("Referer", "https://music.163.com/"),
            ("User-Agent", "Mozilla/5.0"),
        ],
    )?;
    let id = pick_netease_id(&body, title, duration)?;
    let lyric_url = format!("https://music.163.com/api/song/lyric?id={id}&lv=1&kv=1&tv=-1");
    let body = http_get_with(
        &lyric_url,
        &[
            ("Referer", "https://music.163.com/"),
            ("User-Agent", "Mozilla/5.0"),
        ],
    )?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let lrc = v
        .pointer("/lrc/lyric")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    if lrc.trim().is_empty() || lrc.contains("纯音乐，请欣赏") {
        return None;
    }
    let lines = parse_lrc(lrc);
    if lines.is_empty() {
        return None;
    }
    Some(ResolvedLyrics {
        lines,
        plain: Vec::new(),
        source: "netease".into(),
    })
}

fn from_ovh_body(body: &str) -> ResolvedLyrics {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return ResolvedLyrics::empty();
    };
    let text = v.get("lyrics").and_then(|s| s.as_str()).unwrap_or("");
    let plain = split_plain(text);
    if plain.is_empty() {
        return ResolvedLyrics::empty();
    }
    ResolvedLyrics {
        lines: Vec::new(),
        plain,
        source: "ovh".into(),
    }
}

fn fetch_ovh(artist: &str, title: &str) -> Option<ResolvedLyrics> {
    if artist.trim().is_empty() || title.trim().is_empty() {
        return None;
    }
    let url = format!(
        "https://api.lyrics.ovh/v1/{}/{}",
        percent_encode(artist.trim()),
        percent_encode(title.trim())
    );
    let body = http_get(&url)?;
    let lyrics = from_ovh_body(&body);
    if lyrics.is_empty() {
        None
    } else {
        Some(lyrics)
    }
}

/// What to look up for a track. Owned so it can cross into a worker thread.
#[derive(Debug, Clone, PartialEq)]
pub struct LyricsQuery {
    pub path: std::path::PathBuf,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration: Option<f64>,
}

/// Resolution state of the lyrics panel.
#[derive(Debug, Clone, PartialEq)]
pub enum LyricsState {
    /// A worker thread is talking to the online providers.
    Loading,
    Ready(ResolvedLyrics),
    Missing,
}

/// Non-blocking lyrics resolution for interactive surfaces: local sources are
/// read inline, online providers run on a worker thread while the UI keeps drawing.
#[derive(Debug, Default)]
pub struct LyricsLoader {
    key: Option<String>,
    state: Option<LyricsState>,
    rx: Option<std::sync::mpsc::Receiver<Option<ResolvedLyrics>>>,
}

impl LyricsLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the current track (e.g. after a skip).
    pub fn clear(&mut self) {
        self.key = None;
        self.state = None;
        self.rx = None;
    }

    /// State for `query`, starting or polling the provider worker as needed.
    /// Never blocks on the network.
    pub fn state(&mut self, key: &str, query: impl FnOnce() -> LyricsQuery) -> &LyricsState {
        if self.key.as_deref() != Some(key) {
            self.clear();
            self.key = Some(key.to_string());
            let q = query();
            match resolve_local(&q.path, &q.artist, &q.title, &q.album, q.duration) {
                Some(r) => self.state = Some(LyricsState::Ready(r)),
                None => self.spawn(q),
            }
        }
        self.poll();
        self.state.as_ref().unwrap_or(&LyricsState::Missing)
    }

    fn spawn(&mut self, q: LyricsQuery) {
        if q.title.trim().is_empty() {
            self.state = Some(LyricsState::Missing);
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_remote(&q.artist, &q.title, &q.album, q.duration));
        });
        self.rx = Some(rx);
        self.state = Some(LyricsState::Loading);
    }

    fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        match rx.try_recv() {
            Ok(Some(r)) => {
                self.state = Some(LyricsState::Ready(r));
                self.rx = None;
            }
            Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.state = Some(LyricsState::Missing);
                self.rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_selection_roundtrip_and_cleanup_preserve_originals() {
        let root = std::env::temp_dir().join(format!(
            "stillmusic-managed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let audio = root.join("track.mp3");
        let sidecar = root.join("track.lrc");
        fs::write(&audio, b"audio fixture").unwrap();
        fs::write(&sidecar, "[00:01.00] original").unwrap();
        let selected = managed_path_in(&root.join("managed"), &audio);
        assert_eq!(read_managed_file(&selected).unwrap(), None);
        let lyrics = ResolvedLyrics {
            lines: parse_lrc("[00:02.00] <00:02.00> chosen <00:03.00> words"),
            plain: Vec::new(),
            source: "lrclib:42".into(),
        };
        save_managed_file(&selected, &lyrics).unwrap();
        assert_eq!(read_managed_file(&selected).unwrap(), Some(lyrics));
        assert!(save_managed_file(&selected, &ResolvedLyrics::empty()).is_err());
        fs::remove_file(&selected).unwrap();
        assert_eq!(fs::read(&audio).unwrap(), b"audio fixture");
        assert_eq!(fs::read_to_string(sidecar).unwrap(), "[00:01.00] original");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_and_future_managed_documents_fail_without_rewriting() {
        let root = std::env::temp_dir().join(format!(
            "stillmusic-corrupt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("selection.json");
        for body in [
            "{broken",
            r#"{"version":2,"plainLyrics":"future"}"#,
            r#"{"version":1,"plainLyrics":""}"#,
        ] {
            fs::write(&path, body).unwrap();
            assert!(read_managed_file(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), body);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_timestamps_and_overflows_are_rejected_without_panicking() {
        for stamp in [
            "00:01.NaN",
            "00:01.inf",
            "00:01.-1",
            "00:60.00",
            "18446744073709551615:00",
            "999999999999999999999:00",
            "00:01.1e2",
        ] {
            assert_eq!(parse_timestamp(stamp), None, "{stamp}");
            assert!(parse_lrc(&format!("[{stamp}] text")).is_empty());
        }
        assert_eq!(
            parse_timestamp("00:01.123456789123"),
            Some(Duration::new(1, 123456789))
        );
        let line = LrcLine {
            time: Duration::MAX,
            text: "edge".into(),
            words: vec![LyricWord {
                time: Duration::MAX,
                text: "edge".into(),
            }],
        };
        assert_eq!(word_progress_fraction(&line, 0, Duration::MAX, None), 1.0);
    }

    #[test]
    fn managed_key_is_the_same_for_relative_and_absolute_existing_paths() {
        let absolute = std::env::current_dir().unwrap().join("Cargo.toml");
        assert_eq!(
            managed_path_in(Path::new("/fixture"), Path::new("Cargo.toml")),
            managed_path_in(Path::new("/fixture"), &absolute)
        );
    }

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
    fn loader_resolves_sidecar_without_network() {
        let dir = std::env::temp_dir().join(format!("stillmusic-lyrics-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("track.mp3");
        fs::write(&audio, b"x").unwrap();
        fs::write(dir.join("track.lrc"), "[00:01.00] hi\n").unwrap();

        let mut loader = LyricsLoader::new();
        let query = LyricsQuery {
            path: audio.clone(),
            artist: "A".into(),
            title: "T".into(),
            album: String::new(),
            duration: None,
        };
        let state = loader.state("track", || query.clone());
        match state {
            LyricsState::Ready(r) => {
                assert_eq!(r.source, "sidecar");
                assert_eq!(r.lines.len(), 1);
            }
            other => panic!("expected sidecar hit, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn loader_skips_network_without_artist_or_title() {
        let mut loader = LyricsLoader::new();
        let query = LyricsQuery {
            path: std::path::PathBuf::from("/nonexistent/track.mp3"),
            artist: String::new(),
            title: String::new(),
            album: String::new(),
            duration: None,
        };
        assert_eq!(
            loader.state("k", || query.clone()),
            &LyricsState::Missing,
            "no lookup context must resolve immediately"
        );
    }

    #[test]
    fn lrclib_url_encodes_and_includes_optional_fields() {
        let url = lrclib_url("Daft Punk", "One More Time", "Discovery", Some(320.4));
        assert!(url.contains("artist_name=Daft%20Punk"));
        assert!(url.contains("track_name=One%20More%20Time"));
        assert!(url.contains("album_name=Discovery"));
        assert!(url.ends_with("&duration=320"));
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
    fn netease_pick_prefers_title_and_duration() {
        let body = r#"{"result":{"songs":[
            {"id":1,"name":"Other","duration":400000},
            {"id":2,"name":"One More Time","duration":320000},
            {"id":3,"name":"One More Time","duration":90000}
        ]}}"#;
        assert_eq!(pick_netease_id(body, "One More Time", Some(320.0)), Some(2));
    }

    #[test]
    fn ovh_body_splits_plain_lines() {
        let body = r#"{"lyrics":"\nline one\n\nline two\n"}"#;
        let lyrics = from_ovh_body(body);
        assert_eq!(lyrics.source, "ovh");
        assert_eq!(lyrics.plain, vec!["line one", "line two"]);
    }

    #[test]
    fn stored_body_keeps_provider_source() {
        let body = r#"{"source":"netease","syncedLyrics":"[00:01.00] hi\n","plainLyrics":""}"#;
        let lyrics = from_stored_body(body);
        assert_eq!(lyrics.source, "netease");
        assert_eq!(lyrics.lines[0].text, "hi");
    }

    #[test]
    fn search_pick_prefers_synced_near_duration() {
        let body = r#"[
            {"trackName":"A","duration":400,"syncedLyrics":null,"plainLyrics":"plain"},
            {"trackName":"B","duration":201,"syncedLyrics":"[00:01.00] hi\n","plainLyrics":"hi"},
            {"trackName":"C","duration":90,"syncedLyrics":"[00:01.00] other\n","plainLyrics":"other"}
        ]"#;
        let picked = pick_lrclib_search(body, Some(200.0)).unwrap();
        let r = from_lrclib_body(&picked);
        assert_eq!(r.lines[0].text, "hi");
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
