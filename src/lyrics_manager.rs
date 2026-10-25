//! Explicit online search and per-track selections. Local commands never call a provider.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use serde::Deserialize;

use crate::lyrics::{self, ResolvedLyrics};

#[derive(Debug, Clone, Subcommand)]
pub enum LyricsCommand {
    /// Search LRCLIB online; sends artist/title to lrclib.net.
    Search {
        #[arg(long)]
        artist: String,
        #[arg(long)]
        title: String,
    },
    /// Save an LRCLIB search result ID as this track's preferred lyrics.
    Select { track: PathBuf, id: u64 },
    /// Import a .lrc or .txt copy without changing the original files.
    Import { track: PathBuf, file: PathBuf },
    /// Show this track's local lyrics, source and selection path (offline).
    Show { track: PathBuf },
    /// Remove the saved selection; sidecar and embedded lyrics are preserved.
    Remove {
        track: PathBuf,
        /// Also delete the automatic provider cache for this track's metadata.
        #[arg(long)]
        cache: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    id: u64,
    track_name: String,
    artist_name: String,
    #[serde(default)]
    album_name: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    instrumental: bool,
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

impl Candidate {
    fn has_lyrics(&self) -> bool {
        self.synced_lyrics
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
            || self
                .plain_lyrics
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
    }
}

fn request(url: &str) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(concat!("stillmusic/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .context("LRCLIB request failed (check your connection or retry later)")?
        .body_mut()
        .read_to_string()
        .context("Could not read LRCLIB response")
}

fn parse_candidates(body: &str) -> Result<Vec<Candidate>> {
    serde_json::from_str(body).context("LRCLIB returned an invalid search response")
}

fn require_track(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_file(),
        "Track does not exist or is not a file: {}",
        path.display()
    );
    fs::canonicalize(path).context("Cannot resolve track path")
}

fn import_text(text: &str, synced: bool) -> Result<ResolvedLyrics> {
    let mut lyrics = ResolvedLyrics {
        source: "import".into(),
        ..Default::default()
    };
    if synced {
        lyrics.lines = lyrics::parse_lrc(text.trim_start_matches('\u{feff}'));
        ensure!(
            !lyrics.lines.is_empty(),
            "The .lrc file contains no usable timed lines"
        );
    } else {
        lyrics.plain = text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        ensure!(!lyrics.plain.is_empty(), "The text file is empty");
    }
    Ok(lyrics)
}

fn remove_file_if_present(path: &Path) -> Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Cannot remove {}", path.display())),
    }
}

fn query(track: &Path) -> lyrics::LyricsQuery {
    let tags = crate::meta::read_tags(track);
    lyrics::LyricsQuery {
        path: track.to_path_buf(),
        artist: tags.artist.unwrap_or_default(),
        title: tags.title.unwrap_or_else(|| {
            track
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        album: tags.album.unwrap_or_default(),
        duration: tags.duration_secs,
    }
}

pub fn run(command: LyricsCommand) -> Result<()> {
    match command {
        LyricsCommand::Search { artist, title } => {
            ensure!(!title.trim().is_empty(), "Provide a nonempty --title");
            let url = format!(
                "https://lrclib.net/api/search?artist_name={}&track_name={}",
                lyrics::percent_encode(artist.trim()),
                lyrics::percent_encode(title.trim())
            );
            let candidates = parse_candidates(&request(&url)?)?;
            if candidates.is_empty() {
                println!("No lyrics found on LRCLIB.");
                return Ok(());
            }
            println!("ID\tType\tSeconds\tArtist — Track · Album");
            for hit in candidates {
                let kind = if hit.instrumental {
                    "instrumental"
                } else if hit
                    .synced_lyrics
                    .as_deref()
                    .is_some_and(|s| !s.trim().is_empty())
                {
                    "synced"
                } else if hit.has_lyrics() {
                    "plain"
                } else {
                    "no lyrics"
                };
                println!(
                    "{}\t{}\t{:.0}\t{} — {} · {}",
                    hit.id,
                    kind,
                    hit.duration,
                    safe_label(&hit.artist_name),
                    safe_label(&hit.track_name),
                    safe_label(&hit.album_name)
                );
            }
            println!("Save a result: msc lyrics select <track> <ID>");
        }
        LyricsCommand::Select { track, id } => {
            let track = require_track(&track)?;
            let body = request(&format!("https://lrclib.net/api/get/{id}"))?;
            let candidate: Candidate =
                serde_json::from_str(&body).context("Invalid LRCLIB result")?;
            ensure!(candidate.id == id, "LRCLIB returned a different result ID");
            ensure!(
                candidate.has_lyrics(),
                "This record contains no lyrics (it may be instrumental)"
            );
            let mut lyrics = lyrics::from_lrclib_body(&body);
            lyrics.source = format!("lrclib:{id}");
            lyrics::save_managed(&track, &lyrics)?;
            println!(
                "Saved {} — {} for {}",
                safe_label(&candidate.artist_name),
                safe_label(&candidate.track_name),
                track.display()
            );
        }
        LyricsCommand::Import { track, file } => {
            let track = require_track(&track)?;
            let synced = match file
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("lrc") => true,
                Some("txt") => false,
                _ => bail!("Import accepts .lrc or .txt files"),
            };
            let text = fs::read_to_string(&file).context("Cannot read lyrics file as UTF-8")?;
            lyrics::save_managed(&track, &import_text(&text, synced)?)?;
            println!("Saved lyrics copy for {}", track.display());
        }
        LyricsCommand::Show { track } => {
            let track = require_track(&track)?;
            // Diagnose corrupt selections here; playback can still use other local sources.
            let managed = lyrics::read_managed(&track)?;
            let q = query(&track);
            let local = managed.or_else(|| {
                lyrics::resolve_local(&track, &q.artist, &q.title, &q.album, q.duration)
            });
            let Some(local) = local else {
                println!(
                    "No local lyrics. Use msc lyrics search --artist <artist> --title <title>."
                );
                return Ok(());
            };
            println!("Source: {}", safe_label(&local.source));
            println!("Selection: {}", lyrics::managed_path(&track).display());
            if !local.lines.is_empty() {
                for line in lyrics::lrc_from_lines(&local.lines).lines() {
                    println!("{}", safe_label(line));
                }
            } else {
                for line in &local.plain {
                    println!("{}", safe_label(line));
                }
            }
        }
        LyricsCommand::Remove { track, cache } => {
            let track = require_track(&track)?;
            let removed = remove_file_if_present(&lyrics::managed_path(&track))?;
            let mut cached = false;
            if cache {
                let q = query(&track);
                cached = remove_file_if_present(&lyrics::cache_path(
                    &q.artist, &q.title, &q.album, q.duration,
                ))?;
            }
            println!(
                "Selection: {}; provider cache: {}.",
                if removed { "removed" } else { "absent" },
                if !cache {
                    "kept"
                } else if cached {
                    "removed"
                } else {
                    "absent"
                }
            );
        }
    }
    Ok(())
}

fn safe_label(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_candidates_including_instrumental() {
        let hits = parse_candidates(r#"[{"id":12,"trackName":"Song","artistName":"Artist","albumName":"Album","duration":120,"instrumental":false,"syncedLyrics":"[00:01] hi","plainLyrics":null},{"id":13,"trackName":"Instrumental","artistName":"A","instrumental":true,"syncedLyrics":null,"plainLyrics":null}]"#).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits[0].has_lyrics());
        assert!(!hits[1].has_lyrics());
        assert!(hits[1].instrumental);
        assert!(parse_candidates("{broken").is_err());
    }

    #[test]
    fn import_rejects_invalid_lrc_and_preserves_plain_unicode() {
        assert!(import_text("just prose", true).is_err());
        assert!(import_text(" \n ", false).is_err());
        assert_eq!(
            import_text("\u{feff}[00:02.00] olá", true).unwrap().lines[0].text,
            "olá"
        );
        assert_eq!(
            import_text("coração\n\n azul", false).unwrap().plain,
            ["coração", "azul"]
        );
    }

    #[test]
    fn labels_cannot_inject_terminal_controls() {
        assert_eq!(safe_label("A\u{1b}[31m\tB\nC"), "A [31m B C");
    }
}
