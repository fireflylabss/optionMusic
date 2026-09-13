//! In-TUI sleep timer: cycles off → 15 → 30 → 60 minutes, then pauses.
//!
//! The countdown is shown in the existing status line (no new chrome) and the
//! timer survives track changes. It lives only for the session: cleared on quit.
//! `msc sleep <minutes>|off` pre-seeds the next session via a small request
//! file which the TUI consumes at startup.

use std::fs;
use std::time::{Duration, Instant};

/// Cycle steps in minutes (0 = off).
pub const STEPS_MIN: &[u64] = &[0, 15, 30, 60];

#[derive(Debug, Clone)]
pub struct SleepTimer {
    deadline: Option<Instant>,
    total: Option<Duration>,
}

impl Default for SleepTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl SleepTimer {
    pub fn new() -> Self {
        Self {
            deadline: None,
            total: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.deadline.is_some()
    }

    /// Set a timer `minutes` out (0 clears). Returns the status label.
    pub fn set_minutes(&mut self, minutes: u64) -> String {
        if minutes == 0 {
            self.clear();
            return "sleep timer · off".into();
        }
        let total = Duration::from_secs(minutes * 60);
        self.deadline = Some(Instant::now() + total);
        self.total = Some(total);
        format!("sleep timer · {minutes} min")
    }

    /// Cycle off → 15 → 30 → 60 → off. Returns the status label.
    pub fn cycle(&mut self) -> String {
        let current_mins = self.total.map(|t| t.as_secs() / 60).unwrap_or(0);
        let next = match STEPS_MIN.iter().find(|m| **m > current_mins) {
            Some(m) => *m,
            None => 0,
        };
        // A timer already running mid-countdown still steps forward.
        self.set_minutes(next)
    }

    pub fn clear(&mut self) {
        self.deadline = None;
        self.total = None;
    }

    /// True once the deadline has passed (stays true until cleared).
    pub fn is_expired(&self) -> bool {
        match self.deadline {
            Some(d) => Instant::now() >= d,
            None => false,
        }
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.deadline.map(|d| {
            let now = Instant::now();
            if d > now { d - now } else { Duration::ZERO }
        })
    }

    /// Subtle `mm:ss` countdown for the status line.
    pub fn countdown_label(&self) -> Option<String> {
        self.remaining().map(|r| {
            let s = r.as_secs();
            format!("{}:{:02}", s / 60, s % 60)
        })
    }
}

fn request_path() -> std::path::PathBuf {
    crate::config::cache_dir().join("sleep.json")
}

/// Persist a sleep request for the next TUI session (`None` = off).
pub fn write_request(minutes: Option<u64>) -> anyhow::Result<()> {
    let dir = crate::config::cache_dir();
    fs::create_dir_all(&dir)?;
    let body = match minutes {
        Some(m) => format!("{{\"minutes\":{m}}}"),
        None => "{\"minutes\":null}".to_owned(),
    };
    option_sdk::atomic_write(request_path(), body.as_bytes())?;
    Ok(())
}

/// Read a pending `msc sleep` request without consuming it.
pub fn peek_request() -> Option<u64> {
    let raw = fs::read_to_string(request_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("minutes")?.as_u64()
}

/// Read (and consume) a pending `msc sleep` request, if any.
pub fn take_request() -> Option<u64> {
    let path = request_path();
    let raw = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(&path);
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("minutes")?.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_walks_off_15_30_60() {
        let mut t = SleepTimer::new();
        assert_eq!(t.cycle(), "sleep timer · 15 min");
        assert_eq!(t.cycle(), "sleep timer · 30 min");
        assert_eq!(t.cycle(), "sleep timer · 60 min");
        assert_eq!(t.cycle(), "sleep timer · off");
        assert!(!t.is_active());
    }

    #[test]
    fn expiry_logic() {
        let mut t = SleepTimer::new();
        assert!(!t.is_expired());
        t.deadline = Some(Instant::now() - Duration::from_secs(1));
        t.total = Some(Duration::from_secs(60));
        assert!(t.is_expired());
        assert_eq!(t.remaining(), Some(Duration::ZERO));
        t.clear();
        assert!(!t.is_active());
        assert!(!t.is_expired());
    }

    #[test]
    fn countdown_formats_m_ss() {
        let mut t = SleepTimer::new();
        t.set_minutes(15);
        let label = t.countdown_label().unwrap();
        assert!(label.starts_with("14:") || label.starts_with("15:"));
        assert_eq!(label.len(), 5);
    }
}
