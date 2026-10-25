//! Play history for smart shelves (“played this week”).
//!
//! Each line of `history.jsonl` is one playback event. `outcome` separates
//! a completed listen (`played`) from an early exit (`skipped`, via `n`) and
//! an explicit dislike (`disliked`, via `d`). Older lines without `outcome`
//! read back as `played`, so existing files keep working.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

const MAX_ENTRIES: usize = 5_000;
const WEEK_SECS: u64 = 7 * 24 * 60 * 60;

/// How a history event ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryOutcome {
    /// Played past the counting threshold (or to the end).
    #[default]
    Played,
    /// Cut short via `n` / next before the threshold.
    Skipped,
    /// Explicit dislike via `d` (also counts as a skip).
    Disliked,
}

impl HistoryOutcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Played => "played",
            Self::Skipped => "skipped",
            Self::Disliked => "disliked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub at: u64,
    /// Missing in files written before Radio 2.0 → defaults to `played`.
    #[serde(default)]
    pub outcome: HistoryOutcome,
}

fn history_path() -> PathBuf {
    config::cache_dir().join("history.jsonl")
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Append a play event with an outcome (best-effort).
pub fn record_event(id: &str, outcome: HistoryOutcome) -> Result<()> {
    if id.is_empty() {
        return Ok(());
    }
    let dir = config::cache_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = history_path();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    let entry = HistoryEntry {
        id: id.to_owned(),
        at: now_unix(),
        outcome,
    };
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Append a completed play event (best-effort).
pub fn record_play(id: &str) -> Result<()> {
    record_event(id, HistoryOutcome::Played)
}

/// Append a skip event — track cut short via `n` / next (best-effort).
pub fn record_skip(id: &str) -> Result<()> {
    record_event(id, HistoryOutcome::Skipped)
}

/// Append a dislike event — explicit `d`, also counts as a skip (best-effort).
pub fn record_dislike(id: &str) -> Result<()> {
    record_event(id, HistoryOutcome::Disliked)
}

/// Read recent history (newest last). Caps at [`MAX_ENTRIES`].
pub fn load_entries() -> Vec<HistoryEntry> {
    let path = history_path();
    let Ok(file) = fs::File::open(&path) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
            entries.push(entry);
        }
    }
    if entries.len() > MAX_ENTRIES {
        let skip = entries.len() - MAX_ENTRIES;
        entries = entries.split_off(skip);
        let _ = rewrite_trimmed(&entries);
    }
    entries
}

fn rewrite_trimmed(entries: &[HistoryEntry]) -> Result<()> {
    let path = history_path();
    let mut body = String::new();
    for entry in entries {
        body.push_str(&serde_json::to_string(entry)?);
        body.push('\n');
    }
    option_sdk::atomic_write(path, body.as_bytes())?;
    Ok(())
}

/// Unique ids with `outcome` played within the last 7 days (most recent first).
fn ids_in_week(outcome: Option<HistoryOutcome>) -> Vec<String> {
    let cutoff = now_unix().saturating_sub(WEEK_SECS);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in load_entries().into_iter().rev() {
        if entry.at < cutoff {
            continue;
        }
        if outcome.is_some_and(|wanted| entry.outcome != wanted) {
            continue;
        }
        if seen.insert(entry.id.clone()) {
            out.push(entry.id);
        }
    }
    out
}

/// Unique track ids fully played within the last 7 days (most recent first).
/// Skips / dislikes are excluded — see [`skipped_this_week`] and
/// [`negative_this_week`].
pub fn played_this_week() -> Vec<String> {
    ids_in_week(Some(HistoryOutcome::Played))
}

/// Unique track ids skipped (`n`) within the last 7 days (most recent first).
pub fn skipped_this_week() -> Vec<String> {
    ids_in_week(Some(HistoryOutcome::Skipped))
}

/// Unique track ids disliked (`d`) within the last 7 days (most recent first).
pub fn disliked_this_week() -> Vec<String> {
    ids_in_week(Some(HistoryOutcome::Disliked))
}

/// Unique track ids with negative radio feedback (skipped or disliked)
/// within the last 7 days (most recent first). The similarity walk penalizes
/// these tracks and their artists.
pub fn negative_this_week() -> Vec<String> {
    let cutoff = now_unix().saturating_sub(WEEK_SECS);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in load_entries().into_iter().rev() {
        if entry.at < cutoff {
            continue;
        }
        if entry.outcome == HistoryOutcome::Played {
            continue;
        }
        if seen.insert(entry.id.clone()) {
            out.push(entry.id);
        }
    }
    out
}
