//! Interactive yt-dlp downloader — YouTube, YouTube Music, SoundCloud.
//!
//! Flow: provider → URL(s)/search → select → preset → options → download (cwd).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use crossterm::style::Stylize;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{DlFallbackMode, cache_dir};
use crate::ui::{BRIGHT, DIM, GRAY, print_info, print_success, print_warn};

/// Extra extractor-args value used for the 403/PO-token fallback retry.
pub const MWEB_FALLBACK_EXTRACTOR_ARG: &str = "youtube:player_client=mweb";

/// True when yt-dlp stderr/status looks like a YouTube 403 / SABR / PO-token
/// failure worth retrying with the mweb player client.
pub fn is_retryable_download_error(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "403",
        "forbidden",
        "po token",
        "po_token",
        "sabr",
        "requested format is not available",
        "unable to download video data",
    ];
    NEEDLES.iter().any(|n| lower.contains(n))
}

/// Append a second `--extractor-args youtube:player_client=mweb` pair.
/// yt-dlp accepts repeated `--extractor-args`, so we never merge with the
/// existing `youtube:skip=translated_subs` flag.
pub fn with_mweb_fallback(args: &[String]) -> Vec<String> {
    let mut out = args.to_vec();
    out.insert(out.len().saturating_sub(1), "--extractor-args".to_string());
    out.insert(
        out.len().saturating_sub(1),
        MWEB_FALLBACK_EXTRACTOR_ARG.to_string(),
    );
    out
}

/// CLI flags win over config: `--fallback-mweb` → Auto, `--no-fallback` → Off.
pub fn resolve_fallback_policy(
    fallback_mweb: bool,
    no_fallback: bool,
    cfg: DlFallbackMode,
) -> DlFallbackMode {
    if fallback_mweb {
        DlFallbackMode::Auto
    } else if no_fallback {
        DlFallbackMode::Off
    } else {
        cfg
    }
}

fn stdin_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Ask `[s/N]` on interactive TTYs. Returns true when the user opts in.
fn ask_mweb_retry() -> bool {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return false;
    }
    let mut stdout = io::stdout();
    let _ = write!(
        stdout,
        "  {} {} ",
        "?".with(DIM),
        "403/blocked? retry with mweb fallback? [y/N]".with(GRAY)
    );
    let _ = stdout.flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "s" | "sim" | "y" | "yes"
    )
}

pub(crate) const PAGE_SIZE: usize = 8;
const SEARCH_FETCH: usize = 40; // up to 5 pages
const CACHE_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60);

// ── Public types ────────────────────────────────────────────────

/// Supported download providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize)]
pub enum Provider {
    #[value(name = "youtube", aliases = ["yt", "y"])]
    Youtube,
    #[value(name = "youtube-music", aliases = ["ytm", "music", "ym"])]
    YoutubeMusic,
    #[value(name = "soundcloud", aliases = ["sc", "s"])]
    Soundcloud,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Youtube => "youtube",
            Self::YoutubeMusic => "youtube-music",
            Self::Soundcloud => "soundcloud",
        }
    }

    pub fn default_kind(self) -> MediaKind {
        match self {
            Self::Youtube => MediaKind::Video,
            Self::YoutubeMusic | Self::Soundcloud => MediaKind::Audio,
        }
    }

    /// Whether this platform primarily carries video (ask a/v).
    pub fn offers_video(self) -> bool {
        matches!(self, Self::Youtube | Self::YoutubeMusic)
    }

    fn search_expr(self, query: &str, n: usize) -> String {
        match self {
            Self::Youtube | Self::YoutubeMusic => format!("ytsearch{n}:{query}"),
            Self::Soundcloud => format!("scsearch{n}:{query}"),
        }
    }

    pub(crate) fn base_caps(self) -> Caps {
        match self {
            Self::Youtube => Caps {
                video: true,
                audio: true,
                subs: true,
                thumbnail: true,
                music_meta: true,
            },
            Self::YoutubeMusic => Caps {
                video: true,
                audio: true,
                subs: false,
                thumbnail: true,
                music_meta: true,
            },
            Self::Soundcloud => Caps {
                video: false,
                audio: true,
                subs: false,
                thumbnail: true,
                music_meta: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MediaKind {
    #[value(name = "audio", aliases = ["a"])]
    Audio,
    #[value(name = "video", aliases = ["v"])]
    Video,
    /// Separate audio file + separate video file
    #[value(name = "both", aliases = ["av", "b"])]
    Both,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Both => "audio + video",
        }
    }

    pub fn wants_audio(self) -> bool {
        matches!(self, Self::Audio | Self::Both)
    }

    pub fn wants_video(self) -> bool {
        matches!(self, Self::Video | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Best,
    Economy,
    Lower,
    Custom,
}

impl QualityPreset {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::Economy => "economy",
            Self::Lower => "lower",
            Self::Custom => "custom",
        }
    }
}

/// Capability intersection for selected items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub video: bool,
    pub audio: bool,
    pub subs: bool,
    pub thumbnail: bool,
    pub music_meta: bool,
}

impl Caps {
    fn all() -> Self {
        Self {
            video: true,
            audio: true,
            subs: true,
            thumbnail: true,
            music_meta: true,
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            video: self.video && other.video,
            audio: self.audio && other.audio,
            subs: self.subs && other.subs,
            thumbnail: self.thumbnail && other.thumbnail,
            music_meta: self.music_meta && other.music_meta,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub url: String,
    pub uploader: String,
    pub duration: Option<u64>,
    pub provider: Provider,
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub title: String,
    pub url: String,
    pub provider: Provider,
    pub caps: Caps,
}

/// Resolved download options (single item or batch).
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub kind: MediaKind,
    /// yt-dlp `-f` for video streams
    pub format_selector: String,
    pub container: String,
    pub audio_format: String,
    pub audio_quality: String,
    /// Cover art embedded into the media file
    pub embed_thumbnail: bool,
    /// Full metadata pack (tags, chapters, info-json)
    pub embed_metadata: bool,
    /// Download + embed subtitles into the video file (no loose .srt)
    pub embed_subs: bool,
    pub output_dir: PathBuf,
}

/// Simple direct-mode request (non-interactive).
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub query: String,
    pub provider: Provider,
    pub kind: MediaKind,
    pub output_dir: PathBuf,
    pub audio_format: String,
}

// ── Detect / parse ──────────────────────────────────────────────

pub fn detect_provider(input: &str) -> Option<Provider> {
    let lower = input.trim().to_ascii_lowercase();
    if lower.contains("music.youtube.com") {
        return Some(Provider::YoutubeMusic);
    }
    if lower.contains("youtube.com")
        || lower.contains("youtu.be")
        || lower.contains("youtube-nocookie.com")
    {
        return Some(Provider::Youtube);
    }
    if lower.contains("soundcloud.com") || lower.contains("snd.sc") {
        return Some(Provider::Soundcloud);
    }
    None
}

pub fn looks_like_url(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("www.")
        || t.contains("youtube.com/")
        || t.contains("youtu.be/")
        || t.contains("soundcloud.com/")
}

/// Split `url1;url2` (also accepts newlines). Empty parts dropped.
pub fn split_urls(input: &str) -> Vec<String> {
    input
        .split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with("www.") {
                format!("https://{s}")
            } else {
                s.to_string()
            }
        })
        .collect()
}

pub fn resolve_input(query: &str, provider: Provider) -> String {
    let q = query.trim();
    if looks_like_url(q) {
        if q.starts_with("www.") {
            return format!("https://{q}");
        }
        return q.to_string();
    }
    provider.search_expr(q, 1)
}

// ── yt-dlp helpers ──────────────────────────────────────────────

pub fn ensure_yt_dlp() -> Result<String> {
    which_bin("yt-dlp").or_else(|_| {
        bail!(
            "yt-dlp not found on PATH\n  \
             install: https://github.com/yt-dlp/yt-dlp#installation\n  \
             Arch: sudo pacman -S yt-dlp"
        )
    })
}

fn which_bin(name: &str) -> Result<String> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let dirs = std::env::var_os("PATH").context("PATH not set")?;
    for dir in std::env::split_paths(&dirs) {
        let base = dir.join(name);
        if base.is_file() {
            return Ok(base.to_string_lossy().into_owned());
        }
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
    }
    bail!("{name} not found")
}

fn ffmpeg_available() -> bool {
    which_bin("ffmpeg").is_ok()
}

fn run_yt_dlp_output(yt: &str, args: &[String]) -> Result<String> {
    let output = Command::new(yt)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to spawn {yt}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
        let start = lines.len().saturating_sub(6);
        let snippet = lines[start..].join("\n");
        bail!(
            "yt-dlp failed{}\n{}",
            output
                .status
                .code()
                .map(|c| format!(" (exit {c})"))
                .unwrap_or_default(),
            snippet
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// ── Search cache ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct SearchCacheFile {
    created_unix: u64,
    provider: Provider,
    query: String,
    results: Vec<SearchHit>,
}

fn cache_root() -> PathBuf {
    cache_dir().join("dl")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_key(provider: Provider, query: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    provider.label().hash(&mut h);
    query.trim().to_ascii_lowercase().hash(&mut h);
    format!("{}_{:016x}", provider.label(), h.finish())
}

/// Drop cache entries older than 3 days.
pub fn purge_expired_cache() {
    let root = cache_root();
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    let now = now_unix();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<SearchCacheFile>(&raw) else {
            let _ = fs::remove_file(&path);
            continue;
        };
        if now.saturating_sub(file.created_unix) > CACHE_TTL.as_secs() {
            let _ = fs::remove_file(&path);
        }
    }
}

fn load_search_cache(provider: Provider, query: &str) -> Option<Vec<SearchHit>> {
    purge_expired_cache();
    let path = cache_root().join(format!("{}.json", cache_key(provider, query)));
    let raw = fs::read_to_string(path).ok()?;
    let file: SearchCacheFile = serde_json::from_str(&raw).ok()?;
    if now_unix().saturating_sub(file.created_unix) > CACHE_TTL.as_secs() {
        return None;
    }
    if file.provider != provider {
        return None;
    }
    Some(file.results)
}

fn save_search_cache(provider: Provider, query: &str, results: &[SearchHit]) -> Result<()> {
    let root = cache_root();
    fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
    purge_expired_cache();
    let file = SearchCacheFile {
        created_unix: now_unix(),
        provider,
        query: query.to_string(),
        results: results.to_vec(),
    };
    let path = root.join(format!("{}.json", cache_key(provider, query)));
    let body = serde_json::to_string_pretty(&file).context("serialize search cache")?;
    option_sdk::atomic_write(&path, body.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

// ── Search / probe ──────────────────────────────────────────────

fn parse_hit(v: &Value, provider: Provider) -> Option<SearchHit> {
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("(untitled)")
        .to_string();
    let url = v
        .get("webpage_url")
        .or_else(|| v.get("url"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            if id.is_empty() {
                None
            } else {
                Some(match provider {
                    Provider::Soundcloud => format!("https://api.soundcloud.com/tracks/{id}"),
                    Provider::Youtube | Provider::YoutubeMusic => {
                        format!("https://www.youtube.com/watch?v={id}")
                    }
                })
            }
        })?;
    if title.is_empty() && id.is_empty() {
        return None;
    }
    let uploader = v
        .get("uploader")
        .or_else(|| v.get("channel"))
        .and_then(|x| x.as_str())
        .unwrap_or("—")
        .to_string();
    let duration = v
        .get("duration")
        .and_then(|x| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)));
    Some(SearchHit {
        id,
        title,
        url,
        uploader,
        duration,
        provider,
    })
}

pub fn search(provider: Provider, query: &str) -> Result<Vec<SearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        bail!("empty search query");
    }
    if let Some(cached) = load_search_cache(provider, q) {
        print_info("search cache hit");
        return Ok(cached);
    }

    let yt = ensure_yt_dlp()?;
    let expr = provider.search_expr(q, SEARCH_FETCH);

    let args = vec![
        "--flat-playlist".into(),
        "--skip-download".into(),
        "--dump-json".into(),
        "--color".into(),
        "never".into(),
        expr,
    ];
    let stdout = run_yt_dlp_output(&yt, &args)?;
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Nested playlist entries
        if let Some(entries) = v.get("entries").and_then(|e| e.as_array()) {
            for e in entries {
                if let Some(hit) = parse_hit(e, provider)
                    && seen.insert(hit.url.clone())
                {
                    results.push(hit);
                }
            }
            continue;
        }
        if let Some(hit) = parse_hit(&v, provider)
            && seen.insert(hit.url.clone())
        {
            results.push(hit);
        }
    }

    if results.is_empty() {
        bail!("no results for “{q}” on {}", provider.label());
    }

    let _ = save_search_cache(provider, q, &results);
    Ok(results)
}

fn json_has_subs(v: &Value) -> bool {
    let nonempty = |key: &str| {
        v.get(key)
            .and_then(|s| s.as_object())
            .is_some_and(|o| !o.is_empty())
    };
    nonempty("subtitles") || nonempty("automatic_captions")
}

fn probe_caps(yt: &str, url: &str, fallback: Caps) -> Caps {
    let args = vec![
        "--skip-download".into(),
        "-J".into(),
        "--color".into(),
        "never".into(),
        "--no-warnings".into(),
        url.to_string(),
    ];
    let Ok(stdout) = run_yt_dlp_output(yt, &args) else {
        return fallback;
    };
    let Ok(v) = serde_json::from_str::<Value>(&stdout) else {
        return fallback;
    };

    let mut caps = fallback;
    // SoundCloud / audio-only: no real video stream
    if let Some(formats) = v.get("formats").and_then(|f| f.as_array()) {
        let has_video = formats.iter().any(|f| {
            f.get("vcodec")
                .and_then(|c| c.as_str())
                .is_some_and(|c| c != "none")
        });
        if !has_video {
            caps.video = false;
        }
    }
    caps.subs = fallback.subs && json_has_subs(&v);
    if v.get("thumbnail").and_then(|t| t.as_str()).is_none()
        && v.get("thumbnails")
            .and_then(|t| t.as_array())
            .is_none_or(|a| a.is_empty())
    {
        caps.thumbnail = false;
    }
    caps
}

pub(crate) fn probe_items(items: &mut [MediaItem]) -> Result<()> {
    let yt = ensure_yt_dlp()?;
    for item in items.iter_mut() {
        item.caps = probe_caps(&yt, &item.url, item.provider.base_caps());
    }
    Ok(())
}

fn preview_cache_dir() -> PathBuf {
    cache_dir().join("preview")
}

/// Download best audio (no remux) into the preview cache; return the local file path.
///
/// Quiet by design — callers show a spinner. Skips `-x` / format convert so mpv
/// can play the native stream (webm/m4a/opus) as soon as yt-dlp finishes.
pub fn fetch_preview_audio(url: &str) -> Result<PathBuf> {
    let yt = ensure_yt_dlp()?;
    let dir = preview_cache_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;

    // Wipe previous previews so the directory stays small.
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    let template = dir.join("%(id)s.%(ext)s");
    let args = [
        "--quiet",
        "--no-warnings",
        "--no-progress",
        "--color",
        "never",
        "-f",
        "bestaudio/best",
        "--no-playlist",
        "--no-mtime",
        "-o",
        template.to_str().context("preview path utf-8")?,
        url,
    ];

    let status = Command::new(&yt)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to spawn {yt}"))?;
    if !status.success() {
        bail!(
            "preview download failed (yt-dlp {})",
            status.code().unwrap_or(-1)
        );
    }

    let mut found: Option<PathBuf> = None;
    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() {
            found = Some(path);
            break;
        }
    }
    found.with_context(|| format!("no preview file written in {}", dir.display()))
}

pub(crate) fn intersect_caps(items: &[MediaItem]) -> Caps {
    items
        .iter()
        .map(|i| i.caps)
        .reduce(Caps::intersect)
        .unwrap_or_else(Caps::all)
}

// ── Build args / download ───────────────────────────────────────

#[derive(Clone, Copy)]
enum Pass {
    Audio,
    Video,
}

fn push_embed_flags(args: &mut Vec<String>, opts: &DownloadOptions, pass: Pass) {
    if opts.embed_thumbnail {
        args.push("--embed-thumbnail".into());
        args.push("--convert-thumbnails".into());
        args.push("jpg".into());
    }
    if opts.embed_metadata {
        // Full pack: tags + chapters + infojson attachment
        args.push("--embed-metadata".into());
        args.push("--embed-chapters".into());
        args.push("--embed-info-json".into());
    }
    if opts.embed_subs && matches!(pass, Pass::Video) {
        // Never use "all" — YouTube auto-subs explode into 100+ langs and 429.
        // Prefer a small set; ignore errors so a failed sub never kills the video.
        args.push("--write-subs".into());
        args.push("--write-auto-subs".into());
        args.push("--embed-subs".into());
        args.push("--sub-langs".into());
        args.push("en.*,pt.*,pt-BR,es.*,-live_chat".into());
        args.push("--extractor-args".into());
        args.push("youtube:skip=translated_subs".into());
        args.push("--ignore-errors".into());
    }
}

/// Build yt-dlp args for one pass (audio extract or video merge).
pub fn build_args_for_url(url: &str, opts: &DownloadOptions) -> Vec<String> {
    // Default helper used by tests / single-kind requests
    let pass = if opts.kind.wants_video() && !opts.kind.wants_audio() {
        Pass::Video
    } else if opts.kind.wants_audio() && !opts.kind.wants_video() {
        Pass::Audio
    } else {
        Pass::Video
    };
    build_args_pass(url, opts, pass)
}

fn build_args_pass(url: &str, opts: &DownloadOptions, pass: Pass) -> Vec<String> {
    let template = match pass {
        Pass::Audio => "%(title).200B [%(id)s].%(ext)s",
        Pass::Video => "%(title).200B [%(id)s].%(ext)s",
    };
    let out_path = opts
        .output_dir
        .join(template)
        .to_string_lossy()
        .into_owned();

    let mut args = vec![
        "--newline".into(),
        "--color".into(),
        "never".into(),
        "-o".into(),
        out_path,
        "--no-mtime".into(),
    ];

    match pass {
        Pass::Audio => {
            args.push("-f".into());
            args.push("bestaudio/best".into());
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push(opts.audio_format.clone());
            args.push("--audio-quality".into());
            args.push(opts.audio_quality.clone());
        }
        Pass::Video => {
            args.push("-f".into());
            args.push(opts.format_selector.clone());
            args.push("--merge-output-format".into());
            args.push(opts.container.clone());
        }
    }

    push_embed_flags(&mut args, opts, pass);
    args.push(url.to_string());
    args
}

/// Legacy helper used by direct mode / tests.
pub fn build_args(req: &DownloadRequest) -> Vec<String> {
    let input = resolve_input(&req.query, req.provider);
    let opts = DownloadOptions {
        kind: req.kind,
        format_selector: "bv*+ba/b".into(),
        container: "mp4".into(),
        audio_format: req.audio_format.clone(),
        audio_quality: "0".into(),
        embed_thumbnail: false,
        embed_metadata: true,
        embed_subs: false,
        output_dir: req.output_dir.clone(),
    };
    build_args_for_url(&input, &opts)
}

fn run_one_pass(
    yt: &str,
    url: &str,
    opts: &DownloadOptions,
    pass: Pass,
    fallback: DlFallbackMode,
) -> Result<()> {
    let args = build_args_pass(url, opts, pass);
    match run_captured(yt, &args) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            if !is_retryable_download_error(&first_err) {
                bail!("{first_err}");
            }
            match fallback {
                DlFallbackMode::Off => {
                    print_warn(
                        "retryable 403/block detected (fallback off: --fallback-mweb or dl_fallback=auto)",
                    );
                    bail!("{first_err}");
                }
                DlFallbackMode::Auto => {
                    print_info("retry      trying mweb fallback (auto)…");
                }
                DlFallbackMode::Ask => {
                    if !stdin_is_tty() {
                        print_warn(
                            "retryable 403/block detected (non-TTY: use --fallback-mweb to retry)",
                        );
                        bail!("{first_err}");
                    }
                    if !ask_mweb_retry() {
                        bail!("{first_err}");
                    }
                }
            }
            let retry_args = with_mweb_fallback(&args);
            print_info("retry      yt-dlp with youtube:player_client=mweb…");
            match run_captured(yt, &retry_args) {
                Ok(()) => {
                    print_success("fallback mweb ok");
                    Ok(())
                }
                Err(retry_err) => {
                    bail!("{first_err}\nfallback mweb also failed: {retry_err}");
                }
            }
        }
    }
}

/// Spawn yt-dlp with piped output, forward it to the inherited stdio so
/// `--newline` progress still shows, and return the combined output text
/// on failure for retryable-error detection.
fn run_captured(yt: &str, args: &[String]) -> std::result::Result<(), String> {
    let output = Command::new(yt)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to spawn {yt}: {e:#}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
        let _ = io::stdout().flush();
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
        let _ = io::stderr().flush();
    }
    if output.status.success() {
        return Ok(());
    }
    let code = output.status.code().unwrap_or(-1);
    let combined = format!("yt-dlp exited with status {code}\n{stdout}\n{stderr}");
    Err(combined)
}

pub fn run_batch(
    items: &[MediaItem],
    opts: &DownloadOptions,
    fallback: DlFallbackMode,
) -> Result<()> {
    let yt = ensure_yt_dlp()?;
    if opts.kind.wants_audio() && !ffmpeg_available() {
        print_warn("ffmpeg not found — audio extract/convert may fail");
    }
    if (opts.embed_thumbnail || opts.embed_subs || opts.embed_metadata) && !ffmpeg_available() {
        print_warn("ffmpeg not found — embed may fail");
    }

    fs::create_dir_all(&opts.output_dir).with_context(|| {
        format!(
            "cannot create output directory {}",
            opts.output_dir.display()
        )
    })?;

    print_info(&format!("items     {}", items.len()));
    print_info(&format!("kind      {}", opts.kind.label()));
    print_info(&format!("output    {}", opts.output_dir.display()));
    println!();

    let mut failed = 0usize;
    for (i, item) in items.iter().enumerate() {
        println!(
            "  {} [{}/{}] {}",
            "↓".with(BRIGHT),
            i + 1,
            items.len(),
            item.title.as_str().with(GRAY)
        );

        let mut ok = true;
        if opts.kind.wants_video() {
            print_info("pass      video");
            if let Err(e) = run_one_pass(&yt, &item.url, opts, Pass::Video, fallback) {
                print_warn(&format!("video failed: {e:#}"));
                ok = false;
            }
        }
        if opts.kind.wants_audio() {
            print_info("pass      audio");
            if let Err(e) = run_one_pass(&yt, &item.url, opts, Pass::Audio, fallback) {
                print_warn(&format!("audio failed: {e:#}"));
                ok = false;
            }
        }
        if !ok {
            failed += 1;
            print_warn(&format!("failed: {}", item.title));
        }
        println!();
    }

    if failed > 0 {
        bail!("{failed}/{} download(s) failed", items.len());
    }
    print_success(&format!(
        "downloaded {} → {}",
        items.len(),
        opts.output_dir.display()
    ));
    Ok(())
}

pub fn run_download(req: &DownloadRequest, fallback: DlFallbackMode) -> Result<()> {
    let url = resolve_input(&req.query, req.provider);
    let item = MediaItem {
        title: url.clone(),
        url,
        provider: req.provider,
        caps: req.provider.base_caps(),
    };
    let opts = DownloadOptions {
        kind: req.kind,
        format_selector: "bv*+ba/b".into(),
        container: "mp4".into(),
        audio_format: req.audio_format.clone(),
        audio_quality: "0".into(),
        embed_thumbnail: false,
        embed_metadata: true,
        embed_subs: false,
        output_dir: req.output_dir.clone(),
    };
    run_batch(std::slice::from_ref(&item), &opts, fallback)
}

/// Output dir: explicit → else cwd. (`music_dir_flag` kept for CLI `-m` override).
pub fn resolve_output_dir(explicit: Option<&Path>, music_dir_flag: &str) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return validate_out_dir(expand_tilde_path(p));
    }
    if !music_dir_flag.is_empty() {
        return validate_out_dir(expand_tilde_path(Path::new(music_dir_flag)));
    }
    Ok(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn validate_out_dir(path: PathBuf) -> Result<PathBuf> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("output directory must not contain '..' components");
    }
    Ok(path)
}

fn expand_tilde_path(p: &Path) -> PathBuf {
    option_sdk::expand_tilde(p)
}

// ── Interactive wizard ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn run_interactive(
    music_dir_flag: &str,
    prefill_query: Option<&str>,
    prefill_provider: Option<Provider>,
    prefill_kind: Option<MediaKind>,
    prefill_output: Option<&Path>,
    audio_format: &str,
    ui_mode: crate::config::DlUiMode,
    fallback: DlFallbackMode,
) -> Result<()> {
    match ui_mode {
        crate::config::DlUiMode::Arrows => crate::dl_ui::run_interactive_arrows(
            music_dir_flag,
            prefill_query,
            prefill_provider,
            prefill_kind,
            prefill_output,
            audio_format,
            fallback,
        ),
        crate::config::DlUiMode::Type => run_interactive_type(
            music_dir_flag,
            prefill_query,
            prefill_provider,
            prefill_kind,
            prefill_output,
            audio_format,
            fallback,
        ),
    }
}

fn run_interactive_type(
    music_dir_flag: &str,
    prefill_query: Option<&str>,
    prefill_provider: Option<Provider>,
    prefill_kind: Option<MediaKind>,
    prefill_output: Option<&Path>,
    _audio_format: &str,
    fallback: DlFallbackMode,
) -> Result<()> {
    ensure_yt_dlp()?;
    purge_expired_cache();

    println!("  {} {}", "♪".with(BRIGHT), "download".with(BRIGHT).bold());
    println!(
        "  {}",
        "youtube · youtube-music · soundcloud  ·  yt-dlp · type".with(DIM)
    );
    println!();

    // 1) Provider first
    let provider = match prefill_provider {
        Some(p) => {
            print_info(&format!("provider  {}", p.label()));
            p
        }
        None => ask_provider()?,
    };

    // 2) Query / URL(s)
    let raw = match prefill_query {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => {
            let q = prompt("search or url(s)  ·  urls: url1;url2", None)?;
            if q.is_empty() {
                bail!("empty input — nothing to download");
            }
            q
        }
    };

    let mut items = if input_is_urls(&raw) {
        items_from_urls(&raw, provider)?
    } else {
        browse_search(provider, &raw)?
    };

    if items.is_empty() {
        bail!("nothing selected");
    }

    if items.len() == 1
        && ask_yes_no(
            &format!(
                "preview “{}” in optionMusic first?",
                truncate(&items[0].title, 40)
            ),
            true,
        )?
        && let Err(e) = crate::preview::fetch_and_play(&items[0].url)
    {
        print_warn(&format!("preview: {e:#}"));
        println!();
    }

    // Scan capabilities (subs / video presence)
    probe_items(&mut items)?;
    let caps = intersect_caps(&items);
    print_info(&format!(
        "selected  {}  ·  caps video={} subs={} thumb={}",
        items.len(),
        yn(caps.video),
        yn(caps.subs),
        yn(caps.thumbnail)
    ));
    println!();

    // 3) Audio / video (only if platform + caps allow video)
    let kind = match prefill_kind {
        Some(k) => {
            if k.wants_video() && !caps.video {
                print_warn("video not available for selection — using audio");
                MediaKind::Audio
            } else {
                k
            }
        }
        None => ask_kind(provider, caps)?,
    };

    // 4) Preset
    let preset = ask_preset()?;

    // 5) Options (intersection-aware)
    let mut opts = options_from_preset(preset, kind, caps);
    if preset == QualityPreset::Custom || ask_yes_no("tweak options?", false)? {
        opts = refine_options(opts, kind, caps)?;
    } else if kind.wants_video() && caps.subs && opts.embed_subs {
        print_info("subtitles will be downloaded and embedded into the video");
    }

    // 6) Output dir — cwd default
    opts.output_dir = match prefill_output {
        Some(p) => resolve_output_dir(Some(p), music_dir_flag)?,
        None => ask_output_dir(music_dir_flag)?,
    };

    println!();
    summarize(&items, &opts);
    if !ask_yes_no("start download?", true)? {
        bail!("cancelled");
    }
    println!();
    run_batch(&items, &opts, fallback)
}

pub(crate) fn yn(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

pub(crate) fn input_is_urls(raw: &str) -> bool {
    let parts = split_urls(raw);
    !parts.is_empty() && parts.iter().all(|p| looks_like_url(p))
}

pub(crate) fn items_from_urls(raw: &str, session_provider: Provider) -> Result<Vec<MediaItem>> {
    let urls = split_urls(raw);
    if urls.is_empty() {
        bail!("no urls");
    }
    let mut items = Vec::with_capacity(urls.len());
    for url in urls {
        let provider = detect_provider(&url).unwrap_or(session_provider);
        let title = url.clone();
        items.push(MediaItem {
            title,
            url,
            provider,
            caps: provider.base_caps(),
        });
    }
    print_info(&format!("urls      {}", items.len()));
    Ok(items)
}

fn ask_provider() -> Result<Provider> {
    let choice = prompt(
        "provider  [1 youtube · 2 youtube-music · 3 soundcloud]",
        Some("1"),
    )?;
    Ok(match choice.trim() {
        "2" | "ytm" | "music" | "youtube-music" | "ym" => Provider::YoutubeMusic,
        "3" | "sc" | "s" | "soundcloud" => Provider::Soundcloud,
        _ => Provider::Youtube,
    })
}

fn ask_kind(provider: Provider, caps: Caps) -> Result<MediaKind> {
    if !caps.video || !provider.offers_video() {
        print_info("what      audio only (this source has no video stream)");
        return Ok(MediaKind::Audio);
    }
    let def = provider.default_kind();
    let def_s = match def {
        MediaKind::Audio => "a",
        MediaKind::Both => "b",
        MediaKind::Video => "v",
    };
    let choice = prompt(
        "what to download  [a audio only · v video only · b both]",
        Some(def_s),
    )?;
    Ok(match choice.trim().to_ascii_lowercase().as_str() {
        "a" | "audio" => MediaKind::Audio,
        "b" | "both" | "av" => MediaKind::Both,
        "v" | "video" => MediaKind::Video,
        _ => def,
    })
}

fn ask_preset() -> Result<QualityPreset> {
    let choice = prompt(
        "preset    [1 best · 2 economy · 3 lower · 4 custom]",
        Some("1"),
    )?;
    Ok(match choice.trim() {
        "2" | "economy" | "eco" => QualityPreset::Economy,
        "3" | "lower" | "low" => QualityPreset::Lower,
        "4" | "custom" | "c" => QualityPreset::Custom,
        _ => QualityPreset::Best,
    })
}

pub(crate) fn video_format_for_quality(q: &str) -> &'static str {
    match q {
        "1080" => "bv*[height<=1080]+ba/b",
        "720" => "bv*[height<=720]+ba/b",
        "480" => "bv*[height<=480]+ba/b",
        _ => "bv*+ba/b",
    }
}

pub(crate) fn options_from_preset(
    preset: QualityPreset,
    kind: MediaKind,
    caps: Caps,
) -> DownloadOptions {
    let (format_selector, container, audio_format, audio_quality) = match preset {
        QualityPreset::Best | QualityPreset::Custom => {
            ("bv*+ba/b".into(), "mp4".into(), "m4a".into(), "0".into())
        }
        QualityPreset::Economy => (
            "bv*[height<=720]+ba/b".into(),
            "mp4".into(),
            "mp3".into(),
            "5".into(),
        ),
        QualityPreset::Lower => (
            "bv*[height<=480]+ba/b".into(),
            "mp4".into(),
            "mp3".into(),
            "7".into(),
        ),
    };

    DownloadOptions {
        kind,
        format_selector,
        container,
        audio_format,
        audio_quality,
        embed_thumbnail: caps.thumbnail
            && matches!(preset, QualityPreset::Best | QualityPreset::Custom),
        embed_metadata: caps.music_meta && preset != QualityPreset::Lower,
        embed_subs: kind.wants_video()
            && caps.subs
            && matches!(preset, QualityPreset::Best | QualityPreset::Custom),
        output_dir: PathBuf::from("."),
    }
}

fn refine_options(
    mut opts: DownloadOptions,
    kind: MediaKind,
    caps: Caps,
) -> Result<DownloadOptions> {
    println!();
    print_info("customize — only options available for every selected item");

    if kind.wants_video() {
        let q = prompt(
            "video quality  [1 best/original · 2 1080p · 3 720p · 4 480p]",
            Some("1"),
        )?;
        opts.format_selector = video_format_for_quality(match q.trim() {
            "2" | "1080" => "1080",
            "3" | "720" => "720",
            "4" | "480" => "480",
            _ => "best",
        })
        .into();
        let c = prompt("video container  [mp4 · webm · mkv]", Some(&opts.container))?;
        let c = c.trim().to_ascii_lowercase();
        if matches!(c.as_str(), "mp4" | "webm" | "mkv") {
            opts.container = c;
        }
    }

    if kind.wants_audio() {
        let f = prompt(
            "audio file type  [mp3 · m4a · opus · flac · best]",
            Some(&opts.audio_format),
        )?;
        let f = f.trim().to_ascii_lowercase();
        if matches!(f.as_str(), "mp3" | "m4a" | "opus" | "flac" | "best" | "wav") {
            opts.audio_format = f;
        }
        let aq = prompt(
            "audio quality  [0 = best … 10 = smallest]",
            Some(&opts.audio_quality),
        )?;
        if aq.trim().parse::<u8>().is_ok() {
            opts.audio_quality = aq.trim().to_string();
        }
    }

    if caps.music_meta {
        opts.embed_metadata = ask_yes_no(
            "embed full metadata (title, artists, album, chapters, info-json)?",
            opts.embed_metadata,
        )?;
    } else {
        opts.embed_metadata = false;
    }

    if caps.thumbnail {
        opts.embed_thumbnail = ask_yes_no(
            "embed thumbnail as cover art inside the file?",
            opts.embed_thumbnail,
        )?;
    } else {
        opts.embed_thumbnail = false;
    }

    if kind.wants_video() && caps.subs {
        opts.embed_subs = ask_yes_no(
            "download & embed subtitles (en/pt/es · into the video, no .srt files)?",
            opts.embed_subs,
        )?;
    } else {
        opts.embed_subs = false;
        if kind.wants_video() {
            print_info("subtitles  unavailable for this selection");
        }
    }

    Ok(opts)
}

fn ask_output_dir(music_dir_flag: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    print_info(&format!("output    {} (cwd)", cwd.display()));
    if ask_yes_no("use another directory?", false)? {
        let def = if !music_dir_flag.is_empty() {
            music_dir_flag.to_string()
        } else {
            String::new()
        };
        let p = prompt("directory", if def.is_empty() { None } else { Some(&def) })?;
        if p.trim().is_empty() {
            return Ok(cwd);
        }
        return resolve_output_dir(Some(Path::new(p.trim())), "");
    }
    Ok(cwd)
}

pub(crate) fn summarize(items: &[MediaItem], opts: &DownloadOptions) {
    println!("  {}", "── download plan ──".with(BRIGHT).bold());
    for (i, it) in items.iter().enumerate() {
        println!(
            "  {}  {}",
            format!("{:>2}", i + 1).with(DIM),
            truncate(&it.title, 56).with(BRIGHT)
        );
    }
    print_info(&format!("download     {}", opts.kind.label()));
    if opts.kind.wants_video() {
        print_info(&format!(
            "video        {}  →  .{}",
            opts.format_selector, opts.container
        ));
    }
    if opts.kind.wants_audio() {
        print_info(&format!(
            "audio        .{}  (quality {})",
            opts.audio_format, opts.audio_quality
        ));
    }
    print_info(&format!(
        "embed        metadata={}  thumbnail={}  subtitles={}",
        yn(opts.embed_metadata),
        yn(opts.embed_thumbnail),
        yn(opts.embed_subs)
    ));
    print_info(&format!("save to      {}", opts.output_dir.display()));
    println!();
}

fn browse_search(provider: Provider, query: &str) -> Result<Vec<MediaItem>> {
    let results = search(provider, query)?;
    let pages = results.len().div_ceil(PAGE_SIZE).max(1);
    let mut page = 0usize;
    let mut selected: HashMap<String, SearchHit> = HashMap::new();

    loop {
        let start = page * PAGE_SIZE;
        let end = (start + PAGE_SIZE).min(results.len());
        let slice = &results[start..end];

        println!();
        println!(
            "  {}  {}  {}  {}",
            "results".with(BRIGHT).bold(),
            format!("{}/{}", page + 1, pages).with(DIM),
            "·".with(DIM),
            format!("{} selected", selected.len()).with(GRAY)
        );
        for (i, hit) in slice.iter().enumerate() {
            let n = i + 1;
            let mark = if selected.contains_key(&hit.url) {
                "●"
            } else {
                " "
            };
            let dur = hit.duration.map(fmt_secs).unwrap_or_else(|| "--:--".into());
            println!(
                "  {} {}  {}  {}  {}",
                format!("{n}").with(DIM),
                mark.with(BRIGHT),
                truncate(&hit.title, 42).with(BRIGHT),
                dur.with(DIM),
                truncate(&hit.uploader, 16).with(GRAY)
            );
        }
        println!(
            "  {}",
            "[1-8] toggle · n next · p prev · a all-page · d done · q quit".with(DIM)
        );

        let cmd = prompt("select", None)?;
        let cmd = cmd.trim().to_ascii_lowercase();
        if cmd.is_empty() {
            continue;
        }
        if cmd == "q" || cmd == "quit" {
            bail!("cancelled");
        }
        if cmd == "d" || cmd == "done" || cmd == "ok" {
            if selected.is_empty() {
                print_warn("select at least one result");
                continue;
            }
            break;
        }
        if cmd == "n" || cmd == "next" {
            if page + 1 < pages {
                page += 1;
            } else {
                print_warn("last page");
            }
            continue;
        }
        if cmd == "p" || cmd == "prev" {
            if page > 0 {
                page -= 1;
            } else {
                print_warn("first page");
            }
            continue;
        }
        if cmd == "a" || cmd == "all" {
            for hit in slice {
                selected.insert(hit.url.clone(), hit.clone());
            }
            continue;
        }

        // "1", "1,3,5", "1 3"
        let tokens: Vec<&str> = cmd
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .collect();
        let mut handled = false;
        for tok in tokens {
            if let Ok(n) = tok.parse::<usize>()
                && (1..=slice.len()).contains(&n)
            {
                let hit = &slice[n - 1];
                if selected.remove(&hit.url).is_none() {
                    selected.insert(hit.url.clone(), hit.clone());
                }
                handled = true;
            }
        }
        if !handled {
            print_warn("unknown command");
        }
    }

    let items: Vec<MediaItem> = selected
        .into_values()
        .map(|h| MediaItem {
            title: h.title,
            url: h.url,
            provider: h.provider,
            caps: h.provider.base_caps(),
        })
        .collect();
    Ok(items)
}

pub(crate) fn fmt_secs(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m >= 60 {
        let h = m / 60;
        let m = m % 60;
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

fn prompt(label: &str, default: Option<&str>) -> Result<String> {
    let mut stdout = io::stdout();
    match default {
        Some(d) => write!(
            stdout,
            "  {} {} {} ",
            "?".with(DIM),
            label.with(GRAY),
            format!("[{d}]").with(DIM)
        )?,
        None => write!(stdout, "  {} {} ", "?".with(DIM), label.with(GRAY))?,
    }
    stdout.flush()?;

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .context("failed to read stdin")?;
    let trimmed = line.trim().to_string();
    if trimmed.is_empty()
        && let Some(d) = default
    {
        return Ok(d.to_string());
    }
    Ok(trimmed)
}

fn ask_yes_no(label: &str, default: bool) -> Result<bool> {
    let def = if default { "y" } else { "n" };
    let ans = prompt(&format!("{label} [y/n]"), Some(def))?;
    Ok(match ans.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_youtube() {
        assert_eq!(
            detect_provider("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some(Provider::Youtube)
        );
        assert_eq!(
            detect_provider("https://youtu.be/dQw4w9WgXcQ"),
            Some(Provider::Youtube)
        );
    }

    #[test]
    fn detect_youtube_music() {
        assert_eq!(
            detect_provider("https://music.youtube.com/watch?v=abc"),
            Some(Provider::YoutubeMusic)
        );
    }

    #[test]
    fn detect_soundcloud() {
        assert_eq!(
            detect_provider("https://soundcloud.com/artist/track"),
            Some(Provider::Soundcloud)
        );
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_provider("never gonna give you up"), None);
    }

    #[test]
    fn split_urls_semicolon() {
        let u = split_urls("https://youtu.be/a ; https://youtu.be/b");
        assert_eq!(u.len(), 2);
        assert!(u[0].contains("youtu.be/a"));
        assert!(u[1].contains("youtu.be/b"));
    }

    #[test]
    fn input_urls_vs_search() {
        assert!(input_is_urls("https://youtu.be/a;https://youtu.be/b"));
        assert!(!input_is_urls("lofi hip hop"));
    }

    #[test]
    fn resolve_url_passthrough() {
        let u = "https://www.youtube.com/watch?v=abc";
        assert_eq!(resolve_input(u, Provider::Youtube), u);
    }

    #[test]
    fn resolve_search_prefixes() {
        assert_eq!(
            resolve_input("lofi hip hop", Provider::Youtube),
            "ytsearch1:lofi hip hop"
        );
        assert_eq!(
            resolve_input("ambient", Provider::Soundcloud),
            "scsearch1:ambient"
        );
    }

    #[test]
    fn resolve_output_rejects_parent_components() {
        let result = resolve_output_dir(Some(Path::new("downloads/../outside")), "");
        assert!(result.is_err());
    }

    #[test]
    fn build_args_audio_contains_extract() {
        let req = DownloadRequest {
            query: "https://youtu.be/abc".into(),
            provider: Provider::Youtube,
            kind: MediaKind::Audio,
            output_dir: PathBuf::from("/tmp/music"),
            audio_format: "mp3".into(),
        };
        let args = build_args(&req);
        assert!(args.iter().any(|a| a == "-x"));
        assert!(args.iter().any(|a| a == "mp3"));
        assert!(args.iter().any(|a| a.contains("youtu.be/abc")));
    }

    #[test]
    fn build_args_video_merge_mp4() {
        let req = DownloadRequest {
            query: "track name".into(),
            provider: Provider::Soundcloud,
            kind: MediaKind::Video,
            output_dir: PathBuf::from("/tmp"),
            audio_format: "mp3".into(),
        };
        let args = build_args(&req);
        assert!(args.iter().any(|a| a == "--merge-output-format"));
        assert!(args.iter().any(|a| a == "mp4"));
        assert!(args.iter().any(|a| a == "scsearch1:track name"));
    }

    #[test]
    fn provider_defaults() {
        assert_eq!(Provider::Youtube.default_kind(), MediaKind::Video);
        assert_eq!(Provider::YoutubeMusic.default_kind(), MediaKind::Audio);
        assert_eq!(Provider::Soundcloud.default_kind(), MediaKind::Audio);
    }

    #[test]
    fn caps_intersect() {
        let a = Caps {
            video: true,
            audio: true,
            subs: true,
            thumbnail: true,
            music_meta: true,
        };
        let b = Caps {
            video: false,
            audio: true,
            subs: false,
            thumbnail: true,
            music_meta: true,
        };
        let i = a.intersect(b);
        assert!(!i.video);
        assert!(!i.subs);
        assert!(i.audio);
    }

    #[test]
    fn preset_best_embeds_subs_when_available() {
        let caps = Provider::Youtube.base_caps();
        let opts = options_from_preset(QualityPreset::Best, MediaKind::Video, caps);
        assert!(opts.embed_subs);
        assert!(opts.embed_thumbnail);
        assert!(opts.embed_metadata);
    }

    #[test]
    fn preset_soundcloud_no_subs() {
        let caps = Provider::Soundcloud.base_caps();
        let opts = options_from_preset(QualityPreset::Best, MediaKind::Audio, caps);
        assert!(!opts.embed_subs);
        assert!(!caps.video);
    }

    #[test]
    fn build_args_embeds_full_metadata_and_subs() {
        let opts = DownloadOptions {
            kind: MediaKind::Video,
            format_selector: "bv*+ba/b".into(),
            container: "mp4".into(),
            audio_format: "m4a".into(),
            audio_quality: "0".into(),
            embed_thumbnail: true,
            embed_metadata: true,
            embed_subs: true,
            output_dir: PathBuf::from("/tmp"),
        };
        let args = build_args_for_url("https://youtu.be/abc", &opts);
        assert!(args.iter().any(|a| a == "--embed-subs"));
        assert!(args.iter().any(|a| a == "--ignore-errors"));
        assert!(args.iter().any(|a| a.contains("en.*")));
        assert!(!args.iter().any(|a| a == "all,-live_chat"));
        assert!(args.iter().any(|a| a == "--embed-metadata"));
        assert!(args.iter().any(|a| a == "--embed-chapters"));
        assert!(args.iter().any(|a| a == "--embed-info-json"));
        assert!(args.iter().any(|a| a == "--embed-thumbnail"));
    }

    #[test]
    fn both_kind_wants_audio_and_video() {
        assert!(MediaKind::Both.wants_audio());
        assert!(MediaKind::Both.wants_video());
    }

    #[test]
    fn retryable_errors_detected() {
        assert!(is_retryable_download_error("ERROR: 403 Forbidden"));
        assert!(is_retryable_download_error(
            "Sign in to confirm… PO Token missing"
        ));
        assert!(is_retryable_download_error("po_token refresh failed"));
        assert!(is_retryable_download_error("SABR streaming failed"));
        assert!(is_retryable_download_error(
            "ERROR: Requested format is not available"
        ));
        assert!(is_retryable_download_error(
            "ERROR: unable to download video data"
        ));
        assert!(!is_retryable_download_error(
            "ERROR: Video unavailable — private video"
        ));
        assert!(!is_retryable_download_error("yt-dlp exited with status 1"));
    }

    #[test]
    fn mweb_fallback_appends_repeated_extractor_args() {
        let opts = DownloadOptions {
            kind: MediaKind::Video,
            format_selector: "bv*+ba/b".into(),
            container: "mp4".into(),
            audio_format: "m4a".into(),
            audio_quality: "0".into(),
            embed_thumbnail: false,
            embed_metadata: false,
            embed_subs: true,
            output_dir: PathBuf::from("/tmp"),
        };
        let base = build_args_for_url("https://youtu.be/abc", &opts);
        // base already carries the subs skip flag
        assert!(base.iter().any(|a| a == "youtube:skip=translated_subs"));
        let retry = with_mweb_fallback(&base);
        assert!(retry.iter().any(|a| a == "youtube:player_client=mweb"));
        // repeated flag (original kept) + url still last
        assert_eq!(
            retry.iter().filter(|a| *a == "--extractor-args").count(),
            base.iter().filter(|a| *a == "--extractor-args").count() + 1
        );
        assert_eq!(
            retry.last().map(String::as_str),
            Some("https://youtu.be/abc")
        );
    }

    #[test]
    fn fallback_policy_cli_wins_over_config() {
        use crate::config::DlFallbackMode;
        assert_eq!(
            resolve_fallback_policy(true, false, DlFallbackMode::Off),
            DlFallbackMode::Auto
        );
        assert_eq!(
            resolve_fallback_policy(false, true, DlFallbackMode::Auto),
            DlFallbackMode::Off
        );
        assert_eq!(
            resolve_fallback_policy(false, false, DlFallbackMode::Auto),
            DlFallbackMode::Auto
        );
        assert_eq!(
            resolve_fallback_policy(false, false, DlFallbackMode::Ask),
            DlFallbackMode::Ask
        );
    }
}
