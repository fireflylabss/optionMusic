//! Play history for smart shelves (“played this week”).

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

const MAX_ENTRIES: usize = 5_000;
const WEEK_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub at: u64,
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

/// Append a play event (best-effort).
pub fn record_play(id: &str) -> Result<()> {
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
    };
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

/// Read recent history (newest last). Caps at [`MAX_ENTRIES`].
pub fn load_entries() -> Vec<HistoryEntry> {
    let path = history_path();
    let Ok(file) = fs::File::open(&path) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for line in BufReader::new(file).lines().flatten() {
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

/// Unique track ids played within the last 7 days (most recent first).
pub fn played_this_week() -> Vec<String> {
    let cutoff = now_unix().saturating_sub(WEEK_SECS);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in load_entries().into_iter().rev() {
        if entry.at < cutoff {
            continue;
        }
        if seen.insert(entry.id.clone()) {
            out.push(entry.id);
        }
    }
    out
}
