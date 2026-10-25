//! `msc doctor` — concise environment checks with `OK` / `FALTA` lines.
//!
//! Covers: linkable libmpv (+ version), the audio backend
//! (PipeWire / PulseAudio / ALSA), `yt-dlp` / `ffmpeg` / `cava` on `PATH`
//! with versions, `~/Music` + extra folder permissions, a valid
//! `config.toml`, an intact cache/library index (the
//! `library-baseline-*.toml` rows behind `msc library refresh`), and
//! readable session/history files (`stats.json`, `history.jsonl`,
//! `sleep.json`).
//!
//! Output is plain B&W text (one line per check, greppable). Any `FALTA`
//! means exit code 1; a clean run exits 0.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
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
    /// First line of `--version` output, when the tool answered.
    pub version: Option<String>,
    /// Shown only when the tool is missing.
    pub hint: &'static str,
}

/// Everything the player shells out to, in report order.
pub fn checks() -> Vec<Check> {
    let probe = |tool: &'static str, need, purpose, hint| {
        let found = which::lookup(tool);
        let version = found.as_deref().and_then(|p| probe_version(p, tool));
        Check {
            tool,
            need,
            purpose,
            found,
            version,
            hint,
        }
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
    ]
}

/// Whether `msc dl` can run end to end.
pub fn downloads_ready(checks: &[Check]) -> bool {
    !checks
        .iter()
        .any(|c| c.need == Need::Downloads && c.found.is_none())
}

// ── Report ────────────────────────────────────────────────────────

/// One check line: `OK` (healthy) or `FALTA` (missing / broken).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Missing,
}

impl Status {
    fn tag(self) -> &'static str {
        match self {
            Self::Ok => "OK   ",
            Self::Missing => "FALTA",
        }
    }
}

/// A single doctor line. `hint` is empty for healthy checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub status: Status,
    pub detail: String,
    pub hint: String,
}

impl Item {
    fn ok(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: Status::Ok,
            detail: detail.into(),
            hint: String::new(),
        }
    }

    fn missing(
        label: impl Into<String>,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status: Status::Missing,
            detail: detail.into(),
            hint: hint.into(),
        }
    }

    fn line(&self) -> String {
        if self.hint.is_empty() {
            format!("  {} {} {}", self.status.tag(), self.label, self.detail)
        } else {
            format!(
                "  {} {} {} — {}",
                self.status.tag(),
                self.label,
                self.detail,
                self.hint
            )
        }
        .trim_end()
        .to_owned()
    }
}

/// Whole doctor run: items in display order plus the `FALTA` count.
#[derive(Debug, Clone)]
pub struct Report {
    pub items: Vec<Item>,
    pub missing: usize,
}

impl Report {
    fn new(items: Vec<Item>) -> Self {
        let missing = items.iter().filter(|i| i.status == Status::Missing).count();
        Self { items, missing }
    }

    pub fn ok(&self) -> bool {
        self.missing == 0
    }
}

/// Exit code for a report: 0 clean, 1 with any `FALTA`.
pub fn exit_code(report: &Report) -> i32 {
    if report.ok() { 0 } else { 1 }
}

/// Plain B&W rendering of a report (no ANSI codes, greppable).
pub fn format_report(report: &Report) -> String {
    let mut out = String::new();
    for item in &report.items {
        out.push_str(&item.line());
        out.push('\n');
    }
    if report.ok() {
        out.push_str("  tudo certo\n");
    } else {
        out.push_str(&format!("  {} falta(s)\n", report.missing));
    }
    out
}

// ── Full run (real paths) ─────────────────────────────────────────

/// Run every check against the real environment.
pub fn run() -> Report {
    let mut items = Vec::new();

    items.push(check_libmpv());
    items.push(check_audio());

    for c in checks() {
        items.push(check_tool(&c));
    }

    let cfg = config::AppConfig::load();
    items.extend(check_music_dirs(
        &config::default_music_dir(),
        &cfg.music_dirs,
    ));
    items.push(check_config_toml(&config::config_path()));

    let app_dir = config::config_dir();
    let cache_dir = config::cache_dir();
    items.push(check_library_index(&cache_dir));
    items.extend(check_data_files(&app_dir));
    items.push(check_rice());
    items.push(check_scrobble());

    Report::new(items)
}

// ── Individual checks (pure path params, testable) ───────────────

/// libmpv must initialize and answer `mpv-version`.
pub fn check_libmpv() -> Item {
    match libmpv_version() {
        Some(v) => Item::ok("libmpv", v),
        None => Item::missing(
            "libmpv",
            "não inicializou",
            "instale libmpv (Arch: sudo pacman -S mpv)",
        ),
    }
}

/// What [`check_libmpv`] reports: `None` when libmpv won't start.
pub fn libmpv_version() -> Option<String> {
    let mpv = libmpv2::Mpv::new().ok()?;
    let version: String = mpv.get_property("mpv-version").ok()?;
    let version = version.trim().to_owned();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

/// Audio backend from `pactl info` → PipeWire socket → `aplay -l`.
pub fn check_audio() -> Item {
    let pactl = run_capture("pactl", &["info"]);
    let socket = pipewire_socket_present();
    let aplay = run_capture("aplay", &["-l"]);
    match classify_audio_backend(pactl.as_deref(), socket, aplay.as_deref()) {
        Some((backend, via)) => Item::ok("audio", format!("{backend} (via {via})")),
        None => Item::missing(
            "audio",
            "nenhum backend (PipeWire/Pulse/ALSA) detectado",
            "Arch: sudo pacman -S pipewire pipewire-pulse alsa-utils",
        ),
    }
}

/// Pure backend classifier behind [`check_audio`].
pub fn classify_audio_backend(
    pactl_info: Option<&str>,
    pipewire_socket: bool,
    aplay_list: Option<&str>,
) -> Option<(String, String)> {
    if let Some(out) = pactl_info {
        if out.contains("PipeWire") {
            return Some(("PipeWire".into(), "pactl".into()));
        }
        // `pactl info` answering at all means a Pulse-compatible server.
        return Some(("PulseAudio".into(), "pactl".into()));
    }
    if pipewire_socket {
        return Some(("PipeWire".into(), "socket".into()));
    }
    if let Some(out) = aplay_list
        && out.lines().any(|l| l.trim_start().starts_with("card "))
    {
        return Some(("ALSA".into(), "aplay".into()));
    }
    None
}

/// `~/.local`/runtime PipeWire socket (`$XDG_RUNTIME_DIR/pipewire-0`).
fn pipewire_socket_present() -> bool {
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR")
        && Path::new(&rt).join("pipewire-0").exists()
    {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        let uid = unsafe { libc::getuid() };
        if Path::new(&format!("/run/user/{uid}/pipewire-0")).exists() {
            return true;
        }
    }
    false
}

/// One PATH-tool line, with version when it answered.
fn check_tool(c: &Check) -> Item {
    match (&c.found, &c.version) {
        (Some(path), Some(v)) => Item::ok(c.tool, format!("{v} ({})", path.display())),
        (Some(path), None) => Item::ok(c.tool, path.display().to_string()),
        (None, _) => Item::missing(c.tool, "não encontrado no PATH", c.hint),
    }
}

/// First line of `<tool> <version-args>` output, trimmed to 80 chars.
pub fn probe_version(path: &Path, tool: &str) -> Option<String> {
    let args: &[&str] = match tool {
        "ffmpeg" => &["-version"],
        "cava" => &["-v"],
        _ => &["--version"],
    };
    let first = run_capture_path(path, args)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            // Fallback flag (ffmpeg also answers `--version` on some builds).
            if args == ["--version"] {
                None
            } else {
                run_capture_path(path, &["--version"]).filter(|s| !s.trim().is_empty())
            }
        })?;
    Some(first_line_version(&first))
}

/// First non-empty line, single-spaced, capped at 80 chars.
pub fn first_line_version(output: &str) -> String {
    let line = output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let single: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short: String = single.chars().take(80).collect();
    if single.chars().count() > 80 {
        short.push('…');
    }
    short
}

/// Run `tool args` on PATH; `Some(stdout)` only on exit 0 with output.
fn run_capture(tool: &str, args: &[&str]) -> Option<String> {
    let path = which::lookup(tool)?;
    run_capture_path(&path, args)
}

fn run_capture_path(path: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(path).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// `~/Music` plus every extra folder from config: must exist, read, write.
pub fn check_music_dirs(default: &Path, extra: &[PathBuf]) -> Vec<Item> {
    let mut items = Vec::new();
    items.push(check_music_dir("music", default));
    for dir in extra {
        // Skip the default when listed twice; anything else gets its own line.
        if dir == default {
            continue;
        }
        items.push(check_music_dir("music-extra", dir));
    }
    items
}

fn check_music_dir(label: &str, dir: &Path) -> Item {
    let shown = dir.display().to_string();
    match dir_health(dir) {
        Ok(mode) => Item::ok(label, format!("{shown} ({mode})")),
        Err(reason) => Item::missing(
            label,
            format!("{shown} ({reason})"),
            "crie a pasta ou aponte outra com -m / --music-dir",
        ),
    }
}

/// Exists + is a dir + lists + not read-only.
fn dir_health(dir: &Path) -> Result<&'static str, String> {
    if !dir.exists() {
        return Err("não existe".into());
    }
    if !dir.is_dir() {
        return Err("não é pasta".into());
    }
    if let Err(e) = fs::read_dir(dir) {
        return Err(format!("sem leitura ({e})"));
    }
    let readonly = fs::metadata(dir)
        .map(|m| m.permissions().readonly())
        .unwrap_or(true);
    if readonly {
        return Err("somente leitura".into());
    }
    Ok("leitura+escrita")
}

/// `config.toml` must parse (a missing file just means defaults).
pub fn check_config_toml(path: &Path) -> Item {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Item::ok("config.toml", "ausente (usa padrões)");
        }
        Err(e) => {
            return Item::missing(
                "config.toml",
                format!("ilegível ({e})"),
                format!("cheque as permissões de {}", path.display()),
            );
        }
    };
    match toml::from_str::<config::AppConfig>(&raw) {
        Ok(_) => Item::ok("config.toml", path.display().to_string()),
        Err(e) => {
            let one_line = e
                .to_string()
                .lines()
                .next()
                .unwrap_or("inválido")
                .to_owned();
            Item::missing(
                "config.toml",
                one_line,
                "corrija o TOML ou apague para regenerar",
            )
        }
    }
}

/// Every `library-baseline-*.toml` must parse with `root` + `tracks`,
/// and `library.db` (SQLite) must open with a `tracks` table when present.
/// No index yet is fine (fresh state, not a fault).
pub fn check_library_index(cache_dir: &Path) -> Item {
    let db = cache_dir.join("library.db");
    if db.is_file() {
        if let Err(reason) = validate_library_db(&db) {
            return Item::missing(
                "library index",
                format!("library.db ({reason})"),
                "apague o índice corrompido e rode `msc library refresh`",
            );
        }
        return Item::ok("library index", "library.db ok");
    }
    let mut baselines: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(cache_dir) {
        let mut names: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.starts_with("library-baseline") && s.ends_with(".toml"))
            })
            .collect();
        names.sort();
        baselines = names;
    }
    if baselines.is_empty() {
        return Item::ok(
            "library index",
            "sem índice ainda (`msc library refresh` cria)",
        );
    }
    for path in &baselines {
        if let Err(reason) = validate_baseline(path) {
            return Item::missing(
                "library index",
                format!(
                    "{} ({reason})",
                    path.file_name().and_then(|s| s.to_str()).unwrap_or("?")
                ),
                "apague o índice corrompido e rode `msc library refresh`",
            );
        }
    }
    Item::ok("library index", format!("{} índice(s) ok", baselines.len()))
}

fn validate_baseline(path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("ilegível ({e})"))?;
    let value: toml::Value = toml::from_str(&raw).map_err(|e| {
        e.to_string()
            .lines()
            .next()
            .unwrap_or("TOML inválido")
            .to_owned()
    })?;
    if value.get("root").and_then(|v| v.as_str()).is_none() {
        return Err("sem campo `root`".into());
    }
    if value.get("tracks").and_then(|v| v.as_array()).is_none() {
        return Err("sem tabela `tracks`".into());
    }
    Ok(())
}

/// `library.db` must be a readable SQLite file with a `tracks` table.
fn validate_library_db(path: &Path) -> Result<(), String> {
    let conn = rusqlite::Connection::open(path).map_err(|e| format!("ilegível ({e})"))?;
    let has: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tracks'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("sem tabela `tracks` ({e})"))?;
    if !has {
        return Err("sem tabela `tracks`".into());
    }
    Ok(())
}

/// Rice theme: matugen `scheme.json` active, or classic B&W fallback.
pub fn check_rice() -> Item {
    let (_, active) = crate::rice::load();
    if active {
        Item::ok("rice", crate::rice::scheme_path().display().to_string())
    } else {
        Item::ok("rice", "off (B&W fallback)")
    }
}

/// ListenBrainz scrobbling: on with a token, off (default) without one.
pub fn check_scrobble() -> Item {
    let token = crate::scrobble::configured_token();
    if crate::scrobble::enabled(&token) {
        Item::ok("scrobble", "ListenBrainz on")
    } else {
        Item::ok(
            "scrobble",
            "off (`listenbrainz_token` em config.toml ativa)",
        )
    }
}

/// `stats.json` + `history.jsonl` + `sleep.json` must parse when present.
/// Missing files are fresh state, not faults.
pub fn check_data_files(app_dir: &Path) -> Vec<Item> {
    vec![
        check_stats_json(&app_dir.join("stats.json")),
        check_history_jsonl(&app_dir.join("cache").join("history.jsonl")),
        check_sleep_json(&app_dir.join("cache").join("sleep.json")),
    ]
}

fn check_stats_json(path: &Path) -> Item {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Item::ok("stats.json", "sem plays ainda");
        }
        Err(e) => {
            return Item::missing(
                "stats.json",
                format!("ilegível ({e})"),
                permissions_hint(path),
            );
        }
    };
    match serde_json::from_str::<crate::stats::StatsStore>(&raw) {
        Ok(store) => Item::ok("stats.json", format!("{} plays", store.total_plays)),
        Err(_) => Item::missing(
            "stats.json",
            "JSON inválido",
            "apague para recomeçar (perde as contagens)",
        ),
    }
}

fn check_history_jsonl(path: &Path) -> Item {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Item::ok("history.jsonl", "vazio");
        }
        Err(e) => {
            return Item::missing(
                "history.jsonl",
                format!("ilegível ({e})"),
                permissions_hint(path),
            );
        }
    };
    let (total, bad) = scan_history(&raw);
    if bad > 0 {
        Item::missing(
            "history.jsonl",
            format!("{bad} linha(s) ilegíveis de {total}"),
            "apague as linhas quebradas ou o arquivo para recomeçar",
        )
    } else {
        Item::ok("history.jsonl", format!("{total} evento(s)"))
    }
}

/// `(total lines, bad lines)`; a line is bad unless it has a string `id`
/// and a numeric `at`. Blank lines don't count.
pub fn scan_history(raw: &str) -> (usize, usize) {
    let mut total = 0usize;
    let mut bad = 0usize;
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        let ok = serde_json::from_str::<serde_json::Value>(line).is_ok_and(|v| {
            v.get("id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| !id.is_empty())
                && v.get("at").and_then(|at| at.as_u64()).is_some()
        });
        if !ok {
            bad += 1;
        }
    }
    (total, bad)
}

fn check_sleep_json(path: &Path) -> Item {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Item::ok("sleep.json", "off");
        }
        Err(e) => {
            return Item::missing(
                "sleep.json",
                format!("ilegível ({e})"),
                permissions_hint(path),
            );
        }
    };
    let valid = serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|v| {
        matches!(
            v.get("minutes"),
            Some(serde_json::Value::Null) | Some(serde_json::Value::Number(_))
        )
    });
    if valid {
        Item::ok("sleep.json", "legível")
    } else {
        Item::missing("sleep.json", "JSON inválido", "`msc sleep off` regenera")
    }
}

fn permissions_hint(path: &Path) -> String {
    format!("cheque as permissões de {}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stillmusic-doctor-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    #[test]
    fn version_first_line_is_short_and_single_spaced() {
        assert_eq!(first_line_version("2026.08.19\n"), "2026.08.19");
        assert_eq!(
            first_line_version("\nffmpeg version n9.0.2 Copyright (c)\nbuilt with gcc\n"),
            "ffmpeg version n9.0.2 Copyright (c)"
        );
        assert_eq!(first_line_version("  a   b \n"), "a b");
        assert_eq!(first_line_version(""), "");
        let long = "x".repeat(200);
        let short = first_line_version(&long);
        assert!(short.len() <= "x".repeat(80).len() + "…".len());
        assert!(short.ends_with('…'));
    }

    #[test]
    fn audio_classifier_prefers_pipewire_over_pulse() {
        let (backend, via) = classify_audio_backend(
            Some("Server Name: PulseAudio (on PipeWire 1.4)"),
            false,
            None,
        )
        .unwrap();
        assert_eq!((backend.as_str(), via.as_str()), ("PipeWire", "pactl"));
        let (backend, via) = classify_audio_backend(
            Some("Server Name: pulseaudio\nServer Version: 17.0"),
            false,
            None,
        )
        .unwrap();
        assert_eq!((backend.as_str(), via.as_str()), ("PulseAudio", "pactl"));
        let (backend, via) = classify_audio_backend(None, true, None).unwrap();
        assert_eq!((backend.as_str(), via.as_str()), ("PipeWire", "socket"));
        let (backend, via) =
            classify_audio_backend(None, false, Some("card 0: PCH [HDA]")).unwrap();
        assert_eq!((backend.as_str(), via.as_str()), ("ALSA", "aplay"));
        assert!(classify_audio_backend(None, false, None).is_none());
        assert!(classify_audio_backend(None, false, Some("no soundcards")).is_none());
    }

    #[test]
    fn music_dir_needs_read_write_folder() {
        let root = scratch("music");
        let missing = root.join("nope");
        assert_eq!(check_music_dir("music", &missing).status, Status::Missing);
        let file = root.join("file");
        fs::write(&file, "x").unwrap();
        assert_eq!(check_music_dir("music", &file).status, Status::Missing);
        let dir = root.join("tunes");
        fs::create_dir_all(&dir).unwrap();
        let item = check_music_dir("music", &dir);
        assert_eq!(item.status, Status::Ok);
        assert!(item.detail.contains("leitura+escrita"));
        // No duplicates when the extra list repeats the default.
        assert_eq!(check_music_dirs(&dir, std::slice::from_ref(&dir)).len(), 1);
        assert_eq!(
            check_music_dirs(&dir, std::slice::from_ref(&missing)).len(),
            2
        );
    }

    #[test]
    fn config_toml_missing_is_default_but_broken_is_falta() {
        let root = scratch("config");
        let missing = root.join("config.toml");
        let item = check_config_toml(&missing);
        assert_eq!(item.status, Status::Ok);
        let good = root.join("good.toml");
        fs::write(&good, "volume = 80\n").unwrap();
        assert_eq!(check_config_toml(&good).status, Status::Ok);
        let bad = root.join("bad.toml");
        fs::write(&bad, "volume = [unclosed\n").unwrap();
        let item = check_config_toml(&bad);
        assert_eq!(item.status, Status::Missing);
        assert!(!item.hint.is_empty());
    }

    #[test]
    fn library_index_empty_is_fresh_but_corrupt_is_falta() {
        let root = scratch("index").join("cache");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(check_library_index(&root).status, Status::Ok);
        fs::write(
            root.join("library-baseline-music.toml"),
            "root = \"/m\"\ntracks = []\n",
        )
        .unwrap();
        assert_eq!(check_library_index(&root).status, Status::Ok);
        fs::write(root.join("library-baseline-bad.toml"), "root = [oops\n").unwrap();
        let item = check_library_index(&root);
        assert_eq!(item.status, Status::Missing);
        assert!(item.detail.contains("library-baseline-bad.toml"));
        // Unrelated cache files never count.
        fs::remove_file(root.join("library-baseline-bad.toml")).unwrap();
        fs::write(root.join("tags.toml"), "x = [\n").unwrap();
        assert_eq!(check_library_index(&root).status, Status::Ok);
    }

    #[test]
    fn history_scan_counts_bad_lines() {
        assert_eq!(scan_history(""), (0, 0));
        assert_eq!(
            scan_history("{\"id\":\"a\",\"at\":1}\n\n{\"id\":\"b\",\"at\":2}\n"),
            (2, 0)
        );
        assert_eq!(
            scan_history("not json\n{\"id\":\"\",\"at\":1}\n{\"id\":\"a\"}\n"),
            (3, 3)
        );
    }

    #[test]
    fn data_files_missing_is_fresh_but_corrupt_is_falta() {
        let app = scratch("data");
        fs::create_dir_all(app.join("cache")).unwrap();
        let items = check_data_files(&app);
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|i| i.status == Status::Ok));

        fs::write(app.join("stats.json"), "{\"total_plays\":2,\"tracks\":{}}").unwrap();
        fs::write(
            app.join("cache").join("history.jsonl"),
            "{\"id\":\"a\",\"at\":1}\nboom\n",
        )
        .unwrap();
        fs::write(app.join("cache").join("sleep.json"), "{\"minutes\":30}").unwrap();
        let items = check_data_files(&app);
        let status = |label: &str| items.iter().find(|i| i.label == label).unwrap().status;
        assert_eq!(status("stats.json"), Status::Ok);
        assert_eq!(status("history.jsonl"), Status::Missing);
        assert_eq!(status("sleep.json"), Status::Ok);

        fs::write(app.join("stats.json"), "{oops").unwrap();
        fs::write(app.join("cache").join("sleep.json"), "nope").unwrap();
        let items = check_data_files(&app);
        let status = |label: &str| items.iter().find(|i| i.label == label).unwrap().status;
        assert_eq!(status("stats.json"), Status::Missing);
        assert_eq!(status("sleep.json"), Status::Missing);
    }

    #[test]
    fn report_counts_missing_and_renders_plain_lines() {
        let report = Report::new(vec![
            Item::ok("libmpv", "0.41.0"),
            Item::missing("cava", "não encontrado no PATH", "sudo pacman -S cava"),
        ]);
        assert_eq!(report.missing, 1);
        assert!(!report.ok());
        assert_eq!(exit_code(&report), 1);
        let text = format_report(&report);
        assert!(text.contains("OK    libmpv 0.41.0"));
        assert!(text.contains("FALTA cava"));
        assert!(text.contains("1 falta(s)"));
        assert!(!text.contains('\x1b'), "no ANSI escapes");

        let clean = Report::new(vec![Item::ok("libmpv", "0.41.0")]);
        assert!(clean.ok());
        assert_eq!(exit_code(&clean), 0);
        assert!(format_report(&clean).contains("tudo certo"));
    }

    #[test]
    fn libmpv_links_and_answers() {
        // A running test binary already proves the link; the version string
        // just needs to be non-empty.
        let v = libmpv_version().expect("libmpv must initialize in tests");
        assert!(!v.trim().is_empty());
    }
}
