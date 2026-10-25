//! `msc sync export` / `msc sync import` — backup and restore your data.
//!
//! What's inside (paths relative to `~/.option/music`):
//! - `config.toml` — settings, folders, favorites, resume state
//! - `stats.json` — play counts and listening totals
//! - `stations/*.toml` — saved radio stations, when present (optional)
//! - `playlists/*.toml` — saved playlists
//! - `lyrics/*.json` — manually selected or imported lyrics
//! - `cache/history.jsonl` — play history ("played this week")
//! - `cache/library.db` — the SQLite library index (path + mtime + size +
//!   tags; lets `msc library refresh` spot adds/removes/changes across runs)
//! - `manifest.json` — format marker + member list, validated on import
//!
//! Rebuildable caches (`cache/tags`, `cache/covers`, `cache/lyrics`,
//! `cache/dl`, `cache/preview`) are intentionally excluded: they are big
//! and regenerate on demand, so backups stay small and portable.
//!
//! Packing runs through the system `tar` binary — no new dependencies.
//! Overwrites (an existing archive on export, existing data files on
//! import) only happen with `force` (`--force` on the CLI).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::which;

/// Archive format version written into `manifest.json`.
pub const SYNC_FORMAT_VERSION: u32 = 1;
/// Manifest member inside every backup archive.
pub const MANIFEST_NAME: &str = "manifest.json";
/// Marker identifying our own archives.
pub const SYNC_TOOL_ID: &str = "stillmusic-sync";
const LEGACY_SYNC_TOOL_ID: &str = "optionmusic-sync";
/// Saved radio stations (optional backup member, top-level dir).
pub const STATIONS_FILE: &str = "stations.toml";
/// Saved radio stations directory (optional backup members).
pub const STATIONS_DIR: &str = "stations";
/// SQLite library index (optional backup member).
pub const LIBRARY_DB: &str = "cache/library.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    tool: String,
    version: u32,
    files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub files: usize,
    pub overwritten: usize,
}

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Unique scratch dir under the temp dir; removed on drop.
struct Stage(PathBuf);

impl Stage {
    fn create(tag: &str) -> Result<Self> {
        let n = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("stillmusic-sync-{}-{n}-{tag}", std::process::id()));
        fs::create_dir_all(&dir).with_context(|| format!("create staging {}", dir.display()))?;
        Ok(Self(dir))
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Archive member name with `/` separators (portable across platforms).
fn arcname(rel: &str) -> String {
    rel.replace('\\', "/")
}

/// Every existing backup member: `(archive name, source path)`, sorted.
pub fn collect_entries(app_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut push_file = |rel: &str| {
        let path = app_dir.join(rel);
        if path.is_file() {
            out.push((arcname(rel), path));
        }
    };
    push_file("config.toml");
    push_file("stats.json");
    push_file(STATIONS_FILE);
    for name in sorted_dir_names(&app_dir.join(STATIONS_DIR), "toml") {
        push_file(&format!("{STATIONS_DIR}/{name}"));
    }
    for name in sorted_dir_names(&app_dir.join("playlists"), "toml") {
        push_file(&format!("playlists/{name}"));
    }
    for name in sorted_dir_names(&app_dir.join("lyrics"), "json") {
        push_file(&format!("lyrics/{name}"));
    }
    push_file("cache/history.jsonl");
    push_file("cache/sleep.json");
    push_file(LIBRARY_DB);
    for name in sorted_dir_names(&app_dir.join("cache"), "toml")
        .into_iter()
        .filter(|n| n.starts_with("library-baseline"))
    {
        push_file(&format!("cache/{name}"));
    }
    out
}

/// Sorted file names with the given extension inside `dir` (empty when missing).
fn sorted_dir_names(dir: &Path, ext: &str) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let p = e.path();
            (p.extension().and_then(|s| s.to_str()) == Some(ext))
                .then(|| e.file_name().to_string_lossy().into_owned())
        })
        .collect();
    names.sort();
    names
}

fn require_tar() -> Result<PathBuf> {
    which::lookup("tar").with_context(|| "tar not found on PATH (needed for `msc sync`)")
}

fn tar_error(what: &str, out: &std::process::Output) -> String {
    let mut msg = format!("tar {what} failed");
    let err = String::from_utf8_lossy(&out.stderr);
    if let Some(line) = err.lines().map(str::trim).find(|l| !l.is_empty()) {
        msg.push_str(": ");
        msg.push_str(&line.chars().take(160).collect::<String>());
    } else if let Some(code) = out.status.code() {
        msg.push_str(&format!(" (exit {code})"));
    }
    msg
}

/// Write a `.tar.gz` backup of `app_dir` to `archive`.
/// Refuses to overwrite an existing archive unless `force`.
pub fn export_to(archive: &Path, app_dir: &Path, force: bool) -> Result<ExportSummary> {
    let entries = collect_entries(app_dir);
    if entries.is_empty() {
        bail!("nothing to back up in {}", app_dir.display());
    }
    if archive.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            archive.display()
        );
    }
    if let Some(parent) = archive.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let tar = require_tar()?;
    let stage = Stage::create("manifest")?;
    let manifest = Manifest {
        tool: SYNC_TOOL_ID.into(),
        version: SYNC_FORMAT_VERSION,
        files: entries.iter().map(|(name, _)| name.clone()).collect(),
    };
    fs::write(
        stage.0.join(MANIFEST_NAME),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // One `tar` call: members relative to the app dir, manifest from staging.
    let mut cmd = Command::new(&tar);
    cmd.arg("-czf").arg(archive).arg("-C").arg(app_dir);
    for (name, _) in &entries {
        cmd.arg(name);
    }
    cmd.arg("-C").arg(&stage.0).arg(MANIFEST_NAME);
    let out = cmd.output().with_context(|| "run tar")?;
    if !out.status.success() {
        let _ = fs::remove_file(archive);
        bail!("{}", tar_error("export", &out));
    }
    // The archive must list cleanly straight after writing it.
    list_members(&tar, archive).with_context(|| "verify backup")?;

    let bytes = fs::metadata(archive)
        .with_context(|| format!("read {}", archive.display()))?
        .len();
    Ok(ExportSummary {
        files: entries.len(),
        bytes,
    })
}

/// Raw `tar -tzf` listing (member names as stored).
fn list_members(tar: &Path, archive: &Path) -> Result<Vec<String>> {
    let out = Command::new(tar)
        .arg("-tzf")
        .arg(archive)
        .output()
        .with_context(|| "run tar")?;
    if !out.status.success() {
        bail!("{}", tar_error("list", &out));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// Normalize one stored member: `None` for directory / dot entries.
/// The backup manifest itself is validated separately (presence + fields),
/// so it is skipped here too.
fn normalize_member(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.ends_with('/') {
        return None; // directory entry
    }
    let mut name = name;
    while let Some(stripped) = name.strip_prefix("./") {
        name = stripped;
    }
    let name = name.trim();
    if name.is_empty() || name == "." || name == MANIFEST_NAME {
        None
    } else {
        Some(name.to_string())
    }
}

/// Reject absolute paths and `..` escapes; every member must be manifest-listed
/// and every manifest file must be present. Returns files in manifest order.
pub fn validate_members(raw: &[String], manifest_files: &[String]) -> Result<Vec<String>> {
    let mut members = Vec::new();
    for r in raw {
        if let Some(name) = normalize_member(r) {
            if name.starts_with('/') {
                bail!("unsafe entry in backup: {name}");
            }
            if name.split('/').any(|c| c == "..") {
                bail!("unsafe entry in backup: {name}");
            }
            members.push(name);
        }
    }
    let listed: std::collections::BTreeSet<&str> = members.iter().map(String::as_str).collect();
    for name in manifest_files {
        if !listed.contains(name.as_str()) {
            bail!("backup is missing listed file: {name}");
        }
    }
    for name in &members {
        if !manifest_files.iter().any(|f| f == name) {
            bail!("backup contains unlisted file: {name}");
        }
    }
    Ok(manifest_files.to_vec())
}

/// Extract + parse `manifest.json`; errors mean "not one of our backups".
fn read_manifest(tar: &Path, archive: &Path) -> Result<Manifest> {
    let stage = Stage::create("manifest")?;
    let out = Command::new(tar)
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&stage.0)
        .arg(MANIFEST_NAME)
        .output()
        .with_context(|| "run tar")?;
    if !out.status.success() {
        bail!(
            "{} is not a still backup (manifest missing)",
            archive.display()
        );
    }
    let raw =
        fs::read_to_string(stage.0.join(MANIFEST_NAME)).with_context(|| "read backup manifest")?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .with_context(|| format!("{} is not a still backup", archive.display()))?;
    if manifest.tool != SYNC_TOOL_ID && manifest.tool != LEGACY_SYNC_TOOL_ID {
        bail!("{} is not a still backup", archive.display());
    }
    if manifest.version > SYNC_FORMAT_VERSION {
        bail!(
            "backup needs a newer still (format v{}, this build reads v{SYNC_FORMAT_VERSION})",
            manifest.version
        );
    }
    Ok(manifest)
}

/// Restore a backup archive into `app_dir`.
/// Refuses to overwrite existing files unless `force`.
pub fn import_from(archive: &Path, app_dir: &Path, force: bool) -> Result<ImportSummary> {
    if !archive.is_file() {
        bail!("archive not found: {}", archive.display());
    }
    let tar = require_tar()?;
    let manifest = read_manifest(&tar, archive)?;
    let raw = list_members(&tar, archive)?;
    let files = validate_members(&raw, &manifest.files)?;

    let dests: Vec<(String, PathBuf)> = files
        .iter()
        .map(|name| (name.clone(), app_dir.join(name)))
        .collect();
    let clashes: Vec<String> = dests
        .iter()
        .filter(|(_, p)| p.exists())
        .map(|(n, _)| n.clone())
        .collect();
    if !clashes.is_empty() && !force {
        let mut shown = clashes
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if clashes.len() > 5 {
            shown.push_str(&format!(" …and {} more", clashes.len() - 5));
        }
        bail!(
            "would overwrite {} file(s): {shown} (use --force to overwrite)",
            clashes.len()
        );
    }

    // The listing is validated (no `..`, no absolute paths, manifest-only),
    // so extracting to staging and copying the listed files is safe.
    let stage = Stage::create("extract")?;
    let out = Command::new(&tar)
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&stage.0)
        .output()
        .with_context(|| "run tar")?;
    if !out.status.success() {
        bail!("{}", tar_error("extract", &out));
    }

    fs::create_dir_all(app_dir).with_context(|| format!("create {}", app_dir.display()))?;
    let mut overwritten = 0usize;
    for (name, dest) in &dests {
        let src = stage.0.join(name);
        if !src.is_file() {
            bail!("backup is missing listed file: {name}");
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        if dest.exists() {
            overwritten += 1;
        }
        fs::copy(&src, dest).with_context(|| format!("restore {}", dest.display()))?;
    }
    Ok(ImportSummary {
        files: dests.len(),
        overwritten,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stillmusic-sync-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn tar_present() -> bool {
        which::lookup("tar").is_some()
    }

    fn fake_app(tag: &str) -> PathBuf {
        let app = scratch(tag).join("app");
        fs::create_dir_all(app.join("playlists")).unwrap();
        fs::create_dir_all(app.join("lyrics")).unwrap();
        fs::create_dir_all(app.join("cache")).unwrap();
        fs::write(app.join("config.toml"), "volume = 80\n").unwrap();
        fs::write(app.join("stats.json"), "{\"total_plays\":3}").unwrap();
        fs::write(app.join("stations.toml"), "[lofi]\nurl = \"x\"\n").unwrap();
        fs::write(app.join("playlists").join("a.toml"), "name = \"A\"\n").unwrap();
        fs::write(
            app.join("lyrics").join("selected.json"),
            "{\"version\":1}\n",
        )
        .unwrap();
        fs::write(
            app.join("cache").join("history.jsonl"),
            "{\"id\":\"a\",\"at\":1}\n",
        )
        .unwrap();
        fs::write(app.join("cache").join("sleep.json"), "{\"minutes\":null}").unwrap();
        fs::write(
            app.join("cache").join("library-baseline-music.toml"),
            "root = \"/m\"\n",
        )
        .unwrap();
        // Rebuildable junk that must never enter a backup.
        fs::create_dir_all(app.join("cache").join("tags")).unwrap();
        fs::write(app.join("cache").join("tags").join("x.toml"), "y = 1\n").unwrap();
        app
    }

    #[test]
    fn collect_skips_missing_and_caches() {
        let app = fake_app("collect");
        let names: Vec<String> = collect_entries(&app).into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"config.toml".to_string()));
        assert!(names.contains(&"stats.json".to_string()));
        assert!(names.contains(&STATIONS_FILE.to_string()));
        assert!(names.contains(&"playlists/a.toml".to_string()));
        assert!(names.contains(&"lyrics/selected.json".to_string()));
        assert!(names.contains(&"cache/history.jsonl".to_string()));
        assert!(names.contains(&"cache/sleep.json".to_string()));
        assert!(names.contains(&"cache/library-baseline-music.toml".to_string()));
        assert!(!names.iter().any(|n| n.contains("tags")), "caches stay out");
        assert!(collect_entries(&scratch("collect-empty").join("nope")).is_empty());
    }

    #[test]
    fn member_validation_rejects_escapes() {
        let manifest = vec!["config.toml".to_string()];
        assert!(validate_members(&["config.toml".to_string()], &manifest).is_ok());
        assert!(validate_members(&["./config.toml".to_string()], &manifest).is_ok());
        assert!(validate_members(&["../evil".to_string()], &manifest).is_err());
        assert!(validate_members(&["/abs".to_string()], &manifest).is_err());
        assert!(validate_members(&["a/../../evil".to_string()], &manifest).is_err());
        assert!(validate_members(&["extra.toml".to_string()], &manifest).is_err());
        assert!(validate_members(&[] as &[String], &manifest).is_err());
        assert!(normalize_member("playlists/").is_none());
        assert!(normalize_member("./playlists/").is_none());
        assert!(normalize_member("./").is_none());
    }

    #[test]
    fn export_import_roundtrip() {
        if !tar_present() {
            eprintln!("skipping: tar missing");
            return;
        }
        let root = scratch("roundtrip");
        let app = fake_app("roundtrip");
        let archive = root.join("backup.tar.gz");
        let exp = export_to(&archive, &app, false).unwrap();
        assert!(exp.files >= 6);
        assert!(exp.bytes > 0);

        let tar = require_tar().unwrap();
        assert_eq!(
            read_manifest(&tar, &archive).unwrap().tool,
            "stillmusic-sync"
        );
        let listing = list_members(&tar, &archive).unwrap();
        assert!(listing.iter().any(|m| m == MANIFEST_NAME));

        // Restore into a wiped tree and compare bytes.
        let before: Vec<(String, Vec<u8>)> = collect_entries(&app)
            .into_iter()
            .map(|(n, p)| (n, fs::read(p).unwrap()))
            .collect();
        let _ = fs::remove_dir_all(&app);
        let imp = import_from(&archive, &app, false).unwrap();
        assert_eq!(imp.files, before.len());
        assert_eq!(imp.overwritten, 0);
        for (name, body) in &before {
            assert_eq!(&fs::read(app.join(name)).unwrap(), body, "{name} intact");
        }
    }

    #[test]
    fn import_accepts_legacy_tool_id() {
        if !tar_present() {
            eprintln!("skipping: tar missing");
            return;
        }
        let root = scratch("legacy");
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let config = "volume = 42\n";
        fs::write(source.join("config.toml"), config).unwrap();
        fs::write(
            source.join(MANIFEST_NAME),
            r#"{"tool":"optionmusic-sync","version":1,"files":["config.toml"]}"#,
        )
        .unwrap();
        let archive = root.join("legacy.tar.gz");
        let out = Command::new(require_tar().unwrap())
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .arg("config.toml")
            .arg(MANIFEST_NAME)
            .output()
            .unwrap();
        assert!(out.status.success());

        let restored = root.join("restored");
        let imported = import_from(&archive, &restored, false).unwrap();
        assert_eq!(imported.files, 1);
        assert_eq!(imported.overwritten, 0);
        assert_eq!(
            fs::read_to_string(restored.join("config.toml")).unwrap(),
            config
        );
    }

    #[test]
    fn export_refuses_overwrite_without_force() {
        if !tar_present() {
            eprintln!("skipping: tar missing");
            return;
        }
        let root = scratch("expforce");
        let app = fake_app("expforce");
        let archive = root.join("b.tar.gz");
        export_to(&archive, &app, false).unwrap();
        assert!(export_to(&archive, &app, false).is_err());
        export_to(&archive, &app, true).unwrap();
        assert!(
            export_to(
                &root.join("missing-dir-app.tar.gz"),
                &root.join("no-app"),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn import_refuses_overwrite_without_force() {
        if !tar_present() {
            eprintln!("skipping: tar missing");
            return;
        }
        let root = scratch("impforce");
        let app = fake_app("impforce");
        let archive = root.join("b.tar.gz");
        export_to(&archive, &app, false).unwrap();
        // config.toml already exists → clash without force.
        assert!(import_from(&archive, &app, false).is_err());
        let imp = import_from(&archive, &app, true).unwrap();
        assert!(imp.overwritten >= 1);
    }

    #[test]
    fn import_rejects_non_backup_archives() {
        if !tar_present() {
            eprintln!("skipping: tar missing");
            return;
        }
        let root = scratch("nonbackup");
        fs::write(root.join("note.txt"), "hi").unwrap();
        let tar = require_tar().unwrap();
        let archive = root.join("plain.tar.gz");
        let out = Command::new(&tar)
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&root)
            .arg("note.txt")
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(import_from(&archive, &root.join("restore"), false).is_err());
        assert!(import_from(&root.join("nope.tar.gz"), &root.join("restore"), false).is_err());
    }
}
